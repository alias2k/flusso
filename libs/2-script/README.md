# flusso-script

Script fields for [flusso](https://github.com/alias2k/flusso): typed computed fields, written in
[Rhai](https://rhai.rs), run over each document after it is built and before any sink sees it.

```yaml
- keyword: fullName
  required: true
  script: |
    `${this.firstName} ${this.lastName}`.trim()
```

## At a glance

| Piece | Job |
| --- | --- |
| [`Scripts`] | compile every script of a deployment once; run an index's scripts over one document and strip its input-only (`emit: false`) fields |
| [`ScriptedBuilder`] | wrap any `DocumentBuilder` so every document it builds is finished by the scripts; a failure becomes `Document::Failed` |
| [`sandbox`] | the engine: no clock, no imports, no `eval`, strict variables and properties, fixed limits |
| [`value`] | the conversion into Rhai and the strict, typed conversion back |
| [`dates`] | the `date`, `time`, `timestamp` and `duration` types a script works with: parts, arithmetic, rounding, parsing, formatting, time zones (bundled IANA database) |
| [`input`] | typing JSON by the schema, as the builder types a row: `flusso script run --input`, and a `map` of dates for the script's view |

## What a script sees

- `this`: the object at the field's own level (one element when the field sits inside a
  `has_many`).
- `root`: the whole document, as built.
- Nothing else. No database, no clock, no other script's output.

A `date`, `time` or `timestamp` arrives as a typed value (`(this.end - this.start).days`); see
[Dates in a script](https://alias2k.github.io/flusso/author/scripts/dates.html).

The full reference is the
[Script fields reference](https://alias2k.github.io/flusso/reference/scripts.html) of the manual.
