-- Deterministic seed for the `reference` scenario over the dev store schema
-- (dev/postgres/init/01_schema.sql). Every value derives from the series index,
-- no random(), so a scale yields a byte-identical dataset every run.
--
-- Placeholders the harness substitutes: {users} {products} {orders}
-- {items_per_order} {reviews_per_product}.

INSERT INTO categories (id, name, slug)
SELECT g, 'Category ' || g, 'category-' || g FROM generate_series(1, 6) g;

INSERT INTO tags (id, label)
SELECT g, 'tag-' || g FROM generate_series(1, 8) g;

INSERT INTO users (id, email, full_name, tier, country, created_at, updated_at, deleted)
SELECT g,
       'user' || g || '@example.com',
       'Customer ' || g,
       (ARRAY['free', 'pro', 'enterprise'])[1 + g % 3],
       (ARRAY['US', 'GB', 'DE', 'FR', 'NL', 'CA', 'AU', 'JP', 'PL', 'IT'])[1 + g % 10],
       timestamptz '2023-01-01 00:00+00' + (g % 700) * interval '1 hour',
       timestamptz '2023-01-01 00:00+00' + (g % 700) * interval '1 hour',
       false
FROM generate_series(1, {users}) g;

INSERT INTO profiles (user_id, bio, avatar_url, birth_date)
SELECT g,
       'Bio of customer ' || g,
       'https://cdn.example.com/avatars/' || g || '.png',
       date '1960-01-01' + (g % 15000)
FROM generate_series(1, {users}) g
WHERE g % 4 <> 0;

INSERT INTO addresses (id, user_id, kind, line1, city, postal_code, country)
SELECT (g - 1) * 2 + k + 1,
       g,
       CASE WHEN k = 0 THEN 'billing' ELSE 'shipping' END,
       (1 + (g * 7 + k) % 900) || ' Main St',
       (ARRAY['Springfield', 'Riverside', 'Fairview', 'Madison', 'Georgetown', 'Clinton', 'Salem', 'Franklin'])[1 + (g + k) % 8],
       lpad(((g * 53 + k) % 100000)::text, 5, '0'),
       (ARRAY['US', 'GB', 'DE', 'FR', 'NL', 'CA', 'AU', 'JP', 'PL', 'IT'])[1 + g % 10]
FROM generate_series(1, {users}) g
CROSS JOIN generate_series(0, 1) k
WHERE k = 0 OR g % 2 = 0;

INSERT INTO products (id, sku, name, description, title, price, currency, in_stock, category_id, created_at, updated_at)
SELECT g,
       'SKU-' || lpad(g::text, 6, '0'),
       (ARRAY['Widget', 'Gadget', 'Gizmo', 'Doohickey', 'Contraption', 'Apparatus', 'Module', 'Implement', 'Tool', 'Instrument'])[1 + g % 10]
         || ' ' ||
       (ARRAY['Pro', 'Max', 'Lite', 'Plus', 'Mini', 'Ultra', 'Eco', 'Prime'])[1 + g % 8],
       'A catalog item, number ' || g || ', described at some length so the text field has words to analyze.',
       jsonb_build_object('en', 'Product ' || g, 'de', 'Produkt ' || g, 'it', 'Prodotto ' || g),
       round((10 + (g * 7 % 990) + 0.99)::numeric, 2),
       'USD',
       (g % 5 <> 0),
       1 + g % 6,
       timestamptz '2023-01-01 00:00+00' + (g % 400) * interval '1 day',
       timestamptz '2023-01-01 00:00+00' + (g % 400) * interval '1 day'
FROM generate_series(1, {products}) g;

INSERT INTO product_tags (product_id, tag_id)
SELECT g, 1 + (g + t) % 8
FROM generate_series(1, {products}) g
CROSS JOIN LATERAL generate_series(0, g % 3) t
ON CONFLICT DO NOTHING;

INSERT INTO orders (id, user_id, status, total, placed_at, shipped_at, updated_at)
SELECT g,
       1 + g % {users},
       s.status,
       0,
       d.placed_at,
       CASE WHEN s.status IN ('shipped', 'delivered') THEN d.placed_at + interval '2 days' END,
       d.placed_at
FROM generate_series(1, {orders}) g
CROSS JOIN LATERAL (
    SELECT (ARRAY['pending', 'paid', 'shipped', 'delivered', 'cancelled'])[1 + g % 5] AS status
) s
CROSS JOIN LATERAL (
    SELECT timestamptz '2023-01-01 00:00+00' + (g % 500) * interval '1 day' + (g % 24) * interval '1 hour' AS placed_at
) d;

INSERT INTO order_items (id, order_id, product_id, quantity, unit_price)
SELECT (o - 1) * {items_per_order} + k + 1,
       o,
       1 + (o + k) % {products},
       1 + (o + k) % 4,
       round((10 + ((o + k) * 13 % 500) + 0.50)::numeric, 2)
FROM generate_series(1, {orders}) o
CROSS JOIN generate_series(0, {items_per_order} - 1) k;

UPDATE orders o
SET    total = s.t
FROM (SELECT order_id, sum(quantity * unit_price) AS t FROM order_items GROUP BY order_id) s
WHERE  o.id = s.order_id;

INSERT INTO reviews (id, product_id, user_id, rating, body, created_at)
SELECT (p - 1) * {reviews_per_product} + k + 1,
       p,
       CASE WHEN (p + k) % 4 = 0 THEN NULL ELSE 1 + (p + k) % {users} END,
       1 + (p + k) % 5,
       'Review ' || k || ' for product ' || p || '.',
       timestamptz '2023-06-01 00:00+00' + ((p + k) % 200) * interval '1 day'
FROM generate_series(1, {products}) p
CROSS JOIN generate_series(0, {reviews_per_product} - 1) k;

ANALYZE;
