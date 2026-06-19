use schema_core::{DatabaseSchema, TableName};

use super::*;

fn table(schema: &str, name: &str) -> QualifiedTable {
    QualifiedTable::new(
        DatabaseSchema::try_new(schema).unwrap(),
        TableName::try_new(name).unwrap(),
    )
}

#[test]
fn create_sql_lists_all_tables_quoted() {
    let missing = vec![table("public", "books"), table("public", "reviews")];
    assert_eq!(
        publication_sql(false, "flusso", &missing),
        r#"CREATE PUBLICATION "flusso" FOR TABLE "public"."books", "public"."reviews";"#
    );
}

#[test]
fn alter_sql_adds_missing_tables() {
    let missing = vec![table("public", "orders")];
    assert_eq!(
        publication_sql(true, "flusso", &missing),
        r#"ALTER PUBLICATION "flusso" ADD TABLE "public"."orders";"#
    );
}
