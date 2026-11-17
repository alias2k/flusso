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
3. `cargo check --workspace --all-targets --features flusso-source-postgres/bench,flusso-sink-opensearch/bench` (compiles benches and examples, which clippy and nextest skip; the features unlock the three feature-gated in-process bench targets)
4. `cargo check -p flusso-cli --no-default-features` (the server-only image build)
5. `cargo nextest run --profile ci --workspace --run-ignored all`
6. `cargo test --workspace --doc`
7. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items`

On a PR confined to the query train (`sdk/query`, `sdk/query-derive`, `dev/query-e2e`), step 5 skips every integration-test binary of `source-postgres`, `engine` and `sink-opensearch`, which those paths can't break; everything else, the query train's own e2e included, still runs. Pushes to `main` run everything.

### Docs-only pull requests

A PR whose every changed file is documentation skips what documentation can't break. The required checks still report under their own names, and each skipping job's summary starts with **docs-only: skipped** and names what it left out, so a skip never passes for a full run.

- **What counts as documentation.** `docs/**` (the manual, ADRs, `STYLE.md`), the top-level `*.md` files, the `*.md` files under `plugin/`, `.claude/commands/` and `.claude/skills/`, the issue and PR templates, `apps/cli/ADOPTIONS.md`, and any `README.md` or `CHANGELOG.md` outside a `tests`, `fixtures`, `benches`, `examples`, `src`, `fuzz`, `scenarios`, `frontend` or `dist` directory. Anything else, one file is enough, runs everything.
- **What is diffed.** The whole PR as it would merge: the merge commit against the base. So a docs-only push to a PR that also changes code still runs everything. Diffing against the previous push would be cheaper, but the required checks vouch for the merge result, and a green docs-only push would hide a code change that never got a full run. A rename counts both of its paths.
- **What still runs.** The `manual (build + links)` and `CI scripts (tests)` jobs, always in full. In `clippy + tests`, only the tests that read the changed files: `agent_docs_paths` for a change under `plugin/` or `.claude/`, and for any deleted or renamed file; the generated-tables drift test for `docs/src/reference/generated/`; the upgrade-page snippets for `upgrade-to-0-18.md`; the derive's two manual checks for `document-structs.md` and the query `maps.md` pages. A crate `README.md` compiled into its crate (`#![doc = include_str!("../README.md")]`) also runs that crate's doctests and doc links. Removing such a README runs everything.
- **What is skipped.** fmt, clippy, the `cargo check` steps, the full nextest run with every container suite, the workspace doctests and doc links, the fuzz run, the designer SPA job, the CLI check on Windows and macOS, and the in-process bench A/B.
- **Where the rule lives.** `.github/scripts/change_class.py`: the patterns, and `DOC_TESTS`, which test reads which files. `test_change_class.py` fails when a Rust source reads a documentation file no `DOC_TESTS` entry covers, so a new `include_str!` of a page needs an entry there. Every other event (a push to `main`, `workflow_dispatch`) runs everything, and so does a diff the job can't take.

A separate job runs a 60-second `pgoutput_decode` fuzz on a pinned nightly; another builds and lints the designer SPA; a `docs` job builds the book and link-checks it. A `postgres-matrix` workflow runs the Postgres-backed suites on the other tested majors; see [Other majors](#other-majors). A `bench` workflow gates PRs on the in-process benches and records scenario history on `main`; see [Benchmarks](benchmarks.md). An `audit` workflow checks RustSec advisories weekly and on manifest or lock changes; see [Dependency updates](dependencies.md).

## Why clippy runs without --all-targets

The workspace lints deny `unwrap_used` and `expect_used`, and test targets use `.unwrap()`. Test modules opt out with `#![allow(clippy::unwrap_used)]`. A test crate needing a normally denied dependency uses `#![cfg_attr(test, allow(unused_crate_dependencies))]`.

## The e2e suites

The `#[ignore]`d suites spin up containers with `testcontainers`. They're legitimately slow and occasionally flaky, so `.config/nextest.toml` puts each binary in a test group by what it needs and retries it twice: the `docker` group (no OpenSearch, eight at a time) and the `opensearch` group (anything with OpenSearch, one at a time). Both groups get a longer slow-timeout than ordinary tests, a little longer still under the `ci` profile; `.config/nextest.toml` holds the numbers. A new e2e binary must be added to the filtersets in both profiles.

### Which Postgres

Every test and bench that starts Postgres takes its image from one place, `dev/test-postgres` (`flusso-test-postgres`, unpublished, a dev-dependency only): `postgres:16-alpine`. `FLUSSO_TEST_PG_TAG` names another tag of the official image for the whole run, with no code change:

```sh
FLUSSO_TEST_PG_TAG=14-alpine cargo nextest run -p flusso-source-postgres --run-ignored all
```

Two tests need one version whatever the run's tag, and name it with a `test_postgres::Pinned` variant that carries the reason (each start prints one line saying which image it took instead of the run's, and why): `bytea_output` runs on Postgres 18 (the first with `min`/`max` over `bytea`), and `integration`'s JIT refusal test on Postgres 10 (no `jit` parameter). The scenario harness (`dev/bench`) and the Docker-backed component benches start the same image; the [`review-ab`](benchmarks.md#comparing-a-branch-with-its-base) command starts its own, from `--pg`.

`testcontainers-modules`' `Postgres::default()` alone starts `postgres:11-alpine`, a version flusso doesn't support. `.github/scripts/check-postgres-image.py`, run by CI's `scripts` job through its unit test, fails on any import of `testcontainers_modules::postgres` outside `dev/test-postgres` (nested, glob and renamed imports included; callers name the container type `test_postgres::Container`), on `Postgres::default`, on `with_tag`/`with_name`, and on a `GenericImage` whose arguments mention `postgres` or whose image name isn't a literal. `review-ab.sh` reads its default `--pg` from the same crate's `DEFAULT_TAG`.

| Crate | Binary | Needs | Proves |
| --- | --- | --- | --- |
| `source-postgres` | `integration` | Postgres | capture end to end: slot, stream, decode, backfill |
| `source-postgres` | `config_coverage` | Postgres | every schema key builds against a real catalog; validation reports a typo in every relation and root position |
| `source-postgres` | `publication` | Postgres | publication create/extend and `REPLICA IDENTITY FULL` management, incl. kept identity indexes, unowned tables and lock timeouts |
| `source-postgres` | `introspection` | Postgres | catalog enumeration for the designer |
| `source-postgres` | `tls` | Postgres | a hostssl-only Postgres honors `sslmode` on both the stream and the SQL pool |
| `source-postgres` | `wal_idle` | Postgres | the slot advances from keepalives while watched tables are idle |
| `source-postgres` | `partitions` | Postgres | a partitioned root and child: leaf changes arrive under the parent's name and resolve, a deleted child's pre-image reaches its old parent, backfill reads the partitions |
| `source-postgres` | `continuity` | Postgres | `continuity()` is read-only and `Fresh` exactly while the slot is missing; `prepare()` creates it |
| `source-postgres` | `record_pgoutput` | Postgres | not a test: re-records the decode bench's fixture (see [Benchmarks](benchmarks.md#re-recording-the-pgoutput-fixture)) |
| `engine` | `wal` | Postgres | which op the engine emits per change, against a recording sink, including none for an update of columns no document reads |
| `engine` | `wal_release` | Postgres | the slot releases WAL that produces no document, confirms within 5 s of the last flush, and a stalled sink pins it while a `backfill = false` sink does not |
| `engine` | `backfill` | Postgres | a backfill of indexes that read each other's root tables runs no reverse lookup and writes every document once, equal to its `build` |
| `engine` | `bytea_output` | Postgres + PgBouncer | a `bytea` reads as the same text in backfill and live changes whatever the server's `bytea_output`, through a transaction-mode pooler |
| `engine` | `pipeline` | both | the full source-to-OpenSearch path, read back over HTTP: live ops across key types, soft-delete, backfill, the seed-marker contradictions, reverse resolution of deleted and re-parented children |
| `sink-opensearch` | `reindex` | OpenSearch | generations, alias swap, retracted markers, index-prefix isolation, per-index `index_settings`, and the visibility a flush reports |
| `sink-opensearch` | `map_keys` | OpenSearch | map keys take their `values` type under the strict root and inside a `nested` array; an `indexed: false` subtree is stored but matches no query |
| `sink-opensearch` | `tls` | OpenSearch | over `https://` with an untrusted CA: `tls_verify = true` refuses promptly without retrying, `ca_file` makes it trusted, `false` writes anyway |
| `sink-opensearch` | `document_ids` | OpenSearch | ids OpenSearch refuses before running `_bulk` (over 512 bytes and the like) don't take the rest of the request down |
| `daemon` | `fence` | both | the real daemon: one unpolled read after a fence sees the commit (applied and searchable), across unwatched WAL, split transactions, concurrent writers and a stalled sibling sink |
| `flusso-cli` | `duplicate_names` | both | the live matrix behind the `duplicate_field_name` warning: every pair of field kinds sharing a name, built and written for real |
| `flusso-cli` | `shutdown` | both | the real binary drains on `SIGTERM` under steady writes (everything read is indexed and confirmed, a restart rewrites nothing), gives up at the deadline naming the sink when OpenSearch is paused, answers a fence across a restart, treats a repeat signal within 1 s as the same stop, leaves a half-open request after its bound, stops on `SIGHUP` and with `--exit-with-parent`; a second start names the flusso holding the address and the slot |
| `flusso-cli` | `time_columns` | both | a `time`/`timetz` column under a `timestamp` or `date` field: which cases index and which `validate_indexes` reports |
| `flusso-query-e2e` | `combined_search` | OpenSearch | generation-suffix normalization against a real cluster |
| `flusso-query-e2e` | `cursor_paging` | OpenSearch | page cursors: a live walk follows a reindex, a pinned walk reads one view, a streamed walk closes its point-in-time |
| `flusso-query-e2e` | `map_keys` | OpenSearch | every cross-key map query matches the per-key `or`, adversarial values included; sorting a map by key |
| `flusso-query-e2e` | `tls` | OpenSearch | `flusso-query` refuses an untrusted certificate and reads once given the CA |
| `flusso-query-e2e` | `array_columns` | OpenSearch | an `array: true` field is queried like a plain field of its element type |
| `flusso-query-e2e` | `geo_points` | OpenSearch | `GeoPoint` reads every point form a `text` or `jsonb` column can hold with the coordinates OpenSearch indexed |

The full source-to-sink e2e lives in `engine`, not in a source crate: a leaf source must not dev-depend on the engine or it can't be published first.

### Other majors

flusso supports Postgres 14 or newer. CI tests three majors: the oldest supported, `14-alpine`; the default, `16-alpine`; and the newest, `18-alpine`. They're `TESTED_TAGS` in `dev/test-postgres`, beside the default. Step 5 of the [CI order](#ci-order) runs the default on every PR. The `postgres-matrix` workflow runs each of the others as one leg:

- **What a leg runs.** The test suites of every crate that starts Postgres, with `FLUSSO_TEST_PG_TAG` set: `cargo nextest run --profile ci --workspace --run-ignored all -E 'rdeps(flusso-test-postgres)'`. It uses the same `ci` profile as step 5 and the same setup steps (the composite action `.github/actions/rust-test-setup`). It doesn't run the Docker-backed benches (`postgres`, `pipeline`, `cases`) or `just review-ab`, which takes its image from `--pg`. A leg takes about as long as `clippy + tests`.
- **When.** Nightly and on demand (`workflow_dispatch`). Never on a PR by itself, never a required check. To run it for a PR, dispatch it on the PR's branch. Its checks attach to the branch's head commit (the commit's checks page and the Actions tab), not to the PR's checks list:

  ```sh
  gh workflow run postgres-matrix.yml --ref <branch>
  ```

- **The summary** of each leg lists its failures, the tests that passed on a retry, and the tests that started a [pinned](#which-postgres) image instead of the leg's. The `ci` profile keeps passing output in its JUnit report for the binaries with a pinned test; `test_postgres_matrix.py` fails when a pinned test sits in a binary that isn't listed.
- **A failing nightly** opens or updates one issue, *Postgres matrix fails on a supported major* (label `area: ci-release`), naming each failing version and its tests with a link to the run. The next all-green nightly closes it. `.github/scripts/postgres-matrix.py` maintains it and touches only the newest issue with that title that the workflow's bot opened. A closed issue isn't reopened: the next failure opens a new one. Older bot issues with the same title, and anyone else's, are left alone. A dispatched run reports through its checks only.
- **A failing `plan` job** (it reads the tags) writes no issue; the failed run shows in the Actions tab.

15 and 17 aren't tested: the oldest, the default and the newest catch what differs between majors for less than half the cost of five legs. To test another major, add its tag to `TESTED_TAGS`; the workflow reads its legs from `cargo run -q -p flusso-test-postgres -- matrix`. To run one leg locally, a package at a time:

```sh
FLUSSO_TEST_PG_TAG=18-alpine cargo nextest run --profile ci --run-ignored all -p flusso-engine
git checkout libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin   # record_pgoutput re-records it
```

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
