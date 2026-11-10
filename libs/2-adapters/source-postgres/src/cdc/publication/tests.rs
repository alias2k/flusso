use kernel::{DatabaseSchema, TableName};

use super::*;

fn table(schema: &str, name: &str) -> QualifiedTable {
    QualifiedTable::new(
        DatabaseSchema::try_new(schema).unwrap(),
        TableName::try_new(name).unwrap(),
    )
}

#[test]
fn create_sql_lists_all_tables_quoted_and_publishes_via_the_root() {
    let missing = vec![table("public", "books"), table("public", "reviews")];
    assert_eq!(
        publication_sql(false, "flusso", &missing, false),
        [
            r#"CREATE PUBLICATION "flusso" FOR TABLE "public"."books", "public"."reviews" WITH (publish_via_partition_root = true);"#
        ]
    );
}

#[test]
fn alter_sql_adds_missing_tables() {
    let missing = vec![table("public", "orders")];
    assert_eq!(
        publication_sql(true, "flusso", &missing, false),
        [r#"ALTER PUBLICATION "flusso" ADD TABLE "public"."orders";"#]
    );
}

#[test]
fn reroute_sets_the_option_on_an_existing_publication() {
    assert_eq!(
        publication_sql(true, "flusso", &[], true),
        [r#"ALTER PUBLICATION "flusso" SET (publish_via_partition_root = true);"#]
    );
    let missing = vec![table("public", "orders")];
    assert_eq!(
        publication_sql(true, "flusso", &missing, true),
        [
            r#"ALTER PUBLICATION "flusso" ADD TABLE "public"."orders";"#,
            r#"ALTER PUBLICATION "flusso" SET (publish_via_partition_root = true);"#,
        ]
    );
}
