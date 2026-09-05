-- The `complex` scenario's store: the most complex `users` document the
-- builder supports (see users.schema.yml). Ids are explicit so the seed and the
-- writer stay deterministic.

CREATE TABLE users (
    id         int PRIMARY KEY,
    name       text,
    email      text,
    status     text        NOT NULL,
    bio        text,
    archived   boolean     NOT NULL DEFAULT false,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE profiles (
    id       int PRIMARY KEY,
    user_id  int NOT NULL,
    headline text
);

CREATE TABLE orders (
    id        int PRIMARY KEY,
    user_id   int            NOT NULL,
    total     numeric(10, 2) NOT NULL,
    status    text           NOT NULL,
    placed_at timestamptz    NOT NULL
);

CREATE TABLE order_items (
    id       int PRIMARY KEY,
    order_id int            NOT NULL,
    sku      text           NOT NULL,
    qty      int            NOT NULL,
    price    numeric(10, 2) NOT NULL
);

CREATE TABLE tags (
    id    int PRIMARY KEY,
    label text NOT NULL
);

CREATE TABLE user_tags (
    user_id int NOT NULL,
    tag_id  int NOT NULL,
    PRIMARY KEY (user_id, tag_id)
);

CREATE INDEX profiles_user_id_idx      ON profiles (user_id);
CREATE INDEX orders_user_id_idx        ON orders (user_id);
CREATE INDEX order_items_order_id_idx  ON order_items (order_id);
CREATE INDEX user_tags_tag_id_idx      ON user_tags (tag_id);
