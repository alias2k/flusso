//! Live-service e2e for `flusso-query`, kept out of the published crate.
//!
//! The one test here (`tests/combined_search.rs`) drives the **real
//! OpenSearch sink** and queries back through `flusso-query`, guarding the
//! sink↔query naming contract end to end. It lives in this unpublished
//! crate — not in `apps/query/tests/` — so the published `flusso-query` has
//! **zero** flusso lib dependencies (not even dev-deps): the query release
//! train can never be dragged into a release by a libs version bump.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all
//! ```

// The dev-deps exist for `tests/combined_search.rs`; the (empty) lib's own
// `--test` build links them and would flag every one as unused.
#![cfg_attr(test, allow(unused_crate_dependencies))]
