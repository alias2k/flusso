# Results and the escape hatch

The three terminals a search can end in, the response shape, and the `raw` hatch for DSL the typed builder doesn't cover.

## Terminals

| Call | Returns | Notes |
| --- | --- | --- |
| `.send(&client)` | `SearchResponse<T>` | a typed page |
| `.ids(&client)` | `Vec<String>` | matching document ids (root primary keys, stringified) in order, with `_source: false`; sort and `from`/`size` apply, `filter_nested` is dropped |
| `.count(&client)` | `u64` | the same query to `_count`; sort, pagination, and projections are ignored |
| `Type::get(&client, id)` | `Option<T>` | one document by id; `id` is typed as the root primary key |

`ids` is the cheap way to search in OpenSearch and load full rows from Postgres.

## The response

```rust
pub struct SearchResponse<T> {
    pub total: u64,             // total matches, not the page size
    pub max_score: Option<f32>,
    pub hits: Vec<Hit<T>>,
    pub took: std::time::Duration,
}

pub struct Hit<T> {
    pub id: String,
    pub score: f32,
    pub source: T,              // your struct
}
```

There is no `serde_json::Value` in the common path.

## The escape hatch

`raw` takes OpenSearch query DSL verbatim and still deserializes into the typed struct:

```rust
let page: SearchResponse<User> = User::query()
    .raw(serde_json::json!({
        "knn": { "embedding": { "vector": [/* … */], "k": 10 } }
    }))
    .send(&client)
    .await?;
```

It's the valve for the few types with no flusso field (`knn`, `geo_shape`, span and parent/child queries, percolators). `function_score`, `script`, `constant_score`, `query_string`, `search_after`, and the rest are in the typed surface; see [Composing queries and options](composing.md).

## Out of scope

- **Aggregations** (facets, histograms, cardinality). They need their own typed result tree; `raw` covers them meanwhile.
- **Writes.** flusso owns the index; the client never upserts or deletes.
- **Correlating hits across indexes.** Both multi-index shapes ship; joining their results is the caller's job.
- **Scroll pagination.** `from`/`size` and `search_after` ship; a scroll cursor is a follow-on.
- **Generating the struct.** By design; the developer owns it.
