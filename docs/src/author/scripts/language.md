# The Rhai you need

Script fields are written in [Rhai](https://rhai.rs/book/), a small Rust-embedded language that reads like a blend of JavaScript and Rust; this page covers the part of it a script field uses, and the handful of places it bites.

> ℹ️ **Info** — The [Rhai book](https://rhai.rs/book/) is the full language reference. flusso runs it with the standard library, minus the clock, modules and `eval`; [Script fields](../../reference/scripts.md#the-sandbox) lists the sandbox rules.

## A script is an expression

The value of the last expression is the field's value. Statements end with `;`; the last line doesn't need one. `return` ends the script early.

```rust
let first = this.firstName;
let last = this.lastName;
`${first} ${last}`
```

```rust
if this.stock == 0 { return "sold out"; }
"available"
```

`this` is the object at the field's own level, `root` the whole document; both are read-only snapshots of the document as built, before any script ran. Assigning to them changes nothing outside the script.

## Values

Every document value arrives as a Rhai value; the full mapping is in [Values in and out](../../reference/scripts.md#values-in-and-out).

| You read | You get | Check with |
| --- | --- | --- |
| an integer column | `i64` | `type_of(x) == "i64"` |
| `float`, `double` | `f64` | `"f64"` |
| `decimal` | `decimal`, exact | `"decimal"` |
| text, `uuid`, `date`, `timestamp` | string | `"string"` |
| a group, a to-one join, a `map` | object map | `"map"` |
| a to-many join, an `ids` aggregate | array | `"array"` |
| null | `()` | `x == ()` |

`type_of(this.price)` is the quickest way to find out what arrived; `print` it (see [Test and debug a script](debugging.md)).

## Strings

Interpolation uses backticks: `` `${this.sku} — ${this.name}` ``. `+` concatenates a string with anything: `"#" + this.id`.

> ⚠️ **Warning** — Some string methods change the string **in place and return `()`**. Chaining them silently yields `()`, which is a `null` on a nullable field and an error on a required one.

| Changes in place, returns `()` | Returns a new value |
| --- | --- |
| `trim()`, `replace(a, b)`, `pad(len, ch)`, `truncate(len)`, `clear()`, `make_upper()`, `make_lower()` | `to_upper()`, `to_lower()`, `sub_string(start, len)`, `split(sep)`, `index_of(s)`, `contains(s)`, `starts_with(s)`, `ends_with(s)`, `len()` |

So copy into a variable first, then change it:

```rust
let sku = this.sku;
sku.trim();
sku.replace(" ", "-");
sku.to_upper()
```

`pad` pads on the **right**. For a zero-padded number, build it up: `let s = "" + this.id; while s.len() < 5 { s = "0" + s; } s`.

## Numbers and decimals

- `i64` and `f64` mix freely: `this.stock * 1.5` is a float. A float result for an integer field is an error; turn it into one with `.round().to_int()`, `.floor().to_int()`, or `.to_int()` (truncates).
- Integer division truncates: `7 / 2` is `3`. Write `7.0 / 2` for `3.5`.
- `decimal` stays exact, and **does not mix with floats**: `price * 1.22` fails with `Function not found: * (decimal, f64)`. Use a decimal literal, `price * parse_decimal("1.22")`, or leave exactness behind with `price.to_float() * 1.22`.
- A decimal does mix with integers: `price + 1`, `price > 20`.
- Useful: `round(digits)` on a decimal, `round()`/`floor()`/`ceiling()` on a float, `abs()`, `sqrt()`, `min(a, b)`, `max(a, b)`, `parse_int(s)`, `parse_float(s)`.

## Null and missing

`()` is null. Two different things look alike:

| Situation | `this.x` | Test it with |
| --- | --- | --- |
| the field is in the document, its value is null | `()` | `this.x == ()` |
| the field isn't in the object at all | an error: `Property not found: x` | `"x" in this` |

Reading a property the object doesn't have fails on purpose, so a typo is an error rather than a quiet null. Inside a `map` field, keys are runtime data, so test before you read: `if "de" in this.title { … }`.

- `a ?? b` is `a`, or `b` when `a` is `()`: `this.nickname ?? this.name`.
- `a?.b` reads `b` only when `a` isn't `()`: `this.nickname?.to_upper() ?? "none"`.
- Calling a method on `()` fails: `Function not found: to_upper (())`. Guard with `?.` or `??`.

## Conditions and branching

`if` is an expression, so it returns a value:

```rust
if this.price < 20 { "budget" } else if this.price < 100 { "mid" } else { "premium" }
```

`switch` matches values and ranges (`1..5` excludes 5, `1..=5` includes it); `_` is the default. A case can't have a condition of its own, so use `if` for comparisons.

```rust
switch this.stock {
  0 => "out",
  1..5 => "low",
  _ => "plenty",
}
```

## Arrays and closures

A `has_many`/`many_to_many` arrives as an array of objects. Closures are `|v| expression`.

| Want | Write |
| --- | --- |
| keep some | `this.variants.filter(\|v\| v.stock > 0)` |
| one value each | `this.variants.map(\|v\| v.color)` |
| how many | `….len()`, `….is_empty()` |
| any / every | `….some(\|v\| v.stock == 0)`, `….all(\|v\| v.price > 5)` |
| the first match | `….find(\|v\| v.color == "blue")` (`()` when none) |
| a total | `….reduce(\|sum, v\| sum + v.price, 0.0)` |
| membership | `"sale" in this.tags.map(\|t\| t.label)` |
| sort | `list.sort()` on plain values; `list.sort(\|a, b\| …)` returning `-1`, `0`, `1` on objects |
| unique | `list.sort(); list.dedup();` |
| index | `list[0]`, `list[-1]` (the last) |

`sort`, `dedup` and `reverse` change the array in place, like the string methods above. Give `reduce` a starting value: without one it starts from `()`.

A loop gives the element and, if asked, its position:

```rust
let line = "";
for (tag, i) in this.tags {
  if i > 0 { line += ", "; }
  line += tag.label;
}
line
```

Indexing past the end fails: `Array index 3 out of bounds`. Check `len()` first, or `is_empty()`.

## Objects

An object map reads with `.name` or `["name"]`; the brackets take a runtime key (`this.title[lang]`). `keys()` and `values()` list it; `in` tests a key. Build one with `#{ a: 1, b: 2 }`. A script field can't *return* an object; see [Script fields](../../reference/scripts.md#script) for the types it can.

## What isn't there

- **The database.** A script reads only the built document; fold what it needs in with a [join](../../reference/joins.md) or an [aggregate](../../reference/aggregates.md), and mark it [`emit: false`](../../reference/scripts.md#emit) if it shouldn't be indexed.
- **The clock.** `timestamp()` doesn't exist: a script runs again on every rebuild and must give the same answer. Dates are strings; compare them as strings (`this.createdAt > "2024-01-01"`, ISO dates sort correctly) or cut them up (`this.createdAt.sub_string(0, 4)`).
- **Other scripts' output.** Each script sees the document before any script ran; a sibling script field reads as `()`.
- **Functions of your own.** A script is one expression body; `fn` definitions aren't accepted. Closures work.
- **Imports and `eval`.**

## Related

- [Script recipes](recipes.md): tested scripts for the common jobs.
- [Test and debug a script](debugging.md).
- [Script fields](../../reference/scripts.md): every key and rule.
