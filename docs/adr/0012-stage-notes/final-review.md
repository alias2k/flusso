# Final review: the source, sink, daemon, CLI and docs half (at `cacfccf1`)

**Verdict:** mergeable after one fix (finding 1, a regression against `main`).

**Split:** track C took findings 1, 3, 4 and 5; track A takes finding 2 (resume) and records it in `stage-8b-resume.md`. Tests were written first for 1, 3 and 4 and run red before the fix.

**Verified by the reviewer, nothing to do:**

- the keyset walk (22 key types, partitions);
- the checkpoint tied to its generation;
- the refresh scope;
- the fence states;
- the query client's per-index fence;
- the deprecated `[stream] capacity`;
- the nextest filtersets;
- the alert copies;
- the Reference rows.

## Findings and what was done

| # | Severity | Finding | Fix | Commits | Check |
| --- | --- | --- | --- | --- | --- |
| 1 | medium, regression vs `main` | A first start fails forever when a plain index already has the convenience alias's name: the create request carried the alias `{prefix}{logical}`, and OpenSearch refuses the whole request (`invalid_alias_name_exception`). | A create refused that way is retried with the hash alias alone. The alias step after it moves the hash alias and warns for the convenience alias, as `main` did. `mark_seeded`'s swap already fell back the same way. | `b800f5d1` (test, red), `a76c2032` (fix) | `seeding`'s `a_plain_index_named_like_the_convenience_alias_never_stops_a_first_start`: red with the reviewer's error, green after. It checks `ensure_index`, writes and `mark_seeded` through the hash alias, and that the plain index is left as it was. |
| 2 | (track A) | resume | track A | track A's | track A's |
| 3 | low | A crash between creating a first generation (with its aliases) and writing its meta doc: the next start created generation 2 beside it, and readers saw the empty generation 1 until the seed. | With no meta doc, a generation that already holds the hash alias is reused, unseeded. Any other generation is swept by `mark_seeded`, as before. | `b800f5d1` (test, red), `a76c2032` (fix) | `seeding`'s `a_first_generation_left_without_its_meta_doc_is_reused`: red (`[_1, _2]`), green after. It checks one generation holding both aliases, the meta doc at `(1, false)`, then the seed. |
| 4 | low | After a process restart in the middle of a reindex, the fence answered `503 backfilling`, against the upgrade page's table, and couldn't say why. | `Sink::is_rebuilding(index)` (default `false`). The OpenSearch sink answers whether its hash alias serves another generation than its target. Staging asks it for each unseeded index and announces a rebuilding one before the backfill request, so the lane is `Rebuilding` with nothing vouched. About 40 lines without tests. | `fcfd2e71` | The engine's `a_start_in_the_middle_of_a_rebuild_announces_it` (and a first seed announces nothing); the sink's `the_sink_says_when_reads_see_another_generation_than_it_writes`. Docs: the upgrade table gains the restart row; `http.md`; `CLAUDE.md`. |
| 5 | docs | The upgrade page's crate table had a headerless second half that rendered as text; the opening summary didn't link the new sections; `http.md`'s `/status` example showed `wal_behind_bytes` on a lane with nothing outstanding. | The table is joined into one; the summary links every section (24, checked against the built HTML's ids); the field is dropped from that lane. | `5f314aa7` | `mdbook build docs`, `check-manual-links.sh`, and the built HTML checked |

## Checks before reporting

At `5f314aa7` (rebased on track A's `c6cc82c3`):

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace` | clean |
| `cargo check --workspace --all-targets --features flusso-source-postgres/bench,flusso-sink-opensearch/bench` | clean |
| `cargo nextest run --workspace` | 1853 passed |
| e2e under the lock, `--profile ci --run-ignored all` | 58 passed |

The 58 e2e tests by binary:

| binary | passed |
| --- | --- |
| sink-opensearch `seeding` | 13 |
| sink-opensearch `reindex` | 9 |
| daemon `fence` | 3 |
| cli `fence_restart` | 2 |
| cli `shutdown` | 14 |
| engine `pipeline` | 17 |

**One flaky:** `pipeline`'s `an_untyped_number_lands_where_0_17_s_did` failed its first try on the OpenSearch container not starting within 180 s (`WaitContainer(StartupTimeout)`, the machine loaded by other tracks' runs). It passed on nextest's retry, in 9.8 s. This is not a code failure.

Earlier, on the new tests alone: `seeding` + `reindex` 21/21 after findings 1 and 3, and `seeding` 13/13 after finding 4.
