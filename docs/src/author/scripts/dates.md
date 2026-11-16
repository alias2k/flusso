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
| `to_string()`, `` `${…}` ``, `"…" + d` | `` `${this.day}` `` | `"2024-05-01"` |

A **pattern** is a [chrono strftime](https://docs.rs/chrono/latest/chrono/format/strftime/index.html) pattern: `%Y-%m-%d`, `%H:%M`, `%A`, `%b`, `%Z` (a zone's abbreviation), `%z` (its offset). A **zone** is an IANA name (`Europe/Rome`, `UTC`) or an offset (`+05:30`).

`to_string` writes the ISO text: `2024-05-01`, `10:15:30.250`, `2024-05-01T10:00:00` (no zone), `2024-05-01T10:00:00Z`, `2024-05-01T12:00:00+02:00`. A duration writes ISO 8601: `P30D`, `PT1H30M`. `debug(ts)` adds a named zone: `2024-05-01T12:00:00+02:00[Europe/Rome]`.

## The rules

- **Month-end.** `add_months` and `add_years` keep the day, clamped to the month's last: 31 Jan + 1 month is 29 Feb (28 in a common year); 29 Feb 2024 + 1 year is 28 Feb 2025.
- **Leap years** follow the Gregorian calendar: 2000 is one, 1900 and 2100 aren't.
- **Whole months and years.** `a.months_until(b)` is the most months, toward zero, that `a.add_months(n)` can move without passing `b`; negative when `b` is earlier. So `a.months_until(a.add_months(n))` is always `n`. `years_until` is that divided by 12. A timestamp counts its time of day too.
- **Differences.** Between dates, whole days. Between two zoned timestamps, the time elapsed, whatever their zones. Between two plain ones, the wall-clock difference. A duration truncates toward zero: 34 hours is `.days == 1`, −34 hours `.days == -1`.
- **Calendar units vs clock units.** On a zoned timestamp, `add_days` to `add_years` and the rounding functions move the wall clock (so a day across a daylight-saving change is 23 or 25 hours); `add_hours`, `add_minutes`, `add_seconds` add elapsed time.
- **Daylight saving.** A wall clock that doesn't exist (02:30 on a spring-forward night) moves forward by the length of the gap, to 03:30. One that happens twice (02:30 on a fall-back night) is the first. The rule holds wherever a wall clock becomes an instant: `at_zone`, the calendar adds, rounding.
- **Across a change, a day may be 23 hours.** Two Rome midnights either side of the spring change are 23 hours apart, so `(b - a).days` is `0`. Count calendar days with `(b.date() - a.date()).days`.
- **Rounding.** `start_of_week` is Monday. On a date, `end_of_*` is the period's last day; on a timestamp, the last nanosecond before the next period starts (`23:59:59.999999999`).
- **`time` wraps** around midnight like a clock, and `b - a` between two times may be negative.
- **Comparison** is between two values of one kind. Two zoned timestamps compare as instants: `this.at == this.at.in_zone("Asia/Tokyo")` is `true`. A date against a timestamp, a zoned against a plain timestamp, or a date against a string fails the script with how to convert, rather than answering `false`.
- **Zones.** `in_zone` needs a zoned timestamp, `at_zone` one without a zone; each fails on the other, naming the one to use. An unknown name fails: `unknown time zone 'Europe/Roma'`. The zone database is built into flusso, so every host gives the same answer; a flusso release can carry a newer one.
- **Out of range.** A result past about ±262,000 years, or an argument too big for its unit, fails the document with `date out of range`. A constructor with no such day fails with the value: `no such date: 2023-02-29`.
- **Patterns.** An unknown specifier fails (`bad format pattern '%Q'`), and so does asking for a part the value lacks (`%H` on a date, `%z` on a timestamp without a zone). Text that doesn't parse fails with the text, the pattern, and why.
- **Null.** A null date is `()`, and no function takes one: guard with `?.` or `??`, as in `this.endedAt?.year ?? 0`.
- **Strings.** No function takes a string where a date goes; parse it first.

## Returning a date

| Field | Takes | Written as |
| --- | --- | --- |
| `date` | a `date` | the day |
| `timestamp` | a `timestamp` | without a zone, as it reads; zoned, its UTC instant |
| `timestamp` | a `time` | the time of day, as a `time` column is |

Everything else is refused, with what to write instead: a timestamp to a `date` field (`.date()`), a date to a `timestamp` field (`.at_time(time(0, 0, 0))`, since midnight in which zone is the script's choice), a string (`parse_date`, `parse_timestamp`), a date to a text field (`.to_string()` or `.format(…)`).

## No clock

There's no `now()` or `today()`. A script runs again on every rebuild, retry, reindex and backfill, and must give the same value each time; a field computed from the current time (`age`, `isOverdue`, `daysLeft`) is wrong from the moment it's indexed and stays wrong until the row next changes. Index the dates themselves and compare them to now **in the query**: an OpenSearch `range` with `"gte": "now-30d"`, or a script sort.

## Related

- [Script recipes](recipes.md#dates): tested date scripts.
- [The Rhai you need](language.md).
- [Script fields](../../reference/scripts.md#values-in-and-out): how each value crosses into a script.
