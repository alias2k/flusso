# Compute a field with a script

Derive a field from the rest of the document with a short Rhai script, and keep the columns it needs out of the index.

## When to use this

A field a search needs is a function of other values: a display label, an `onSale` flag, a price with tax, a score from counters, the cheapest variant. Without scripts that means a generated column or a view in Postgres for each one.

A script reads only the document flusso already built, so it can't reach a table the schema doesn't fold in. Fold the data in first ([joins](../reference/joins.md), [aggregates](../reference/aggregates.md)), then compute over it. A single `lowercase`/`trim` needs no script: use [transforms](../reference/field-types.md#transforms).

## Steps

1. **Declare the field with its type and a script.** The type key is the result's type; the mapping and the query side read it as for any field.

   ```yaml
   - keyword: label
     required: true
     script: |
       `${this.sku} — ${this.name}`
   ```

   `this` is the object at the field's level; `root` is the whole document. The full rules are in [Script fields](../reference/scripts.md).

2. **Fetch what the script needs without indexing it.** Mark a field `emit: false` and it's built, readable by scripts, then stripped:

   ```yaml
   - decimal: rawPrice
     column: price
     required: true
     emit: false
   - enum: priceBand
     variants: [budget, mid, premium]
     required: true
     script: |
       let price = this.rawPrice;
       if price < 20 { "budget" } else if price < 100 { "mid" } else { "premium" }
   ```

3. **Compute per element inside a join.** A script under a `has_many` runs once per child, with `this` set to that child:

   ```yaml
   - has_many: reviews
     table: reviews
     foreign_key: product_id
     primary_key: id
     fields:
       - short: rating
         required: true
       - boolean: positive
         required: true
         script: this.rating >= 4
   ```

4. **Check it compiles.** `flusso check --offline` compiles every script and lists the broken ones with line, column, and a caret. The designer's preview shows the same errors under the editor as you type.

5. **Run it on real rows.** `flusso script run --index products --id 3` builds that row and prints the input, anything the script printed, and the finished document; `--field priceBand` shows one field, `--watch` re-runs on every save. Use `--input doc.json` to test without a database. `print(...)` inside a script lands in that output.

6. **Decide what a failing document does.** A script that fails on a document fails that document. With the default `on_error = "stop"` the run stops until the script or the data changes; with `skip` the document keeps its indexed version. See [Script fields](../reference/scripts.md#failures).

7. **Alert on failures under `skip`.** Each one is logged at `error` with the script line, counted, and shown in `/status` under `script_failures`:

   ```promql
   increase(flusso_script_failures_total[10m]) > 0
   ```

## Options and variations

- **An array result.** `array: true` makes the script return an array of the field's type: `- keyword: tags` + `array: true`.
- **A fallback.** A missing value is `()`; write `this.nickname ?? this.name`. Reading a property the object doesn't have is an error, so test optional keys with `"key" in this`.
- **Decimals stay exact.** A `decimal` arrives as a Rhai `Decimal`; an integer result is accepted for a `decimal`, `double`, or `float` field.
- **Editing a script rebuilds the index** into a new generation, as any schema change does.

## Related

- [Script fields](../reference/scripts.md): every key, the bindings, the value mapping, the sandbox.
- [CLI: script](../reference/cli.md#script).
- [Handle rejected documents](../operate/rejected-documents.md): the same `on_error` policy, applied at the sink.
