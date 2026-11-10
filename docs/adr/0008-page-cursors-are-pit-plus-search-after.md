---
status: accepted
---

# Page cursors are a point-in-time plus `search_after`

`flusso-query` paged only by offset (`from`/`size`). flusso's index changes continuously from the WAL, and a reindex swaps the alias to a new generation, so offset pages drift, duplicate, skip, and stop at `index.max_result_window` (issue #156). A **page cursor** is the alternative: an opaque token returned with a page that resumes the same ordered result list right after that page's last hit, reading one consistent view.

The mechanism: the first page opens an OpenSearch **point-in-time** (PIT) over the index (or the union's indexes), and every page searches that PIT with `search_after` the previous page's last sort values. The token is base64url JSON carrying a version, the PIT id, the last sort values, and an FNV-1a fingerprint of the query, sort, and prefixed index path, so a token handed back with a different query is rejected (`Error::CursorMismatch`) rather than paging nonsense. It is not signed: tampering only changes the caller's own pages.

Verified against OpenSearch 2.19 before deciding: a PIT doesn't see documents written after it opened, keeps reading the old generation after the alias swap, and fails with `index_not_found_exception` once that generation is deleted. OpenSearch has **no `_shard_doc`** (sorting on it is "No mapping found") and adds no implicit tiebreak, but `_id` and `_index` are sortable and come back in each hit's `sort` array.

## Considered options

- **Expose `sort` on `Hit` + a typed `search_after` only.** Smallest, but pages a live view: drifts under writes and mixes generations across a reindex. (`Hit::sort` ships anyway, for callers who want it.)
- **Scroll.** Consistent, but heavyweight, forward-only, discouraged for user-facing paging; a PIT walk already covers the export case.
- **PIT + `search_after` (chosen).** Consistent across writes and the generation swap; the cost is a server-side resource with a keep-alive, a version floor, and a close call.
- **Reject a non-total sort** instead of fixing it. A runtime error for something the client can fix silently.
- **Support Elasticsearch's PIT too.** Its API differs (`/_pit`, `id`, `DELETE /_pit`) and needs a backend switch on the `Client`; left as a follow-up.

## Consequences

- **OpenSearch ≥ 2.4 only** for cursors. Offset paging keeps working on OpenSearch and Elasticsearch 7.x alike.
- **The sort is made total automatically.** A cursor search appends `_id` ascending (`MultiSearch`: `_index` then `_id`, since ids collide across indexes) unless the caller's sort already holds that key; an unsorted search gets `_score` descending first, so it keeps relevance order. flusso's `_id` is the root primary key, so it is unique per index. It relies on `indices.id_field_data.enabled` (default `true` in OpenSearch 2.x).
- **A walk can end early.** The PIT expires after its keep-alive (renewed by every page; default one minute), and a reindex that drops the old generation removes it. Both surface as `Error::CursorExpired`: restart from page one. A walk never mixes generations.
- A cursor walk holds one PIT context; the cluster caps them (`search.max_open_pit_context`, 300 per node). The last page closes the PIT; `PageCursor::close` closes it early.
- `inner_hits` takes no cursor (OpenSearch supports only `from`/`size` there); inside a cursor page they read the same snapshot, so element pages are consistent with their parent.
