-- Sample changes to watch flusso react in real time, across all three indexes
-- (users, products, orders).
--
-- With flusso running, apply these in another terminal:
--
--   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
--
-- Each statement triggers one or more document rebuilds. A change to a child
-- row reverse-resolves to every document that embeds it — so a single
-- `order_items` edit re-emits both the `orders` doc and the owning `users` doc.

-- 1. A new user (no profile/orders yet) → one `users` document.
INSERT INTO users (id, email, full_name, tier, country)
VALUES (5, 'Barbara@Example.com', 'Barbara Liskov', 'pro', 'US');

-- 2. Give user 4 a profile → rebuilds user 4 with the profile object filled in.
INSERT INTO profiles (user_id, bio, birth_date)
VALUES (4, 'Originator of the THE multiprogramming system.', '1930-05-11');

-- 3. A new order + its items → a new `orders` doc, and user 5's `users` doc
--    rebuilt with the order and updated rollups (lifetimeValue, orderCount).
INSERT INTO orders (id, user_id, status, total, placed_at)
VALUES (15, 5, 'paid', 129.50, now());
INSERT INTO order_items (id, order_id, product_id, quantity, unit_price)
VALUES (106, 15, 3, 1, 129.50);

-- 4. Bump a line item's quantity → reverse-resolves to order 15 (orders doc)
--    and user 5 (users doc embeds orders → items), and changes unitsSold.
UPDATE order_items SET quantity = 2 WHERE id = 106;

-- 5. Cancel an order → it drops out of the user's `orders` array (filtered) but
--    the `orders` doc itself updates to status = cancelled.
UPDATE orders SET status = 'cancelled' WHERE id = 11;

-- 6. A new review → rebuilds the `products` doc for product 5 (reviewCount,
--    avgRating, min/max change; the review is added to the nested array).
INSERT INTO reviews (id, product_id, user_id, rating, body)
VALUES (1005, 5, 2, 5, 'Rock solid desk.');

-- 7. Tag a product → product 1 gains the `sale` tag in its `tags` array.
INSERT INTO product_tags (product_id, tag_id) VALUES (1, 2);

-- 8. Reprice a product → product 3's `pricing.amount` updates.
UPDATE products SET price = 119.99 WHERE id = 3;

-- 9. Update a root user → re-emits user 2 with the new email.
UPDATE users SET email = 'grace.hopper@example.com' WHERE id = 2;

-- 10. Soft-delete → user 3 becomes a tombstone (delete op) in the users index.
UPDATE users SET deleted = true WHERE id = 3;
