-- Sample changes to watch storno react in real time.
--
-- With storno running, apply these in another terminal:
--
--   psql "postgres://postgres:postgres@127.0.0.1:5432/storno" -f dev/changes.sql
--
-- Each statement below should produce one document on storno's stdout.

-- 1. New user → upsert for user 4 (no orders yet).
INSERT INTO users (id, email, name) VALUES (4, 'Edsger@Example.com', 'Edsger Dijkstra');

-- 2. New order for an existing user → rebuilds user 4's document with the order.
INSERT INTO orders (id, user_id, total, status) VALUES (13, 4, 99.00, 'pending');

-- 3. Update an order → reverse-resolves to its owner, re-emits user 1.
UPDATE orders SET status = 'delivered' WHERE id = 10;

-- 4. Update the root row → re-emits user 2 with the new email.
UPDATE users SET email = 'grace.hopper@example.com' WHERE id = 2;

-- 5. Soft-delete → user 3 becomes a tombstone (delete op).
UPDATE users SET deleted = true WHERE id = 3;
