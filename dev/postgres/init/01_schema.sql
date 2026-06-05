-- Schema for the flusso dev environment — a small e-commerce store.
--
-- It is deliberately broad so the dev documents exercise every feature: all
-- scalar types, one-to-one / one-to-many / many-to-many joins, deep nesting,
-- every aggregate, filters, groups, and soft-delete. Three indexes are built
-- from it (users, products, orders) — see dev/flusso.toml.
--
-- Ids are explicit (not serial) so the seed and changes stay deterministic.

-- People ---------------------------------------------------------------------

CREATE TABLE users (
    id         int PRIMARY KEY,
    email      text         NOT NULL,
    full_name  text,
    tier       text         NOT NULL DEFAULT 'free', -- free | pro | enterprise
    country    char(2),                              -- ISO-3166 alpha-2
    created_at timestamptz  NOT NULL DEFAULT now(),
    deleted    boolean      NOT NULL DEFAULT false   -- soft-delete flag
);

-- One-to-one extension of a user.
CREATE TABLE profiles (
    user_id    int PRIMARY KEY REFERENCES users (id),
    bio        text,
    avatar_url text,
    birth_date date
);

-- One-to-many: a user has many addresses.
CREATE TABLE addresses (
    id          int PRIMARY KEY,
    user_id     int  NOT NULL REFERENCES users (id),
    kind        text NOT NULL,            -- billing | shipping
    line1       text NOT NULL,
    city        text NOT NULL,
    postal_code text,
    country     char(2)
);

-- Catalog --------------------------------------------------------------------

CREATE TABLE categories (
    id   int PRIMARY KEY,
    name text NOT NULL,
    slug text NOT NULL
);

CREATE TABLE products (
    id          int PRIMARY KEY,
    sku         text          NOT NULL,
    name        text          NOT NULL,
    description text,
    price       numeric(10, 2) NOT NULL,
    currency    char(3)       NOT NULL DEFAULT 'USD',
    in_stock    boolean       NOT NULL DEFAULT true,
    category_id int           REFERENCES categories (id),
    created_at  timestamptz   NOT NULL DEFAULT now()
);

CREATE TABLE tags (
    id    int PRIMARY KEY,
    label text NOT NULL
);

-- Junction for the products ⇄ tags many-to-many.
CREATE TABLE product_tags (
    product_id int NOT NULL REFERENCES products (id),
    tag_id     int NOT NULL REFERENCES tags (id),
    PRIMARY KEY (product_id, tag_id)
);

-- Commerce -------------------------------------------------------------------

CREATE TABLE orders (
    id         int PRIMARY KEY,
    user_id    int           NOT NULL REFERENCES users (id),
    status     text          NOT NULL DEFAULT 'pending', -- pending|paid|shipped|delivered|cancelled
    total      numeric(12, 2) NOT NULL DEFAULT 0,
    placed_at  timestamptz   NOT NULL DEFAULT now(),
    shipped_at timestamptz                                -- null until shipped
);

-- One-to-many under orders; each line references a product.
CREATE TABLE order_items (
    id         int PRIMARY KEY,
    order_id   int           NOT NULL REFERENCES orders (id),
    product_id int           NOT NULL REFERENCES products (id),
    quantity   int           NOT NULL,
    unit_price numeric(10, 2) NOT NULL
);

-- Product reviews, optionally by a known user.
CREATE TABLE reviews (
    id         int PRIMARY KEY,
    product_id int          NOT NULL REFERENCES products (id),
    user_id    int          REFERENCES users (id),
    rating     smallint     NOT NULL,           -- 1..5
    body       text,
    created_at timestamptz  NOT NULL DEFAULT now()
);

CREATE INDEX addresses_user_id_idx   ON addresses (user_id);
CREATE INDEX orders_user_id_idx      ON orders (user_id);
CREATE INDEX order_items_order_id_idx ON order_items (order_id);
CREATE INDEX order_items_product_idx ON order_items (product_id);
CREATE INDEX reviews_product_id_idx  ON reviews (product_id);
CREATE INDEX product_tags_tag_idx    ON product_tags (tag_id);

-- REPLICA IDENTITY defaults to the primary key, which every table has — enough
-- for flusso to recover row keys on UPDATE/DELETE and reverse-resolve a child
-- change back to the document(s) that embed it.
