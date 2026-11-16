# Read-time fields

A field whose value depends on the clock or the caller is declared on the root and decided when a document is read: a **rule field** gives the value plus a filter, sort and counts on it with no script, a **computed field** gives the value from any Rust function.

## When a field belongs here

Whatever a row decides on its own belongs at write time: a [script field](../author/compute-with-scripts.md) or an [aggregate](../reference/aggregates.md) is stored, indexed and free to read. A read-time field is for what cannot be stored:

- it changes with no write: a contract becomes `expiring` because time passed;
- it differs per request: `is_mine`, a fee at today's rate.

The test: a function that needs nothing from the request is a write-time field.

## The context

The request-time values live in one consumer-defined type, the root's **context**. Naming it changes the entry points: they take it.

```rust
#[derive(Clone)]
struct Viewing { now: DateTime<Utc>, viewer: String, rate: f64 }

#[derive(Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = visible_state)]
    state: ContractState,
    #[flusso(computed = days_left)]
    days_left: i64,
}

let page = Contract::query(&cx).send(&client).await?;
let one = Contract::get(&cx, &client, "c1").await?;
```

A root without `context` is untouched: `Contract::query()`, `Contract::get(&client, id)`. Code generic over roots reaches either through `R::query_in(cx)` / `R::get_in(cx, …)`, where `R::Context` is `()` for a plain root.

> ⚠️ **Warning** — Build the context once per request and pass that one value everywhere. A filter built from one clock on a search built from another describes two different moments, and nothing can catch it.

## Rule fields

A rule function returns an ordered list of *condition → outcome* arms. The first arm whose condition holds decides the value.

```rust
fn visible_state(cx: &Viewing) -> Rules<ContractState> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(ContractState::Draft)
        .when(doc.state().eq(ContractState::Terminated).or(doc.ends_at().lt(cx.now)))
        .then(ContractState::Closed)
        .when(doc.notice_starts_at().lte(cx.now).and(doc.auto_renew().eq(false)))
        .then(ContractState::Expiring)
        .otherwise(doc.state())
}
```

That one list gives four things, all answered by the index from the stored fields the rules read:

```rust
let state = Contract::state(&cx);                    // the rule field, for this context

Contract::query(&cx)
    .filter(state.eq(ContractState::Expiring))       // also ne, any_of, lt, lte, gt, gte, between
    .sort(state.asc())
    .send(&client).await?;                           // every hit's `state` is decided too

let tabs = state.counts(&Contract::query(&cx), &client).await?;   // [(Draft, 3), (Closed, 12), …]
```

The value is decided in Rust on each fetched document; the filter is the same conditions as a plain query. A listing never shows a value its own filter does not select.

A rule field named like a stored field replaces it on every document. The stored field's handle moves to `Contract::stored_state()`, so `Contract::state` always means what documents show. The same prefix covers a schema key that would collide with an entry point: a `query` field is `Contract::stored_query()`.

## What a rule can test

`Contract::doc()` has a handle for each stored field a rule can read, generated from the schema, with the tests a rule can make:

| Stored field | Tests |
| --- | --- |
| `keyword`, `enum`, `uuid` | `eq`, `ne`, `any_of`, `exists`, `missing` |
| `boolean` | `eq`, `ne`, `exists`, `missing` |
| `byte` … `long`, `double`, `decimal` | the above, plus `lt`, `lte`, `gt`, `gte`, `between` |
| `date`, `timestamp` | the same as numbers |
| `object`, a to-one join | reach through it: `doc.billing().country()` |

Combine conditions with `.and(..)`, `.or(..)`, `.not()`. A condition is also an ordinary filter: `Contract::query(&cx).filter(doc.ends_at().lt(cx.now))`.

Every test is decided twice, by the index for a filter and in Rust for a value, so only tests both sides decide alike exist. `text`, `float`, `geo`, `binary`, `map` and `json` fields, `nested` arrays, `indexed: false` fields and fields with matching-related `options` (a normalizer, a date format) have no handle, and neither do `prefix`, wildcards or field-against-field comparison: such a rule does not compile.

How values compare, on both sides:

- **Missing** fails every test except `missing()`. An **array** passes when any element does.
- **Keywords** compare exactly, as the document spells them.
- **Dates** compare at the millisecond. A timestamp written to the second is that instant; a bare date is its whole day, so `lt("2026-12-01")` is "before the 1st" and `lte("2026-12-01")` is "through the 1st".
- **Decimals** compare as the index holds them: as the nearest `double`.

> 💡 **Did you know** — Arithmetic between a field and the clock has no test (`ends_at - now < notice_days`). Move the maths to one side: store `notice_starts_at` at write time and test `notice_starts_at <= now`, or compare `ends_at` with `now + 30 days` computed in the rule function.

## What a rule can yield

An outcome is a fixed value of the field's type, or a stored field of the matching kind. A fixed value is matched as it serializes: `eq(None)` selects a fixed `None` and a stored field holding nothing alike.

| Stored field is | It yields as |
| --- | --- |
| required | `T`, or `Option<T>` |
| nullable, or under a nullable object | `Option<T>`; `T` after `.assume_present()` |
| an array | `Vec<T>` |

`assume_present()` is the rule's word that the field is there whenever its arm holds, so test it in the arm: `.when(doc.discount_until().gt(cx.now).and(doc.discounted_fee().exists())).then(doc.discounted_fee().assume_present())`.

A fixed value can be any serializable type, a small struct included. A struct is filtered and sorted as a whole, not by its parts.

## Order, for sorts and counts

A rule field sorts and counts in the order `Rules::…otherwise(..).order([..])` lists. Without it the order is the `variants:` of the stored `enum` field it replaces, followed by every other fixed value in the order the rules name them. A stored enum usually declares only what it stores (`active`, `suspended`), while the rules add states of their own (`draft`, `expiring`), so a rule field with an order that matters spells it out.

Ordered filters (`lt`, `between`, …) follow the Rust type's own `PartialOrd` instead, so keep the two in step for an enum.

A rule sort travels in the score, which costs three things:

- text relevance is replaced for that search, and `Sort::score()` or `min_score` beside it is refused;
- several rule sorts must sit next to each other in the sort list;
- `Sort` options (`missing_first`, `mode`, …) do not apply to it.

`SortBuilder::by(Contract::state(&cx), dir)` takes a rule field like any sortable handle.

## Computed fields

A computed function takes whatever it needs from the document, plus the context, and returns the field's type:

```rust
#[derive(Deserialize)]
struct Term { ends_at: DateTime<Utc>, state: ContractState }

fn days_left(term: &Term, cx: &Viewing) -> i64 {
    (term.ends_at - cx.now).num_days()
}
```

- The first argument is any `Deserialize` type. It reads from the document with the rule fields already written, so `state` above is the *visible* state.
- Computed fields run in declaration order, and each sees the ones above it.
- There is no handle: a computed field cannot be filtered, sorted or counted on. When that is needed, the part to filter on is a rule field.
- The field's type is both written and read, so it is `Serialize` and `Deserialize`. The context is cloned once per search: `Clone + Send + Sync + 'static`.

## What is refused, and when

Most misuse is a compile error: a field with no rule handle, a value of another kind, a nullable field yielded as a plain type, a function returning another type than its field, a read-time field with no `context`, on a fragment, or in a [combined search](several-indexes.md).

What only shows at run time is `Error::ReadTimeField`, raised before any request is sent or while a document is decoded:

- a date compared with text that is not a date (`lt("next tuesday")`);
- rule sorts that cannot share the score (see above);
- a document missing a field the schema declares `required`, which a rule tests;
- a computed function whose input does not read from the document.

## Paging

A [page cursor](paging.md) is bound to its query, and a rule filter or sort carries the clock. Keep one context for a whole walk (send `now` back with the cursor); a moved clock is `Error::CursorMismatch`. A search that only *shows* read-time fields has no clock in its query and resumes with any context.

## Related

- [Compute a field with a script](../author/compute-with-scripts.md) and [Aggregates](../reference/aggregates.md) for everything that needs no context.
- [Sorting](sorting.md) and [Enums and custom values](enums-and-values.md) for stored-field order.
- [Binding to the schema](binding.md) for what the derive generates.
