-- Deterministic seed for the `complex` scenario. Placeholders the harness
-- substitutes: {users} {orders_per_user} {items_per_order} {tags} {tags_per_user}.
-- Order ids are user * 1000 + n; item ids are order * 100 + k.

INSERT INTO users (id, name, email, status, bio, archived, updated_at)
SELECT g,
       'Customer ' || g,
       '  USER' || g || '@EXAMPLE.IO  ',
       CASE WHEN g % 3 = 0 THEN 'banned' ELSE 'active' END,
       CASE WHEN g % 2 = 0 THEN NULL ELSE 'bio ' || g END,
       false,
       timestamptz '2023-01-01 00:00+00' + (g % 700) * interval '1 hour'
FROM generate_series(1, {users}) g;

INSERT INTO profiles (id, user_id, headline)
SELECT g, g, 'Headline for ' || g FROM generate_series(1, {users}) g;

INSERT INTO orders (id, user_id, total, status, placed_at)
SELECT u * 1000 + n,
       u,
       (n + 1) * 10.50,
       CASE WHEN n % 2 = 0 THEN 'fulfilled' ELSE 'pending' END,
       timestamptz '2021-01-01' + (u * {orders_per_user} + n) * interval '1 hour'
FROM generate_series(1, {users}) u, generate_series(0, {orders_per_user} - 1) n;

INSERT INTO order_items (id, order_id, sku, qty, price)
SELECT (u * 1000 + n) * 100 + k,
       u * 1000 + n,
       'sku-' || k,
       k + 1,
       (k + 1) * 2.25
FROM generate_series(1, {users}) u,
     generate_series(0, {orders_per_user} - 1) n,
     generate_series(0, {items_per_order} - 1) k;

INSERT INTO tags (id, label)
SELECT g, 'tag-' || g FROM generate_series(1, {tags}) g;

INSERT INTO user_tags (user_id, tag_id)
SELECT u, ((u + k) % {tags}) + 1
FROM generate_series(1, {users}) u, generate_series(0, {tags_per_user} - 1) k
ON CONFLICT DO NOTHING;

ANALYZE;
