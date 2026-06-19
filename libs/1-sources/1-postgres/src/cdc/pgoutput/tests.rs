use super::*;

/// Encode a pgoutput `Relation` for `public.users(id PK, email)`.
fn relation_message() -> Vec<u8> {
    let mut m = vec![b'R'];
    m.extend_from_slice(&16384u32.to_be_bytes()); // oid
    m.extend_from_slice(b"public\0");
    m.extend_from_slice(b"users\0");
    m.push(b'd'); // replica identity default
    m.extend_from_slice(&2i16.to_be_bytes()); // 2 columns
    // id: key
    m.push(1);
    m.extend_from_slice(b"id\0");
    m.extend_from_slice(&23u32.to_be_bytes()); // int4 oid
    m.extend_from_slice(&(-1i32).to_be_bytes()); // typmod
    // email: not key
    m.push(0);
    m.extend_from_slice(b"email\0");
    m.extend_from_slice(&25u32.to_be_bytes()); // text oid
    m.extend_from_slice(&(-1i32).to_be_bytes());
    m
}

fn text_cell(value: &str) -> Vec<u8> {
    let mut c = vec![b't'];
    c.extend_from_slice(&(value.len() as i32).to_be_bytes());
    c.extend_from_slice(value.as_bytes());
    c
}

#[test]
fn decodes_relation_and_marks_key() {
    let Decoded::Relation(rel) = decode(&relation_message()).unwrap() else {
        panic!("expected Relation");
    };
    assert_eq!(rel.oid, 16384);
    assert_eq!(rel.table.as_ref(), "users");
    assert_eq!(rel.columns.len(), 2);
    assert!(rel.columns[0].is_key);
    assert!(!rel.columns[1].is_key);
}

#[test]
fn insert_yields_only_key_columns() {
    let Decoded::Relation(rel) = decode(&relation_message()).unwrap() else {
        panic!("expected Relation");
    };

    let mut msg = vec![b'I'];
    msg.extend_from_slice(&16384u32.to_be_bytes());
    msg.push(b'N');
    msg.extend_from_slice(&2i16.to_be_bytes());
    msg.extend(text_cell("42"));
    msg.extend(text_cell("a@b.com"));

    let Decoded::Insert { rel: oid, new } = decode(&msg).unwrap() else {
        panic!("expected Insert");
    };
    assert_eq!(oid, 16384);

    let key = row_key(&rel, &new).unwrap();
    assert_eq!(key.0.len(), 1);
    assert_eq!(key.0[0].0.as_ref(), "id");
    assert_eq!(key.0[0].1, GenericValue::BigInt(42)); // id is int4 (oid 23)
}

#[test]
fn delete_uses_old_key_tuple() {
    let Decoded::Relation(rel) = decode(&relation_message()).unwrap() else {
        panic!("expected Relation");
    };

    let mut msg = vec![b'D'];
    msg.extend_from_slice(&16384u32.to_be_bytes());
    msg.push(b'K');
    msg.extend_from_slice(&2i16.to_be_bytes());
    msg.extend(text_cell("42"));
    msg.push(b'n'); // email null in key-only old tuple

    let Decoded::Delete { old, .. } = decode(&msg).unwrap() else {
        panic!("expected Delete");
    };
    let key = row_key(&rel, &old).unwrap();
    assert_eq!(key.0[0].1, GenericValue::BigInt(42)); // id is int4 (oid 23)
}

#[test]
fn truncated_message_errors_without_panicking() {
    let mut msg = vec![b'I'];
    msg.extend_from_slice(&16384u32.to_be_bytes());
    // missing 'N' marker and tuple
    assert!(matches!(decode(&msg), Err(SourceError::Decode(_))));
}

#[test]
fn unknown_tag_is_other() {
    assert!(matches!(decode(b"Y\0\0").unwrap(), Decoded::Other));
}
