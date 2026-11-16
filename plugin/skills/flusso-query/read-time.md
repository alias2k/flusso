# Read-time fields: `context`, `rules`, `computed`

Reach for this file when a field's value depends on something known only at request time: the
clock, the caller, a rate. Everything a row decides on its own is a **write-time** field (a script
field or an aggregate in the schema): stored, indexed, free to read. If the function you are about
to write needs nothing from the request, stop and put it in the schema instead.

Two kinds, declared on the root struct only (never on a fragment):

| Attribute | Function | Gives |
| --- | --- | --- |
| `#[flusso(rules = f)]` | `fn f(cx: &Ctx) -> Rules<FieldType>` | value, filter, sort, counts. No script. |
| `#[flusso(computed = f)]` | `fn f(input: &Input, cx: &Ctx) -> FieldType` | value only. Any Rust. |

## The context changes the entry points

```rust
#[derive(Clone)]
struct Viewing { now: DateTime<Utc>, viewer: String }

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = visible_state)]
    state: ContractState,          // replaces the stored `state` on every document
    #[flusso(computed = days_left)]
    days_left: i64,
}

Contract::query(&cx)               // not `Contract::query()`: that no longer exists for this root
Contract::get(&cx, &client, id)
```

- Build **one** context per request and pass it to `query`, `get` and every `Contract::field(&cx)`.
  Mixing two contexts in one search is not caught.
- Generic code: `R::query_in(cx)` / `R::get_in(cx, …)` work for every root; `R::query()` needs
  `R: FlussoRoot<Context = ()>`.
- A root with a context cannot be a `FlussoMultiDocument` variant (compile error). Search its index
  on its own.

## Rule fields

```rust
use flusso_query::rules::Rules;

fn visible_state(cx: &Viewing) -> Rules<ContractState> {
    let doc = Contract::doc();                         // rule handles, generated from the schema
    Rules::when(doc.signed_at().missing())
        .then(ContractState::Draft)
        .when(doc.state().eq(ContractState::Terminated).or(doc.ends_at().lt(cx.now)))
        .then(ContractState::Closed)
        .otherwise(doc.state())                        // or a stored field
}

let state = Contract::state(&cx);                      // RuleField<ContractState>
Contract::query(&cx).filter(state.eq(ContractState::Closed)).sort(state.asc());
let tabs = state.counts(&Contract::query(&cx), &client).await?;   // Vec<(ContractState, u64)>
```

First arm that holds wins. The value is decided in Rust on each hit; the filter, sort and counts are
plain queries over the stored fields the rules read. They always agree.

**The stored field under a read-time field** keeps a handle as `Contract::stored_state()`.

**What `doc()` offers.** Only fields and tests Rust decides exactly as the index does:

| Stored field | Tests |
| --- | --- |
| `keyword` / `enum` / `uuid` | `eq` `ne` `any_of` `exists` `missing` |
| `boolean` | `eq` `ne` `exists` `missing` |
| `byte`…`long`, `double`, `decimal`, `date`, `timestamp` | those, plus `lt` `lte` `gt` `gte` `between` |
| `object` / to-one join | chain: `doc.billing().country()` |

No handle, so it does not compile: `text`, `float`, `geo`, `map`, `json`, `nested` arrays,
`indexed: false`, a field with a `normalizer` / `format` option. No `prefix`, no wildcard, no
field-against-field. Combine with `.and()` / `.or()` / `.not()`; a condition is also a plain filter
(`.filter(doc.ends_at().lt(cx.now))`).

**No field-vs-clock arithmetic.** `ends_at - now < notice_days` has no test. Move the maths to one
side: store `notice_starts_at` at write time and test `doc.notice_starts_at().lte(cx.now)`, or test
`doc.ends_at().lt(cx.now + Duration::days(30))`.

**Dates** compare at the millisecond. A bare date (`"2026-12-01"`) is its whole day: `lt` is before
the day, `lte` through its end.

**What an arm may yield** (`then` / `otherwise`), checked by the compiler:

| Stored field | Reads as |
| --- | --- |
| required | `T` or `Option<T>` |
| nullable, or under a to-one join | `Option<T>`; or `T` via `.assume_present()` when the arm tests `.exists()` |
| array | `Vec<T>` |

**Order** for `asc` / `desc` / `counts`: `.order([..])` on the rules. Without it: the replaced
enum's `variants:`, then every other fixed value in the order the rules name them. A stored enum
rarely declares the states the rules add, so **write `.order([..])`** whenever the order matters.
`lt` / `between` on a `RuleField` follow the Rust type's `PartialOrd`, so keep an enum's variant
order in step. `eq(None)` on an `Option` rule field selects "no value", fixed or stored.

What sorting by a rule field costs (it travels in the score) is in `SKILL.md`, `## Sorting`.

## Computed fields

```rust
#[derive(serde::Deserialize)]
struct Term { ends_at: DateTime<Utc>, state: ContractState }   // any subset of the document

fn days_left(term: &Term, cx: &Viewing) -> i64 { (term.ends_at - cx.now).num_days() }
```

- `Input` reads from the document **after** the rule fields are written (`state` above is the
  visible one), and after the computed fields declared above it.
- No handle: it cannot be filtered or sorted. If you need that, the part you filter on is a rule
  field; build the rest from it in a computed function.
- The field type is `Serialize + Deserialize`; the context is `Clone + Send + Sync + 'static`.

## Run-time failures: `Error::ReadTimeField`

Raised before the request is sent, or while a document decodes. Causes: a date compared with text
that is not a date; rule sorts that cannot share the score; a document missing a `required` field a
rule tests; a computed input that does not deserialize.

## Paging

One context for a whole cursor walk: see `SKILL.md`, `## Paging`.
