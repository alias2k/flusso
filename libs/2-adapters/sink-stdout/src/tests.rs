use super::*;
use std::collections::BTreeMap;

fn index() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn document() -> GenericValue {
    GenericValue::Map(BTreeMap::from([(
        "email".to_owned(),
        GenericValue::String("ada@x.io".to_owned()),
    )]))
}

const TS: &str = "2026-06-03T10:20:30.123Z";

#[test]
fn upsert_is_compact_ndjson_with_provenance_and_meta() {
    let line = StdoutSink::new(false)
        .render(&upsert_envelope(1, TS, &index(), "42", &document()))
        .unwrap();
    // Compact NDJSON: a single line.
    assert!(!line.contains('\n'));

    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["sink"], "stdout");
    assert_eq!(value["version"], VERSION);
    assert_eq!(value["ts"], TS);
    assert_eq!(value["seq"], 1);
    assert_eq!(value["index"], "users");
    assert_eq!(value["op"], "upsert");
    assert_eq!(value["id"], "42");
    assert_eq!(value["document"]["email"], "ada@x.io");
    // `{"email":"ada@x.io"}` is one field, 20 bytes compact.
    assert_eq!(value["meta"]["fields"], 1);
    assert_eq!(value["meta"]["bytes"], 20);
}

#[test]
fn delete_carries_provenance_but_no_document_or_meta() {
    let line = StdoutSink::new(false)
        .render(&delete_envelope(7, TS, &index(), "7"))
        .unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["op"], "delete");
    assert_eq!(value["id"], "7");
    assert_eq!(value["seq"], 7);
    assert_eq!(value["sink"], "stdout");
    assert!(value.get("document").is_none());
    assert!(value.get("meta").is_none());
}

#[test]
fn seq_increments_per_emit_and_is_shared_across_clones() {
    let sink = StdoutSink::new(false);
    assert_eq!(sink.next_seq(), 1);
    let clone = sink.clone();
    // The clone shares the counter, so it continues the sequence.
    assert_eq!(clone.next_seq(), 2);
    assert_eq!(sink.next_seq(), 3);
}

#[test]
fn document_meta_reports_null_fields_for_non_objects() {
    let meta = document_meta(&json!("scalar"));
    assert!(meta["fields"].is_null());
    // `"scalar"` is 8 bytes once serialized (with quotes).
    assert_eq!(meta["bytes"], 8);
}

#[test]
fn pretty_is_multiline() {
    let line = StdoutSink::new(true)
        .render(&delete_envelope(1, TS, &index(), "7"))
        .unwrap();
    assert!(line.contains('\n'));
    assert!(line.contains("\"op\": \"delete\""));
}

#[test]
fn flush_runs_via_an_executor() {
    // Exercises the async `Sink` surface end-to-end (without writing output).
    futures::executor::block_on(StdoutSink::new(false).flush(true)).unwrap();
}
