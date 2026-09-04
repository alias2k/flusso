use kernel::TableName;
use source::RowKey;

use super::*;

fn state() -> (DecodeState, Arc<Positions>) {
    let ack = Arc::new(Positions::new(0, 0));
    let decode = DecodeState {
        relations: HashMap::new(),
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
