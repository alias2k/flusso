//! Which old list item each new list item is.
//!
//! A comment follows the thing it describes, so the patcher has to know, for
//! every item of a list it rewrites, which item of the file it came from. The
//! designer knows: it keeps a stable id on every field and on every
//! `[[index]]` entry, and sends the ids of the model the file was read as (the
//! **before**) next to the ids of the model being saved (the **after**). Ids
//! never reach the schema; they only say which is which.
//!
//! Sinks are keyed by name, so their identity is a map: every sink being
//! saved, by its new name, to its name in the file.
//!
//! Without them, or when the file no longer reads as the before (it changed on
//! disk), or when they are malformed (an empty or repeated id, a position or a
//! name the file does not have), items pair by name only. An item matching
//! nothing is written fresh, and an old item matching nothing is deleted with
//! its comments. Nothing is paired by its slot.

use std::collections::{BTreeMap, HashMap, HashSet};

use kernel::{Field, FieldSource, Relation};
use serde::Deserialize;

/// The ids of a schema's fields before and after an edit (see the module docs).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SchemaIdentity {
    /// The fields of the model the base text reads as.
    pub before: Vec<FieldId>,
    /// The fields of the model being saved.
    pub after: Vec<FieldId>,
}

/// One field's name and id, with its child fields.
#[derive(Debug, Clone, Deserialize)]
pub struct FieldId {
    /// The field's document name.
    pub name: String,
    /// The designer's stable id for it.
    pub uid: String,
    /// Its child fields (a join or an object), else empty.
    #[serde(default)]
    pub fields: Vec<FieldId>,
}

/// The `[[index]]` entries and the sinks before and after an edit.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigIdentity {
    /// The index names of the config the file reads as, in file order.
    pub before: Vec<String>,
    /// For every index entry being saved, its position in `before`, or `None`
    /// for a new entry.
    pub after: Vec<Option<usize>>,
    /// For every sink being saved, by its name, its name in the file, or
    /// `None` for a new sink. Absent, sinks pair by name.
    #[serde(default)]
    pub sinks: Option<BTreeMap<String, Option<String>>>,
}

/// What the sinks' names do in a save.
#[derive(Debug, Clone, Default)]
pub(super) struct SinkNames {
    /// A sink's name in the file to the name it is saved under. A sink the
    /// identity says was removed, whose name a saved sink takes, is renamed
    /// out of the way, so it goes with its comments instead of lending them.
    pub(super) renames: HashMap<String, String>,
    /// The names of removed sinks that may still exist under another name
    /// (no identity): their comments are reported as lost.
    pub(super) doubtful: HashSet<String>,
}

/// How the sinks named `old` in the file become the sinks named `new`.
pub(super) fn sinks(
    identity: Option<&ConfigIdentity>,
    old: &[String],
    new: &[String],
) -> SinkNames {
    match identity.and_then(|i| i.sinks.as_ref()) {
        Some(map) if describes_sinks(map, old, new) => {
            let mut renames: HashMap<String, String> = map
                .iter()
                .filter_map(|(is, was)| Some((was.clone()?, is.clone())))
                .collect();
            let mut spare = 0;
            for was in old {
                if !renames.contains_key(was) && new.contains(was) {
                    let placeholder = loop {
                        spare += 1;
                        let name = format!("flusso-removed-sink-{spare}");
                        if !old.contains(&name) && !new.contains(&name) {
                            break name;
                        }
                    };
                    renames.insert(was.clone(), placeholder);
                }
            }
            SinkNames {
                renames,
                doubtful: HashSet::new(),
            }
        }
        _ => {
            let gained = new.iter().any(|is| !old.contains(is));
            SinkNames {
                renames: HashMap::new(),
                doubtful: old
                    .iter()
                    .filter(|was| gained && !new.contains(was))
                    .cloned()
                    .collect(),
            }
        }
    }
}

/// Whether `map` names every new sink once and only old sinks, each once.
fn describes_sinks(map: &BTreeMap<String, Option<String>>, old: &[String], new: &[String]) -> bool {
    let olds: Vec<&String> = map.values().flatten().collect();
    let distinct: HashSet<&String> = olds.iter().copied().collect();
    map.len() == new.len()
        && new.iter().all(|is| map.contains_key(is))
        && distinct.len() == olds.len()
        && olds.iter().all(|was| old.contains(was))
}

/// How the items of one list pair up, and how their own child lists do.
#[derive(Debug, Clone, Default)]
pub(super) struct Pairing {
    /// For each new item, the old item it is.
    pub(super) pairs: Vec<Option<usize>>,
    /// For each old item: whether, dropped, it may still exist (renamed, or
    /// moved to another list), so its comments are reported as lost.
    pub(super) doubtful: Vec<bool>,
    /// For each new item, how its child fields pair (empty: pair by name).
    pub(super) children: Vec<Pairing>,
}

impl Pairing {
    /// Pair `new` items to `old` ones with `same`, first match wins.
    pub(super) fn matching<O, N>(old: &[O], new: &[N], same: impl Fn(&O, &N) -> bool) -> Self {
        let mut used = vec![false; old.len()];
        let pairs: Vec<Option<usize>> =
            new.iter()
                .map(|is| {
                    let found = old.iter().enumerate().position(|(i, was)| {
                        !used.get(i).copied().unwrap_or(true) && same(was, is)
                    });
                    if let Some(flag) = found.and_then(|i| used.get_mut(i)) {
                        *flag = true;
                    }
                    found
                })
                .collect();
        let unmatched_new = pairs.iter().any(Option::is_none);
        Self {
            doubtful: used.iter().map(|u| !u && unmatched_new).collect(),
            pairs,
            children: Vec::new(),
        }
    }

    /// The pairing of the child list of the new item `j`, if known.
    pub(super) fn child(&self, j: usize) -> Option<&Pairing> {
        self.children.get(j)
    }
}

/// The fields' pairing from `identity`, when it describes `old` and `new`.
pub(super) fn fields(
    identity: Option<&SchemaIdentity>,
    old: &[Field],
    new: &[Field],
) -> Option<Pairing> {
    let identity = identity?;
    if !describes(&identity.before, old)
        || !describes(&identity.after, new)
        || !distinct_ids(&identity.before)
        || !distinct_ids(&identity.after)
    {
        return None;
    }
    let mut everywhere = HashSet::new();
    collect(&identity.after, &mut everywhere);
    Some(by_uid(&identity.before, &identity.after, &everywhere))
}

/// The `[[index]]` pairing from `identity`, when it describes `old` and `new`.
pub(super) fn indexes(
    identity: Option<&ConfigIdentity>,
    old: &[String],
    new: usize,
) -> Option<Pairing> {
    let identity = identity?;
    let paired: Vec<usize> = identity.after.iter().flatten().copied().collect();
    let distinct: HashSet<usize> = paired.iter().copied().collect();
    if identity.before != old
        || identity.after.len() != new
        || paired.iter().any(|&i| i >= old.len())
        || distinct.len() != paired.len()
    {
        return None;
    }
    Some(Pairing {
        pairs: identity.after.clone(),
        doubtful: vec![false; old.len()],
        children: Vec::new(),
    })
}

fn children(field: &Field) -> &[Field] {
    match &field.source {
        FieldSource::Group(fields) => fields,
        FieldSource::Relation(Relation::Join(join)) => &join.fields,
        _ => &[],
    }
}

fn describes(ids: &[FieldId], fields: &[Field]) -> bool {
    ids.len() == fields.len()
        && ids.iter().zip(fields).all(|(id, field)| {
            id.name == field.field.as_ref() && describes(&id.fields, children(field))
        })
}

/// Whether every id in the tree is non-empty and used once.
fn distinct_ids(ids: &[FieldId]) -> bool {
    fn walk<'a>(ids: &'a [FieldId], seen: &mut HashSet<&'a str>) -> bool {
        ids.iter()
            .all(|id| !id.uid.is_empty() && seen.insert(&id.uid) && walk(&id.fields, seen))
    }
    walk(ids, &mut HashSet::new())
}

fn collect<'a>(ids: &'a [FieldId], into: &mut HashSet<&'a str>) {
    for id in ids {
        into.insert(&id.uid);
        collect(&id.fields, into);
    }
}

fn by_uid(before: &[FieldId], after: &[FieldId], everywhere: &HashSet<&str>) -> Pairing {
    let mut pairing = Pairing::matching(before, after, |was, is| was.uid == is.uid);
    pairing.doubtful = before
        .iter()
        .enumerate()
        .map(|(i, was)| !pairing.pairs.contains(&Some(i)) && everywhere.contains(was.uid.as_str()))
        .collect();
    pairing.children = after
        .iter()
        .zip(&pairing.pairs)
        .map(|(is, pair)| {
            let was = pair
                .and_then(|i| before.get(i))
                .map_or(&[][..], |b| &b.fields);
            by_uid(was, &is.fields, everywhere)
        })
        .collect();
    pairing
}
