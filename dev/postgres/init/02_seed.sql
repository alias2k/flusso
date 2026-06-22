-- Initial fixtures. flusso is a CDC consumer: it streams *changes*, not a
-- snapshot of pre-existing rows. This data exists so the build SQL produces
-- interesting documents once a change touches a row (and so the initial
-- backfill has something to seed). Use dev/changes.sql to emit live changes.

-- People ---------------------------------------------------------------------

INSERT INTO users (id, email, full_name, tier, country, created_at) VALUES
    (1, 'ada@example.com',   'Ada Lovelace',    'pro',        'GB', '2023-01-04 09:00+00'),
    (2, 'grace@example.com', 'Grace Hopper',    'enterprise', 'US', '2023-02-11 14:30+00'),
    (3, 'alan@example.com',  'Alan Turing',     'free',       'GB', '2023-03-22 08:15+00'),
    (4, 'edsger@example.com','Edsger Dijkstra', 'pro',        'NL', '2023-05-01 11:45+00');

-- Profiles for some users (user 4 intentionally has none → null one-to-one).
INSERT INTO profiles (user_id, bio, avatar_url, birth_date) VALUES
    (1, 'Mathematician and the first programmer.',  'https://cdn.example.com/a/ada.png',   '1815-12-10'),
    (2, 'Pioneer of machine-independent languages.','https://cdn.example.com/a/grace.png', '1906-12-09'),
    (3, 'Father of theoretical computer science.',  NULL,                                  '1912-06-23');

INSERT INTO addresses (id, user_id, kind, line1, city, postal_code, country) VALUES
    (1, 1, 'billing',  '1 Analytical Way', 'London',   'EC1A 1BB', 'GB'),
    (2, 1, 'shipping', '1 Analytical Way', 'London',   'EC1A 1BB', 'GB'),
    (3, 2, 'billing',  '42 Compiler Ave',  'Arlington','22203',    'US'),
    (4, 4, 'shipping', '7 Structured St',  'Nuenen',   '5671',     'NL');

-- Catalog --------------------------------------------------------------------

INSERT INTO categories (id, name, slug) VALUES
    (1, 'Books',       'books'),
    (2, 'Electronics', 'electronics'),
    (3, 'Home',        'home');

INSERT INTO products (id, sku, name, description, price, currency, in_stock, category_id, created_at) VALUES
    (1, 'BK-0001', 'The Art of Computer Programming', 'The classic multi-volume reference on algorithms.', 199.99, 'USD', true,  1, '2023-01-01 00:00+00'),
    (2, 'BK-0002', 'Structure and Interpretation',    'A foundational text on programming.',               54.00,  'USD', true,  1, '2023-01-02 00:00+00'),
    (3, 'EL-1001', 'Mechanical Keyboard',             'Tactile switches, hot-swappable, RGB.',            129.50, 'USD', true,  2, '2023-02-01 00:00+00'),
    (4, 'EL-1002', 'Noise-Cancelling Headphones',     'Over-ear, 30h battery.',                           249.00, 'USD', false, 2, '2023-02-15 00:00+00'),
    (5, 'HM-2001', 'Standing Desk',                   'Electric height-adjustable, oak top.',             499.00, 'USD', true,  3, '2023-03-01 00:00+00');

-- Localized titles for the first few products — the open-ended set of language
-- keys the `map: title` field keeps searchable per key.
UPDATE products SET title = '{"en": "Mechanical Keyboard", "it": "Tastiera Meccanica", "es": "Teclado Mecánico"}'::jsonb WHERE id = 3;
UPDATE products SET title = '{"en": "Noise-Cancelling Headphones", "it": "Cuffie con Cancellazione del Rumore"}'::jsonb WHERE id = 4;
UPDATE products SET title = '{"en": "Standing Desk", "de": "Stehpult"}'::jsonb WHERE id = 5;

INSERT INTO tags (id, label) VALUES
    (1, 'bestseller'),
    (2, 'sale'),
    (3, 'new');

INSERT INTO product_tags (product_id, tag_id) VALUES
    (1, 1),          -- TAOCP: bestseller
    (3, 1), (3, 2),  -- keyboard: bestseller + sale
    (4, 3),          -- headphones: new
    (5, 2);          -- desk: sale

-- Commerce -------------------------------------------------------------------

INSERT INTO orders (id, user_id, status, total, placed_at, shipped_at) VALUES
    (10, 1, 'delivered', 253.99, '2023-04-01 10:00+00', '2023-04-02 12:00+00'),
    (11, 1, 'pending',    54.00, '2023-06-10 16:20+00', NULL),
    (12, 2, 'shipped',   249.00, '2023-06-12 09:05+00', '2023-06-13 08:00+00'),
    (13, 2, 'cancelled', 129.50, '2023-06-15 18:40+00', NULL),
    (14, 4, 'paid',      499.00, '2023-07-01 13:00+00', NULL);

INSERT INTO order_items (id, order_id, product_id, quantity, unit_price) VALUES
    (100, 10, 1, 1, 199.99),
    (101, 10, 2, 1,  54.00),
    (102, 11, 2, 1,  54.00),
    (103, 12, 4, 1, 249.00),
    (104, 13, 3, 1, 129.50),
    (105, 14, 5, 1, 499.00);

INSERT INTO reviews (id, product_id, user_id, rating, body, created_at) VALUES
    (1000, 1, 2,    5, 'A lifetime of reading.',      '2023-04-10 00:00+00'),
    (1001, 1, 3,    4, 'Dense but rewarding.',        '2023-04-12 00:00+00'),
    (1002, 3, 1,    5, 'Best keyboard I have owned.', '2023-06-20 00:00+00'),
    (1003, 3, 4,    3, 'Loud switches.',              '2023-06-21 00:00+00'),
    (1004, 4, NULL, 4, 'Anonymous: great sound.',     '2023-06-25 00:00+00');

-- Bulk volume ----------------------------------------------------------------
--
-- The rows above are hand-curated and readable; the block below is generated
-- with generate_series purely to give the dev indexes *volume* (≈60 users,
-- 50 products, 400 orders → hundreds of documents) so paging, aggregates, and
-- search relevance are interesting to poke at. Everything is deterministic
-- (derived from the series index, no random()), so re-seeding is reproducible.
--
-- Id ranges are deliberately high and disjoint from the curated rows AND from
-- the ids dev/changes.sql touches (user 5, order 15, item 106, review 1005,
-- tag pair (1,2)) so nothing collides:
--   users 100–159, addresses 100–159 + 1100–1159, products 100–149,
--   orders 1000–1399, order_items 10000+, reviews 2000–2099.
-- To dial the volume up or down, edit the generate_series bounds below.

INSERT INTO categories (id, name, slug) VALUES
    (4, 'Clothing', 'clothing'),
    (5, 'Toys',     'toys'),
    (6, 'Office',   'office');

INSERT INTO tags (id, label) VALUES
    (4, 'premium'),
    (5, 'clearance'),
    (6, 'eco'),
    (7, 'limited');

-- Users 100–159: name/tier/country/created_at all derived from the index.
-- ~6% are soft-deleted (g % 17 = 0) so the users index has tombstones.
INSERT INTO users (id, email, full_name, tier, country, created_at, deleted)
SELECT g.n,
       lower(p.first) || '.' || lower(p.last) || g.n || '@example.com',
       p.first || ' ' || p.last,
       (ARRAY['free','pro','enterprise'])[1 + g.n % 3],
       (ARRAY['US','GB','DE','FR','NL','CA','AU','JP'])[1 + g.n % 8],
       timestamptz '2023-01-01 00:00+00' + (g.n % 365) * interval '1 day',
       (g.n % 17 = 0)
FROM generate_series(100, 159) AS g(n)
CROSS JOIN LATERAL (
    SELECT (ARRAY['Avery','Blair','Casey','Drew','Emery','Finley','Gray','Harper',
                  'Indigo','Jordan','Kai','Lane','Morgan','Noah','Oakley','Parker',
                  'Quinn','Reese','Sage','Tatum'])[1 + g.n % 20]            AS first,
           (ARRAY['Stone','Vale','West','Brooks','Cole','Dane','Frost','Greer',
                  'Hale','Iver','Jett','Knox','Lowe','Mercer','North','Pace',
                  'Reed','Shaw','Tate','Wells'])[1 + (g.n * 7) % 20]        AS last
) AS p;

-- Profiles for ~75% of bulk users (g % 4 = 0 → no profile → null one-to-one).
INSERT INTO profiles (user_id, bio, avatar_url, birth_date)
SELECT g.n,
       'Generated profile for user ' || g.n || '.',
       CASE WHEN g.n % 2 = 0 THEN 'https://cdn.example.com/a/u' || g.n || '.png' END,
       date '1960-01-01' + (g.n * 97 % 14000)
FROM generate_series(100, 159) AS g(n)
WHERE g.n % 4 <> 0;

-- One billing address per bulk user, plus a shipping address for the even ids.
INSERT INTO addresses (id, user_id, kind, line1, city, postal_code, country)
SELECT g.n, g.n, 'billing',
       (g.n % 100) || ' Main St',
       (ARRAY['Springfield','Riverton','Fairview','Madison',
              'Georgetown','Clinton','Franklin','Salem'])[1 + g.n % 8],
       lpad((g.n * 37 % 100000)::text, 5, '0'),
       (ARRAY['US','GB','DE','FR','NL','CA','AU','JP'])[1 + g.n % 8]
FROM generate_series(100, 159) AS g(n);

INSERT INTO addresses (id, user_id, kind, line1, city, postal_code, country)
SELECT g.n + 1000, g.n, 'shipping',
       (g.n % 50) || ' Oak Ave',
       (ARRAY['Lakeside','Hillcrest','Brookfield','Glenwood',
              'Maple Grove','Cedar Falls','Pine Bluff','Elmwood'])[1 + g.n % 8],
       lpad((g.n * 53 % 100000)::text, 5, '0'),
       (ARRAY['US','GB','DE','FR','NL','CA','AU','JP'])[1 + g.n % 8]
FROM generate_series(100, 159) AS g(n)
WHERE g.n % 2 = 0;

-- Products 100–149 across all six categories; ~20% out of stock (g % 5 = 0).
INSERT INTO products (id, sku, name, description, price, currency, in_stock, category_id, created_at)
SELECT g.n,
       'GEN-' || lpad(g.n::text, 5, '0'),
       (ARRAY['Widget','Gadget','Gizmo','Doohickey','Contraption',
              'Apparatus','Module','Implement','Tool','Instrument'])[1 + g.n % 10]
         || ' ' ||
       (ARRAY['Pro','Max','Lite','Plus','Mini','Ultra','Eco','Prime'])[1 + g.n % 8],
       'A generated catalog item #' || g.n || '.',
       round((10 + (g.n * 7 % 990) + 0.99)::numeric, 2),
       'USD',
       (g.n % 5 <> 0),
       1 + g.n % 6,
       timestamptz '2023-01-01 00:00+00' + (g.n % 400) * interval '1 day'
FROM generate_series(100, 149) AS g(n);

-- 1–3 tags per bulk product (tags 1–7); ON CONFLICT guards the composite key.
INSERT INTO product_tags (product_id, tag_id)
SELECT g.n, 1 + (g.n + t.k) % 7
FROM generate_series(100, 149) AS g(n)
CROSS JOIN LATERAL generate_series(0, g.n % 3) AS t(k)
ON CONFLICT DO NOTHING;

-- Orders 1000–1399 spread over the bulk users; total filled in after items.
INSERT INTO orders (id, user_id, status, total, placed_at, shipped_at)
SELECT g.n,
       100 + g.n % 60,
       s.status,
       0,
       d.placed_at,
       CASE WHEN s.status IN ('shipped','delivered')
            THEN d.placed_at + interval '2 days' END
FROM generate_series(1000, 1399) AS g(n)
CROSS JOIN LATERAL (
    SELECT (ARRAY['pending','paid','shipped','delivered','cancelled'])[1 + g.n % 5] AS status
) AS s
CROSS JOIN LATERAL (
    SELECT timestamptz '2023-01-01 00:00+00' + (g.n % 500) * interval '1 day' AS placed_at
) AS d;

-- 1–3 line items per order, each referencing a bulk product.
INSERT INTO order_items (id, order_id, product_id, quantity, unit_price)
SELECT 10000 + (o.n - 1000) * 3 + i.k,
       o.n,
       100 + (o.n + i.k) % 50,
       1 + (o.n + i.k) % 4,
       round((10 + ((o.n + i.k) * 13 % 500) + 0.50)::numeric, 2)
FROM generate_series(1000, 1399) AS o(n)
CROSS JOIN LATERAL generate_series(0, o.n % 3) AS i(k);

-- Backfill each bulk order's total from its line items.
UPDATE orders o
SET    total = s.t
FROM (
    SELECT order_id, sum(quantity * unit_price) AS t
    FROM order_items
    GROUP BY order_id
) AS s
WHERE o.id = s.order_id AND o.id >= 1000;

-- 1–2 reviews per bulk product; ~25% are anonymous (null user_id).
INSERT INTO reviews (id, product_id, user_id, rating, body, created_at)
SELECT 2000 + (g.n - 100) * 2 + r.k,
       g.n,
       CASE WHEN (g.n + r.k) % 4 = 0 THEN NULL ELSE 100 + (g.n + r.k) % 60 END,
       1 + (g.n + r.k) % 5,
       'Review ' || r.k || ' for product ' || g.n || '.',
       timestamptz '2023-06-01 00:00+00' + ((g.n + r.k) % 200) * interval '1 day'
FROM generate_series(100, 149) AS g(n)
CROSS JOIN LATERAL generate_series(0, g.n % 2) AS r(k);
