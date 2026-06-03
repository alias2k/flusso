-- Schema for the storno dev environment.
--
-- A `users` root table with a one-to-many `orders` child. `users.deleted` is a
-- soft-delete flag — flipping it to true turns the document into a tombstone.

CREATE TABLE users (
    id      int PRIMARY KEY,
    email   text NOT NULL,
    name    text,
    deleted boolean NOT NULL DEFAULT false
);

CREATE TABLE orders (
    id      int PRIMARY KEY,
    user_id int NOT NULL REFERENCES users (id),
    total   numeric(10, 2) NOT NULL,
    status  text NOT NULL DEFAULT 'pending'
);

CREATE INDEX orders_user_id_idx ON orders (user_id);

-- REPLICA IDENTITY defaults to the primary key, which both tables have. That is
-- enough for storno to recover row keys on UPDATE/DELETE and reverse-resolve
-- order changes back to the owning user document.
