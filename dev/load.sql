-- A continuous load generator for the dev/demo store — a read→modify→write loop
-- that mutates rows for a while so you can watch flusso's throughput, in-flight
-- backlog, replication-slot lag, and flush latency move under sustained traffic
-- (Grafana at :3000, or `curl -s localhost:9464/{status,metrics}`).
--
-- It mixes the operations that exercise the pipeline's interesting paths: plain
-- doc rebuilds (user/product updates), reverse-resolution (a line-item change
-- rebuilds its order *and* the owning user's doc), new orders with items,
-- review rollups, and soft-delete toggles.
--
-- Load it once, then CALL it. It COMMITs each tick, so changes stream out as it
-- runs (not all at the end):
--
--   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/load.sql
--   psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" \
--     -c "CALL simulate_production(duration_secs => 300, ops_per_tick => 25, sleep_ms => 150)"
--
-- Against the containerized Postgres (demo stack):
--
--   docker exec -i flusso-postgres psql -U postgres -d flusso < dev/load.sql
--   docker exec flusso-postgres psql -U postgres -d flusso \
--     -c "CALL simulate_production(600, 40, 100)"
--
-- Tune the throughput: more ops_per_tick and/or a smaller sleep_ms = higher
-- rate. `sleep_ms => 0` with a large ops_per_tick approximates a burst/stress
-- run; the defaults below are a gentle, steady "production-ish" trickle.

CREATE OR REPLACE PROCEDURE simulate_production(
    duration_secs int DEFAULT 60,   -- how long to keep mutating
    ops_per_tick  int DEFAULT 20,   -- mutations committed together each tick
    sleep_ms      int DEFAULT 200   -- pause between ticks
)
LANGUAGE plpgsql
AS $$
DECLARE
    deadline       timestamptz := clock_timestamp() + make_interval(secs => duration_secs);
    -- Ids are explicit in this schema, so hand out fresh ones from the current
    -- max (this session is the only writer of these while it runs).
    next_order_id  int := (SELECT COALESCE(MAX(id), 0) FROM orders) + 1;
    next_item_id   int := (SELECT COALESCE(MAX(id), 0) FROM order_items) + 1;
    next_review_id int := (SELECT COALESCE(MAX(id), 0) FROM reviews) + 1;
    total_ops      bigint := 0;
    op             int;
    uid            int;
    pid            int;
    pprice         numeric(10, 2);
    oid            int;
    items          int;
BEGIN
    WHILE clock_timestamp() < deadline LOOP
        FOR _tick IN 1..ops_per_tick LOOP
            op := floor(random() * 7)::int;   -- 0..6

            IF op = 0 THEN
                -- read→modify→save a user → rebuilds its `users` doc
                UPDATE users
                   SET tier = (ARRAY['free', 'pro', 'enterprise'])[1 + floor(random() * 3)::int]
                 WHERE id = (SELECT id FROM users WHERE NOT deleted ORDER BY random() LIMIT 1);

            ELSIF op = 1 THEN
                -- read→modify→save a product price → rebuilds its `products` doc
                -- (cast random() to numeric: there's no numeric * double operator)
                UPDATE products
                   SET price = round(price * (0.95 + random()::numeric * 0.10), 2),
                       in_stock = (random() < 0.9)
                 WHERE id = (SELECT id FROM products ORDER BY random() LIMIT 1);

            ELSIF op = 2 THEN
                -- advance an order's status → rebuilds the order + owner's docs
                UPDATE orders
                   SET status = CASE status
                                    WHEN 'pending' THEN 'paid'
                                    WHEN 'paid'    THEN 'shipped'
                                    WHEN 'shipped' THEN 'delivered'
                                    ELSE 'pending'
                                END,
                       shipped_at = CASE WHEN status = 'paid' THEN now() ELSE shipped_at END
                 WHERE id = (SELECT id FROM orders ORDER BY random() LIMIT 1);

            ELSIF op = 3 THEN
                -- change a line item qty → reverse-resolves to its order + user
                UPDATE order_items
                   SET quantity = 1 + floor(random() * 5)::int
                 WHERE id = (SELECT id FROM order_items ORDER BY random() LIMIT 1);

            ELSIF op = 4 THEN
                -- a brand-new order with a few items → new order doc + user rebuild
                SELECT id INTO uid FROM users WHERE NOT deleted ORDER BY random() LIMIT 1;
                IF uid IS NOT NULL THEN
                    INSERT INTO orders (id, user_id, status, total, placed_at)
                    VALUES (next_order_id, uid, 'pending', 0, now());
                    oid := next_order_id;
                    next_order_id := next_order_id + 1;
                    items := 1 + floor(random() * 3)::int;
                    FOR _item IN 1..items LOOP
                        SELECT id, price INTO pid, pprice FROM products ORDER BY random() LIMIT 1;
                        INSERT INTO order_items (id, order_id, product_id, quantity, unit_price)
                        VALUES (next_item_id, oid, pid, 1 + floor(random() * 4)::int, pprice);
                        next_item_id := next_item_id + 1;
                    END LOOP;
                    UPDATE orders
                       SET total = (SELECT COALESCE(SUM(quantity * unit_price), 0)
                                      FROM order_items WHERE order_id = oid)
                     WHERE id = oid;
                END IF;

            ELSIF op = 5 THEN
                -- a new review → rebuilds the product's rating rollups
                SELECT id INTO pid FROM products ORDER BY random() LIMIT 1;
                SELECT id INTO uid FROM users WHERE NOT deleted ORDER BY random() LIMIT 1;
                INSERT INTO reviews (id, product_id, user_id, rating, body, created_at)
                VALUES (next_review_id, pid, uid, 1 + floor(random() * 5)::int, 'auto-generated', now());
                next_review_id := next_review_id + 1;

            ELSE
                -- toggle a soft-delete → a user leaves / re-enters its index
                UPDATE users
                   SET deleted = NOT deleted
                 WHERE id = (SELECT id FROM users ORDER BY random() LIMIT 1);
            END IF;

            total_ops := total_ops + 1;
        END LOOP;

        COMMIT;   -- flush this tick's changes so flusso captures them now
        PERFORM pg_sleep(sleep_ms / 1000.0);
    END LOOP;

    RAISE NOTICE 'simulate_production done: % ops over ~%s', total_ops, duration_secs;
END;
$$;
