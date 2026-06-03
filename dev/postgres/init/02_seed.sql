-- Initial fixtures. storno is a CDC consumer: it streams *changes* from the
-- replication slot, not a snapshot of pre-existing rows. So this seed data
-- exists mainly to make the build SQL produce interesting documents once a
-- change touches a row. Use dev/changes.sql to emit live changes.

INSERT INTO users (id, email, name) VALUES
    (1, 'ada@example.com',   'Ada Lovelace'),
    (2, 'grace@example.com', 'Grace Hopper'),
    (3, 'alan@example.com',  'Alan Turing');

INSERT INTO orders (id, user_id, total, status) VALUES
    (10, 1, 19.99, 'shipped'),
    (11, 1,  5.00, 'pending'),
    (12, 2, 42.50, 'shipped');
