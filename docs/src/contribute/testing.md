# Testing

Fast tests run with nextest and no dependencies; the container-backed end-to-end suites are opted in with a flag; a few special-purpose guards cover formats, fuzzing, and the designer.

## The commands

```sh
cargo nextest run --workspace                       # unit + parse/convert, no deps
cargo nextest run --workspace --run-ignored all     # + Postgres/OpenSearch e2e (needs Docker)
cargo nextest run -E 'test(name_substr)'            # one test
cargo test --doc --workspace                        # doctests; nextest doesn't run them
cargo clippy --workspace                            # lint, deliberately not --all-targets
cargo bench                                         # Criterion: engine, opensearch, postgres
cargo +nightly fuzz run pgoutput_decode             # from libs/1-sources/1-postgres
```

`default-members = ["apps/cli"]`, so a bare `cargo nextest run` tests only the CLI. Always pass `--workspace`. The `justfile` wraps these: `just test`, `just test-all`, `just doc`, `just lint`, `just ci`.

## CI order

Match this before assuming green:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace`
3. `cargo check --workspace --all-targets` (compiles benches and examples, which clippy and nextest skip)
4. `cargo check -p flusso-cli --no-default-features` (the server-only image build)
5. `cargo nextest run --profile ci --workspace --run-ignored all`
6. `cargo test --workspace --doc`
7. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items`

A separate job runs a 60-second `pgoutput_decode` fuzz on a pinned nightly; another builds and lints the designer SPA; a `docs` job builds the book and link-checks it.

## Why clippy runs without --all-targets

The workspace lints deny `unwrap_used` and `expect_used`, and test targets use `.unwrap()`. Test modules opt out with `#![allow(clippy::unwrap_used)]`. A test crate needing a normally denied dependency uses `#![cfg_attr(test, allow(unused_crate_dependencies))]`.

## The e2e suites

The `#[ignore]`d suites spin up containers with `testcontainers`. They're legitimately slow and occasionally flaky, so `.config/nextest.toml` caps their concurrency (`docker` group: eight Postgres at once; `opensearch` group: one at a time) and retries them. A new e2e binary must be added to the filtersets in both profiles.

| Crate | Binary | Proves |
| --- | --- | --- |
| `sources-postgres` | `integration`, `config_coverage`, `publication`, `introspection` | capture, coverage reports, publication management, catalog enumeration |
| `sources-postgres` | `tls` | a hostssl-only PG 16 honors `sslmode` on both the stream and the SQL pool |
| `sources-postgres` | `wal_idle` | the slot advances from keepalives while watched tables are idle |
| `sources-postgres` | `continuity` | `continuity()` is read-only and `Fresh` exactly while the slot is missing; `prepare()` creates it |
| `engine` | `wal` | which op the engine emits per change, against a recording sink |
| `engine` | `pipeline` | the full source to OpenSearch path, reading the index back over HTTP: live insert/update/delete across key types, soft-delete tombstoning, backfill, and the two seed-marker contradictions |
| `sinks-opensearch` | `reindex` | generations, alias swap, retracted markers |
| `flusso-query-e2e` | `combined_search` | generation-suffix normalization against a real cluster |

The full source-to-sink e2e lives in `engine`, not in a source crate: a leaf source must not dev-depend on the engine or it can't be published first.

## Special guards

- **Env-var tests are process-isolated.** The `schema` crate's config tests mutate `DATABASE_URL` and `<SINK>_OPENSEARCH_URL`; nextest gives each test a process. Under plain `cargo test` use `--test-threads=1`.
- **Format freeze.** `golden_lock.rs` and `compat.rs`; see [The config layer](config-layer.md#the-freeze).
- **Fuzzing.** The one target drives the crate-private pgoutput decoder through the `fuzzing` feature. Contract: never panic on arbitrary bytes; `Err` is correct. Needs nightly and `cargo-fuzz`.
- **Agent docs paths.** `apps/cli/tests/agent_docs_paths.rs` asserts every repo path named under `plugin/**` and `.claude/commands/**` exists.
- **Designer.** A proptest round-trip (`apps/design/tests/roundtrip.rs`: random `IndexSchema` through codegen, parse, convert, identity) plus the CI job: Prettier, type-aware ESLint, an i18n completeness check, and a dist-drift guard that the committed SPA matches a fresh build.
- **Trybuild UI tests** in `flusso-query-derive` compare exact compiler diagnostics, which is why `rust-toolchain.toml` pins the channel. Re-bless after a bump with `TRYBUILD=overwrite cargo test -p flusso-query-derive`.

## Where this shows up

- [Releasing](releasing.md) for what a green build feeds.
