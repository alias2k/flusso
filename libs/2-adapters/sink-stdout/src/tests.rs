use super::*;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use kernel::{ENVELOPE_VERSION, IndexName, Position, SinkName};
use serde_json::Value;

fn index() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn audit() -> StdoutSink {
    StdoutSink::new(SinkName::try_new("audit").unwrap(), false)
}

fn document() -> GenericValue {
    GenericValue::Map(BTreeMap::from([(
        "email".to_owned(),
        GenericValue::String("ada@x.io".to_owned()),
    )]))
}

const TS: &str = "2026-06-03T10:20:30.123Z";

fn ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(TS)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn upsert_is_compact_ndjson_with_provenance_and_meta() {
    let envelope = Envelope::upsert(index(), "42", document(), Some(Position(1)), ts());
    let sink = audit();
    let line = sink
        .render(&sink.wire_envelope(&envelope).unwrap())
        .unwrap();
    assert!(!line.contains('\n'));

    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["sink"], "audit", "the sink stamps its own name");
    assert_eq!(value["version"], ENVELOPE_VERSION);
    assert_eq!(value["ts"], TS);
    assert_eq!(value["seq"], "1");
    assert_eq!(value["index"], "users");
    assert_eq!(value["op"], "upsert");
    assert_eq!(value["id"], "42");
    assert_eq!(value["document"]["email"], "ada@x.io");
    // `{"email":"ada@x.io"}` is one field, 20 bytes compact.
    assert_eq!(value["meta"]["fields"], 1);
    assert_eq!(value["meta"]["bytes"], 20);

    let back: Envelope<Value> = serde_json::from_str(&line).unwrap();
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(sink.wire_envelope(&envelope).unwrap()).unwrap(),
        "a consumer reads the same type back"
    );
}

#[test]
fn delete_carries_provenance_but_no_document_or_meta() {
    let envelope = Envelope::delete(index(), "7", Some(Position(7)), ts());
    let sink = audit();
    let line = sink
        .render(&sink.wire_envelope(&envelope).unwrap())
        .unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["op"], "delete");
    assert_eq!(value["id"], "7");
    assert_eq!(value["seq"], "7");
    assert_eq!(value["sink"], "audit");
    assert!(value.get("document").is_none());
    assert!(value.get("meta").is_none());
}

#[test]
fn snapshot_rows_carry_no_seq() {
    let envelope = Envelope::upsert(index(), "1", document(), None, ts());
    let value = serde_json::to_value(audit().wire_envelope(&envelope).unwrap()).unwrap();
    assert!(value.get("seq").is_none());
}

#[test]
fn document_meta_reports_null_fields_for_non_objects() {
    let meta = document_meta(&GenericValue::String("scalar".to_owned())).unwrap();
    assert_eq!(meta.fields, None);
    // `"scalar"` is 8 bytes once serialized (with quotes).
    assert_eq!(meta.bytes, 8);
}

/// A decimal past `Decimal`'s range is written as a JSON number with every
/// digit, compact and pretty (issue #327).
#[test]
fn a_big_decimal_is_written_as_a_number_with_every_digit() {
    let big = kernel::BigDecimal::parse("1000000000000000000000000000000.5").unwrap();
    let document = GenericValue::Map(BTreeMap::from([(
        "amount".to_owned(),
        GenericValue::BigDecimal(big),
    )]));
    let envelope = Envelope::upsert(index(), "1", document, None, ts());
    for pretty in [false, true] {
        let sink = StdoutSink::new(SinkName::try_new("audit").unwrap(), pretty);
        let line = sink
            .render(&sink.wire_envelope(&envelope).unwrap())
            .unwrap();
        assert!(
            line.contains("1000000000000000000000000000000.5")
                && !line.contains("\"1000000000000000000000000000000.5\""),
            "{line}"
        );
    }
    let meta = document_meta(envelope.document.as_ref().unwrap()).unwrap();
    assert_eq!(
        meta.bytes,
        r#"{"amount":1000000000000000000000000000000.5}"#.len()
    );
}

#[test]
fn pretty_is_multiline() {
    let envelope = Envelope::delete(index(), "7", Some(Position(1)), ts());
    let sink = StdoutSink::new(SinkName::try_new("audit").unwrap(), true);
    let line = sink
        .render(&sink.wire_envelope(&envelope).unwrap())
        .unwrap();
    assert!(line.contains('\n'));
    assert!(line.contains("\"op\": \"delete\""));
}

#[test]
fn a_write_runs_via_an_executor() {
    futures::executor::block_on(audit().write(&[], true)).unwrap();
}

/// Writes run concurrently, and each lands as one run of lines: two writes'
/// lines never interleave, so a reader can split the output per write.
#[test]
fn concurrent_writes_never_interleave_their_lines() {
    const WRITES: usize = 8;
    const LINES: usize = 200;
    let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = StdoutSink {
        out: Output::Buffer(std::sync::Arc::clone(&buffer)),
        ..audit()
    };
    std::thread::scope(|scope| {
        for writer in 0..WRITES {
            let sink = &sink;
            scope.spawn(move || {
                let envelopes: Vec<Envelope> = (0..LINES)
                    .map(|line| {
                        Envelope::upsert(
                            index(),
                            format!("{writer}-{line}"),
                            document(),
                            None,
                            ts(),
                        )
                    })
                    .collect();
                futures::executor::block_on(sink.write(&envelopes, false)).unwrap();
            });
        }
    });

    let text = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    let writers: Vec<String> = text
        .lines()
        .map(|line| {
            let value: Value = serde_json::from_str(line).unwrap();
            let id = value["id"].as_str().unwrap().to_owned();
            id.split('-').next().unwrap().to_owned()
        })
        .collect();
    assert_eq!(writers.len(), WRITES * LINES);
    let runs = 1 + writers.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert_eq!(runs, WRITES, "each write's lines are one run");
}
