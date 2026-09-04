# Write flusso.toml

Describe a deployment in one file: the source, the stream, the sinks, the indexes, and how secrets reach it.

## When to use this

You're standing up flusso against a database and a cluster, or adding a sink or an index to an existing deployment. Every key is defined in [flusso.toml top level](../reference/config-toml.md) and the pages it links; this is the order to write them in.

## Steps

1. **Declare the source with a deferred secret.** Never write a password into the file; reference an environment variable that exists where the pipeline runs.

   ```toml
   [source]
   type = "postgres"
   connection_url = { env = "PG_URL" }
   ssl_mode = "verify-full"
   ```

   `ssl_mode` matters: `require` encrypts but verifies nothing. See [TLS](../reference/source-postgres.md#tls). Every key besides `type` is the adapter's own option; `flusso check` rejects a misspelled one. The `[stream]` table can be left out entirely; see [Stream: channel](../reference/stream-channel.md) for when to size it.

2. **Declare the sink.** Give it a name; that name also names its override variables.

   ```toml
   [sinks.primary]
   type = "opensearch"
   url = { env = "PRIMARY_OPENSEARCH_URL" }
   username = "flusso"
   password = { env = "OS_PASSWORD" }
   ```

   `PRIMARY_OPENSEARCH_URL` is also the override variable for this sink's `url`, so `url` could be omitted entirely. The same rule names the source's: `SOURCE_POSTGRES_CONNECTION_URL`. Rules in [Environment variables](../reference/environment.md#config-values).

3. **List the indexes.** One entry each; paths resolve from this file's directory.

   ```toml
   [[index]]
   name = "users"
   schema = "schemas/users.schema.yml"
   enabled = true
   ```

4. **Decide the rejection policy.** The default `stop` halts on a single bad document. Set `on_error = "skip"` globally or per index only where dropping a document is preferable to stopping. See [on_error](../reference/index-and-on-error.md#on_error).

5. **Bind the HTTP surfaces for the environment.** In a container, the public surface must listen on all interfaces; the private one should stay on localhost.

   ```toml
   [server]
   public_address = "0.0.0.0:9464"
   ```

6. **Validate.**

   ```sh
   flusso check --config flusso.toml --offline   # files only
   flusso check --config flusso.toml             # plus the database
   ```

## Options and variations

- **Share one cluster across environments** with `prefix = "staging_"`. The read side must use the same prefix. See [prefix](../reference/config-toml.md#prefix).
- **Add a stdout sink** during development to see every document as it's written: a `[sinks.audit]` table with `type = "stdout"`. Sinks fan out.
- **Hand-manage the publication** with `manage_publication = false` when the source role can't own the tables. `check` prints the SQL.
- **Editor completion.** `flusso schema config > config.schema.json`, or point a `.taplo.toml` rule at `https://alias2k.github.io/flusso/schemas/latest/config.schema.json`.

## Related

- [Your own Postgres and OpenSearch](own-postgres-opensearch.md) for what the two servers must provide.
- [Compile and run](compile-and-run.md) for turning this file into a running pipeline.
