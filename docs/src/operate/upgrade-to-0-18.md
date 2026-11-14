# Upgrade to 0.18

0.18 indexes a `binary` field as base64 of its bytes and a `constant` as written, so each index with a `binary` field or a `constant` that is stored unindexed (an object, or a list holding one or mixing kinds) is rebuilt once into a new physical index. It also writes a `timestamp` column in ISO 8601, which changes stored documents and can break a tuned `format`: see [A timestamp column is written in ISO 8601](#a-timestamp-column-is-written-in-iso-8601). A `decimal` field is written with every digit, and a `NaN` or infinity in a number field fails its document: see [A decimal is written with every digit](#a-decimal-is-written-with-every-digit).

## When to use this

A deployment going from 0.17 to 0.18. The steps apply when its schemas declare, anywhere in the document, joined tables included:

- a [`binary`](../reference/field-types.md) field, or an `ids` aggregate with `element_type: binary`;
- a [`constant`](../reference/field-types.md#constant) whose `value` is an object, or a list holding one or mixing kinds of scalar (`[1, "n/a"]`, or `[1, 2.5]`: a fractional number after a whole one).

Without either, no index is rebuilt. A `constant` holding a number, a boolean, or a list of one kind of scalar needs nothing either: see the last step.

**What was wrong.** A `binary` field indexed the column's Postgres hex text (`\x0102…`) where OpenSearch's `binary` expects base64, at every position: the root, `bytea[]`, every join, any depth. OpenSearch stored it, so `_source` returned the hex text, not the bytes. A `constant` object or list built `null`, a number or boolean its text (`"5"`, `"true"`).

**Why the rebuild.** The content hash is the physical index name. The fix moves the hash of every schema listed above (and of no other schema), so the first run finds a new hash alias with no seed marker and backfills it from Postgres on every OpenSearch sink, with a committed `flusso.lock` and `--locked` too. Such a constant is mapped as an unindexed `object`, which the old index can't take: the strict root refuses an object's keys, and a field mapped from a list's first element refuses an element of another kind (`[1, "n/a"]` in a `long`).

## Steps

1. **Upgrade flusso.** Stop 0.17 and start 0.18 on the same slot. The sink backfills the affected indexes. Until that snapshot completes, `/status` shows the phase `backfilling`, each affected index `backfilling`, and every other index `pending`, although those keep their seeded index and serve reads throughout.
2. **Switch consumers that decoded the hex text.** A consumer that read a `binary` field's `\x…` text and decoded it itself must decode base64 instead. A `flusso-query` document still types the field as `String`; only its contents change.
3. **Roll out the consumers and remove the old index** as in [Upgrade to 0.17](upgrade-to-0-17.md#steps), steps 5 to 8: a `flusso-query` consumer bakes the schema hash, so rebuild it against 0.18 once the new index is `seeded`, then delete the old generation and its seed marker by hand.
4. **Rewrite scalar constants, if `_source` matters.** A number or boolean constant was indexed from its text, which OpenSearch coerced, so search, sort, and aggregations are unchanged; only `_source` held `"5"` for `5`. A document picks up the typed value at its next write. To rewrite all of them now, run [`flusso reindex <index>`](../reference/cli.md#reindex). A list of one kind of scalar was `null`; the same reindex fills it in.

## Options and variations

- **Rolling back to 0.17** returns to the old index, which missed every change made since. Run `flusso reindex <index>` for each affected index after the rollback.
- **Several OpenSearch sinks** each rebuild their own copy. A `stdout` sink has no index; its output carries the corrected values from the first run.

## A timestamp column is written in ISO 8601

**Breaking.** A `timestamp` (without time zone) value was written with a space, `2024-01-02 03:04:05.123`; 0.18 writes `2024-01-02T03:04:05.123`, everywhere the value goes (see [Field types](../reference/field-types.md#dates-and-times)). This alone moves no content hash and rebuilds nothing. What a deployment sees depends on the field's mapping:

| Mapping on 0.17 | On 0.17 | On 0.18 | What to do |
| --- | --- | --- | --- |
| default (no `format`) | every document holding a value rejected | indexed | Nothing to change; `flusso reindex` fills in documents that were quarantined. |
| a `format` with only space patterns (`yyyy-MM-dd HH:mm:ss…`) | indexed | **every write holding a value rejected** | Add `strict_date_optional_time` to the `format` with the upgrade. |
| a `format` that also reads ISO, or `ignore_malformed: true` | indexed (`ignore_malformed`: stored, not searchable) | indexed; `_source` changes at each document's next write | `flusso reindex <index>` to make every document uniform. |
| inside an `indexed: false` join | stored | stored in the new form at each document's next write | `flusso reindex <index>`. |

`flusso check` warns about the second row with [`timestamp_format`](../reference/cli.md#check), with `--offline` too, and so does the start of `run`; neither stops.

1. **A default mapping.** Under `on_error = "stop"` the sink engine stopped on the first such document and resumes from the batch it held. Under `"skip"` each was quarantined: it lands at its row's next write, or run [`flusso reindex <index>`](../reference/cli.md#reindex) to fill them all in now.
2. **A space-only `format`.** After the upgrade the first write holding a value is rejected: under `"stop"` the sink engine stops; under `"skip"` the document keeps its 0.17 version (an update is lost) and a new row is missing. Before starting 0.18, put `strict_date_optional_time` first in the `format`, keeping the space patterns so a query bound written with a space still parses:

   ```yaml
   - timestamp: placedAt
     options:
       format: "strict_date_optional_time||yyyy-MM-dd HH:mm:ss.SSS||yyyy-MM-dd HH:mm:ss"
   ```

   `options` is part of the content hash, so the first 0.18 run backfills that index into a new physical index, every document in the new form. Then roll out consumers and remove the old index as in [Steps](#steps).
3. **Stored copies.** An `ignore_malformed` field and an unindexed join hold both forms until each document is next written. `flusso reindex <index>` rewrites every one in the new form, with no restart. An `ignore_malformed` field was malformed under the default format on 0.17, so its values were stored but never searchable; once rewritten they are.
4. **Consumers reading `_source`.** `chrono::NaiveDateTime` reads only the new form, `jiff::civil::DateTime` and `String` read both, `time::PrimitiveDateTime`'s default serde reads only the old one. Give the `time` field a format that reads the new one:

   ```rust
   time::serde::format_description!(
       flusso_timestamp,
       PrimitiveDateTime,
       "[year]-[month]-[day]T[hour]:[minute]:[second][optional [.[subsecond]]]"
   );

   #[derive(serde::Deserialize)]
   struct Order {
       #[serde(with = "flusso_timestamp")]
       placed_at: time::PrimitiveDateTime,
   }
   ```

   A `stdout` sink consumer sees the new form from the first run.

## A decimal is written with every digit

**Breaking.** A `decimal` value was rounded to the nearest `f64` on its way to the document, and one past `rust_decimal`'s range (±7.9 × 10^28, or more than 28 fractional digits) was written as a JSON string, `"1000000000000000000000000000000"`. 0.18 writes the number Postgres stores, every digit, everywhere a `decimal` goes: a column, an array element, a `sum`/`min`/`max`, an `ids` key (see [numbers](../reference/field-types.md#numbers)). No content hash moves and nothing is rebuilt; search, sort and aggregations still see a `double`.

| Value | 0.17 `_source` | 0.18 `_source` |
| --- | --- | --- |
| `12.50` | `12.5` | `12.50` |
| `1234567890.123456789` | `1234567890.1234567` | `1234567890.123456789` |
| `1e30` | `"1000000000000000000000000000000"` | `1000000000000000000000000000000` |
| `NaN`, `Infinity`, `-Infinity` | the string, which OpenSearch rejects | the document fails to build |

1. **Stored documents.** Each document takes the new form at its row's next write. Run [`flusso reindex <index>`](../reference/cli.md#reindex) on each index with a `decimal` field to rewrite every one now, with no restart.
2. **Consumers.** A field read as `String` because it could hold the string form now fails on that document: read it as `f64` or `serde_json::Value` (see [What the derive trusts](../query/document-structs.md#what-the-derive-trusts)). A `rust_decimal::Decimal` still can't hold a value past its range.
3. **`NaN` and infinities.** A `NaN`, `Infinity` or `-Infinity` under any number field now fails the document on the ingest side, decided by [`on_error`](../reference/index-and-on-error.md#on_error), where OpenSearch used to reject the string at the sink. Under `stop` the ingest engine stops before any sink receives the batch (`DocumentFailed` in the log, not a rejection); under `skip` no sink receives the document and each keeps its previous version, where 0.17 quarantined it. A `stdout` sink no longer prints such a document.

## Related

- [Field types](../reference/field-types.md) for what a `binary` field, a `constant`, a `timestamp` and a [number](../reference/field-types.md#numbers) index.
- [Reindex without downtime](reindex.md) for rebuilding an index on demand.
- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for hash aliases and generations.
