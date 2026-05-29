//! The `pg_sync_rs` sync engine.
//!
//! Drives the flow from a Postgres source to the configured sinks: initial
//! backfill, change capture, and assembling each document from its index
//! schema. Not yet implemented.
