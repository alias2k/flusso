//! Minimal decoder for the pgoutput logical replication protocol (v1).
//!
//! `pgwire-replication` decodes only the transaction-boundary messages
//! (`Begin`, `Commit`, `Message`) and hands every other message to us as raw
//! bytes inside [`ReplicationEvent::XLogData`]. This module decodes the ones we
//! care about — `Relation`, `Insert`, `Update`, `Delete`, `Truncate` — far
//! enough to recover a row's primary key. Column *values* are not needed:
//! events are thin (see [`sources_core::cdc::ChangeEvent`]), so we only extract the
//! key columns the [`Relation`] marks.
//!
//! [`ReplicationEvent::XLogData`]: pgwire_replication::ReplicationEvent::XLogData

use schema_core::{ColumnName, GenericValue, TableName};
use sources_core::{RowKey, SourceError};

/// A decoded pgoutput message — only the variants this source acts on.
#[derive(Debug)]
pub(crate) enum Decoded {
    /// Table metadata. Must be seen before any DML referencing its OID.
    Relation(Relation),
    Insert {
        rel: u32,
        new: Tuple,
    },
    Update {
        rel: u32,
        /// Old tuple, present only when `REPLICA IDENTITY` sends it (key or full).
        old: Option<Tuple>,
        new: Tuple,
    },
    Delete {
        rel: u32,
        old: Tuple,
    },
    /// `TRUNCATE` of the listed relation OIDs.
    Truncate {
        rels: Vec<u32>,
    },
    /// A message we don't act on (`Type`, `Origin`, …).
    Other,
}

/// Table metadata from a pgoutput `Relation` message.
#[derive(Debug, Clone)]
pub(crate) struct Relation {
    pub(crate) oid: u32,
    pub(crate) table: TableName,
    pub(crate) columns: Vec<Column>,
}

#[derive(Debug, Clone)]
pub(crate) struct Column {
    pub(crate) name: ColumnName,
    /// Part of the replica-identity key (the `flags & 1` bit).
    pub(crate) is_key: bool,
    /// The column's Postgres type OID, used to type its (text-encoded) value.
    pub(crate) type_oid: u32,
}

/// A row's column values, in `Relation` column order.
pub(crate) type Tuple = Vec<Cell>;

/// One column value within a [`Tuple`].
#[derive(Debug, Clone)]
pub(crate) enum Cell {
    /// SQL `NULL`.
    Null,
    /// An unchanged TOASTed value the server chose not to resend (`'u'`).
    Unchanged,
    /// A value, as the pgoutput text representation.
    Text(String),
}

/// Build a [`RowKey`] from a relation's key columns and a tuple.
///
/// pgoutput sends every value as text; we type each key value by its column's
/// OID (integer, boolean, …) so it binds against the real column type when the
/// document is re-read — a text-encoded `"1"` against an `integer` key would
/// otherwise be an `operator does not exist: integer = text` error.
///
/// Errors if the relation declares no key columns, which means the table's
/// `REPLICA IDENTITY` is `NOTHING` (or otherwise keyless) and changes cannot be
/// addressed — a configuration problem worth surfacing loudly.
pub(crate) fn row_key(rel: &Relation, tuple: &Tuple) -> Result<RowKey, SourceError> {
    let mut pairs = Vec::new();
    for (col, cell) in rel.columns.iter().zip(tuple.iter()) {
        if col.is_key {
            let value = match cell {
                Cell::Text(text) => typed_value(text, col.type_oid),
                Cell::Null | Cell::Unchanged => GenericValue::Null,
            };
            pairs.push((col.name.clone(), value));
        }
    }
    if pairs.is_empty() {
        return Err(SourceError::Decode(format!(
            "relation {} carries no key columns; set REPLICA IDENTITY so changes can be addressed",
            rel.table
        )));
    }
    Ok(RowKey(pairs))
}

/// Interpret a pgoutput text value by its Postgres type OID. Unknown or
/// unparseable types fall back to the text itself.
fn typed_value(text: &str, type_oid: u32) -> GenericValue {
    match type_oid {
        // bool
        16 => match text {
            "t" => GenericValue::Bool(true),
            "f" => GenericValue::Bool(false),
            _ => GenericValue::String(text.to_owned()),
        },
        // int2 / int4 / int8 / oid — a WAL value is a lookup key (re-bound, then
        // cast to its column type), so a single wide integer is enough here.
        21 | 23 | 20 | 26 => text.parse::<i64>().map_or_else(
            |_| GenericValue::String(text.to_owned()),
            GenericValue::BigInt,
        ),
        // float4 / float8 / numeric
        700 | 701 | 1700 => rust_decimal::Decimal::from_str_exact(text).map_or_else(
            |_| GenericValue::String(text.to_owned()),
            GenericValue::Decimal,
        ),
        // everything else (text, varchar, uuid, timestamps, …) stays text
        _ => GenericValue::String(text.to_owned()),
    }
}

/// Decode one raw pgoutput message.
pub(crate) fn decode(data: &[u8]) -> Result<Decoded, SourceError> {
    let (&tag, rest) = data
        .split_first()
        .ok_or_else(|| SourceError::Decode("pgoutput: empty message".into()))?;
    let mut cur = Cursor::new(rest);

    match tag {
        b'R' => decode_relation(&mut cur),
        b'I' => {
            let rel = cur.u32()?;
            expect(&mut cur, b'N', "insert new-tuple marker")?;
            Ok(Decoded::Insert {
                rel,
                new: decode_tuple(&mut cur)?,
            })
        }
        b'U' => {
            let rel = cur.u32()?;
            let marker = cur.u8()?;
            let (old, new) = match marker {
                b'K' | b'O' => {
                    let old = decode_tuple(&mut cur)?;
                    expect(&mut cur, b'N', "update new-tuple marker")?;
                    (Some(old), decode_tuple(&mut cur)?)
                }
                b'N' => (None, decode_tuple(&mut cur)?),
                other => {
                    return Err(SourceError::Decode(format!(
                        "pgoutput update: unexpected tuple marker {other:#x}"
                    )));
                }
            };
            Ok(Decoded::Update { rel, old, new })
        }
        b'D' => {
            let rel = cur.u32()?;
            let marker = cur.u8()?;
            match marker {
                b'K' | b'O' => {}
                other => {
                    return Err(SourceError::Decode(format!(
                        "pgoutput delete: unexpected tuple marker {other:#x}"
                    )));
                }
            }
            Ok(Decoded::Delete {
                rel,
                old: decode_tuple(&mut cur)?,
            })
        }
        b'T' => {
            let nrels = cur.i16_count()?;
            let _flags = cur.u8()?;
            let mut rels = Vec::with_capacity(nrels);
            for _ in 0..nrels {
                rels.push(cur.u32()?);
            }
            Ok(Decoded::Truncate { rels })
        }
        _ => Ok(Decoded::Other),
    }
}

fn decode_relation(cur: &mut Cursor<'_>) -> Result<Decoded, SourceError> {
    let oid = cur.u32()?;
    let _namespace = cur.cstring()?;
    let relname = cur.cstring()?;
    let table = TableName::try_new(relname.clone()).map_err(|e| {
        SourceError::Decode(format!("pgoutput relation: invalid table {relname:?}: {e}"))
    })?;
    let _replica_identity = cur.u8()?;
    let ncols = cur.i16_count()?;
    let mut columns = Vec::with_capacity(ncols);
    for _ in 0..ncols {
        let flags = cur.u8()?;
        let colname = cur.cstring()?;
        let type_oid = cur.u32()?;
        let _type_modifier = cur.u32()?;
        let name = ColumnName::try_new(colname.clone()).map_err(|e| {
            SourceError::Decode(format!(
                "pgoutput relation: invalid column {colname:?}: {e}"
            ))
        })?;
        columns.push(Column {
            name,
            is_key: (flags & 1) != 0,
            type_oid,
        });
    }
    Ok(Decoded::Relation(Relation {
        oid,
        table,
        columns,
    }))
}

fn decode_tuple(cur: &mut Cursor<'_>) -> Result<Tuple, SourceError> {
    let ncols = cur.i16_count()?;
    let mut cells = Vec::with_capacity(ncols);
    for _ in 0..ncols {
        let kind = cur.u8()?;
        let cell = match kind {
            b'n' => Cell::Null,
            b'u' => Cell::Unchanged,
            // 't' text or 'b' binary. We request text (proto v1 default); a
            // binary value, if it ever appears, is rendered lossily.
            b't' | b'b' => {
                let len = cur.i32_len()?;
                Cell::Text(String::from_utf8_lossy(cur.take(len)?).into_owned())
            }
            other => {
                return Err(SourceError::Decode(format!(
                    "pgoutput tuple: unknown cell kind {other:#x}"
                )));
            }
        };
        cells.push(cell);
    }
    Ok(cells)
}

fn expect(cur: &mut Cursor<'_>, want: u8, what: &str) -> Result<(), SourceError> {
    let got = cur.u8()?;
    if got == want {
        Ok(())
    } else {
        Err(SourceError::Decode(format!(
            "pgoutput: expected {what} {want:#x}, got {got:#x}"
        )))
    }
}

/// A forward-only reader over a byte slice. Every read is bounds-checked via
/// `get`, so it can never panic (the workspace denies `indexing_slicing`).
struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], SourceError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| truncated("length overflow"))?;
        let slice = self
            .buf
            .get(self.pos..end)
            .ok_or_else(|| truncated("bytes"))?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, SourceError> {
        let byte = self
            .buf
            .get(self.pos)
            .copied()
            .ok_or_else(|| truncated("u8"))?;
        self.pos += 1;
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32, SourceError> {
        let arr: [u8; 4] = self.take(4)?.try_into().map_err(|_| truncated("u32"))?;
        Ok(u32::from_be_bytes(arr))
    }

    fn i32_len(&mut self) -> Result<usize, SourceError> {
        let arr: [u8; 4] = self.take(4)?.try_into().map_err(|_| truncated("i32"))?;
        Ok(i32::from_be_bytes(arr).max(0) as usize)
    }

    /// Read an `Int16` element count, clamped to a non-negative `usize`.
    fn i16_count(&mut self) -> Result<usize, SourceError> {
        let arr: [u8; 2] = self.take(2)?.try_into().map_err(|_| truncated("i16"))?;
        Ok(i16::from_be_bytes(arr).max(0) as usize)
    }

    fn cstring(&mut self) -> Result<String, SourceError> {
        let rest = self
            .buf
            .get(self.pos..)
            .ok_or_else(|| truncated("cstring"))?;
        let nul = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| SourceError::Decode("pgoutput: unterminated cstring".into()))?;
        let text = rest.get(..nul).ok_or_else(|| truncated("cstring"))?;
        let out = String::from_utf8_lossy(text).into_owned();
        self.pos += nul + 1;
        Ok(out)
    }
}

fn truncated(what: &str) -> SourceError {
    SourceError::Decode(format!("pgoutput: truncated {what}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
