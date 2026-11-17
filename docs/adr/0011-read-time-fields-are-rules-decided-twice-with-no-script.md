---
status: accepted
---

# Read-time fields are rules decided twice, with no script

Some fields depend on a value that exists only when a document is read: the clock, the caller, a rate (issue #381). A contract is `expiring` from a moment that arrives with no write, so the value cannot be stored. Consumers recomputed it after every search and wrote the matching filter by hand, and the two drifted: a listing showed a state its own filter did not select.

The mechanism: a root names its request-time values as a **context** type and declares two kinds of field.

- A **rule field** is an ordered list of *condition → outcome* arms over stored fields; the first arm that holds wins. The one list is used twice. In Rust it decides the value on each fetched document's `_source`. Rendered as plain queries it gives the filter ("this arm holds and no earlier one does", OR-ed over the arms that yield the value), the sort (a `function_score` whose weights are the value's rank, sorted on `_score`) and the counts (one `_msearch` of counts).
- A **computed field** is a Rust function over the document and the context. Value only.

Three facts carry the design:

- A condition is built only from **rule handles**, which the derive generates for the stored fields, and with the tests, that Rust decides exactly as the index does: keyword equality, boolean, whole numbers, `double`, `decimal` as a double, dates at the millisecond. Anything the two sides could disagree on (analyzed text, a 32-bit float, a field with a normalizer or a date format, a `nested` array) has no handle, so a rule that cannot be decided is a compile error.
- The handle's type carries whether the field always holds one value, can be missing, or is an array. That decides what a rule may yield it as (`T`, `Option<T>`, `Vec<T>`), and whether a negated range may be sent as the opposite range.
- The Rust side decides the form that is sent, not the form that was written. A document missing a field the schema declares required matches neither a range nor its opposite, so no filter selects it under any value; it is refused rather than shown under a value its filter would not find.

## Considered options

- **A painless script per field** (`script` query, `script_fields`, script sort). The first spike. One definition in a second language, 56 ms for a filter over 200,000 documents against 18 ms for the rules and 5 ms for a stored field, and the value still had to be mirrored in Rust or fetched per hit.
- **OpenSearch `derived` fields.** Scripts under another name, with the same cost, and unavailable on Elasticsearch.
- **Return which arm matched from the index** (`matched_queries`, one named query per arm) instead of deciding the value in Rust. It ties the value to a search: `get` by id has no query, and every search would carry every rule field's arms whether or not it filters on them.
- **A general expression language** compiled to both a query and a Rust evaluator (arithmetic between fields and the context). Field-against-field arithmetic has no index-side form but a script. Everything that needs no context belongs at write time, where flusso already computes it; what is left for read time compares a stored field with a value the rule function computes, which plain range and term queries answer.
- **One `filters` aggregation for the counts.** Measured at 73 to 138 ms against 23 ms for an `_msearch` of count queries over the same data.
- **Conditions built from the ordinary query handles.** They offer tests Rust cannot decide (`prefix`, `matches`, subfields, time zones), so a rule could silently disagree with itself. A separate, smaller handle family makes the supported set the only one that compiles.
- **Two root traits**, one with a context and one without. One trait with `type Context` (`()` for a plain root) keeps generic code and every transport on one path; the plain entry points stay behind `Context = ()`.

## Consequences

- **`FlussoRoot` gained `type Context`**, which breaks a hand-written impl: a query-train minor. A root with a context has no `query()` / `get(client, id)`; it gets `query(&cx)` / `get(&cx, …)`, and the trait has `query_in` / `get_in` for generic code.
- **A rule sort takes the score.** Text relevance is replaced for that search; `min_score`, a relevance sort beside it, rule sorts separated by another key, and more rank combinations than a 32-bit score keeps apart (2^24) are refused before the request is sent.
- **A page cursor binds the clock.** The context's values are in the query, and a cursor is bound to its query, so a walk keeps one context. A moved clock is `CursorMismatch`, which is the honest answer: the pages would not be one list.
- **The context is not checked across a search.** A filter built with one context on a search built with another runs, and describes two moments.
- **A combined search refuses a root with a context**: a blended search has nowhere to take it from (issue #383).
- **Errors that survive compilation surface before the request is sent**, as `Error::ReadTimeField`: a date compared with text that is not a date, a sort combination that cannot run. A rule list that cannot be built fails every search and fetch of its root, whether or not the request would have decoded a document. They travel as a stand-in clause the transports refuse, so no query type had to grow an error channel; sent by hand, OpenSearch rejects the clause too.
- **Plain date filters still differ on one case** (issue #382): given a timestamp written to the second, the ordinary `Date` handle lets OpenSearch read it as the whole second, while a rule condition sends the instant.
- **Exact double decisions rest on `float_roundtrip`** (issue #366): without it a `double` read back from `_source` can be one unit in the last place off, and a rule decides differently from the index.
- **Guarded by** the `rules::tests` unit tests (both forms of 2,000 random conditions decide alike in Rust), the derive's `read_time` and UI tests (every refusal), and the `read_time` live test: 23,040 values, 822 filters, 24 sorts and 6 count sets against a plain-Rust oracle, and 800 random conditions decided by the index and by Rust.
