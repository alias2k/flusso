# Upgrade to 0.18

0.18 indexes a `binary` field as base64 of its bytes and a `constant` as written, so each index with a `binary` field or a `constant` that is stored unindexed (an object, or a list holding one or mixing kinds) is rebuilt once into a new physical index. It also writes a `timestamp` column in ISO 8601, which changes stored documents and can break a tuned `format`: see [A timestamp column is written in ISO 8601](#a-timestamp-column-is-written-in-iso-8601). A `decimal` field is written with every digit: see [A decimal is written with every digit](#a-decimal-is-written-with-every-digit). A `double` is indexed as the exact value Postgres holds: see [A float or double is indexed as Postgres holds it](#a-float-or-double-is-indexed-as-postgres-holds-it). A filter value that doesn't fit its column no longer matches by being cut to fit: see [A filter value is compared as written](#a-filter-value-is-compared-as-written). A script reads a date as a date, not a string: see [A date reaches a script as a date](#a-date-reaches-a-script-as-a-date). A change is indexed only once its transaction is visible: see [A change waits until its commit is visible](#a-change-waits-until-its-commit-is-visible). A fenced read in `flusso-query` waits through a restart of flusso, with a 20 s default timeout: see [A fenced read waits through a restart](#a-fenced-read-waits-through-a-restart). A stop drains and has a deadline and new exit codes: see [A stop drains, then exits](#a-stop-drains-then-exits). A document OpenSearch refuses as busy is resent instead of reaching `on_error`: see [A busy cluster no longer drops documents](#a-busy-cluster-no-longer-drops-documents). `/metrics` gains per-stage timings, and the engine's observer types change for code embedding the crates: see [Stage timings in /metrics](#stage-timings-in-metrics). Each sink writes several batches at once, and a custom `Sink` implements one `write` instead of `apply` and `flush`: see [Each sink writes several batches at once](#each-sink-writes-several-batches-at-once). A backfill reads its keys in pages with no long transaction: see [A backfill reads key pages, with no long transaction](#a-backfill-reads-key-pages-with-no-long-transaction). Several batches of documents are built at once: see [Several batches build at once](#several-batches-build-at-once). Memory is bounded in bytes and `[stream] capacity` is deprecated: see [Memory is bounded in bytes](#memory-is-bounded-in-bytes).

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

**Breaking.** A `decimal` value was rounded to the nearest `f64` on its way to the document, and one past `rust_decimal`'s range (±7.9 × 10^28, or more than 28 fractional digits) was written as a JSON string. 0.18 writes the number as Postgres wrote it into the document, every digit, wherever a `decimal` is declared: a column, an array element, a `sum`/`min`/`max`, an `ids` key, a `map` value, a script field (see [numbers](../reference/field-types.md#numbers)). No content hash moves and nothing is rebuilt. Every other number is read exactly too, which moves some of them: see [A float or double is indexed as Postgres holds it](#a-float-or-double-is-indexed-as-postgres-holds-it).

| Value under `decimal` | 0.17 `_source` | 0.18 `_source` |
| --- | --- | --- |
| `5` | `5.0` | `5` |
| `5.00`, `1.10` | `5.0`, `1.1` | `5.00`, `1.10` |
| `1234567890.123456789`, a `bigint`-sized value | rounded to 17 digits | every digit |
| `1e30` | `"1000000000000000000000000000000"` | `1000000000000000000000000000000` |
| `"2.5"`, a text value in a `map` with `values: decimal` | `"2.5"` | `2.5` |
| a `float8` column: `1e-07`, `9.007199254740992e+15`, `1e+308` | `1e-7`, `9007199254740992.0`, a string of 309 digits | as Postgres wrote it |
| `1e2` in a `json` column | `100.0` | `1e2` |

A level of more than 50 fields is built as `jsonb`, which writes a number without an exponent (`0.0000001`, `100`).

1. **Indexed values.** OpenSearch now parses the exact text, where 0.17 parsed a rounded one. For some long values the indexed `double` moves by one unit in the last place (`357273734.880428249` was indexed as `357273734.8804282` and is now `357273734.88042825`), so a `term` query for a previously stored double can miss, and range, sort and histogram edges next to those values shift. A number literal longer than 1000 characters (a `numeric` like `1e-1200`, which 0.17 indexed as `0.0`) and a value of `1e309` or more are item-level rejections the index's [`on_error`](../reference/index-and-on-error.md#on_error) decides; under `stop` the sink engine stops. In 0.17, `1e309` or more failed the whole batch's document query.
2. **`_source` filtering.** `_source_includes`/`_source_excludes` make OpenSearch render the numbers again (`12.50` comes back `12.5`): the exact text is only in the unfiltered `_source`.
3. **Consumers.** An `f64` or `rust_decimal::Decimal` (default serde features, which read a number through `f64`) reads the nearest `f64` of the exact text. 0.17's `_source` held a value already rounded once, so some long values now read one unit in the last place differently (`357273734.880428249` read as `357273734.8804282` in 0.17 and `357273734.88042825` in 0.18). A `serde_json::Value` changes shape: `5.0` becomes the integer `5`, and a past-range string becomes a number. A field read as `String` because it could hold the string form now fails on that document: read it as `f64` or `serde_json::Value` (see [What the derive trusts](../query/document-structs.md#what-the-derive-trusts)). A `rust_decimal::Decimal` still can't hold a value past its range.
4. **Scripts.** A declared decimal reaches a script as the nearest `Decimal` (about 28 significant digits, at most 28 after the point; a next digit of 5 or more rounds away from zero), where it held about 17 (see [Values in and out](../reference/scripts.md#values-in-and-out)): `to_string` keeps trailing zeros (a `keyword` script field built from `1.10` was `1.1`), and comparisons use every digit. A value from about 79228162514264333195497439232 up to `Decimal::MAX` is a decimal where it was a string (`f64` rounded it to 2^96, past the range): a script returning it to a `keyword` field now fails its type check, and `+ 1` can overflow and fail the document. A value past the range is still a string, now of the exact text Postgres wrote (`"79228162514264337593543950336"`, `"1e+30"` for a `float8` read under `decimal`) where it held the `f64`'s digits (`"79228162514264340000000000000"`, `"1000000000000000000000000000000"`): `len()`, `+` and `parse_decimal` see the new text. A decimal's `to_float()` is the `f64` nearest its exact value. A `decimal` script field's result is written with every digit, a zero without a sign, and it still refuses a string.
5. **Stored documents.** Each document takes the new form at its row's next write. Run [`flusso reindex <index>`](../reference/cli.md#reindex) on each index with a `decimal` field to rewrite every one now, with no restart.

## A float or double is indexed as Postgres holds it

**Breaking for stored values.** 0.17 read the document's numbers with a fast parser that isn't always correctly rounded, so a `double` was often indexed one unit in the last place away from the value Postgres holds: about one random `float8` in three, about one in ten between ±1,000,000 (`906404.2095968979` was indexed as `906404.209596898`). 0.18 reads every number correctly rounded and writes it as its shortest digits (see [numbers](../reference/field-types.md#numbers)). No content hash moves and nothing is rebuilt.

"No number type" below means any number flusso reads with no declared number type: a `json` field, a `map` value of any kind, a `keyword`, `text` or `custom` field over a number, and a value its declared type doesn't fit (a fraction under `long` or a `count`, a nested array in a typed column).

| Value | 0.17 `_source` | 0.18 `_source` |
| --- | --- | --- |
| a `double` column, array element, `sum`/`min`/`max`: `906404.2095968979` | `906404.209596898` | `906404.2095968979` |
| an `avg`: `4251.1509216589861751` | `4251.150921658986` | `4251.150921658987` |
| no number type: `1234567890.123456789` | `1234567890.123457` | `1234567890.1234567` |
| no number type: `0.000000000000000000000000000001234` | `0.0` | `1.234e-30` |
| no number type, past ±7.9 × 10^28: `1.5e+300` | `"1500…000"`, a string of 301 digits | unchanged |
| a `float` over a `real`, `double precision` or `numeric` column | unchanged, but for a `double precision` value at an exact `f32` midpoint, now rounded as Postgres's `::float4` rounds it | |

1. **Indexed values.** A `term` query for a double read from a 0.17 document can miss the same row's 0.18 document, and range, sort and histogram edges at the moved values shift by one unit in the last place. A value 0.17 indexed as `0.0` because it was too small is now that number: a dynamically mapped key takes it (a value under `float`'s smallest one is stored as `0`), on a fresh index and on one 0.17 filled. A number past ±7.9 × 10^28 stays the string of its digits, so no document 0.17 indexed is refused, `flusso reindex` included.
2. **Scripts.** A number with no number type still reaches a script as a `decimal` (or a string past ±7.9 × 10^28), now holding the double's own digits, not about 15 of them: `9223372036854775808` stays `9223372036854775808` (an integer past `i64` is exact), a value that `Decimal` can't hold exactly is rounded to 28 places after the point (`3.141592653589793e-14` is `0.0000000000000314159265358979`, where 0.17 kept fewer digits), and from `5e-29` up a tiny value is `0.0000000000000000000000000001`, where 0.17 gave `0`. A zero is an unsigned `0` (0.17 gave `-0` for a tiny negative, and `0.0` for some zeros). A declared `decimal` written with an exponent (a `float8` under `decimal`) follows the same rule: `${x}` loses 0.17's trailing `.0` (`1e2` is `100`, not `100.0`). A script that prints or compares such text sees the new digits; arithmetic is unchanged but for those digits. `flusso script run --input` reads a number with no number type the same way (it read a fraction as a float).
3. **Consumers.** `flusso-query` now turns on `serde_json`'s `float_roundtrip`, always: an `f64` read from `_source`, and a sort value a page cursor carries back, is the value flusso wrote. Cargo features unify, so this applies to the consumer's whole build: every float `serde_json` parses there is correctly rounded, and a float-heavy parse is slower (measured: a float-only array about 30% slower, about 12 ns a float; a typed `_search` decode of 1,000 hits with 14 floats each 7–12% slower; integers and strings unchanged; about 46 KB more code). A consumer reading `_source` with its own `serde_json` in a build without the feature still reads some doubles one unit in the last place off. The cost on flusso's own reader: about 15% on a 512-element `double` array, 3–7% on 64 `double` columns, under 1% on a typical row.
4. **Stored documents.** Each document takes the exact value at its row's next write. Run [`flusso reindex <index>`](../reference/cli.md#reindex) on each index with a `float` or `double` field, an `avg`, or a number with no number type to rewrite every one now, with no restart.

## A filter value is compared as written

**Breaking.** A value filter's `value` was cast to its column's sized type before the comparison, and Postgres cuts or rounds such a cast without an error. 0.18 compares it as written (see [How a value is compared](../reference/filters-and-soft-delete.md#how-a-value-is-compared)), in every place a filter sits: root `filters`, a `soft_delete` `when`, a join, a `through`, a `via` hop, an aggregate. No content hash moves and nothing is rebuilt: the schema is the same, and what it asks for is what 0.18 does.

**Who is affected.** A deployment with a value filter whose `value` doesn't fit its column:

| Column | Example `value` | 0.17 compared | 0.17 effect |
| --- | --- | --- | --- |
| `varchar(n)`, `char(n)`, a domain over one | `archived` on `varchar(5)` | `archi` | `eq`/`in`/`like` kept rows holding `archi`; `neq`/`not_in` dropped them |
| `varchar(n)` | `'archi  '` (trailing spaces past the size) on `varchar(5)` | `archi` | `eq` kept rows holding `archi`; `char(n)` is unaffected, its trailing spaces never count |
| `numeric(p,s)` | `1.005` on `numeric(10,2)` | `1.01` | `eq` kept `1.01`; `gt` dropped it |
| `numeric(p,s)` | more integer digits than `p - s` | — | every build of the index failed (`numeric field overflow`) |
| `bit(n)`, `varbit(n)` | `101` on `bit(5)` | `10100` | `eq` kept `10100` |
| `time(p)`, `timestamp(p)`, `timestamptz(p)`, `interval(p)`, `interval year` | `09:00:00.6` on `time(0)` | `09:00:01` | `eq` kept `09:00:01` |
| a domain with a `CHECK` | a value the check refuses | — | every build of the index failed |

A filter whose values fit its column (the usual case) compares exactly as before.

1. **Find them.** Check each value filter against its column's type in Postgres (`\d <table>` in `psql`). A filter on a column without a size (`text`, `numeric`, `timestamptz`) is unaffected.
2. **Fix the value, if the old match was what was meant.** A filter written as `archived` that relied on matching `archi` now matches nothing; write the value the rows hold.
3. **Rewrite stored documents.** A document picks up the corrected filter at its row's next write. Run [`flusso reindex <index>`](../reference/cli.md#reindex) on each affected index to rewrite every one now, with no restart: a root filter or `soft_delete` `when` that matched by a cut value tombstones those documents, one that dropped a row by it adds them back, and a join or aggregate filter changes the related rows each document holds.

## A date reaches a script as a date

**Breaking, for scripts that treat a date as text.** A `date`, `time` or `timestamp` value reached a script as a string; 0.18 hands over a typed [date, time or timestamp](../author/scripts/dates.md). No content hash moves and nothing is rebuilt. What changes in a script:

Which columns: every `date`, `time` and `timestamp` field, at any depth, in arrays, `min`/`max`/`ids` aggregates of those types, and the values of a `map` with `values: date` or `values: timestamp` (typed for the script when they parse; the document keeps the text). A `json` field stays text.

| 0.17 script | On 0.18 | Write instead |
| --- | --- | --- |
| `this.createdAt.sub_string(0, 4)`, `len()`, `split`, any string method | fails: `Function not found: sub_string (timestamp, …)` | `this.createdAt.year`, or the text first: `this.createdAt.to_string().sub_string(0, 4)` |
| `this.createdAt > "2024-01-01"` | fails: a timestamp and a string can't be compared | `this.createdAt.date() >= date(2024, 1, 1)`, on any `timestamp` field. A full timestamp must be of the same kind: a `timestamptz` column compares with `parse_timestamp("2024-01-01T00:00:00Z")`, a `timestamp` one with `parse_timestamp("2024-01-01T00:00:00")`; the schema doesn't say which, so prefer `.date()` |
| `this.day == "2024-05-01"`, `this.day in ["2024-05-01"]` | fails: a date and a string can't be compared | `this.day == date(2024, 5, 1)`, `this.day == parse_date("2024-05-01")`, `this.day in [date(2024, 5, 1)]` |
| `switch this.day { "2024-05-01" => … }` | **takes the `_` arm silently**: a switch can't match a date | `switch this.day.to_string() { "2024-05-01" => "may day", _ => "another day" }` |
| `m[this.day] = 1` | fails: `expecting string` | `m[this.day.to_string()] = 1` |
| `this.days.sort()`, `max(a, b)` | unchanged for values of one kind; mixing a date with a timestamp, or a zoned with a plain timestamp, fails | convert first: `max(this.createdAt.date(), this.day)`; one kind sorts as before: `this.days.sort()` |
| `[1, this.day].contains(1)`, `[this.day] == [1]`, `#{a: this.day} == #{a: "2024-05-01"}`: a container mixing dates with other kinds, searched or compared | fails: a date and a number can't be compared, even when looking for a non-date | keep one kind in a container, or compare text: `this.days.map(\|d\| d.to_string()).contains("2024-05-01")`, `[1, this.day.to_string()].contains(1)` |
| `this.day` returned to a `keyword` field | fails its type check | `this.day.to_string()`, or `this.day.format("%d/%m/%Y")` |
| a string returned to a `date` or `timestamp` field | fails its type check | `parse_date("2024-05-01")`, `parse_timestamp("2024-05-01T00:00:00")`, or return the date itself |
| `type_of(this.day) == "string"` | `false` | `type_of(this.day) == "date"`, `type_of(this.createdAt) == "timestamp"` (or `"time"`) |
| `` `${this.day}` ``, `"on " + this.day`, `this.day.to_string()`, `` `${[a, b]}` ``, `#{d: this.day}.to_json()` | unchanged: the same text, quoted inside a container | — |
| a `values: date` map's value through a string method | fails | its parts, `this.due.en.year`, or the text first: `this.due.en.to_string().sub_string(0, 4)` |

Every replacement in the last column is read from this page and run by `libs/2-script/tests/dates.rs` (`every_replacement_on_the_upgrade_page_runs`), on a `timestamptz` and on a `timestamp` value.

A comparison with a string, a number or another kind fails rather than answering `false`, so a script that compared dates as text stops its documents (under `on_error = "stop"`) instead of quietly flipping a flag. `switch` is the one place that stays silent: search the scripts for `switch` over a date field.

1. **Find them.** `flusso check --offline` compiles scripts but can't see types. Run each index with a script that reads a date through [`flusso script run`](../author/scripts/debugging.md#flusso-script-run) on a real row (or `--input`, which now types dates by the schema).
2. **Rewrite** them with the table above and the [date functions](../author/scripts/dates.md#functions).
3. **Upgrade.** A script edit changes the content hash, so the edited index is rebuilt into a new generation on the first 0.18 run, as for any script edit.
## A change waits until its commit is visible

**Breaking.** 0.17 could read a change before its transaction was visible to other sessions and index the state before the commit; nothing rebuilt it. On a quiet primary that was rare; behind a synchronous standby it happened to most changes. 0.18 holds a change until its commit is visible (see [Behind a synchronous standby](../reference/source-postgres.md#behind-a-synchronous-standby)). What changes for a running deployment:

- **A commit that never becomes visible stalls indexing** instead of being indexed stale: a synchronous standby that is down holds every later change too. `/status` shows `visibility_wait_seconds`, the `flusso.replication.visibility_wait` gauge rises, and a warning names the waiting sessions after 5 seconds.
- **flusso refuses a server whose `synchronous_standby_names` takes it in**, by name (`flusso`) or by `*` in any form (`*`, `ANY 1 (*)`, `FIRST 2 (a, *)`), and a standby (`pg_is_in_recovery()`). Both started on 0.17. Under the setting, 0.17 confirmed each commit after indexing it stale, so commits waited on flusso but finished: a deployment that ran that way, slowly, **will not start on 0.18**, because a commit would now wait for flusso while flusso waits for the commit to become visible. `*` is refused even where real standbys could fill the quorum (`ANY 1 (*)` with a physical standby attached), since flusso counts among the candidates. The fix depends on the form:
  - a name: take `flusso` out of the setting and reload, or give flusso another `application_name`;
  - `*`: replace it with the names of the synchronous standbys and reload. Renaming flusso doesn't help: `*` matches every name.

  `flusso check` reports the same refusal under *Source server*, so it can be run against the server before the upgrade. For a standby, point the connection URL at the primary.
- **Documents indexed stale by 0.17 stay stale** until their rows change. Behind a synchronous standby, run [`flusso reindex <index>`](../reference/cli.md#reindex) on every index once 0.18 is live.

## A fenced read waits through a restart

`flusso-query`'s fenced reads (`client.after(..)`, `wait`, `wait_applied`) no longer fail on the first refused connection or `503`: they retry until `fence_timeout`, so a deploy of flusso is a slow read, not an error (see [While flusso restarts](../query/read-your-writes.md#while-flusso-restarts)). What changes for a consumer:

- **The default `fence_timeout` is 20 s, was 5 s**, and the fence returns by it. Three cases a caller notices take the whole deadline now:

  | Case | 0.17 | Now |
  | --- | --- | --- |
  | a stalled sink | `FenceTimeout` after 5 s | `FenceTimeout` after 20 s |
  | flusso down | `FenceUnavailable` at once | `FenceUnavailable` after 20 s, retrying |
  | flusso up but never ready | `FenceUnavailable` at once | `FenceUnavailable` after 20 s, retrying |

  A handler that must answer sooner sets `.fence_timeout(..)`; see [Sizing the timeout](../query/read-your-writes.md#sizing-the-timeout).
- **`Error::FenceUnavailable` is a struct now**, `{ cause, reason }`: a pattern `FenceUnavailable(reason)` no longer compiles. It means "unavailable until the timeout", after retrying.
- **A missing `.flusso(url)` is `Error::FenceMisconfigured`**, which was a `FenceUnavailable`. `Error` is `#[non_exhaustive]`, so this is silent: code that matched `FenceUnavailable` to spot the missing address compiles and falls through to its wildcard arm. Match `FenceMisconfigured` for it. A certificate the client refuses and a redirect loop are `FenceMisconfigured` too, at once.
- **Remove a retry loop around fenced reads**, if one was written for deploys: it now multiplies the timeout.

## A stop drains, then exits

0.17 dropped everything on `SIGTERM`, and could then wait forever: a client that went silent mid-request held the HTTP surfaces open and every further signal was ignored, so only `SIGKILL` ended it. A process whose task runner died kept running unnoticed. 0.18 drains instead (see [Stopping](../reference/cli.md#stopping)):

- **A stop lands what is in flight and confirms it**, then exits `0`. The next start reads nothing already indexed again. It takes up to `--shutdown-timeout-secs` (30 s); past it flusso exits `1`, naming the step and the sinks it gave up on.
- **Give the supervisor more than that.** Docker's default 10 s and moon's 2 s kill the drain; Kubernetes' 30 s ties it. The Helm chart now sets `terminationGracePeriodSeconds: 45`. Settings for each are in [Run under a task runner or supervisor](../deploy/compile-and-run.md#run-under-a-task-runner-or-supervisor).
- **Under moon or another task runner, add `--exit-with-parent`** (or `FLUSSO_EXIT_WITH_PARENT=true`). Without it a flusso whose runner died keeps running and holds the slot and port 9464, as in 0.17: it is opt-in, because a `flusso run &` meant to outlive its shell must keep doing so.
- **Press Ctrl-C twice quickly to exit at once** (`130`; `143` for `SIGTERM`): the same sender twice within 500 ms, or three signals within a second. A single Ctrl-C forwarded by a task runner is one stop; any other repeat logs that the stop is under way.
- **`SIGHUP` is a stop**, not a crash: 0.17 died of it without flushing anything. A run started under `nohup` still ignores it on Linux; on macOS `SIGHUP` keeps ending the process at once, as in 0.17.
- **A sink whose cluster keeps refusing documents as busy holds the drain** until the deadline: see [A busy cluster no longer drops documents](#a-busy-cluster-no-longer-drops-documents).
- **`/status` has a `stopping` phase** and a `pid`; `/readyz` is `503` while stopping; a `/wait` `503` now carries `Retry-After` and a `reason` (`starting` or `stopping`).
- **Embedding the crates.** `RunningDaemon::run` no longer returns as soon as its `shutdown` future fires: it drains first, with no deadline of its own (bound it in the caller). `daemon::Phase` gains `Stopping` and `ControlError` gains `Stopping`; both are `#[non_exhaustive]` now. `ChangeCapture` gains `close_live` (default: not supported). `flusso_design::serve` takes the shutdown future as a second argument.

## A busy cluster no longer drops documents

0.17 treated a document OpenSearch refused as busy, HTTP 429 (a full write queue) or 503 (a shard unavailable), like any rejected document: under `on_error = "skip"` it was quarantined and never written until its row changed again; under `"stop"` the sink engine stopped. 0.18 [resends](../reference/sink-opensearch.md#retries) only those documents, with backoff, within `max_retries`; once the retries run out the flush fails and the batch comes back after the sink engine restarts.

- **`flusso_documents_quarantined_total` and `documents_quarantined` no longer count busy refusals.** An alert on them now fires only for documents OpenSearch refused for what they hold.
- **A sustained 429 shows up as a sink restarting**, `last_error` reading `… still refused as busy after <n> retries`, where 0.17 under `skip` showed quarantines. Raise `max_retries` to wait longer inside one flush, or relieve the cluster.
- **A stop during a sustained 429 waits for the deadline and exits `1`**: the batch it holds can't be written, so the drain never finishes (see [A stop drains, then exits](#a-stop-drains-then-exits)). 0.17 under `skip` quarantined the documents and exited `0`. Nothing is lost: the next start redelivers the batch.
- **Documents 0.17 quarantined as busy are still missing.** The quarantine log line (`document rejected by sink; quarantining and continuing`) carries OpenSearch's `reason` only, not the error type. Search those lines for these reasons, then run [`flusso reindex <index>`](../reference/cli.md#reindex) on each index they name once 0.18 is live:

  | Reason contains | Refused because |
  | --- | --- |
  | `rejected execution of` | the write queue was full |
  | `disk usage exceeded flood-stage watermark` | the flood-stage disk block was on |
  | `primary shard is not active` | a primary shard was unavailable |
  | `Data too large` | a circuit breaker tripped (may appear; not seen in testing) |

## Each sink writes several batches at once

0.17 wrote one batch at a time per sink, and OpenSearch sat partly idle while flusso waited for each bulk request. 0.18 keeps up to [`[concurrency] writes`](../reference/config-toml.md#concurrency) bulk requests in flight per sink (default 4, at most 64). On the `reference` benchmark a backfill ran about 1.4× faster at 2 or 4 writes than at one; live latency and burst drain didn't move. What changes:

- **More requests at once reach OpenSearch.** A small cluster that answered 0.17's single request comfortably may now answer some items `429`; flusso retries those, but if the sink keeps restarting with `… still refused as busy`, lower `writes` (`--writes` / `FLUSSO_WRITES`) or `[concurrency] writes`. `writes = 1` writes one batch at a time, as 0.17 did.
- **Order holds per document.** Two batches holding the same document are never written at once, so a document's newer version still lands after its older one. Live changes go before a backfill's pages, except every fourth write slot both could take.
- **A `stop` rejection still halts the sink.** In the run that meets it, batches written beside the rejected one, and batches for other documents that start while its write is still in flight, can land; the slot is not confirmed past the rejected batch, which stays unacknowledged. From then on every restart of the sink writes the rejected batch alone, and nothing behind it lands until it is fixed, as in 0.17: the restart backoff grows and the lane fills. Each start of flusso writes its oldest unconfirmed batch alone. When that is the rejected batch, nothing behind it lands. When it is an older batch already written (another sink is still behind, or flusso stopped before Postgres heard the confirmation), later batches, including ones for other documents beside the rejected one, can land again before the rejected batch fails; nothing is lost, and the slot is never confirmed past it. `writes = 1` stops exactly where 0.17 did.
- **A stdout sink keeps lane order.** It writes one batch at a time in the order they were published, whatever `writes` says, since its readers consume its output in order. A custom `Sink` that emits a stream says so with `fn ordered(&self) -> bool { true }`.
- **`[concurrency]` is never written into `flusso.lock`.** `run` with a `flusso.toml` reads it; `run --locked` takes the flag, the env var, or the default.
- **A burst is searchable as it ends.** A burst's last writes can finish after the write that refreshed, so a sink that goes idle with documents not yet searchable sends one empty caught-up write. On OpenSearch every caught-up write also refreshes the indexes earlier writes left unrefreshed, not only under `refresh_interval = "-1"` as in 0.17, so a fence's `searchable` no longer waits up to `refresh_interval` (default 10 s) after a burst. The cost is at most one `_refresh` request per catch-up, and none when the only index left unrefreshed has one primary shard and the caught-up write's own `?refresh=true` bulk wrote it (the common single-index case). Caught-up refreshes run one at a time.
- **Each sink holds a few more batches.** A sink engine takes up to `writes` + 32 batches from its lane before acknowledging them; they count against the memory budget like any other: see [Memory is bounded in bytes](#memory-is-bounded-in-bytes).
- **Metrics read per write.** `flusso.flush.duration` and `last_flush_micros` time one write; with several in flight their sum exceeds wall time. `flusso.batches` counts writes.

**Embedding the crates.** The `Sink` port changed:

| 0.17 | 0.18 |
| --- | --- |
| `apply(&self, &Envelope)` for each envelope, then `flush(&self, caught_up)` | `write(&self, &[Envelope], caught_up) -> WriteReport`, one call per batch, **safe for concurrent calls**; keep no buffer between calls |
| `FlushReport` | `WriteReport`, the same fields |
| `caught_up`: refresh this flush's documents | `caught_up`: make everything written by the writes that returned before this call searchable too, then report `Immediate` |
| `Visibility::Deferred`: until the next `Immediate` flush | until the next caught-up write that reports `Immediate` |
| `Stream::watermark`: the highest acknowledgement per lane, minimum over lanes | removed: see [below](#confirmation-and-fences-read-the-engines-progress) |
| — | `Sink::ordered()` (default `false`): `true` for a sink whose readers consume its output in order; the engine then writes one batch at a time in lane order |
| `SinkEngine::acknowledged()` | `SinkEngine::advanced()`: how often the oldest unfinished lane item landed, which a supervisor reads as progress |

`SinkEngine::with_writes(n)` sets the writes in flight (default 1 for the engine on its own; the daemon passes `[concurrency] writes`), and `daemon::writes(&config)` resolves the setting.

## A backfill of related indexes is faster and holds more in memory

0.17 ran one query per row and relation while backfilling indexes that read each other's tables, and built the same documents several times. 0.18 builds each document once, with no query per row.

- **Peak memory during a backfill can rise.** Documents are now built faster than a sink takes them, so built batches wait in the sink's lane, up to the memory budget: see [Memory is bounded in bytes](#memory-is-bounded-in-bytes).
- **A root without a single-column primary key gets no document from a backfill.** That was already the rule (see [Postgres source](../reference/source-postgres.md)), but 0.17 could index some of its rows by accident while backfilling a related index.

## A backfill reads key pages, with no long transaction

0.17 read a backfill's root rows through one cursor in a `REPEATABLE READ` transaction on a connection of its own, held open until the whole backfill ended. 0.18 reads each index's root keys in pages of 1,024, in key order, one statement per page on the connections that build documents (see [Postgres source](../reference/source-postgres.md#capture)). A deployment needs nothing.

- **No lock held across the backfill.** The transaction kept an `ACCESS SHARE` lock on every root table it had read until the end, so an `ALTER TABLE` on one waited for the whole backfill, and every query queued behind it waited too. Now it waits for one page at most.
- **Vacuum isn't held back.** The transaction's snapshot kept every dead row newer than it for the length of the backfill. A page's snapshot lasts one statement.
- **One connection fewer** while a backfill runs, and a transaction-mode PgBouncer no longer pins a server connection for its length.
- **Key order, not table order.** Documents are built in the order of their root key. Two indexes rooted at the same table each read it once, where 0.17 read it once for both.
- **A root whose `primary_key` isn't its table's primary key gets nothing from a backfill.** 0.17 read such a root's rows by the table's key and wrote a tombstone for each (the build finds no row by the declared column). 0.18 reads no keys for it. `run` already refuses to start on it (`root_primary_key_mismatch`), so only a deployment started some other way sees the difference.
- **Embedding the crates.** `ChangeCapture::snapshot`, `source::SnapshotTable` and `source::document::IndexScope` are gone. A `DocumentBuilder` implements `backfill_indexes()` and `keys(index, after, limit)`, both required, so a decorator can't silently backfill nothing: forward both to the builder it wraps.

## Several batches build at once

0.17 built one batch of documents at a time, and nothing else moved while it ran. 0.18 builds up to [`[concurrency] builds`](../reference/config-toml.md#concurrency) batches at once (default 4, at most 32), and each sink still receives them in the order they started. On the `reference` benchmark a backfill ran about 15% faster at 2 or 4 builds; the sink's writes then bound it. What changes:

- **More memory during a backfill.** Built batches can now outrun the sink and wait in its lane, up to the memory budget: see [Memory is bounded in bytes](#memory-is-bounded-in-bytes).
- **More Postgres connections.** The document builder keeps up to `builds` + 2 connections (0.17: up to ten, opened as needed). A role with a tight `CONNECTION LIMIT`, or a small PgBouncer pool, needs room for them; `builds = 1` builds one batch at a time, as 0.17 did, on 3.
- **Order holds per document.** Two batches holding the same document are never built at once, and a later build reaches every sink after an earlier one.
- **Changes are read and resolved while a batch builds.** A burst no longer waits for each build before reading the next changes.
- **A failed build publishes nothing after it.** The builds running beside it finish first, then the run ends with the error and restarts as before.
- **Embedding the crates.** `IngestEngine::with_builds(n)` sets the number; it defaults to 1. The `DocumentBuilder`'s `build_many` is called from several tasks at once and must be safe for that (it takes `&self`, so it usually is).

## Memory is bounded in bytes

0.17 bounded the batches waiting for a sink by count: `[stream] capacity`, 1,024 per sink, whatever their size. 0.18 bounds them by the heap they hold: [`[concurrency] max_built_mib`](../reference/config-toml.md#concurrency) (default 512, `--max-built-mib` / `FLUSSO_MAX_BUILT_MIB`). What changes:

- **`[stream] capacity` and `--queue-capacity` are deprecated.** They still load, flusso warns at start, and they do nothing. Remove them when convenient. The Helm chart's `run.queueCapacity` now defaults to unset.
- **A stalled sink paces ingest by bytes.** Once built documents waiting for it reach the budget, the ingest engine builds nothing more until it acknowledges some, as a full queue did. A backfill holds at most half the budget, so live changes keep room.
- **Size memory from the budget.** Plan `max_built_mib` + `builds` batches + about 100 MiB, plus 10–20% for the allocator. The Helm chart now requests 512 MiB of memory and sets no limit; its `values.yaml` gives the formula for one.
- **New in `/status` and `/metrics`:** `memory` (`limit_bytes`, `built_bytes`, `backfill_bytes`), `flusso.memory.built` and `flusso.memory.refused`.
- **Embedding the crates.** `IngestEngine::with_budget(Arc<Budget>)` counts built documents (without it, nothing is counted, as before). `stream::Batch` gains `hold: Hold`; a `Batch` written by hand sets `hold: Default::default()`. `ChannelConfig::capacity` is now `Option<usize>` and is ignored by `ChannelStream::from_config`.

## Indexes are staged at once at start

0.17 created and checked a sink's indexes one after another before following the source. 0.18 does eight at a time and checks four indexes against Postgres at once. With 24 indexes, `live` came a quarter to a third sooner on a fresh cluster, where OpenSearch creates indexes one at a time, and about twice as fast on a restart.

- **A start sends more requests at once**: up to eight per sink to OpenSearch, and four catalog queries to Postgres at once, which takes up to four more Postgres connections while the start validates (measured: `flusso check` on 8 indexes went from 5 connections to 9). A role with a tight `CONNECTION LIMIT`, or a small PgBouncer pool in front of Postgres, needs room for them.
- **A failed start names every index that failed**, each with its own error (``staging failed for index `users_1`: …; index `orders_3`: …``), where 0.17 named none. Once one index fails no further one is started, so a cluster that is down costs one request timeout per attempt, not one per index; the whole stage is retried as before.

## Stage timings in /metrics

0.18 records where the pipeline's time goes: [`flusso.stage.duration`](../reference/metrics.md#where-the-time-goes) per stage, `flusso.flush.documents` and `flusso.flush.size` per flush, and [`flusso.startup.duration`](../reference/metrics.md#start-up) per start-up step. Existing series are unchanged. A deployment needs nothing.

- **Embedding the crates.** `Observer` gains `on_stage` and `on_startup_step` (both default to nothing). `sink::FlushReport` and `engine::CommitStats` are `#[non_exhaustive]` and gain `bytes` (and `CommitStats` `work`): build them with `FlushReport::clean()` and its builders (`visible`, `rejecting`, `sent`) and `CommitStats::new(…).sent(…)`, not a struct literal. A sink that counts the bytes it sends reports them with `sent`; it is a metric only.

## Confirmation and fences read the engine's progress

The Postgres source no longer keeps a map of every change it emitted until every sink confirmed it. Each position carries what the source needs to confirm or fence at it, and the engine keeps, per sink, what it routed there and the sink hasn't landed. A deployment needs nothing; nothing is rebuilt.

- **Memory no longer grows with a stalled sink's backlog in the source.** The map held one entry per emitted change until every sink caught up.
- **Idle WAL is released by the engine's once-a-second confirmation**, not by the keepalive itself: the slot follows unrelated WAL within about two seconds, as before.
- **A fence can answer a little lower while a change is between the source and the lanes.** It never claims a change the sinks haven't landed; at worst a waiter waits until that change lands.
- **An emitting sink's output is unchanged:** the position is still written as `seq`, the same string.

**Embedding the crates.**

| 0.17 | 0.18 |
| --- | --- |
| `kernel::Position(n)`, `position.0` | `Position::new(n)`, `position.seq`; a source stamps its own with `Position::stamped(seq, fence_before, confirm_before, confirm_through)` |
| a `Position` serialized on its own: a bare number (`42`) | a struct, `{"seq":42,"fence_before":…,"confirm_before":…,"confirm_through":…}`, and a bare number no longer reads back. An `Envelope` still writes its position as the `seq` string |
| `ChangeCapture::confirm(&self, Position)` | `confirm(&self, Landed)`: `Before(oldest)`, `Through(newest routed)` or `Idle`. Called after each commit and once a second whatever changed; keep it monotonic |
| `ChangeCapture::reached(&self, Option<Position>)` | `reached(&self, Landed)` |
| `Stream::watermark()` | removed; `engine::Progress` keeps what each lane landed |
| `engine::drain_lanes(stream, source, &mut Option<Position>)` | `drain_lanes(stream, source, &Progress, &mut Option<Landed>)` |
| `Fence::new(source, sinks)` | `Fence::new(source, progress, sinks)` |
| `IngestEngine::new(source, documents, stream, sinks)`, `SinkEngine::new(name, sink, stream, mappings)` | each takes the deployment's `Arc<Progress>` last: the one both engines over a stream share (`Progress::of(indexes, sinks)`, see below) |

## One lane per index and sink

Each `(index, sink)` pair now has its own lane, its own progress and its own fence. A sink is still one engine, writing all its indexes, but one index whose writes are stuck (a refused document under `on_error = "stop"`, a missing index) no longer holds up the sink's other indexes, nor a reader waiting on them. A deployment needs nothing; nothing is rebuilt.

- **`/wait` takes `index`, repeatable.** `index=users&index=orders` waits on those indexes only; without it the wait covers every index, as before. An index that is not enabled is a `404`. The answer echoes `indexes`. See [HTTP endpoints](../reference/http.md#get-waitpositionposition).
- **`flusso-query` waits on the indexes a read touches**, and once per level and index. A root's `get` or search waits on its index, a combined search, cursor walk or `msearch` on each index it names. `Client::wait_on(position, indexes)` is the explicit form. An older flusso ignores `index` and waits on every index: still correct, only slower to pass.
- **`/status` reports each lane** under `sinks.<sink>.lanes.<index>`: `applied_position`, `searchable_position`, `outstanding`. The new gauge [`flusso.lane.outstanding{index,sink}`](../reference/metrics.md) names the lane holding back the slot.
- **The replication slot still waits for every lane**: confirmation is the lowest lane's.
- **Still sink-wide:** a failed write stops the sink's engine, which restarts with backoff; on restart only the lanes that failed write one item at a time until one lands. A reindex is staged between the sink's writes. Parking one index while the rest of the sink goes on is a later change.

**Embedding the crates.**

| 0.17 | 0.18 |
| --- | --- |
| `Stream::lane(&SinkName)` | `Stream::lane(&LaneKey)` (`LaneKey { index, sink }`), plus `Stream::lanes(&SinkName)`: a sink engine's lanes |
| `ChannelStream::new(capacity, sinks)` | `ChannelStream::new(capacity, lanes)` or `ChannelStream::over(capacity, indexes, sinks)` |
| `LaneItem::SnapshotComplete` | `LaneItem::SnapshotComplete { index }` |
| `Request::Backfill { sink, indexes }` | `Request::Backfill { index, sink }`: one request per index |
| `Progress::new(sinks)` | `Progress::new(lanes)` or `Progress::of(indexes, sinks)` |
| `Fence::new(source, progress, sinks)` | `Fence::new(source, progress, lanes)` |
| `FenceRequest { position, level, sink, timeout }` | gains `indexes: Vec<IndexName>`; empty waits on every index. `FenceError::UnknownIndex` is new |
| `Observer::on_position_applied(&SinkName, …)`, `on_position_searchable` | take the `&LaneKey`; `on_changes_settled(sink, changes)` is new (a batch that gave a sink no item) |
| `Backends::stream(config)` | `Backends::stream(config, &[LaneKey])` |
| `MsearchBundle` (`flusso-query`) | gains `indexes()`, defaulting to none (a fence on every index); the tuple bundles name their slots' indexes |

| `Sink::write` per lane item | the engine calls `Sink::write_parts(&[&[Envelope]], caught_up)`: the items ready at once, merged into one write up to `[batch] max_changes` documents and `Sink::write_limit`. Its default writes each part with `write`, so a sink that implements only `write` works unchanged; override it to send the parts in one request |
| `SinkEngine` writes one item per write | `SinkEngine::with_write_documents(n)` bounds a merged write (the daemon passes `max_changes`) |

`Sink::write` is unchanged: every part holds one index's documents, read it off the envelopes when a sink needs it. **Requests per document:** a burst over several indexes goes out in writes of about one batch, as before this release; one write per index would have tripled the `_bulk` requests.

## Related

- [Field types](../reference/field-types.md) for what a `binary` field, a `constant`, a `timestamp` and a [`decimal`](../reference/field-types.md#numbers) index.
- [Reindex without downtime](reindex.md) for rebuilding an index on demand.
- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for hash aliases and generations.
