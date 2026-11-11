# Test and debug a script

Catch a broken script before it runs, run it over a real row while you write it, and read what each error means.

## Three places a script is checked

| When | Where | Catches |
| --- | --- | --- |
| as you type | the designer's inspector | compile errors, underlined in the editor |
| before a deploy | `flusso check --offline`, `flusso build` | compile errors, every broken script at once |
| on real data | `flusso script run`, the designer's **Sample** tab | runtime errors and wrong types, on an actual row |
| in production | the log, the metric, `/status` | a row nobody thought of; see [Run scripts in production](production.md) |

A compile error stops `check`, `build` and `run` before anything connects:

```text
Error: 1 script failed to compile:

index `products`, field `priceBand`: Undefined variable: price
  --> line 2, column 4
2 | if price < 20 { "budget" } else { "premium" }
  |    ^
```

Compiling can't know what a row holds, so a misspelled property or a value of the wrong type only shows when the script runs. That's what the next two are for.

## flusso script run

[`flusso script run`](../../reference/cli.md#script) builds one document the way `run` does, runs its index's scripts, and prints three things: the document as built, anything the scripts printed, and the finished document (or the failure).

```sh
flusso script run --index products --id 3                 # one row from the database
flusso script run --index products                        # the first row
flusso script run --index products --id 3 --field label   # only the field you're working on
flusso script run --index products --id 3 --watch         # re-run on every save
```

The input shows exactly what `this` and `root` contain, including the input-only fields; read it before guessing at a property name or a type.

**Without a database**, `--input` takes a JSON document. That makes a script testable in CI, or on a laptop with no access to production:

```sh
flusso script run --index products --input fixtures/mug.json --field label
```

The JSON is untyped: whole numbers become integers, other numbers floats, and everything else stays text, so a `decimal` or a `date` arrives as a string. For a script that does decimal arithmetic, test against a real row.

The exit status is non-zero when a script fails, so `script run --input` works as a test step.

## print and debug

`print(x)` and `debug(x)` inside a script show up under **printed** in `script run` and in the designer's **Sample** tab, tagged with the field:

```rust
print(type_of(this.price));
print(this.variants.len());
this.price * 2
```

In a running deployment they go to the log at `debug`, target `script`, tagged with the index, the document id and the field. They're off by default; switch them on with `RUST_LOG=info,script=debug`. Leave a `print` in only while you need it: on a busy index it's one log line per document.

## In the designer

- The inspector compiles the script on every edit (through the live preview) and underlines an error at its line and column, with the message under the editor.
- A blank script is flagged on the canvas until it has a body.
- The **Sample** tab builds a real row, by **Row id** or the first one, runs the scripts, and shows the finished document. When a script fails, it shows the failure with the line and a caret, the row as built, and what the scripts printed.

See [Designer reference](../designer-reference.md).

## Reading the errors

The messages are Rhai's or flusso's own; this is what they mean in a script field.

| Message | Cause | Fix |
| --- | --- | --- |
| `Undefined variable: x` (compile) | a name that isn't `this`, `root`, or a `let` | check the spelling; read fields as `this.x` |
| `Unexpected …` (compile) | a syntax error | the caret marks the spot |
| `Function definitions must be at global level …` (compile) | an `fn` in a script | use a closure, or inline it |
| `reserved keyword 'eval' is disabled` (compile) | `eval` | not available |
| `Property not found: nmae` | the object has no such field | fix the name; for optional keys test `"x" in this` |
| `Function not found: to_upper (())` | a method on a null | guard with `?.` or `??` |
| `Function not found: * (decimal, f64)` | a decimal times a float | use `parse_decimal("1.22")`, or `.to_float()` |
| `Array index 3 out of bounds` | indexing past the end | check `len()` or `is_empty()` first |
| `returned () but the field is required: true` | the script produced null | return a value, or make the field `required: false` |
| `returned a string, expected an integer` | the result's type differs from the field's | convert (`parse_int`, `.to_int()`), or change the field's type |
| `returned "x", which is not a declared variant` | an `enum` result outside `variants` | add the variant, or fix the script |
| `Too many operations` | a loop that doesn't end, or a lot of work | see the [limits](../../reference/scripts.md#the-sandbox) |

A result of `null` where you expected a string is usually an in-place method at the end of a chain, `name.to_lower().replace(" ", "-")`. See [Strings](language.md#strings).

## Related

- [CLI: script](../../reference/cli.md#script).
- [The Rhai you need](language.md).
- [Run scripts in production](production.md).
