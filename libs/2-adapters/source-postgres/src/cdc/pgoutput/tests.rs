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
    assert_eq!(key.0[0].1, GenericValue::Int(42)); // id is int4 (oid 23)
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
    assert_eq!(key.0[0].1, GenericValue::Int(42)); // id is int4 (oid 23)
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

/// Encode a `Relation` for `public.child(id PK int4, parent_id int4, label text)`
/// under `identity`, flagging the columns pgoutput would for it.
fn child_relation_message(identity: u8) -> Vec<u8> {
    let full = identity == b'f';
    let mut m = vec![b'R'];
    m.extend_from_slice(&16385u32.to_be_bytes());
    m.extend_from_slice(b"public\0");
    m.extend_from_slice(b"child\0");
    m.push(identity);
    m.extend_from_slice(&3i16.to_be_bytes());
    for (name, oid, identity_column) in [
        ("id", 23u32, true),
        ("parent_id", 23, full),
        ("label", 25, full),
    ] {
        m.push(u8::from(identity_column));
        m.extend_from_slice(name.as_bytes());
        m.push(0);
        m.extend_from_slice(&oid.to_be_bytes());
        m.extend_from_slice(&(-1i32).to_be_bytes());
    }
    m
}

/// A child relation as the stream holds it: decoded, its primary key looked up.
fn keyed_child(identity: u8) -> Relation {
    let Decoded::Relation(mut rel) = decode(&child_relation_message(identity)).unwrap() else {
        panic!("expected Relation");
    };
    rel.primary_key = vec![ColumnName::try_new("id").unwrap()];
    rel
}

fn child_delete(marker: u8, cells: [Vec<u8>; 3]) -> Tuple {
    let mut msg = vec![b'D'];
    msg.extend_from_slice(&16385u32.to_be_bytes());
    msg.push(marker);
    msg.extend_from_slice(&3i16.to_be_bytes());
    for cell in cells {
        msg.extend(cell);
    }
    let Decoded::Delete { old, .. } = decode(&msg).unwrap() else {
        panic!("expected Delete");
    };
    old
}

#[test]
fn full_identity_keys_by_the_primary_key_and_carries_the_pre_image() {
    let rel = keyed_child(b'f');
    assert!(rel.columns.iter().all(|column| column.is_key));
    let old = child_delete(b'O', [text_cell("7"), text_cell("1"), text_cell("x")]);

    let key = row_key(&rel, &old).unwrap();
    assert_eq!(
        key.0,
        vec![(ColumnName::try_new("id").unwrap(), GenericValue::Int(7))]
    );

    let image = pre_image(&rel, &old).unwrap();
    assert_eq!(
        image.get(&ColumnName::try_new("parent_id").unwrap()),
        Some(&GenericValue::Int(1))
    );
    assert_eq!(image.0.len(), 3);
}

#[test]
fn default_identity_carries_no_pre_image() {
    let rel = keyed_child(b'd');
    let old = child_delete(b'K', [text_cell("7"), vec![b'n'], vec![b'n']]);
    assert_eq!(row_key(&rel, &old).unwrap().0.len(), 1);
    assert_eq!(pre_image(&rel, &old), None);
}

#[test]
fn an_unchanged_toast_value_is_left_out_of_the_pre_image() {
    let rel = keyed_child(b'f');
    let old = child_delete(b'O', [text_cell("7"), text_cell("1"), vec![b'u']]);
    let image = pre_image(&rel, &old).unwrap();
    assert_eq!(image.get(&ColumnName::try_new("label").unwrap()), None);
}

#[test]
fn a_table_without_a_primary_key_is_keyed_by_its_identity_columns() {
    let Decoded::Relation(rel) = decode(&child_relation_message(b'f')).unwrap() else {
        panic!("expected Relation");
    };
    let old = child_delete(b'O', [text_cell("7"), text_cell("1"), text_cell("x")]);
    assert_eq!(row_key(&rel, &old).unwrap().0.len(), 3);
    assert_eq!(pre_image(&rel, &old), None);
}

#[test]
fn the_post_image_carries_the_new_values() {
    let rel = keyed_child(b'f');
    let old = child_delete(b'O', [text_cell("7"), text_cell("1"), text_cell("x")]);
    let new = child_delete(b'O', [text_cell("7"), text_cell("3"), vec![b'n']]);
    let image = post_image(&rel, &old, &new);
    assert_eq!(
        image.0,
        vec![
            (ColumnName::try_new("id").unwrap(), GenericValue::Int(7)),
            (
                ColumnName::try_new("parent_id").unwrap(),
                GenericValue::Int(3)
            ),
            (ColumnName::try_new("label").unwrap(), GenericValue::Null),
        ]
    );
}

#[test]
fn an_unchanged_toast_value_takes_the_old_value_in_the_post_image() {
    let rel = keyed_child(b'f');
    let old = child_delete(b'O', [text_cell("7"), text_cell("1"), text_cell("x")]);
    let new = child_delete(b'O', [text_cell("7"), text_cell("1"), vec![b'u']]);
    let label = ColumnName::try_new("label").unwrap();
    assert_eq!(
        post_image(&rel, &old, &new).get(&label),
        pre_image(&rel, &old).unwrap().get(&label)
    );
}

#[test]
fn an_unchanged_toast_value_outside_the_identity_is_left_out_of_the_post_image() {
    // `USING INDEX` on `(id, parent_id)`: `label`'s old cell is a placeholder.
    let mut rel = keyed_child(b'i');
    rel.columns
        .iter_mut()
        .for_each(|column| column.is_key = column.name.as_ref() != "label");
    let old = child_delete(b'K', [text_cell("7"), text_cell("1"), vec![b'n']]);
    let new = child_delete(b'K', [text_cell("7"), text_cell("1"), vec![b'u']]);
    let image = post_image(&rel, &old, &new);
    assert_eq!(image.get(&ColumnName::try_new("label").unwrap()), None);
    assert_eq!(image.0.len(), 2);
}

/// A WAL value decodes into the variant the document builder reads the column
/// back as (`decode_column`), so a key of that type addresses its document
/// (issue #295). Text a type can't parse stays text.
#[test]
fn a_value_decodes_as_the_document_builder_reads_it_back() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    let at = date.and_hms_milli_opt(10, 0, 0, 500).unwrap();
    for (oid, text, value) in [
        (17, "\\x01ab", GenericValue::Bytes(vec![0x01, 0xab])),
        (17, "\\x", GenericValue::Bytes(Vec::new())),
        (1082, "2024-02-29", GenericValue::Date(date)),
        (1083, "10:00:00.5", GenericValue::Time(at.time())),
        (1114, "2024-02-29 10:00:00.5", GenericValue::Timestamp(at)),
        (
            1184,
            "2024-02-29 12:00:00.5+02",
            GenericValue::TimestampTz(Utc.from_utc_datetime(&at)),
        ),
        (
            1184,
            "2024-02-29 15:30:00.5+05:30",
            GenericValue::TimestampTz(Utc.from_utc_datetime(&at)),
        ),
        (17, "\\x1", GenericValue::String("\\x1".into())),
        (1082, "infinity", GenericValue::String("infinity".into())),
        (1184, "-infinity", GenericValue::String("-infinity".into())),
    ] {
        assert_eq!(typed_value(text, oid), value, "{oid} {text}");
    }
}
