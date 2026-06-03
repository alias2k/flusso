-- Replication objects flusso requires.
--
-- The name matches the CLI default (`--publication flusso`). The publication
-- must cover every table any index reads from — here the `users` root and its
-- joined `orders` child.
--
-- The replication SLOT is intentionally not created here. flusso creates it
-- automatically on first connect if it does not exist, so there is nothing for
-- the operator to do.

CREATE PUBLICATION flusso FOR TABLE users, orders;
