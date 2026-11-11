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

## What a script sees

- `this`: the object at the field's own level (one element when the field sits inside a
  `has_many`).
- `root`: the whole document, as built.
- Nothing else. No database, no clock, no other script's output.

The full reference is the
[script fields page](https://alias2k.github.io/flusso/reference/schema/scripts.html) of the manual.
