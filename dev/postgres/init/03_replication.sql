-- The replication objects flusso requires to already exist.
--
-- These names match the CLI defaults (`--slot flusso`, `--publication flusso`).
-- The publication must cover every table any index reads from — here that is
-- the `users` root and its joined `orders` child.

CREATE PUBLICATION flusso FOR TABLE users, orders;

-- Logical slot using the built-in `pgoutput` plugin. This succeeds only because
-- the server is started with wal_level=logical (see docker-compose.yml). The
-- slot retains WAL from this point on, so changes made before it existed are
-- not replayed — that's expected for a CDC consumer.
SELECT pg_create_logical_replication_slot('flusso', 'pgoutput');
