# Reading your own writes

`client.after(&position)` gives a client whose reads wait until a commit is in the index: a fetch by id until it's applied, a search until it's searchable.

## The fence

OpenSearch is fed asynchronously, so a read right after a database commit can miss it. A fence is a wait on flusso for one specific commit, named by the **source position** read after it. The mechanics, and the two levels, are in [Read your own writes](../operate/read-your-writes.md); this chapter is the Rust side.

Reads still go straight to OpenSearch. The client asks flusso one thing only: to hold a request until the position is readable. So it needs both addresses:

```rust
use flusso_query::Client;

let client = Client::connect("https://opensearch:9200")?
    .flusso("http://flusso:9464")?      // flusso's public address, the one serving /status
    .fence_sink("primary");             // the [sinks.<name>] this cluster is fed by
```

`.fence_sink` is optional but worth setting: without it a fence waits for every sink of the deployment, and a stalled second sink holds up reads from a healthy one.

## Reading the position

The position comes from the database, after the commit. `flusso_query::position` has the statement and, behind a feature per driver, a helper that runs it:

```toml
flusso-query = { version = "…", features = ["derive", "diesel-async"] }   # or "diesel"
```

```rust
use flusso_query::position;

save_user(&mut conn, &user).await?;                                  // the write commits
let position = position::diesel_async::current(&mut conn).await?;   // then the position
```

| Driver | Feature | Call |
| --- | --- | --- |
| Diesel | `diesel` | `position::diesel::current(&mut conn)?` |
| diesel-async | `diesel-async` | `position::diesel_async::current(&mut conn).await?` |
| anything else | none | run `position::CURRENT_SQL`, read one `text` column |

Two rules, both easy to break without an error:

- **After the commit, outside the transaction.** Inside it, the position lies before the commit and the fence can pass too early.
- **On the primary.** A replica reports its own replay position.

Concurrent writers need no coordination. Each reads its own position after its own commit; a position covers every commit before it, so a later one only makes the wait a little longer than strictly needed.

## Fenced reads

```rust
let fresh = client.after(&position);

let user = User::get(&fresh, id).await?;                 // waits until applied
let page = User::query()
    .filter(User::team().eq(team))
    .send(&fresh)                                        // waits until searchable
    .await?;
```

`after` returns a `Client`: the same connection pool, plus the position. Every read through it waits first, at the level that read needs.

| Read | Waits until |
| --- | --- |
| `get` | applied |
| `send`, `ids`, `count`, cursors, walks, `msearch`, combined search | searchable |

A fenced client waits **once per level**, not once per request, and its clones share that. Three searches through `fresh` cost one wait. Searchable covers applied, so a `get` after a search costs nothing.

To wait explicitly, for instance before reads made elsewhere:

```rust
client.wait(&position).await?;           // searchable: every read sees the commit
client.wait_applied(&position).await?;   // applied: a get sees it; returns sooner under a backlog
```

## When the wait fails

A fence never falls back to a stale read. The read fails, and OpenSearch isn't asked.

| Error | Means |
| --- | --- |
| `Error::FenceTimeout { reached }` | flusso was waiting on the position when the timeout passed. `reached` is how far the sink got. flusso is behind, or a sink is stalled. |
| `Error::FenceUnavailable(reason)` | flusso couldn't be asked when the timeout passed: unreachable, starting, or stopping. The fence retried the whole time; `reason` is the last cause. |
| `Error::FenceNotConfigured` | No `.flusso(url)` on this client. Fails at once. |
| `Error::FenceRefused { status, reason }` | flusso rejected the wait: a malformed position, or a sink it doesn't have. Never retried. |

The timeout is thirty seconds unless `.fence_timeout(duration)` says otherwise. It's the whole budget of one fence, counted once across every request it makes.

```rust
match User::get(&client.after(&position), id).await {
    Ok(user) => respond(user),
    Err(Error::FenceTimeout { .. }) => respond_accepted(),   // saved; the index will catch up
    Err(error) => return Err(error.into()),
}
```

## While flusso restarts

flusso runs as one process, so a deploy leaves a gap: the old process stops, the new one connects to Postgres and starts following the source. A fence made in that gap keeps asking until its timeout:

- a refused or dropped connection, or a `503`: flusso is unavailable; ask again after a pause (100 ms doubling to 2 s, or the `Retry-After` flusso sent);
- a `408`: flusso's own wait ended before the client's (it caps one at sixty seconds); ask again at once with what's left.

The position is the database's own, so the new process answers the same question the old one was asked. To a caller, a deploy is a slow read, not an error, as long as the timeout covers the gap: the old process's [stop](../reference/cli.md#run), plus the new one's start until `/readyz` answers. A `Retry-After` that reaches past the timeout fails the fence at once. Dropping the read's future stops the retries.

> ℹ️ **Info** — When flusso is caught up, both levels are reached within one flush of the commit. While a backlog drains, `searchable` trails `applied` by up to the sink's `refresh_interval`, because flusso never refreshes on a reader's behalf.

## Related

- [Read your own writes](../operate/read-your-writes.md): the mechanism, and the same thing over plain HTTP.
- [HTTP endpoints](../reference/http.md#get-waitpositionposition): `/wait`.
- [Results and the escape hatch](results-and-escape-hatch.md) for `get`.
