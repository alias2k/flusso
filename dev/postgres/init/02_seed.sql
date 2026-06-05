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
