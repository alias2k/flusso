# Dates in a script

A `date`, `time` or `timestamp` value reaches a script as a typed value: read its parts, add to it, round it, move it into a time zone, and return one to a `date` or `timestamp` field.

```yaml
- date: start_date
- date: end_date
- integer: days_active
  script: (this.end_date - this.start_date).days
```

## The types

| `type_of` | Comes from | Holds |
| --- | --- | --- |
| `date` | a `date` column | a calendar day |
| `time` | a `time` column (declared `timestamp`) | a time of day, to the nanosecond |
| `timestamp` | a `timestamp` column (no zone), a `timestamptz` column (in UTC) | a wall-clock date and time, with or without a zone |
| `duration` | `a - b` of two values of one kind | an exact length of time |

A timestamp's **zone** is `UTC` (every `timestamptz`), an IANA name (`Europe/Rome`), or a fixed offset (`+02:00`, from parsing). A timestamp **without** a zone is a wall clock: it has parts but no instant, so it can be placed in a zone (`at_zone`) but not moved into one (`in_zone`).

Dates arrive typed wherever the document holds one: columns, `array: true` columns, joins at any depth, `min`/`max`/`ids` aggregates of a date type. A `map` with `values: date` or `values: timestamp` holds JSON text; its values reach a script typed when they parse as that type, and stay strings when they don't. The document written to the sink keeps the text. A `json` field stays text.

> ℹ️ **Info** — Before 0.18 a date reached a script as a string. See [Upgrade to 0.18](../../operate/upgrade-to-0-18.md#a-date-reaches-a-script-as-a-date).

## Parts

Parts are **properties**. A zoned timestamp's parts are taken in its zone; a plain one's are its wall clock.

| Property | On | Example | Gives |
| --- | --- | --- | --- |
| `year`, `month`, `day` | date, timestamp | `date(2024, 5, 1).month` | `5` |
| `weekday` | date, timestamp | `date(2024, 5, 1).weekday` | `3` (1 Monday … 7 Sunday) |
| `day_of_year` | date, timestamp | `date(2024, 12, 31).day_of_year` | `366` |
| `iso_week`, `iso_year` | date, timestamp | `date(2021, 1, 1).iso_week` | `53` (of `iso_year` 2020) |
| `quarter` | date, timestamp | `date(2024, 5, 1).quarter` | `2` |
| `hour`, `minute`, `second` | time, timestamp | `time(13, 5, 9).minute` | `5` |
| `millisecond`, `nanosecond` | time, timestamp | `parse_time("10:00:00.250").millisecond` | `250` (the sub-second part) |
| `zone` | timestamp | `this.createdAt.zone` | `"UTC"`; `()` without a zone |
| `weeks`, `days`, `hours`, `minutes`, `seconds`, `milliseconds` | duration | `(date(2024, 3, 1) - date(2024, 2, 1)).days` | `29` (the whole length in that unit, toward zero) |

## Functions

| Function | Example | Gives |
| --- | --- | --- |
| `date(y, m, d)` | `date(2024, 2, 29)` | a date |
| `time(h, m, s)` | `time(23, 59, 59)` | a time |
| `timestamp(y, mo, d, h, mi, s)` | `timestamp(2024, 5, 1, 9, 0, 0)` | a timestamp without a zone |
| `parse_date(s)`, `parse_date(s, pattern)` | `parse_date("01/05/2024", "%d/%m/%Y")` | `2024-05-01`; without a pattern, `YYYY-MM-DD` |
| `parse_time(s)`, `parse_time(s, pattern)` | `parse_time("9:30 PM", "%I:%M %p")` | `21:30:00`; without a pattern, `HH:MM[:SS[.f]]` |
| `parse_timestamp(s)`, `parse_timestamp(s, pattern)` | `parse_timestamp("2024-05-01T10:00:00+02:00")` | zoned at `+02:00` (`Z` is `UTC`); with no offset in the text, no zone |
| `add_days(n)`, `add_weeks(n)`, `add_months(n)`, `add_years(n)` | `date(2024, 1, 31).add_months(1)` | `2024-02-29`; on a date or a timestamp |
| `add_hours(n)`, `add_minutes(n)`, `add_seconds(n)` | `time(23, 0, 0).add_hours(2)` | `01:00:00`; on a time or a timestamp |
| `a - b` | `this.end - this.start` | a duration; both of one kind |
| `a.months_until(b)`, `a.years_until(b)` | `date(2000, 6, 1).years_until(date(2024, 5, 31))` | `23`; dates or timestamps |
| `==` `!=` `<` `<=` `>` `>=` | `this.end > this.start` | between two of one kind, or two durations |
| `start_of_week()`, `end_of_week()`, and `month`, `quarter`, `year` | `date(2024, 5, 15).end_of_month()` | `2024-05-31`; on a date or a timestamp |
| `start_of_day()`, `end_of_day()` | `this.createdAt.start_of_day()` | midnight in its zone; on a timestamp |
| `at_time(t)` | `this.day.at_time(time(9, 0, 0))` | a timestamp without a zone; on a date |
| `date()`, `time()` | `this.createdAt.in_zone("Asia/Tokyo").date()` | the day in Tokyo; on a timestamp |
| `in_zone(zone)` | `this.createdAt.in_zone("Europe/Rome")` | the same instant, read in Rome |
| `at_zone(zone)` | `this.localTime.at_zone("Europe/Rome")` | this wall clock, in Rome |
| `without_zone()` | `this.createdAt.in_zone("Europe/Rome").without_zone()` | Rome's wall clock, no zone |
| `format(pattern)` | `this.day.format("%d %b %Y")` | `"01 May 2024"`; on all four |
| `abs()` | `(this.start - this.end).abs()` | on a duration |
| `min(a, b)`, `max(a, b)` | `max(this.shippedOn, this.paidOn)` | the earlier or later; two of one kind |
| `sort()`, `sort_desc()` | `list.sort()` | an array of one kind, in order; a comparator works too: `list.sort(\|a, b\| if a < b { -1 } else if a > b { 1 } else { 0 })` |
| `to_string()`, `` `${…}` ``, `"…" + d` | `` `${this.day}` `` | `"2024-05-01"` |
| `to_json()` on an object | `#{d: this.day}.to_json()` | `{"d":"2024-05-01"}`: each date as its `to_string()` text |

A **pattern** is a [chrono strftime](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) pattern: `%Y-%m-%d`, `%H:%M`, `%A`, `%b`, `%Z` (a zone's abbreviation), `%z` (its offset). A **zone** is an IANA name in any case (`Europe/Rome`, `europe/rome`, `UTC`) or an offset (`+05:30`).

Without a pattern, `parse_timestamp` reads RFC 3339 (`2024-05-01T10:00:00+02:00`, `…Z`), Postgres's text form (`2024-05-01 10:00:00+02`, what a `timestamptz` cast to text or kept in a `json` column looks like), a compact offset (`+0200`), and either form without an offset, which gives a timestamp with no zone.

`to_string` writes the ISO text: `2024-05-01`, `10:15:30.250`, `2024-05-01T10:00:00` (no zone), `2024-05-01T10:00:00Z`, `2024-05-01T12:00:00+02:00`. A duration writes ISO 8601: `P30D`, `PT1H30M`. In an array or an object, and through `debug`, each is quoted, as a string was: `["2024-05-01", "2024-05-02"]`; `debug` also names a zone: `"2024-05-01T12:00:00+02:00[Europe/Rome]"`.

## The rules

- **Month-end.** `add_months` and `add_years` keep the day, clamped to the month's last: 31 Jan + 1 month is 29 Feb (28 in a common year); 29 Feb 2024 + 1 year is 28 Feb 2025.
- **Leap years** follow the Gregorian calendar: 2000 is one, 1900 and 2100 aren't.
- **Step from the anchor.** `add_months` clamps each time, so stepping drifts: 31 Jan stepped one month at a time is 29 Feb, 29 Mar, 29 Apr. Add to the start instead, `start.add_months(i)`, which gives 31 Mar, 30 Apr ([recipe](recipes.md#dates)).
- **Whole months and years** count as Postgres's `age()` does: `a.months_until(b)` is the months between their calendar months, less one while `b`'s day of month (and time of day) hasn't reached `a`'s; negative when `b` is earlier, and `b.months_until(a)` is always its negation. No clamping: 31 Jan to 29 Feb is `0` months, to 1 Mar `1`. A 29 Feb birthday is a year old on 1 March of a common year, not on 28 February: `date(2000, 2, 29).years_until(date(2001, 2, 28))` is `0`. `years_until` is `months_until` divided by 12, toward zero.
- **Differences.** Between dates, whole days. Between two zoned timestamps, the time elapsed, whatever their zones. Between two plain ones, the wall-clock difference. A duration truncates toward zero: 34 hours is `.days == 1`, −34 hours `.days == -1`.
- **Calendar units vs clock units.** On a zoned timestamp, `add_days` to `add_years` and the rounding functions move the wall clock (so a day across a daylight-saving change is 23 or 25 hours); `add_hours`, `add_minutes`, `add_seconds` add elapsed time.
- **Daylight saving.** A wall clock that doesn't exist (02:30 on a spring-forward night) moves forward by the length of the gap, to 03:30. One that happens twice (02:30 on a fall-back night) is the first. The rule holds wherever a wall clock becomes an instant: `at_zone`, the calendar adds, rounding.
- **Across a change, a day may be 23 hours.** Two Rome midnights either side of the spring change are 23 hours apart, so `(b - a).days` is `0`. Count calendar days with `(b.date() - a.date()).days`.
- **Rounding.** `start_of_week` is Monday. On a date, `end_of_*` is the period's last day; on a timestamp, the last nanosecond before the next period starts (`23:59:59.999999999`).
- **`time` wraps** around midnight like a clock, and `b - a` between two times may be negative.
- **Comparison** is between two values of one kind. Two zoned timestamps compare as instants: `this.at == this.at.in_zone("Asia/Tokyo")` is `true`. Any other pair fails the script with how to convert, rather than answering `false`: a date against a timestamp, a zoned against a plain timestamp, a duration against a number (read a unit: `(b - a).days > 30`), any of them against a string, a number, a bool or a container. The one exception is null: `d == ()` and `d != ()` answer, `d < ()` fails. `in`, `contains` and `==` on arrays and objects compare element by element, so `this.day in ["2024-05-01"]` fails too, and so does searching a container that mixes dates with other kinds even for a non-date: `[1, this.day].contains(1)`. Keep one kind in a container, or compare text.
- **`switch` can't match a date.** It matches by value hash, which a date has none of, so every arm is skipped and `_` is taken, silently. Switch on the text, `switch d.to_string() { "2024-05-01" => … }`, or use `if`.
- **A map key is text.** `m[this.day] = 1` fails (`expecting string`); write `m[this.day.to_string()]`.
- **Zones.** `in_zone` needs a zoned timestamp, `at_zone` one without a zone; each fails on the other, naming the one to use. An unknown name fails: `unknown time zone 'Europe/Roma'`. The zone database is the IANA time zone database, built into flusso through jiff's `jiff-tzdb` (currently release 2026e), so every host gives the same answer and none reads its own zoneinfo. A flusso release that carries a newer one says so in its release notes; a value already indexed keeps what the old rules gave until its row changes or the index is [rebuilt](../../reference/cli.md#reindex). Past the database's years (about ±9999) a named zone keeps the offset it has at the edge, so it reaches as far as a fixed offset does.
- **Out of range.** A result past about ±262,000 years (any zone), or an argument too big for its unit, fails the document with `date out of range` (or `timestamp`, or the amount: `… is out of range for this unit`). A constructor with no such day fails with the value: `no such date: 2023-02-29`.
- **Patterns.** An unknown specifier fails (`bad format pattern '%Q'`), and so does asking for a part the value lacks (`%H` on a date, `%z` on a timestamp without a zone). Text that doesn't parse fails with the text, the pattern, and why.
- **Null.** A null date is `()`, and no function takes one: guard with `?.` or `??`, as in `this.endedAt?.year ?? 0`. Subtracting with a null says so: `a date and () can't be subtracted`.
- **Strings.** No function takes a string where a date goes; parse it first.

## Returning a date

| Field | Takes | Written as |
| --- | --- | --- |
| `date` | a `date` | the day |
| `timestamp` | a `timestamp` | without a zone, as it reads; zoned, its UTC instant |
| `timestamp` | a `time` | the time of day, as a `time` column is |
| `timestamp` | a `date` | the day, as a `date` column under a `timestamp` field is; OpenSearch reads it as midnight UTC |

So a value read and returned unchanged is always accepted. Everything else is refused, with what to write instead: a timestamp to a `date` field (`.date()`), a string (`parse_date`, `parse_timestamp`), a date to a text field (`.to_string()` or `.format(…)`). To place a date at midnight in a zone, write it: `d.at_time(time(0, 0, 0)).at_zone("Europe/Rome")`.

## No clock

There's no `now()` or `today()`. A script runs again on every rebuild, retry, reindex and backfill, and must give the same value each time; a field computed from the current time (`age`, `isOverdue`, `daysLeft`) is wrong from the moment it's indexed and stays wrong until the row next changes. Index the dates themselves and compare them to now **in the query**: an OpenSearch `range` with `"gte": "now-30d"`, or a script sort.

## Related

- [Script recipes](recipes.md#dates): tested date scripts.
- [The Rhai you need](language.md).
- [Script fields](../../reference/scripts.md#values-in-and-out): how each value crosses into a script.
