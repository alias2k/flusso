//! Postgres implementation of [`SchemaIntrospection`] over `pg_catalog`.
//!
//! Enumerates every ordinary/partitioned base table in the user schemas (system
//! schemas excluded) with its columns, primary key, and foreign keys, mapping
//! each native column type to a suggested [`FlussoType`]. Runs read-only over
//! the shared admin pool — the same small pool the slot check and lag probes
//! use. Three catalog queries (columns, primary keys, foreign keys), assembled
//! into a deterministic [`RelationalCatalog`].
//!
//! Foreign keys and composite primary keys are read straight from
//! `pg_constraint`/`pg_index` with `unnest(... ) WITH ORDINALITY`, so a
//! composite key's columns stay positionally aligned with the columns they
//! reference — `information_schema.constraint_column_usage` does not guarantee
//! that pairing.

use async_trait::async_trait;
use schema_core::common::ColumnName;
use schema_core::{DatabaseSchema, FlussoType, TableName};
use sources_core::{
    ColumnShape, ForeignKey, RelationalCatalog, Result, SchemaIntrospection, SourceError,
    TableShape,
};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;

use super::WalChangeCapture;

#[async_trait]
impl SchemaIntrospection for WalChangeCapture {
    #[tracing::instrument(name = "wal.introspect", skip_all, err)]
    async fn introspect(&self) -> Result<RelationalCatalog> {
        let pool = self.admin_pool().await?;
        introspect_catalog(pool).await
    }
}

/// A `(schema, table)` key used to group catalog rows as they stream back.
type TableKey = (String, String);

/// Raw FK columns accumulated per constraint before newtype conversion.
struct FkRows {
    columns: Vec<String>,
    ref_schema: String,
    ref_table: String,
    ref_columns: Vec<String>,
}

/// Read the relational catalog from `pool` — the query/assembly the trait
/// method delegates to, kept free of `self` so it depends only on a pool.
async fn introspect_catalog(pool: &PgPool) -> Result<RelationalCatalog> {
    let mut tables: BTreeMap<TableKey, TableShape> = BTreeMap::new();

    for row in sqlx::query(COLUMNS_SQL)
        .fetch_all(pool)
        .await
        .map_err(query_err)?
    {
        let schema: String = get(&row, "schema")?;
        let table: String = get(&row, "table")?;
        let column: String = get(&row, "column")?;
        let sql_type: String = get(&row, "sql_type")?;
        let nullable: bool = get(&row, "nullable")?;

        let shape = entry(&mut tables, &schema, &table)?;
        shape.columns.push(ColumnShape {
            name: column_name(&column)?,
            suggested_type: suggest_type(&sql_type),
            sql_type,
            nullable,
            is_primary_key: false,
        });
    }

    for row in sqlx::query(PRIMARY_KEY_SQL)
        .fetch_all(pool)
        .await
        .map_err(query_err)?
    {
        let schema: String = get(&row, "schema")?;
        let table: String = get(&row, "table")?;
        let column: String = get(&row, "column")?;

        if let Some(shape) = tables.get_mut(&(schema, table)) {
            let name = column_name(&column)?;
            if let Some(col) = shape.columns.iter_mut().find(|c| c.name == name) {
                col.is_primary_key = true;
            }
            shape.primary_key.push(name);
        }
    }

    // Group FK columns by constraint (a `BTreeMap` keeps the per-table FK order
    // deterministic) before converting any name, so `references_*` is set once
    // and the composite columns accumulate in `WITH ORDINALITY` order.
    let mut foreign_keys: BTreeMap<(TableKey, String), FkRows> = BTreeMap::new();
    for row in sqlx::query(FOREIGN_KEY_SQL)
        .fetch_all(pool)
        .await
        .map_err(query_err)?
    {
        let schema: String = get(&row, "schema")?;
        let table: String = get(&row, "table")?;
        let constraint: String = get(&row, "constraint_name")?;

        let fk = foreign_keys
            .entry(((schema, table), constraint))
            .or_insert_with(|| FkRows {
                columns: Vec::new(),
                ref_schema: get(&row, "ref_schema").unwrap_or_default(),
                ref_table: get(&row, "ref_table").unwrap_or_default(),
                ref_columns: Vec::new(),
            });
        fk.columns.push(get(&row, "column")?);
        fk.ref_columns.push(get(&row, "ref_column")?);
    }
    for (((schema, table), _), rows) in foreign_keys {
        if let Some(shape) = tables.get_mut(&(schema, table)) {
            shape.foreign_keys.push(ForeignKey {
                columns: rows
                    .columns
                    .iter()
                    .map(|c| column_name(c))
                    .collect::<Result<_>>()?,
                references_schema: database_schema(&rows.ref_schema)?,
                references_table: table_name(&rows.ref_table)?,
                references_columns: rows
                    .ref_columns
                    .iter()
                    .map(|c| column_name(c))
                    .collect::<Result<_>>()?,
            });
        }
    }

    Ok(RelationalCatalog {
        tables: tables.into_values().collect(),
    })
}

const COLUMNS_SQL: &str = "\
SELECT n.nspname AS schema, c.relname AS \"table\", a.attname AS column, \
       format_type(a.atttypid, a.atttypmod) AS sql_type, \
       NOT a.attnotnull AS nullable \
FROM pg_attribute a \
JOIN pg_class c ON c.oid = a.attrelid \
JOIN pg_namespace n ON n.oid = c.relnamespace \
WHERE c.relkind IN ('r', 'p') \
  AND a.attnum > 0 AND NOT a.attisdropped \
  AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
  AND n.nspname NOT LIKE 'pg\\_%' \
ORDER BY n.nspname, c.relname, a.attnum";

const PRIMARY_KEY_SQL: &str = "\
SELECT n.nspname AS schema, c.relname AS \"table\", att.attname AS column \
FROM pg_index i \
JOIN pg_class c ON c.oid = i.indrelid \
JOIN pg_namespace n ON n.oid = c.relnamespace \
JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord) ON TRUE \
JOIN pg_attribute att ON att.attrelid = c.oid AND att.attnum = k.attnum \
WHERE i.indisprimary \
  AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
ORDER BY n.nspname, c.relname, k.ord";

const FOREIGN_KEY_SQL: &str = "\
SELECT n.nspname AS schema, c.relname AS \"table\", con.conname AS constraint_name, \
       att.attname AS column, fn.nspname AS ref_schema, fc.relname AS ref_table, \
       fatt.attname AS ref_column \
FROM pg_constraint con \
JOIN pg_class c ON c.oid = con.conrelid \
JOIN pg_namespace n ON n.oid = c.relnamespace \
JOIN pg_class fc ON fc.oid = con.confrelid \
JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS k(conkey, confkey, ord) ON TRUE \
JOIN pg_attribute att ON att.attrelid = con.conrelid AND att.attnum = k.conkey \
JOIN pg_attribute fatt ON fatt.attrelid = con.confrelid AND fatt.attnum = k.confkey \
WHERE con.contype = 'f' \
  AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
ORDER BY n.nspname, c.relname, con.conname, k.ord";

fn entry<'a>(
    tables: &'a mut BTreeMap<TableKey, TableShape>,
    schema: &str,
    table: &str,
) -> Result<&'a mut TableShape> {
    let key = (schema.to_owned(), table.to_owned());
    if !tables.contains_key(&key) {
        tables.insert(
            key.clone(),
            TableShape {
                schema: database_schema(schema)?,
                name: table_name(table)?,
                columns: Vec::new(),
                primary_key: Vec::new(),
                foreign_keys: Vec::new(),
            },
        );
    }
    tables
        .get_mut(&key)
        .ok_or_else(|| SourceError::Query("table just inserted is missing".to_owned()))
}

fn get<T>(row: &sqlx::postgres::PgRow, col: &str) -> Result<T>
where
    T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get(col).map_err(query_err)
}

fn query_err(e: sqlx::Error) -> SourceError {
    SourceError::Query(e.to_string())
}

fn column_name(raw: &str) -> Result<ColumnName> {
    ColumnName::try_new(raw).map_err(|e| SourceError::Query(format!("invalid column name: {e}")))
}

fn table_name(raw: &str) -> Result<TableName> {
    TableName::try_new(raw).map_err(|e| SourceError::Query(format!("invalid table name: {e}")))
}

fn database_schema(raw: &str) -> Result<DatabaseSchema> {
    DatabaseSchema::try_new(raw)
        .map_err(|e| SourceError::Query(format!("invalid schema name: {e}")))
}

/// Map a Postgres native type (as `format_type` spells it) to the flusso field
/// type a designer would most likely pick, or `None` when the choice is
/// genuinely the user's (`text`-vs-`keyword`, an enum, an array, an unknown
/// user type). Array types (`[]`) and types flusso has no scalar for return
/// `None` so the designer surfaces the decision rather than guessing wrong.
fn suggest_type(sql_type: &str) -> Option<FlussoType> {
    let lower = sql_type.trim().to_ascii_lowercase();
    if lower.ends_with("[]") {
        return None;
    }
    // Strip a type modifier like `(255)` / `(10,2)` and any schema qualifier.
    let base = lower
        .split('(')
        .next()
        .unwrap_or(&lower)
        .rsplit('.')
        .next()
        .unwrap_or(&lower)
        .trim();
    Some(match base {
        "smallint" | "int2" | "smallserial" => FlussoType::Short,
        "integer" | "int" | "int4" | "serial" => FlussoType::Integer,
        "bigint" | "int8" | "bigserial" => FlussoType::Long,
        "real" | "float4" => FlussoType::Float,
        "double precision" | "float8" => FlussoType::Double,
        "numeric" | "decimal" | "money" => FlussoType::Decimal,
        "boolean" | "bool" => FlussoType::Boolean,
        "uuid" => FlussoType::Uuid,
        "text" => FlussoType::Text,
        "character varying" | "varchar" | "character" | "char" | "bpchar" | "citext" | "name" => {
            FlussoType::Keyword
        }
        "date" => FlussoType::Date,
        "timestamp without time zone"
        | "timestamp with time zone"
        | "timestamp"
        | "timestamptz" => FlussoType::Timestamp,
        "json" | "jsonb" => FlussoType::Json,
        "bytea" => FlussoType::Binary,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_pg_types() {
        assert_eq!(suggest_type("integer"), Some(FlussoType::Integer));
        assert_eq!(
            suggest_type("character varying(255)"),
            Some(FlussoType::Keyword)
        );
        assert_eq!(suggest_type("text"), Some(FlussoType::Text));
        assert_eq!(suggest_type("numeric(10,2)"), Some(FlussoType::Decimal));
        assert_eq!(
            suggest_type("timestamp with time zone"),
            Some(FlussoType::Timestamp)
        );
        assert_eq!(suggest_type("uuid"), Some(FlussoType::Uuid));
        assert_eq!(suggest_type("jsonb"), Some(FlussoType::Json));
    }

    #[test]
    fn leaves_ambiguous_types_unsuggested() {
        assert_eq!(suggest_type("integer[]"), None);
        assert_eq!(suggest_type("time without time zone"), None);
        assert_eq!(suggest_type("my_enum"), None);
    }
}
