# Testing

Fast tests run with nextest and no dependencies; the container-backed end-to-end suites are opted in with a flag; a few special-purpose guards cover formats, fuzzing, and the designer.

## The commands

```sh
cargo nextest run --workspace                       # unit + parse/convert, no deps
cargo nextest run --workspace --run-ignored all     # + Postgres/OpenSearch e2e (needs Docker)
cargo nextest run -E 'test(name_substr)'            # one test
cargo test --doc --workspace                        # doctests; nextest doesn't run them
cargo clippy --workspace                            # lint, deliberately not --all-targets
just bench                                          # in-process benches; see Benchmarks
cargo +nightly fuzz run pgoutput_decode             # from libs/2-adapters/source-postgres
```

`default-members = ["apps/cli"]`, so a bare `cargo nextest run` tests only the CLI. Always pass `--workspace`. The `justfile` wraps these: `just test`, `just test-all`, `just doc`, `just lint`, `just ci`.

## CI order

Match this before assuming green:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace`
3. `cargo check --workspace --all-targets --features flusso-source-postgres/bench,flusso-sink-opensearch/bench` (compiles benches and examples, which clippy and nextest skip; the features unlock the two in-process bench targets)
4. `cargo check -p flusso-cli --no-default-features` (the server-only image build)
5. `cargo nextest run --profile ci --workspace --run-ignored all`
6. `cargo test --workspace --doc`
7. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items`

On a PR confined to the query train (`sdk/query`, `sdk/query-derive`, `dev/query-e2e`), step 5 skips every integration-test binary of `source-postgres`, `engine` and `sink-opensearch`, which those paths can't break; everything else, the query train's own e2e included, still runs. Pushes to `main` run everything.

A separate job runs a 60-second `pgoutput_decode` fuzz on a pinned nightly; another builds and lints the designer SPA; a `docs` job builds the book and link-checks it. A `bench` workflow gates PRs on the in-process benches and records scenario history on `main`; see [Benchmarks](benchmarks.md). An `audit` workflow checks RustSec advisories weekly and on manifest or lock changes; see [Dependency updates](dependencies.md).

## Why clippy runs without --all-targets

The workspace lints deny `unwrap_used` and `expect_used`, and test targets use `.unwrap()`. Test modules opt out with `#![allow(clippy::unwrap_used)]`. A test crate needing a normally denied dependency uses `#![cfg_attr(test, allow(unused_crate_dependencies))]`.

## The e2e suites

The `#[ignore]`d suites spin up containers with `testcontainers`. They're legitimately slow and occasionally flaky, so `.config/nextest.toml` puts each binary in a test group by what it needs and retries it twice: the `docker` group (no OpenSearch, eight at a time) and the `opensearch` group (anything with OpenSearch, one at a time). Both groups get a longer slow-timeout than ordinary tests, a little longer still under the `ci` profile; `.config/nextest.toml` holds the numbers. A new e2e binary must be added to the filtersets in both profiles.

| Crate | Binary | Needs | Proves |
| --- | --- | --- | --- |
| `source-postgres` | `integration` | Postgres | capture end to end: slot, stream, decode, backfill |
| `source-postgres` | `config_coverage` | Postgres | every schema key builds against a real catalog; validation reports a typo in every relation and root position |
| `source-postgres` | `publication` | Postgres | publication create/extend and `REPLICA IDENTITY FULL` management, incl. kept identity indexes, unowned tables and lock timeouts |
| `source-postgres` | `introspection` | Postgres | catalog enumeration for the designer |
| `source-postgres` | `tls` | Postgres | a hostssl-only PG 16 honors `sslmode` on both the stream and the SQL pool |
| `source-postgres` | `wal_idle` | Postgres | the slot advances from keepalives while watched tables are idle |
| `source-postgres` | `partitions` | Postgres | a partitioned root and child: leaf changes arrive under the parent's name and resolve, a deleted child's pre-image reaches its old parent, backfill reads the partitions |
| `source-postgres` | `continuity` | Postgres | `continuity()` is read-only and `Fresh` exactly while the slot is missing; `prepare()` creates it |
| `source-postgres` | `record_pgoutput` | Postgres | not a test: re-records the decode bench's fixture (see [Benchmarks](benchmarks.md#re-recording-the-pgoutput-fixture)) |
| `engine` | `wal` | Postgres | which op the engine emits per change, against a recording sink, including none for an update of columns no document reads |
| `engine` | `wal_release` | Postgres | the slot releases WAL that produces no document, confirms within 5 s of the last flush, and a stalled sink pins it while a `backfill = false` sink does not |
| `engine` | `bytea_output` | Postgres + PgBouncer | a `bytea` reads as the same text in backfill and live changes whatever the server's `bytea_output`, through a transaction-mode pooler |
| `engine` | `pipeline` | both | the full source-to-OpenSearch path, read back over HTTP: live ops across key types, soft-delete, backfill, the seed-marker contradictions, reverse resolution of deleted and re-parented children |
| `sink-opensearch` | `reindex` | OpenSearch | generations, alias swap, retracted markers, index-prefix isolation, per-index `index_settings`, and the visibility a flush reports |
| `sink-opensearch` | `map_keys` | OpenSearch | map keys take their `values` type under the strict root and inside a `nested` array; an `indexed: false` subtree is stored but matches no query |
| `sink-opensearch` | `tls` | OpenSearch | over `https://` with an untrusted CA: `tls_verify = true` refuses promptly without retrying, `ca_file` makes it trusted, `false` writes anyway |
| `sink-opensearch` | `document_ids` | OpenSearch | ids OpenSearch refuses before running `_bulk` (over 512 bytes and the like) don't take the rest of the request down |
| `daemon` | `fence` | both | the real daemon: one unpolled read after a fence sees the commit (applied and searchable), across unwatched WAL, split transactions, concurrent writers and a stalled sibling sink |
| `flusso-cli` | `duplicate_names` | both | the live matrix behind the `duplicate_field_name` warning: every pair of field kinds sharing a name, built and written for real |
| `flusso-cli` | `time_columns` | both | a `time`/`timetz` column under a `timestamp` or `date` field: which cases index and which `validate_indexes` reports |
| `flusso-query-e2e` | `combined_search` | OpenSearch | generation-suffix normalization against a real cluster |
| `flusso-query-e2e` | `cursor_paging` | OpenSearch | page cursors: a live walk follows a reindex, a pinned walk reads one view, a streamed walk closes its point-in-time |
| `flusso-query-e2e` | `map_keys` | OpenSearch | every cross-key map query matches the per-key `or`, adversarial values included; sorting a map by key |
| `flusso-query-e2e` | `tls` | OpenSearch | `flusso-query` refuses an untrusted certificate and reads once given the CA |
| `flusso-query-e2e` | `array_columns` | OpenSearch | an `array: true` field is queried like a plain field of its element type |
| `flusso-query-e2e` | `geo_points` | OpenSearch | `GeoPoint` reads every point form a `text` or `jsonb` column can hold with the coordinates OpenSearch indexed |

The full source-to-sink e2e lives in `engine`, not in a source crate: a leaf source must not dev-depend on the engine or it can't be published first.

## Special guards

- **Env-var tests are process-isolated.** The adapters' `config_env` tests mutate the override variables (`SOURCE_POSTGRES_CONNECTION_URL`, `<SINK>_OPENSEARCH_*`); nextest gives each test a process, and each test touches only variables named after its own sink. Under plain `cargo test` the Postgres one is a single sequential test for that reason.
- **Format freeze.** `golden_lock.rs` and `compat.rs`; see [The config layer](config-layer.md#the-freeze).
- **Generated artifacts.** A CLI unit test regenerates the `flusso.toml` editor schema and the adapters' Reference tables and fails when the committed copies differ; `just schema-gen` refreshes them. The derive's own error messages are pinned with trybuild in `libs/0-kernel/derive/tests/ui/`.
- **Fuzzing.** The one target drives the crate-private pgoutput decoder through the `fuzzing` feature. Contract: never panic on arbitrary bytes; `Err` is correct. Needs nightly and `cargo-fuzz`.
- **Agent docs paths.** `apps/cli/tests/agent_docs_paths.rs` asserts every repo path named under `plugin/**` and `.claude/commands/**` exists.
- **Designer.** A proptest round-trip (`apps/design/tests/roundtrip.rs`: random `IndexSchema` through codegen, parse, convert, identity) plus the CI job: Prettier, type-aware ESLint, an i18n completeness check, and a dist-drift guard that the committed SPA matches a fresh build.
- **Trybuild UI tests** in `flusso-query-derive` compare exact compiler diagnostics, which is why `rust-toolchain.toml` pins the channel. Re-bless after a bump with `TRYBUILD=overwrite cargo test -p flusso-query-derive`.

## Where this shows up

- [Releasing](releasing.md) for what a green build feeds.
- [Benchmarks](benchmarks.md) for the performance suite that runs beside these.
