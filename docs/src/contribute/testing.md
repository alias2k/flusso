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

A separate job runs a 60-second `pgoutput_decode` fuzz on a pinned nightly; another builds and lints the designer SPA; a `docs` job builds the book and link-checks it. A `bench` workflow gates PRs on the in-process benches and records scenario history on `main`; see [Benchmarks](benchmarks.md). An `audit` workflow checks RustSec advisories weekly and on manifest or lock changes; see [Dependency updates](dependencies.md).

## Why clippy runs without --all-targets

The workspace lints deny `unwrap_used` and `expect_used`, and test targets use `.unwrap()`. Test modules opt out with `#![allow(clippy::unwrap_used)]`. A test crate needing a normally denied dependency uses `#![cfg_attr(test, allow(unused_crate_dependencies))]`.

## The e2e suites

The `#[ignore]`d suites spin up containers with `testcontainers`. They're legitimately slow and occasionally flaky, so `.config/nextest.toml` caps their concurrency (`docker` group: eight Postgres at once; `opensearch` group: one at a time) and retries them. A new e2e binary must be added to the filtersets in both profiles.

| Crate | Binary | Proves |
| --- | --- | --- |
| `source-postgres` | `integration`, `config_coverage`, `publication`, `introspection` | capture, coverage reports, publication management, catalog enumeration |
| `source-postgres` | `tls` | a hostssl-only PG 16 honors `sslmode` on both the stream and the SQL pool |
| `source-postgres` | `wal_idle` | the slot advances from keepalives while watched tables are idle |
| `source-postgres` | `partitions` | a partitioned root and child: leaf changes arrive under the parent's name and resolve, a deleted child's pre-image reaches its old parent, backfill reads the partitions |
| `source-postgres` | `continuity` | `continuity()` is read-only and `Fresh` exactly while the slot is missing; `prepare()` creates it |
| `source-postgres` | `record_pgoutput` | not a test: re-records the decode bench's fixture from a real Postgres (see [Benchmarks](benchmarks.md#re-recording-the-pgoutput-fixture)) |
| `engine` | `wal` | which op the engine emits per change, against a recording sink, including none for an update of columns no document reads |
| `engine` | `wal_release` | the slot releases WAL that produces no document (unread tables and columns, filtered, soft-deleted and orphaned rows, large and split transactions, idle WAL) and the sinks get exactly the expected ops; it confirms within 5 s of the commit, or of the sink flush for a change that produces a document; a stalled sink pins the slot at the last position it flushed until it catches up, a `backfill = false` sink does not |
| `engine` | `pipeline` | the full source to OpenSearch path, reading the index back over HTTP: live insert/update/delete across key types, soft-delete tombstoning, backfill, the two seed-marker contradictions, and an unread-column update leaving `_version` alone |
| `sink-opensearch` | `reindex` | generations, alias swap, retracted markers, and the visibility a flush reports |
| `sink-opensearch` | `map_keys` | map keys take their `values` type under the strict root and inside a `nested` array; an `indexed: false` subtree is stored but matches no query |
| `sink-opensearch` | `tls` | over `https://` with an untrusted CA: `tls_verify = true` refuses promptly and without retrying, `ca_file` makes it trusted, `false` writes anyway |
| `daemon` | `fence` | the real daemon over Postgres and OpenSearch: after each fence one unpolled read finds the write applied and searchable |
| `flusso-query-e2e` | `combined_search` | generation-suffix normalization against a real cluster |
| `flusso-query-e2e` | `cursor_paging` | page cursors: a live walk follows a reindex (a prefixed union too), a pinned walk reads one view until its generation drops, a streamed walk closes its point-in-time |
| `flusso-query-e2e` | `map_keys` | every cross-key map query matches the per-key `or`, adversarial values included; sorting a map by key |
| `flusso-query-e2e` | `tls` | `flusso-query` refuses an untrusted certificate and reads once given the CA |

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
