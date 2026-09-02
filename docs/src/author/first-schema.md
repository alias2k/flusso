# Your first schema

Write a `*.schema.yml` for one table, wire it into `flusso.toml`, and validate it, first without a database, then against one.

## When to use this

You have a table whose rows should become search documents and you're writing the schema by hand. The visual designer does the same job with a database-aware UI; see [Design a schema visually](design-visually.md).

## Steps

1. **Name the root.** Create `users.schema.yml` next to `flusso.toml`. Every schema starts with the format version, the table, and its primary key.

   ```yaml
   version: 1
   table: users
   primary_key: id
   fields: []
   ```

   `primary_key` is what the document id derives from, and what every join later hangs off. `schema` defaults to `public`.

2. **Add fields, type-first.** Each entry is `- <type>: <name>` plus siblings. Pick the type by how the field will be *queried*, not by the column type: `keyword` for exact match and sort, `text` for full-text prose, `identifier` for codes and names that should be findable by fragment. The full table is in [Field types](../reference/field-types.md).

   ```yaml
   fields:
     - integer: id
       required: false
     - keyword: email
       required: true
     - text: bio
       required: false
     - timestamp: createdAt
       column: created_at
       required: true
   ```

   `required: true` maps the field non-null; the default is nullable. `column` renames: the document key is `createdAt`, the source column `created_at`.

3. **Wire it in.** Add an `[[index]]` entry to `flusso.toml`. The `schema` path resolves from the config file's directory.

   ```toml
   [[index]]
   name = "users"
   schema = "users.schema.yml"
   enabled = true
   ```

4. **Validate the files.** No database needed.

   ```sh
   flusso check --config flusso.toml --offline
   ```

   The output prints the source, the sinks, and the fully typed OpenSearch mapping flusso would create. A grammar mistake (two type keys, a sibling the type doesn't take, an unknown key) fails here with the field named.

5. **Validate against the database.** Drop `--offline` and `check` confirms each declared type against the column's SQL type and each `required` against its `NOT NULL`, then reports publication coverage.

   ```sh
   flusso check --config flusso.toml
   ```

   A disagreement (a `required: true` over a nullable column, an `integer` over a `bigint`) is reported per field. Fix the schema, or fix the column.

## Options and variations

- **Normalize a value** with `transforms: [trim, lowercase]` before it lands. See [transforms](../reference/field-types.md#transforms).
- **Coalesce a null** with `default:`. A required field over a nullable column needs one.
- **Tune the mapping** with `options:`, merged beside the derived type: `analyzer`, `format`, `scaling_factor`. Your key overrides the sink's default for that field.
- **A type the set doesn't cover** is a `custom` field naming the Postgres and OpenSearch types. See [custom](../reference/field-types.md#custom).
- **Group same-row columns** under one key with `object`. See [Objects and maps](../reference/objects-and-maps.md).
- **Editor completion.** Put `# yaml-language-server: $schema=https://alias2k.github.io/flusso/schemas/latest/index.schema.yml` at the top of the file.

## Related

- [Fold in related tables](related-tables.md) for joins and rollups.
- [Filter rows and soft-delete](filters-and-soft-delete.md) for subsets and tombstones.
- [Schema top-level keys](../reference/schema-top-level.md) for every key and sibling.
