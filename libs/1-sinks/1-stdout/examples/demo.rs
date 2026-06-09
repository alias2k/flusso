//! Drives the stdout sink with a sample assembled document, exercising the real
//! stdout write path.
//!
//! Run with: `cargo run -p sinks-stdout --example demo`

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;

use schema_core::{GenericValue, IndexName};
use sinks_core::Sink;
use sinks_stdout::StdoutSink;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let index = IndexName::try_new("users").map_err(|e| e.to_string())?;
    let sink = StdoutSink::new(false);

    // A document shaped like what the Postgres builder assembles: a root with a
    // nested one-to-many relation.
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
        sink.upsert(&index, "42", &document).await?;
        sink.delete(&index, "7").await?;
        sink.flush(true).await
    })?;

    Ok(())
}
