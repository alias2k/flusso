#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use config::yaml::SchemaYaml;
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use source::SourceSpec;

use super::*;

#[derive(Debug)]
struct Fixed(Vec<Document>);

#[async_trait]
impl DocumentBuilder for Fixed {
    async fn resolve(
        &self,
        _: &TableName,
        _: &RowKey,
        _: Option<&RowImage>,
        _: Option<&RowImage>,
    ) -> Result<Vec<DocumentId>> {
        Ok(Vec::new())
    }

    async fn build(&self, _: &DocumentId) -> Result<Document> {
        Ok(self.0.first().unwrap().clone())
    }

    async fn build_many(&self, _: &[DocumentId]) -> Result<Vec<Document>> {
        Ok(self.0.clone())
    }
}

fn id(key: i64) -> DocumentId {
    DocumentId {
        index: IndexName::try_new("products").unwrap(),
        key: RowKey(vec![(
            kernel::ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(key),
        )]),
    }
}

fn scripts(field_script: &str) -> Scripts {
    let yaml = format!(
        "version: 1\ntable: products\nfields:\n  - long: n\n    required: true\n    script: '{field_script}'\n"
    );
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(&yaml).unwrap()).unwrap();
    let index = IndexName::try_new("products").unwrap();
    Scripts::compile(&SourceSpec::new(BTreeMap::from([(index, schema)]))).unwrap()
}

fn body() -> GenericValue {
    GenericValue::Map(BTreeMap::from([("n".to_owned(), GenericValue::Null)]))
}

#[tokio::test]
async fn upserts_are_finished_and_a_failure_becomes_failed() {
    let inner: Arc<dyn DocumentBuilder> = Arc::new(Fixed(vec![
        Document::Upsert {
            id: id(1),
            body: body(),
        },
        Document::Delete { id: id(2) },
    ]));
    let ok = ScriptedBuilder::wrap(Arc::clone(&inner), scripts("41 + 1"));
    let built = ok.build_many(&[id(1), id(2)]).await.unwrap();
    assert!(matches!(
        &built[..],
        [Document::Upsert { body: GenericValue::Map(m), .. }, Document::Delete { .. }]
            if m.get("n") == Some(&GenericValue::BigInt(42))
    ));

    let failing = ScriptedBuilder::wrap(inner, scripts("\"x\""));
    let built = failing.build_many(&[id(1), id(2)]).await.unwrap();
    assert!(matches!(
        &built[..],
        [Document::Failed { failure, .. }, Document::Delete { .. }] if failure.field == "n"
    ));
}
