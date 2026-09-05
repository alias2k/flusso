//! Drives the stdout sink with a sample built document, exercising the real
//! stdout write path.
//!
//! Run with: `cargo run -p flusso-sink-stdout --example demo`

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;

use chrono::Utc;
use kernel::{Envelope, GenericValue, IndexName, Position, SinkName};
use sink::Sink;
use sink_stdout::StdoutSink;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index = IndexName::try_new("users").map_err(|e| e.to_string())?;
    let sink = StdoutSink::new(SinkName::try_new("demo").map_err(|e| e.to_string())?, false);

    let document = GenericValue::Map(BTreeMap::from([
        ("id".to_owned(), GenericValue::Int(42)),
        (
            "email".to_owned(),
            GenericValue::String("ada@example.io".to_owned()),
        ),
        (
            "orders".to_owned(),
            GenericValue::Array(vec![GenericValue::Map(BTreeMap::from([
                ("id".to_owned(), GenericValue::Int(1)),
                ("total".to_owned(), GenericValue::Int(1999)),
            ]))]),
        ),
    ]));

    futures::executor::block_on(async {
        sink.apply(&Envelope::upsert(
            index.clone(),
            "42",
            document,
            Some(Position(1)),
            Utc::now(),
        ))
        .await?;
        sink.apply(&Envelope::delete(index, "7", Some(Position(2)), Utc::now()))
            .await?;
        sink.flush(true).await
    })?;

    Ok(())
}
