-- Replication objects flusso requires.
--
-- The name matches the CLI default (`--publication flusso`). The publication
-- must cover every table any index reads from — a change to any of them can
-- alter a document, and flusso reverse-resolves a child change back to the
-- document(s) that embed it. With three indexes spanning the whole store, the
-- publication covers every table.
--
-- The replication SLOT is intentionally not created here. flusso creates it
-- automatically on first connect if it does not exist, so there is nothing for
-- the operator to do.

CREATE PUBLICATION flusso FOR TABLE
    users,
    profiles,
    addresses,
    categories,
    products,
    tags,
    product_tags,
    orders,
    order_items,
    reviews;
