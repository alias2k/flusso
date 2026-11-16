use kernel::{GenericValue, TableName};
use source::RowKey;

use std::collections::HashSet;

use super::*;

fn state() -> (DecodeState, Arc<Positions>) {
    let ack = Arc::new(Positions::new(0, 0, Default::default()));
    let decode = DecodeState {
        relations: HashMap::new(),
        unkeyed: Vec::new(),
        pre_image_links: Default::default(),
        untraceable: HashMap::new(),
        warned: HashSet::new(),
        warned_unkeyed: HashSet::new(),
        open_txn: Vec::new(),
        open_xid: 0,
        pending: VecDeque::new(),
        readable: HashSet::new(),
        read_tables: Some(HashSet::new()),
        ack: Arc::clone(&ack),
        done: false,
        column_types: None,
    };
    (decode, ack)
}

fn keepalive(wal_end: u64) -> ReplicationEvent {
    ReplicationEvent::KeepAlive {
        wal_end: Lsn::from_u64(wal_end),
        reply_requested: false,
        server_time_micros: 0,
    }
}

fn commit(end_lsn: u64) -> ReplicationEvent {
    ReplicationEvent::Commit {
        lsn: Lsn::from_u64(end_lsn),
        end_lsn: Lsn::from_u64(end_lsn),
        commit_time_micros: 0,
    }
}

fn upsert() -> ChangeEvent {
    ChangeEvent::Upsert {
        table: TableName::try_new("users").unwrap(),
        key: RowKey(Vec::new()),
        before: None,
        after: None,
    }
}

#[test]
fn keepalive_advances_the_watermark_when_idle() {
    let (mut decode, ack) = state();
    handle(&mut decode, keepalive(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 100);
}

#[test]
fn keepalive_is_ignored_while_a_transaction_is_open() {
    let (mut decode, ack) = state();
    decode.open_txn.push(upsert());
    handle(&mut decode, keepalive(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0);
}

#[test]
fn keepalive_is_ignored_while_changes_await_emission() {
    let (mut decode, ack) = state();
    decode.pending.push_back(Pending {
        event: upsert(),
        lsn: 50,
        xid: 1,
    });
    handle(&mut decode, keepalive(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0);
}

#[test]
fn keepalive_waits_for_an_emitted_but_unconfirmed_change() {
    let (mut decode, ack) = state();
    let seq = ack.register(50); // emitted to the engine, not yet flushed
    handle(&mut decode, keepalive(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0, "must not pass the unflushed change");
    ack.confirm(seq);
    assert_eq!(
        ack.confirmed_lsn(),
        100,
        "with the change confirmed, the keepalive's WAL end is covered"
    );
}

#[test]
fn emitting_the_last_change_of_a_commit_moves_the_horizon() {
    let (mut decode, ack) = state();
    decode.open_txn.push(upsert());
    decode.open_txn.push(upsert());
    handle(&mut decode, commit(100)).unwrap();

    let (first, _) = decode.next_change().unwrap();
    assert_eq!(
        ack.reached(Some(first.0)),
        0,
        "half a transaction covers nothing of its commit"
    );
    let (second, _) = decode.next_change().unwrap();
    assert_eq!(ack.reached(Some(first.0)), 99, "still short of the commit");
    assert_eq!(ack.reached(Some(second.0)), 100);
    assert!(decode.next_change().is_none());
}

#[test]
fn a_commit_behind_another_keeps_the_horizon_at_the_emitted_one() {
    let (mut decode, ack) = state();
    handle(&mut decode, begin(1)).unwrap();
    decode.open_txn.push(upsert());
    handle(&mut decode, commit(100)).unwrap();
    handle(&mut decode, begin(2)).unwrap();
    decode.open_txn.push(upsert());
    handle(&mut decode, commit(200)).unwrap();

    let (first, _) = decode.next_change().unwrap();
    assert_eq!(ack.reached(Some(first.0)), 100);
    let (second, _) = decode.next_change().unwrap();
    assert_eq!(ack.reached(Some(first.0)), 199, "short of the next commit");
    assert_eq!(ack.reached(Some(second.0)), 200);
}

#[test]
fn empty_commit_advances_the_watermark() {
    let (mut decode, ack) = state();
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 100);
    assert!(decode.pending.is_empty(), "nothing to emit");
}

#[test]
fn empty_commit_is_ignored_while_changes_await_emission() {
    let (mut decode, ack) = state();
    decode.pending.push_back(Pending {
        event: upsert(),
        lsn: 50,
        xid: 1,
    });
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0);
}

#[test]
fn non_empty_commit_queues_changes_without_advancing() {
    let (mut decode, ack) = state();
    decode.open_txn.push(upsert());
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(decode.pending.len(), 1);
    assert_eq!(decode.pending.front().map(|change| change.lsn), Some(100));
    assert_eq!(
        ack.confirmed_lsn(),
        0,
        "advance waits for the engine confirm"
    );
}

/// A pgoutput `Relation` for `public.child(id PK int4, parent_id int4)`:
/// under `REPLICA IDENTITY FULL` both columns are flagged, under `DEFAULT`
/// only `id`.
fn child_relation(identity: u8) -> Vec<u8> {
    let mut m = vec![b'R'];
    m.extend_from_slice(&16385u32.to_be_bytes());
    m.extend_from_slice(b"public\0child\0");
    m.push(identity);
    m.extend_from_slice(&2i16.to_be_bytes());
    for name in ["id", "parent_id"] {
        m.push(u8::from(name == "id" || identity == b'f'));
        m.extend_from_slice(name.as_bytes());
        m.push(0);
        m.extend_from_slice(&23u32.to_be_bytes());
        m.extend_from_slice(&(-1i32).to_be_bytes());
    }
    m
}

/// A `Relation` message, sent before a table's first change in a stream and
/// after its definition changes, drops that table's cached column types when
/// it names other types than the cache, and no other table's.
#[test]
fn a_relation_message_with_other_types_forgets_its_tables_column_types() {
    let (mut decode, _ack) = state();
    let column_types = crate::ColumnTypes::default();
    column_types.cache("public", "child", "parent_id", 17);
    column_types.cache("public", "parent", "id", 17);
    decode.column_types = Some(column_types.clone());

    handle_xlog(&mut decode, &child_relation(b'd')).unwrap();

    assert!(!column_types.caches("public", "child"));
    assert!(column_types.caches("public", "parent"));
}

/// `child` announced with its `id` of type `type_oid` and modifier `typmod`.
fn child_relation_typed(type_oid: u32, typmod: i32) -> Vec<u8> {
    let mut m = vec![b'R'];
    m.extend_from_slice(&16385u32.to_be_bytes());
    m.extend_from_slice(b"public\0child\0");
    m.push(b'd');
    m.extend_from_slice(&1i16.to_be_bytes());
    m.push(1);
    m.extend_from_slice(b"id\0");
    m.extend_from_slice(&type_oid.to_be_bytes());
    m.extend_from_slice(&typmod.to_be_bytes());
    m
}

/// A resize keeps the type and changes its modifier (`varchar(5)` →
/// `varchar(10)`): the cached entry, which names the old size, goes.
#[test]
fn a_relation_message_with_another_type_modifier_forgets_its_tables_column_types() {
    let (mut decode, _ack) = state();
    let column_types = crate::ColumnTypes::default();
    column_types.cache("public", "child", "id", 1043);
    decode.column_types = Some(column_types.clone());

    handle_xlog(&mut decode, &child_relation_typed(1043, 14)).unwrap();

    assert!(!column_types.caches("public", "child"));
}

/// A `Relation` naming the cached types keeps them: a stream's first one
/// costs no lookup.
#[test]
fn a_relation_message_with_the_cached_types_keeps_them() {
    let (mut decode, _ack) = state();
    let column_types = crate::ColumnTypes::default();
    column_types.cache("public", "child", "id", 23);
    column_types.cache("public", "child", "parent_id", 23);
    decode.column_types = Some(column_types.clone());

    handle_xlog(&mut decode, &child_relation(b'd')).unwrap();

    assert!(column_types.caches("public", "child"));
}

/// pgoutput leaves a stored generated column, or one outside the
/// publication's column list, out of every `Relation`. A cached column the
/// message doesn't list says nothing about its type, so the entries stay:
/// forgetting them would re-read the catalog after each `Relation`.
#[test]
fn a_relation_message_that_omits_a_cached_column_keeps_the_cached_types() {
    let (mut decode, _ack) = state();
    let column_types = crate::ColumnTypes::default();
    column_types.cache("public", "child", "id", 23);
    column_types.cache("public", "child", "generated", 17);
    decode.column_types = Some(column_types.clone());

    handle_xlog(&mut decode, &child_relation(b'd')).unwrap();

    assert!(column_types.caches("public", "child"));
}

fn text(value: &str) -> Vec<u8> {
    let mut c = vec![b't'];
    c.extend_from_slice(&(value.len() as i32).to_be_bytes());
    c.extend_from_slice(value.as_bytes());
    c
}

/// Announce the child relation under `identity` and key it by `id`, as the
/// stream loop does.
fn announce_child(decode: &mut DecodeState, identity: u8) {
    handle_xlog(decode, &child_relation(identity)).unwrap();
    assert_eq!(decode.unkeyed, vec![16385]);
    decode.unkeyed.clear();
    decode.relations.get_mut(&16385).unwrap().primary_key =
        vec![kernel::ColumnName::try_new("id").unwrap()];
    decode.relations.get_mut(&16385).unwrap().partitioned = Some(false);
    record_untraceable(decode, 16385);
}

fn column(name: &str) -> kernel::ColumnName {
    kernel::ColumnName::try_new(name).unwrap()
}

#[test]
fn a_full_identity_delete_is_keyed_by_id_and_carries_the_old_parent() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'f');

    let mut delete = vec![b'D'];
    delete.extend_from_slice(&16385u32.to_be_bytes());
    delete.push(b'O');
    delete.extend_from_slice(&2i16.to_be_bytes());
    delete.extend(text("7"));
    delete.extend(text("1"));
    handle_xlog(&mut decode, &delete).unwrap();

    let [ChangeEvent::Delete { key, before, .. }] = decode.open_txn.as_slice() else {
        panic!("expected one delete, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Int(7))]);
    assert_eq!(
        before.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(1))
    );
}

#[test]
fn a_full_identity_reparent_is_one_upsert_carrying_the_old_parent() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'f');

    let mut update = vec![b'U'];
    update.extend_from_slice(&16385u32.to_be_bytes());
    update.push(b'O');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.extend(text("7"));
    update.extend(text("1"));
    update.push(b'N');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.extend(text("7"));
    update.extend(text("3"));
    handle_xlog(&mut decode, &update).unwrap();

    let [
        ChangeEvent::Upsert {
            key, before, after, ..
        },
    ] = decode.open_txn.as_slice()
    else {
        panic!("expected one upsert, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Int(7))]);
    assert_eq!(
        before.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(1))
    );
    assert_eq!(
        after.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(3))
    );
}

#[test]
fn an_update_of_a_partitioned_table_carries_no_post_image() {
    for partitioned in [Some(true), None] {
        let (mut decode, _) = state();
        announce_child(&mut decode, b'f');
        decode.relations.get_mut(&16385).unwrap().partitioned = partitioned;

        let mut update = vec![b'U'];
        update.extend_from_slice(&16385u32.to_be_bytes());
        update.push(b'O');
        update.extend_from_slice(&2i16.to_be_bytes());
        update.extend(text("7"));
        update.extend(text("1"));
        update.push(b'N');
        update.extend_from_slice(&2i16.to_be_bytes());
        update.extend(text("7"));
        update.extend(text("3"));
        handle_xlog(&mut decode, &update).unwrap();

        assert!(
            matches!(
                decode.open_txn.as_slice(),
                [ChangeEvent::Upsert {
                    before: Some(_),
                    after: None,
                    ..
                }]
            ),
            "{partitioned:?}: {:?}",
            decode.open_txn
        );
    }
}

#[test]
fn an_update_without_a_pre_image_carries_no_post_image() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'd');

    let mut update = vec![b'U'];
    update.extend_from_slice(&16385u32.to_be_bytes());
    update.push(b'N');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.extend(text("7"));
    update.extend(text("3"));
    handle_xlog(&mut decode, &update).unwrap();

    assert!(matches!(
        decode.open_txn.as_slice(),
        [ChangeEvent::Upsert {
            before: None,
            after: None,
            ..
        }]
    ));
}

fn needs_parent_link(decode: &mut DecodeState) {
    decode.pre_image_links.insert(
        source::QualifiedTable::new(
            kernel::DatabaseSchema::try_new("public").unwrap(),
            TableName::try_new("child").unwrap(),
        ),
        vec![column("parent_id")],
    );
}

fn delete_child(decode: &mut DecodeState) {
    let mut delete = vec![b'D'];
    delete.extend_from_slice(&16385u32.to_be_bytes());
    delete.push(b'K');
    delete.extend_from_slice(&2i16.to_be_bytes());
    delete.extend(text("7"));
    delete.push(b'n');
    handle_xlog(decode, &delete).unwrap();
}

#[test]
fn an_identity_carrying_the_parent_link_is_traceable() {
    let (mut decode, _) = state();
    needs_parent_link(&mut decode);
    announce_child(&mut decode, b'f');
    assert!(decode.untraceable.is_empty());
}

#[test]
fn a_change_on_an_identity_missing_the_parent_link_is_flagged_once() {
    let (mut decode, _) = state();
    needs_parent_link(&mut decode);
    announce_child(&mut decode, b'd');
    assert_eq!(decode.untraceable[&16385], vec![column("parent_id")]);

    delete_child(&mut decode);
    delete_child(&mut decode);
    assert_eq!(decode.warned, HashSet::from([16385]));
    assert!(matches!(
        decode.open_txn.as_slice(),
        [
            ChangeEvent::Delete { before: None, .. },
            ChangeEvent::Delete { before: None, .. }
        ]
    ));
}

#[test]
fn an_identity_index_without_the_primary_key_keys_by_its_own_columns() {
    let (mut decode, _) = state();
    // `USING INDEX` on `parent_id` alone: the old tuple carries no `id`.
    handle_xlog(&mut decode, &child_relation(b'i')).unwrap();
    let relation = decode.relations.get_mut(&16385).unwrap();
    relation.primary_key = vec![column("id")];
    relation
        .columns
        .iter_mut()
        .for_each(|c| c.is_key = c.name.as_ref() == "parent_id");

    let mut update = vec![b'U'];
    update.extend_from_slice(&16385u32.to_be_bytes());
    update.push(b'K');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.push(b'n');
    update.extend(text("1"));
    update.push(b'N');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.extend(text("7"));
    update.extend(text("3"));
    handle_xlog(&mut decode, &update).unwrap();

    // The parent link changed, but both keys come from the identity columns:
    // no delete keyed by a null `id`.
    assert!(
        decode.open_txn.iter().all(|change| change
            .key()
            .0
            .iter()
            .all(|(_, v)| *v != GenericValue::Null)),
        "{:?}",
        decode.open_txn
    );
}

#[test]
fn an_identity_index_without_the_primary_key_is_untraceable() {
    let (mut decode, _) = state();
    needs_parent_link(&mut decode);
    // `USING INDEX` on `parent_id` alone: the link is an identity column, but
    // changes are keyed by it, so the pre-image never carries anything more.
    handle_xlog(&mut decode, &child_relation(b'i')).unwrap();
    let relation = decode.relations.get_mut(&16385).unwrap();
    relation.primary_key = vec![column("id")];
    relation
        .columns
        .iter_mut()
        .for_each(|c| c.is_key = c.name.as_ref() == "parent_id");
    record_untraceable(&mut decode, 16385);
    assert_eq!(decode.untraceable[&16385], vec![column("parent_id")]);
}

/// An old tuple whose primary key reads as null: a partition on an identity
/// index without the key, decoded under its parent's relation (issue #224).
/// `kind` is `K` under a parent flagging only the key, `O` under a `FULL` one.
fn old_tuple_without_the_key(message: u8, kind: u8, parent: &str) -> Vec<u8> {
    let mut m = vec![message];
    m.extend_from_slice(&16385u32.to_be_bytes());
    m.push(kind);
    m.extend_from_slice(&2i16.to_be_bytes());
    m.push(b'n');
    m.extend(text(parent));
    m
}

#[test]
fn a_delete_without_its_primary_key_or_old_row_is_emitted_and_warned_once() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'd');
    handle_xlog(&mut decode, &old_tuple_without_the_key(b'D', b'K', "1")).unwrap();
    handle_xlog(&mut decode, &old_tuple_without_the_key(b'D', b'K', "1")).unwrap();
    let [first, second] = decode.open_txn.as_slice() else {
        panic!("expected two deletes, got {:?}", decode.open_txn);
    };
    for change in [first, second] {
        let ChangeEvent::Delete { key, before, .. } = change else {
            panic!("expected a delete, got {change:?}");
        };
        assert!(key.has_null(), "{key:?}");
        assert_eq!(before, &None);
    }
    assert_eq!(
        decode.warned_unkeyed,
        HashSet::from([(16385, Unkeyed::DeleteUntraced)])
    );
}

#[test]
fn a_delete_without_its_primary_key_under_a_full_parent_carries_the_old_row() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'f');
    decode.relations.get_mut(&16385).unwrap().partitioned = Some(true);
    handle_xlog(&mut decode, &old_tuple_without_the_key(b'D', b'O', "77")).unwrap();

    let [ChangeEvent::Delete { key, before, .. }] = decode.open_txn.as_slice() else {
        panic!("expected one delete, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Null)]);
    assert_eq!(
        before.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(77))
    );
    assert_eq!(
        decode.warned_unkeyed,
        HashSet::from([(16385, Unkeyed::DeleteTraced)])
    );
}

#[test]
fn a_commit_of_only_unaddressable_deletes_is_emitted_at_its_position() {
    let (mut decode, ack) = state();
    announce_child(&mut decode, b'd');
    handle_xlog(&mut decode, &old_tuple_without_the_key(b'D', b'K', "1")).unwrap();
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(decode.pending.len(), 1);
    assert_eq!(decode.pending.front().map(|change| change.lsn), Some(100));
    assert_eq!(ack.confirmed_lsn(), 0, "the engine's confirm moves it");
}

/// An update whose old tuple lacks the key, then its new tuple `(id, parent)`.
fn update_without_the_old_key(kind: u8, old_parent: &str, id: &str, parent: &str) -> Vec<u8> {
    let mut update = old_tuple_without_the_key(b'U', kind, old_parent);
    update.push(b'N');
    update.extend_from_slice(&2i16.to_be_bytes());
    update.extend(text(id));
    update.extend(text(parent));
    update
}

#[test]
fn an_update_without_its_old_primary_key_is_an_upsert_of_its_row_alone() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'd');
    handle_xlog(
        &mut decode,
        &update_without_the_old_key(b'K', "1", "7", "3"),
    )
    .unwrap();

    let [ChangeEvent::Upsert { key, .. }] = decode.open_txn.as_slice() else {
        panic!("expected one upsert, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Int(7))]);
    assert_eq!(
        decode.warned_unkeyed,
        HashSet::from([(16385, Unkeyed::Update)])
    );
}

#[test]
fn a_reparent_without_its_old_primary_key_under_a_full_parent_carries_the_old_parent() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'f');
    decode.relations.get_mut(&16385).unwrap().partitioned = Some(true);
    handle_xlog(
        &mut decode,
        &update_without_the_old_key(b'O', "1", "7", "2"),
    )
    .unwrap();

    let [
        ChangeEvent::Upsert {
            key, before, after, ..
        },
    ] = decode.open_txn.as_slice()
    else {
        panic!("expected one upsert, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Int(7))]);
    assert_eq!(
        before.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(1))
    );
    assert_eq!(
        after, &None,
        "a partitioned table's update carries no post-image"
    );
}

#[test]
fn a_table_without_a_primary_key_keeps_its_null_identity_values() {
    let (mut decode, _) = state();
    announce_child(&mut decode, b'f');
    decode.relations.get_mut(&16385).unwrap().primary_key = Vec::new();
    let mut delete = vec![b'D'];
    delete.extend_from_slice(&16385u32.to_be_bytes());
    delete.push(b'O');
    delete.extend_from_slice(&2i16.to_be_bytes());
    delete.extend(text("7"));
    delete.push(b'n');
    handle_xlog(&mut decode, &delete).unwrap();
    assert!(matches!(
        decode.open_txn.as_slice(),
        [ChangeEvent::Delete { .. }]
    ));
    assert!(decode.warned_unkeyed.is_empty());
}

/// A replication connection that never sends an event and records what it
/// was told was applied.
struct SilentClient {
    applied: Arc<std::sync::atomic::AtomicU64>,
}

impl Replication for SilentClient {
    async fn recv(&mut self) -> std::result::Result<Option<ReplicationEvent>, PgWireError> {
        std::future::pending().await
    }

    fn update_applied_lsn(&self, lsn: Lsn) {
        self.applied
            .fetch_max(lsn.as_u64(), std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn a_confirmation_reaches_the_client_without_a_replication_event() {
    use futures::StreamExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    let ack = Arc::new(Positions::new(0, 0, Default::default()));
    let seq = ack.register(10);
    ack.read_through(10);
    let applied = Arc::new(AtomicU64::new(0));
    let client = SilentClient {
        applied: Arc::clone(&applied),
    };
    let catalog = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1/unused")
        .unwrap();
    let mut stream = build(
        client,
        catalog,
        Snapshots::seeing(&[]),
        Settings {
            pre_image_links: Default::default(),
            read_tables: None,
            read_ahead: 16,
            hold_clock: Arc::default(),
        },
        Arc::clone(&ack),
        None,
    );
    let reader = tokio::spawn(async move { stream.next().await.is_some() });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(applied.load(Ordering::SeqCst), 0, "nothing confirmed yet");

    ack.confirm(seq);
    tokio::time::timeout(Duration::from_secs(1), async {
        while applied.load(Ordering::SeqCst) < 10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the confirmed position reached the client");
    assert!(!reader.is_finished(), "no replication event was emitted");
    reader.abort();
}

#[test]
fn an_oversized_message_is_a_setup_error_naming_both_sizes() {
    let error = map_pgwire(PgWireError::MessageTooLarge {
        payload_bytes: 2_000,
        limit_bytes: 1_000,
    });
    let SourceError::Setup(message) = error else {
        panic!("expected a setup error, got {error:?}");
    };
    assert!(message.contains("2000 bytes"), "{message}");
    assert!(message.contains("1000-byte limit"), "{message}");
}

#[test]
fn transport_errors_stay_transient_and_framing_errors_are_decode_errors() {
    let io = PgWireError::Io(Arc::new(std::io::Error::from(
        std::io::ErrorKind::ConnectionReset,
    )));
    assert!(matches!(map_pgwire(io), SourceError::Connection(_)));
    assert!(matches!(
        map_pgwire(PgWireError::Protocol("bad frame".into())),
        SourceError::Decode(_)
    ));
}

// ── visibility (issue #343) ─────────────────────────────────────────────────

/// A visibility double: the snapshot sees the xids in `visible`; a stall check
/// refuses when `refuse` is set. Counts its calls.
#[derive(Clone, Default)]
struct Snapshots {
    visible: Arc<std::sync::Mutex<HashSet<u32>>>,
    refuse: Arc<std::sync::atomic::AtomicBool>,
    snapshots: Arc<std::sync::atomic::AtomicUsize>,
    stalls: Arc<std::sync::atomic::AtomicUsize>,
}

impl Snapshots {
    fn seeing(xids: &[u32]) -> Self {
        let snapshots = Self::default();
        snapshots.see(xids);
        snapshots
    }

    fn see(&self, xids: &[u32]) {
        self.visible.lock().unwrap().extend(xids);
    }

    fn snapshot_now(&self) -> super::super::visibility::Snapshot {
        let visible = self.visible.lock().unwrap();
        // Everything below 1000 runs unless listed visible.
        let running: Vec<String> = (1..1000u64)
            .filter(|xid| !visible.contains(&u32::try_from(*xid).unwrap()))
            .map(|xid| xid.to_string())
            .collect();
        super::super::visibility::Snapshot::parse(&format!("1:1000:{}", running.join(","))).unwrap()
    }
}

impl Visibility for Snapshots {
    async fn snapshot(&self) -> Result<super::super::visibility::Snapshot> {
        self.snapshots
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(self.snapshot_now())
    }

    async fn stall(&self, _xid: u32) -> Result<super::super::visibility::Stall> {
        self.stalls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let refusal = self
            .refuse
            .load(std::sync::atomic::Ordering::SeqCst)
            .then(|| super::super::visibility::standby_refusal("flusso", "flusso"));
        Ok(super::super::visibility::Stall {
            refusal,
            waiting: vec!["pid 1".to_owned()],
        })
    }
}

fn begin(xid: u32) -> ReplicationEvent {
    ReplicationEvent::Begin {
        final_lsn: Lsn::from_u64(0),
        xid,
        commit_time_micros: 0,
    }
}

/// A transaction `xid` with one change, committed at `lsn`.
fn transaction(decode: &mut DecodeState, xid: u32, lsn: u64) {
    handle(decode, begin(xid)).unwrap();
    decode.open_txn.push(upsert());
    handle(decode, commit(lsn)).unwrap();
}

fn holding_state() -> (DecodeState, Arc<Positions>) {
    let (mut decode, ack) = state();
    decode.read_tables = None;
    (decode, ack)
}

/// R3: a held transaction is not emitted, so not registered, and neither a
/// keepalive past it nor a filtered commit after it moves the horizon: no
/// confirmation or fence reaches its commit.
#[test]
fn a_held_transaction_is_not_emitted_and_nothing_passes_it() {
    let (mut decode, ack) = holding_state();
    transaction(&mut decode, 7, 100);
    assert!(decode.next_change().is_none(), "held");
    assert_eq!(decode.held(), Some(7));

    handle(&mut decode, keepalive(500)).unwrap();
    handle(&mut decode, begin(8)).unwrap();
    handle(&mut decode, commit(300)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0, "nothing may be confirmed past it");
    assert!(ack.reached(None) < 100, "no fence may claim its commit");
    assert_eq!(ack.next_seq(), 0, "not registered");

    decode.seen_by(&Snapshots::seeing(&[7]).snapshot_now());
    let (position, _) = decode.next_change().unwrap();
    assert_eq!(ack.reached(Some(position.0)), 100);
    assert!(decode.readable.is_empty(), "an emitted xid is forgotten");
}

/// A later transaction that is visible waits behind an earlier held one, and
/// both come out in commit order once the earlier one is visible.
#[test]
fn an_earlier_held_transaction_holds_back_a_later_visible_one() {
    let (mut decode, ack) = holding_state();
    transaction(&mut decode, 7, 100);
    transaction(&mut decode, 8, 200);
    decode.seen_by(&Snapshots::seeing(&[8]).snapshot_now());
    assert!(decode.next_change().is_none(), "8 is visible, 7 is not");
    assert_eq!(ack.next_seq(), 0);

    decode.seen_by(&Snapshots::seeing(&[7, 8]).snapshot_now());
    let (first, _) = decode.next_change().unwrap();
    let (second, _) = decode.next_change().unwrap();
    assert_eq!(ack.reached(Some(first.0)), 199);
    assert_eq!(ack.reached(Some(second.0)), 200);
    assert!(decode.readable.is_empty());
}

/// A transaction that touches no table an index reads is never checked.
#[test]
fn a_transaction_on_no_table_an_index_reads_is_not_held() {
    let (mut decode, _) = state();
    decode.read_tables = Some(HashSet::from([TableName::try_new("orders").unwrap()]));
    transaction(&mut decode, 7, 100);
    assert!(decode.next_change().is_some(), "users is read by no index");

    decode.read_tables = Some(HashSet::from([TableName::try_new("users").unwrap()]));
    transaction(&mut decode, 8, 200);
    assert!(decode.next_change().is_none(), "users is read: held");
}

/// A stream over `decode`, a scripted client and `snapshots`, holding at most
/// `read_ahead` changes.
fn holding_stream(
    decode: DecodeState,
    client: Script,
    snapshots: Snapshots,
    read_ahead: usize,
    hold_clock: HoldClock,
) -> BoxStream<'static, Result<LiveChange>> {
    into_stream(State {
        client,
        catalog: sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused@127.0.0.1/unused")
            .unwrap(),
        visibility: snapshots,
        decode,
        read_ahead,
        hold: None,
        hold_clock,
    })
}

/// A replication connection that sends its queued events, then waits, and
/// records what it was told was applied.
struct Script {
    events: Arc<std::sync::Mutex<VecDeque<ReplicationEvent>>>,
    applied: Arc<std::sync::atomic::AtomicU64>,
}

impl Replication for Script {
    async fn recv(&mut self) -> std::result::Result<Option<ReplicationEvent>, PgWireError> {
        let next = self.events.lock().unwrap().pop_front();
        match next {
            Some(event) => Ok(Some(event)),
            None => std::future::pending().await,
        }
    }

    fn update_applied_lsn(&self, lsn: Lsn) {
        self.applied
            .fetch_max(lsn.as_u64(), std::sync::atomic::Ordering::SeqCst);
    }
}

fn script(
    events: Vec<ReplicationEvent>,
) -> (
    Script,
    Arc<std::sync::Mutex<VecDeque<ReplicationEvent>>>,
    Arc<std::sync::atomic::AtomicU64>,
) {
    let queue = Arc::new(std::sync::Mutex::new(VecDeque::from(events)));
    let applied = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let client = Script {
        events: Arc::clone(&queue),
        applied: Arc::clone(&applied),
    };
    (client, queue, applied)
}

/// R4: an earlier change the sinks flushed is still confirmed to the server
/// while a later transaction is held, so the slot keeps releasing WAL.
#[tokio::test]
async fn a_confirmation_is_handed_over_during_a_hold() {
    use futures::StreamExt;
    use std::sync::atomic::Ordering;

    let (mut decode, ack) = holding_state();
    let earlier = ack.register(50);
    ack.read_through(50);
    transaction(&mut decode, 7, 100);
    let (client, _, applied) = script(Vec::new());
    let mut stream = holding_stream(decode, client, Snapshots::default(), 16, Arc::default());
    let reader = tokio::spawn(async move { stream.next().await.is_some() });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(applied.load(Ordering::SeqCst), 0);
    ack.confirm(earlier);
    tokio::time::timeout(Duration::from_secs(1), async {
        while applied.load(Ordering::SeqCst) < 50 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the confirmation reached the client during the hold");
    assert!(!reader.is_finished(), "the held transaction is not emitted");
    reader.abort();
}

/// While held, the stream polls on the schedule (0.5 ms for 20 ms, then backing
/// off to 50 ms; tokio's timer rounds the 0.5 ms up to its 1 ms tick), records
/// how long it has held, and after five seconds checks the server once.
#[tokio::test(start_paused = true)]
async fn a_hold_reads_ahead_to_its_bound_polls_on_schedule_and_reports_a_stall() {
    use futures::StreamExt;
    use std::sync::atomic::Ordering;

    let (mut decode, _ack) = holding_state();
    transaction(&mut decode, 7, 100);
    let mut events = Vec::new();
    for xid in 8..40 {
        events.extend([
            begin(xid),
            ReplicationEvent::Commit {
                lsn: Lsn::from_u64(u64::from(xid) * 100),
                end_lsn: Lsn::from_u64(u64::from(xid) * 100),
                commit_time_micros: 0,
            },
        ]);
    }
    let (client, queue, _) = script(events);
    let snapshots = Snapshots::default();
    let clock = HoldClock::default();
    let mut stream = holding_stream(decode, client, snapshots.clone(), 4, Arc::clone(&clock));
    let reader = tokio::spawn(async move { stream.next().await.map(|item| item.is_ok()) });

    tokio::time::sleep(Duration::from_secs(1)).await;
    let polls = snapshots.snapshots.load(Ordering::SeqCst);
    assert!(
        (40..=70).contains(&polls),
        "{polls} checks in the first second"
    );
    assert!(
        held_for(&clock).is_some_and(|held| held >= Duration::from_millis(990)),
        "the hold is recorded"
    );
    assert!(
        queue.lock().unwrap().is_empty(),
        "empty commits add nothing to pending, so the stream reads them all"
    );
    assert_eq!(snapshots.stalls.load(Ordering::SeqCst), 0);

    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        snapshots.stalls.load(Ordering::SeqCst),
        1,
        "one check at 5 s"
    );
    assert!(!reader.is_finished());

    snapshots.see(&[7]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(reader.await.unwrap(), Some(true), "emitted once visible");
    assert_eq!(held_for(&clock), None);
}

/// The read-ahead bound stops reading while the front transaction is held.
#[tokio::test(start_paused = true)]
async fn a_hold_reads_no_further_than_its_bound() {
    use futures::StreamExt;

    let (mut decode, _ack) = holding_state();
    transaction(&mut decode, 7, 100);
    transaction(&mut decode, 8, 200);
    let (client, queue, _) = script(vec![keepalive(300), keepalive(400)]);
    let mut stream = holding_stream(decode, client, Snapshots::default(), 2, Arc::default());
    let reader = tokio::spawn(async move { stream.next().await.is_some() });
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        queue.lock().unwrap().len(),
        2,
        "two pending changes fill the bound"
    );
    reader.abort();
}

/// A server that names flusso a synchronous standby while it holds a
/// transaction: the stall check stops the stream with the refusal.
#[tokio::test(start_paused = true)]
async fn a_hold_on_a_server_waiting_for_flusso_stops_the_stream() {
    use futures::StreamExt;

    let (mut decode, _ack) = holding_state();
    transaction(&mut decode, 7, 100);
    let (client, _, _) = script(Vec::new());
    let snapshots = Snapshots::default();
    snapshots
        .refuse
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut stream = holding_stream(decode, client, snapshots, 16, Arc::default());
    let item = tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("the stream stops within the stall check")
        .unwrap();
    let error = item.unwrap_err().to_string();
    assert!(error.contains("synchronous_standby_names"), "{error}");
}
