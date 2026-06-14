#![no_main]

//! Fuzz the pgoutput message decoder with arbitrary bytes.
//!
//! The decoder reads untrusted binary off the Postgres replication stream; a
//! panic on a malformed message is a denial of service on the whole pipeline.
//! The contract under test is narrow: never panic. Returning a `Decode` error
//! on garbage is the correct, expected outcome.
//!
//! Run with: `cargo +nightly fuzz run pgoutput_decode` from the crate dir.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    sources_postgres::fuzz_pgoutput_decode(data);
});
