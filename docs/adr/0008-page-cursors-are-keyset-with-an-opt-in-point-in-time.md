---
status: accepted
---

# Page cursors are keyset paging, with an opt-in point-in-time

`flusso-query` paged only by offset (`from`/`size`). flusso's index changes continuously from the WAL, so offset pages shift under any insert or delete before the window (duplicates, skips) and stop at `index.max_result_window` (issue #156). A **page cursor** is the alternative: an opaque token returned with a page that resumes the ordered result list right after that page's last hit.

The mechanism: every page searches with `search_after` the previous page's last sort values, with the sort made total. By default the walk is **live**: each page reads the index as it is now. `point_in_time()` opts into a **pinned walk**: the first page opens an OpenSearch point-in-time (PIT) and every page reads it. The token is base64url JSON carrying a version, the last sort values, the PIT id (pinned walks only), and an FNV-1a fingerprint of the query, sort, and prefixed index path; a token handed back with a different query is `Error::CursorMismatch`. It is not signed: tampering only changes the caller's own pages. The token, not the builder, decides the mode on resume.

Verified against OpenSearch 2.19 before deciding: a PIT hides documents written after it opened, keeps reading the old generation after an alias swap, and fails with `index_not_found_exception` once that generation is deleted. OpenSearch has **no `_shard_doc`** and adds no implicit tiebreak, but `_id` and `_index` are sortable and come back in each hit's `sort` array.

## Considered options

- **A PIT for every walk.** The first implementation. Exact (one fixed view per walk), but every first page costs a server context (`search.max_open_pit_context`, 300 per node) that also keeps merged-away segments alive on a CDC-churned index, and a list UI where most visitors stop at page one pays it for nothing. It also freezes a walk's content for as long as the user scrolls. Kept as the opt-in.
- **Live keyset by default (chosen).** No server state, no expiry, fresh content, works on Elasticsearch 7.x too. Inserts and deletes never shift the walk, which is offset paging's real failure on this index. What it gives up: a document whose *sort value* changes mid-walk can repeat or be skipped, `_score` near-ties can swap between pages, and `total` is recounted per page.
- **Expose `sort` on `Hit` + a typed `search_after` only.** The live walk without the token, the tiebreak, or the reindex fix. (`Hit::sort` ships anyway.)
- **Scroll.** Heavyweight, forward-only, discouraged for user-facing paging; a pinned walk covers exports.
- **Reject a non-total sort** instead of fixing it. A runtime error for something the client can fix silently.
- **For a combined search across a reindex:** (a) rewrite each `_index` sort value to the generation its hash alias serves now, before resuming — chosen, one `GET _alias` per resumed page; (b) tiebreak a union on `_id` alone, which permanently skips one of two hits that share an id and every other key; (c) document the repeats.

## Consequences

- **The sort is made total automatically.** A cursor search appends `_id` ascending (`MultiSearch`: `_index` then `_id`, since ids collide across indexes) unless the caller's sort already holds that key; an unsorted search gets `_score` descending first, so it keeps relevance order. flusso's `_id` is the root primary key, unique per index. It relies on `indices.id_field_data.enabled` (default `true` in OpenSearch 2.x).
- **A live walk survives a reindex.** `_id` sort values mean the same thing in every generation. A union's `_index` value names the concrete generation (`users_x_1`), so a live resume first reads which generation each hash alias serves and rewrites it; without that a `match_all` walk would repeat a whole index.
- **A pinned walk can end early.** Its PIT expires after the keep-alive (renewed by every page; default one minute), and a reindex that drops the old generation removes it. Both surface as `Error::CursorExpired`: restart from page one. The last page closes the PIT; `PageCursor::close` closes it early.
- **Pinned walks are OpenSearch ≥ 2.4 only** (Elasticsearch's PIT API differs; #157). Live walks and offset paging work on both.
- `inner_hits` takes no cursor (OpenSearch supports only `from`/`size` there); in a pinned walk inner hits read the same PIT.
