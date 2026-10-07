use kernel::{GenericValue, TableName};
use source::RowKey;

use super::*;

fn state() -> (DecodeState, Arc<Positions>) {
    let ack = Arc::new(Positions::new(0, 0));
    let decode = DecodeState {
        relations: HashMap::new(),
        unkeyed: Vec::new(),
        open_txn: Vec::new(),
        pending: VecDeque::new(),
        ack: Arc::clone(&ack),
        done: false,
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
    decode.pending.push_back((upsert(), 50));
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
    assert_eq!(ack.confirmed_lsn(), 50);
    ack.confirm(seq + 1);
    assert_eq!(ack.confirmed_lsn(), 100, "the queued keepalive follows");
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
    decode.pending.push_back((upsert(), 50));
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(ack.confirmed_lsn(), 0);
}

#[test]
fn non_empty_commit_queues_changes_without_advancing() {
    let (mut decode, ack) = state();
    decode.open_txn.push(upsert());
    handle(&mut decode, commit(100)).unwrap();
    assert_eq!(decode.pending.len(), 1);
    assert_eq!(decode.pending.front().map(|(_, lsn)| *lsn), Some(100));
    assert_eq!(
        ack.confirmed_lsn(),
        0,
        "advance waits for the engine confirm"
    );
}

/// A pgoutput `Relation` for `public.child(id PK int4, parent_id int4)` under
/// `REPLICA IDENTITY FULL` — both columns flagged.
fn full_child_relation() -> Vec<u8> {
    let mut m = vec![b'R'];
    m.extend_from_slice(&16385u32.to_be_bytes());
    m.extend_from_slice(b"public\0child\0");
    m.push(b'f');
    m.extend_from_slice(&2i16.to_be_bytes());
    for name in ["id", "parent_id"] {
        m.push(1);
        m.extend_from_slice(name.as_bytes());
        m.push(0);
        m.extend_from_slice(&23u32.to_be_bytes());
        m.extend_from_slice(&(-1i32).to_be_bytes());
    }
    m
}

fn text(value: &str) -> Vec<u8> {
    let mut c = vec![b't'];
    c.extend_from_slice(&(value.len() as i32).to_be_bytes());
    c.extend_from_slice(value.as_bytes());
    c
}

/// Announce the child relation and key it by `id`, as the stream loop does.
fn announce_child(decode: &mut DecodeState) {
    handle_xlog(decode, &full_child_relation()).unwrap();
    assert_eq!(decode.unkeyed, vec![16385]);
    decode.unkeyed.clear();
    decode.relations.get_mut(&16385).unwrap().primary_key =
        vec![kernel::ColumnName::try_new("id").unwrap()];
}

fn column(name: &str) -> kernel::ColumnName {
    kernel::ColumnName::try_new(name).unwrap()
}

#[test]
fn a_full_identity_delete_is_keyed_by_id_and_carries_the_old_parent() {
    let (mut decode, _) = state();
    announce_child(&mut decode);

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
    announce_child(&mut decode);

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

    let [ChangeEvent::Upsert { key, before, .. }] = decode.open_txn.as_slice() else {
        panic!("expected one upsert, got {:?}", decode.open_txn);
    };
    assert_eq!(key.0, vec![(column("id"), GenericValue::Int(7))]);
    assert_eq!(
        before.as_ref().unwrap().get(&column("parent_id")),
        Some(&GenericValue::Int(1))
    );
}
