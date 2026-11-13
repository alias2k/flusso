//! Patch a `flusso.toml` towards a new config with `toml_edit`, keeping its
//! comments.
//!
//! The same three-way rule as the YAML side: the file's document, the table its
//! config serializes to (`old`), and the table the edited config serializes to
//! (`new`). A key whose value did not change is left as written (with its
//! comments and spacing), a changed table is patched key by key, a changed
//! value is replaced keeping its trailing comment, and `[[index]]` entries pair
//! by [`Pairing`].
//!
//! **Who owns a comment.** `toml_edit` keeps every comment before a header in
//! that header's prefix, so the patcher splits it the way the YAML side splits
//! its trivia:
//!
//! - the comments before the first key or header are the file's header, and
//!   stay first;
//! - the comment block directly above a header or a key, with no blank line
//!   between, is that table's or key's;
//! - the comment block directly below a table's last key, with no blank line
//!   between, is that table's: it moves and goes with it;
//! - any other comment is nobody's: it stays where it is, before whatever
//!   follows it.
//!
//! To place them once tables were removed, added or reordered, every header is
//! tagged before the patch with a marker in its prefix; the order the markers
//! come out in after the patch says what survived and where, and the prefixes
//! are rebuilt from the split pieces.

use std::collections::{HashMap, HashSet};

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

use super::pairing::Pairing;
use super::yaml::Patched;

const MARK: &str = "#<<flusso-keep ";

/// `text` patched from `old` to `new`, or `None` when it does not parse.
/// `indexes` pairs the `[[index]]` entries; without it they pair by name.
pub(super) fn patch(
    text: &str,
    old: &toml::Table,
    new: &toml::Table,
    indexes: Option<&Pairing>,
) -> Option<Patched> {
    if text.contains(MARK) {
        return None;
    }
    let mut doc: DocumentMut = text.parse().ok()?;
    let header = take_header(&mut doc)?;
    let mut stash = Vec::new();
    mark(doc.as_table_mut(), &mut stash)?;
    let before = order(&doc.to_string());
    let trailing = doc.trailing().as_str()?.to_owned();

    let mut patcher = Patcher {
        tails: HashMap::new(),
        doubtful: HashMap::new(),
        dropped: 0,
    };
    patcher.table(doc.as_table_mut(), None, old, new, false, indexes)?;

    mark(doc.as_table_mut(), &mut stash)?;
    let after = order(&doc.to_string());
    let root_values = doc.as_table().iter().any(|(_, item)| item.is_value());
    let prefixes = patcher.place(&stash, &before, &after, &trailing, root_values)?;
    for (id, prefix) in prefixes.headers {
        set_prefix(doc.as_table_mut(), id, &prefix);
    }
    doc.set_trailing(prefixes.trailing);
    Some(Patched {
        text: format!("{header}{doc}"),
        dropped: patcher.dropped,
    })
}

/// Remove and return the file's header: everything before the first key or
/// header. Without it a new top-level key would be written above it.
fn take_header(doc: &mut DocumentMut) -> Option<String> {
    let first_key = doc
        .as_table()
        .iter()
        .find(|(_, item)| item.is_value())
        .map(|(k, _)| k.to_owned());
    if let Some(key) = first_key {
        let mut key = doc.as_table_mut().key_mut(&key)?;
        let decor = key.leaf_decor_mut();
        let header = decor.prefix().map_or(Some(""), |p| p.as_str())?.to_owned();
        decor.set_prefix("");
        return Some(header);
    }
    let mut first: Option<isize> = None;
    for_each_header(doc.as_table_mut(), &mut |table| {
        if let Some(at) = table.position() {
            first = Some(first.map_or(at, |was| was.min(at)));
        }
    });
    let mut header: Option<Option<String>> = None;
    for_each_header(doc.as_table_mut(), &mut |table| {
        if header.is_none() && first.is_some() && table.position() == first {
            header = Some(
                table
                    .decor()
                    .prefix()
                    .map_or(Some(""), |p| p.as_str())
                    .map(str::to_owned),
            );
            table.decor_mut().set_prefix("");
        }
    });
    header.unwrap_or(Some(String::new()))
}

/// Tag every header without a marker with the next id, stashing its prefix.
fn mark(root: &mut Table, stash: &mut Vec<Option<String>>) -> Option<()> {
    let mut failed = false;
    let mut tag = |table: &mut Table| {
        let prefix = table.decor().prefix().map(|p| p.as_str());
        if prefix.flatten().is_some_and(|p| p.starts_with(MARK)) {
            return;
        }
        let original = match prefix {
            None => None,
            Some(Some(p)) => Some(p.to_owned()),
            Some(None) => {
                failed = true;
                None
            }
        };
        table
            .decor_mut()
            .set_prefix(format!("{MARK}{}>>\n", stash.len()));
        stash.push(original);
    };
    for_each_header(root, &mut tag);
    (!failed).then_some(())
}

/// Call `f` on every table that has a header line, depth first.
fn for_each_header(table: &mut Table, f: &mut dyn FnMut(&mut Table)) {
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(table) => {
                if !table.is_implicit() {
                    f(table);
                }
                for_each_header(table, f);
            }
            Item::ArrayOfTables(array) => {
                for table in array.iter_mut() {
                    f(table);
                    for_each_header(table, f);
                }
            }
            _ => {}
        }
    }
}

fn set_prefix(root: &mut Table, id: usize, prefix: &str) {
    let mark = format!("{MARK}{id}>>\n");
    for_each_header(root, &mut |table| {
        if table.decor().prefix().and_then(|p| p.as_str()) == Some(mark.as_str()) {
            table.decor_mut().set_prefix(prefix);
        }
    });
}

/// The marker ids in the order a rendering prints them.
fn order(text: &str) -> Vec<usize> {
    text.lines()
        .filter_map(|l| l.strip_prefix(MARK)?.strip_suffix(">>")?.parse().ok())
        .collect()
}

/// A header's prefix split by owner.
#[derive(Debug, Default, Clone)]
struct Split {
    /// The comment lines directly below the previous table: its trailing.
    below: String,
    /// Nobody's: stays where it is.
    free: String,
    /// The comment lines directly above the header: the header's.
    lead: String,
}

fn split(prefix: &str) -> Split {
    let lines: Vec<&str> = prefix.split_inclusive('\n').collect();
    let blank = |l: &&str| l.trim().is_empty();
    let lead_len = lines.iter().rev().take_while(|l| !blank(l)).count();
    let (rest, lead) = lines.split_at(lines.len() - lead_len);
    let below_len = rest.iter().take_while(|l| !blank(l)).count();
    let (below, free) = rest.split_at(below_len);
    Split {
        below: below.concat(),
        free: free.concat(),
        lead: lead.concat(),
    }
}

/// `free` appended to `pending` without doubling the blank line between them.
fn join_free(pending: &mut String, free: &str) {
    let mut skipping = pending.ends_with("\n\n") || pending == "\n";
    for line in free.split_inclusive('\n') {
        skipping &= line.trim().is_empty();
        if !skipping {
            pending.push_str(line);
        }
    }
}

fn has_comment(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with('#'))
}

fn count_comments(text: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .count()
}

struct Prefixes {
    headers: Vec<(usize, String)>,
    trailing: String,
}

struct Patcher {
    /// Free comments of removed keys with no key after them, per table (`None`
    /// is the root): they go after the table's last key.
    tails: HashMap<Option<usize>, String>,
    /// Removed `[[index]]` entries that may still exist, with the comments
    /// inside them.
    doubtful: HashMap<usize, usize>,
    dropped: usize,
}

impl Patcher {
    fn table(
        &mut self,
        doc: &mut dyn TableLike,
        id: Option<usize>,
        old: &toml::Table,
        new: &toml::Table,
        inline: bool,
        indexes: Option<&Pairing>,
    ) -> Option<()> {
        let keys: Vec<String> = doc.iter().map(|(k, _)| k.to_owned()).collect();
        let mut pending = String::new();
        for key in &keys {
            let before = old.get(key);
            let removed = new.get(key).is_none() && before.is_some();
            if !inline && doc.get(key).is_some_and(Item::is_value) {
                let mut handle = doc.key_mut(key)?;
                let decor = handle.leaf_decor_mut();
                let prefix = decor.prefix().map_or(Some(""), |p| p.as_str())?.to_owned();
                let parts = split(&prefix);
                if removed {
                    if has_comment(&parts.free) || has_comment(&parts.below) {
                        pending.push_str(&parts.below);
                        pending.push_str(&parts.free);
                    }
                } else if !pending.is_empty() {
                    decor.set_prefix(format!("{}{prefix}", std::mem::take(&mut pending)));
                }
            }
            match new.get(key) {
                Some(after) if before == Some(after) => {}
                Some(after) => {
                    let sub = doc.get_mut(key)?;
                    self.item(sub, key, before, after, indexes.filter(|_| key == "index"))?;
                }
                None if before.is_some() => {
                    doc.remove(key);
                }
                None => {}
            }
        }
        if !pending.is_empty() {
            self.tails.entry(id).or_default().push_str(&pending);
        }
        for (key, after) in new {
            if !keys.contains(key) && old.get(key) != Some(after) {
                let item = to_item(key, after)?;
                let item = if inline {
                    Item::Value(item.into_value().ok()?)
                } else {
                    item
                };
                doc.insert(key, item);
            }
        }
        Some(())
    }

    fn item(
        &mut self,
        item: &mut Item,
        key: &str,
        before: Option<&toml::Value>,
        after: &toml::Value,
        indexes: Option<&Pairing>,
    ) -> Option<()> {
        match (item, before, after) {
            (Item::Table(table), Some(toml::Value::Table(was)), toml::Value::Table(is)) => {
                let id = mark_id(table);
                self.table(table, id, was, is, false, None)
            }
            (
                Item::Value(Value::InlineTable(table)),
                Some(toml::Value::Table(was)),
                toml::Value::Table(is),
            ) => self.table(table, None, was, is, true, None),
            (Item::ArrayOfTables(array), Some(toml::Value::Array(was)), toml::Value::Array(is)) => {
                self.array(array, key, was, is, indexes)
            }
            (item, _, after) => {
                let mut next = to_item(key, after)?;
                match (&*item, &mut next) {
                    (Item::Value(was), _) => {
                        self.dropped += inner_comments(was);
                        let mut value = next.into_value().ok()?;
                        *value.decor_mut() = was.decor().clone();
                        next = Item::Value(value);
                    }
                    (Item::Table(was), Item::Table(is)) => {
                        is.set_position(was.position());
                        *is.decor_mut() = was.decor().clone();
                    }
                    _ => {}
                }
                *item = next;
                Some(())
            }
        }
    }

    /// `[[index]]`-style entries, paired by `pairing`, else by `name`.
    fn array(
        &mut self,
        array: &mut ArrayOfTables,
        key: &str,
        old: &[toml::Value],
        new: &[toml::Value],
        pairing: Option<&Pairing>,
    ) -> Option<()> {
        if array.len() != old.len() {
            return None;
        }
        let name = |v: &toml::Value| {
            v.get("name")
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
        };
        let local;
        let pairing = match pairing.filter(|p| p.pairs.len() == new.len()) {
            Some(pairing) => pairing,
            None => {
                local = Pairing::matching(old, new, |was, is| {
                    name(was).is_some() && name(was) == name(is)
                });
                &local
            }
        };
        for (i, table) in array.iter().enumerate() {
            if !pairing.pairs.contains(&Some(i))
                && pairing.doubtful.get(i).copied().unwrap_or(false)
                && let Some(id) = mark_id(table)
            {
                self.doubtful.insert(id, table_comments(table));
            }
        }
        let mut tables = Vec::with_capacity(new.len());
        for (is, pair) in new.iter().zip(&pairing.pairs) {
            let table = match pair {
                Some(i) => {
                    let mut table = array.get(*i)?.clone();
                    let was = old.get(*i)?;
                    if was != is {
                        let id = mark_id(&table);
                        self.table(&mut table, id, was.as_table()?, is.as_table()?, false, None)?;
                    }
                    table
                }
                None => match to_item(key, &toml::Value::Array(vec![is.clone()]))? {
                    Item::ArrayOfTables(fresh) => fresh.get(0)?.clone(),
                    _ => return None,
                },
            };
            tables.push(table);
        }
        let kept: Vec<usize> = pairing.pairs.iter().flatten().copied().collect();
        let in_order = kept.windows(2).all(|w| w.first() < w.get(1));
        array.clear();
        for mut table in tables {
            if !in_order {
                unplace(&mut table);
            }
            array.push(table);
        }
        Some(())
    }

    /// Rebuild every header's prefix and the document's trailing text from the
    /// marker order before (`was`) and after (`now`) the patch.
    fn place(
        &mut self,
        stash: &[Option<String>],
        was: &[usize],
        now: &[usize],
        trailing: &str,
        root_values: bool,
    ) -> Option<Prefixes> {
        let original: HashSet<usize> = was.iter().copied().collect();
        let mut parts: HashMap<usize, Split> = HashMap::new();
        for &id in was {
            parts.insert(id, split(stash.get(id)?.as_deref().unwrap_or("")));
        }
        let end = split(trailing);
        let end_free = format!("{}{}", end.free, end.lead);
        // What follows a table's last key and is its own.
        let mut own_trailing: HashMap<Option<usize>, String> = HashMap::new();
        let mut prev: Option<usize> = None;
        for &id in was {
            own_trailing.insert(prev, parts.get(&id)?.below.clone());
            prev = Some(id);
        }
        own_trailing.insert(prev, end.below.clone());
        for (id, tail) in std::mem::take(&mut self.tails) {
            own_trailing.entry(id).or_default().push_str(&tail);
        }

        let survivors: Vec<usize> = now
            .iter()
            .copied()
            .filter(|id| original.contains(id))
            .collect();
        let at: HashMap<usize, usize> = was.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let positions: Vec<Option<usize>> =
            survivors.iter().map(|id| at.get(id).copied()).collect();
        let stays = super::yaml::stationary(&positions, was.len());
        let surviving: HashSet<usize> = survivors.iter().copied().collect();

        let mut carried: HashMap<usize, String> = HashMap::new();
        let mut pending = String::new();
        for (i, &id) in was.iter().enumerate() {
            let split = parts.get(&id)?;
            let stationary = stays.get(i).copied().unwrap_or(false);
            if stationary {
                join_free(&mut pending, &split.free);
                carried.insert(id, std::mem::take(&mut pending));
                continue;
            }
            if has_comment(&split.free) {
                join_free(&mut pending, &split.free);
            }
            if !surviving.contains(&id)
                && let Some(inner) = self.doubtful.get(&id)
            {
                let below = own_trailing.get(&Some(id)).map_or(0, |t| count_comments(t));
                self.dropped += inner + count_comments(&split.lead) + below;
            }
        }

        let mut headers = Vec::new();
        let mut prev: Option<usize> = None;
        for &id in now {
            let mut prefix = own_trailing.get(&prev).cloned().unwrap_or_default();
            match parts.get(&id) {
                Some(split) => {
                    match carried.get(&id) {
                        Some(carried) => prefix.push_str(carried),
                        None if !has_comment(&split.free) => prefix.push_str(&split.free),
                        None => {}
                    }
                    prefix.push_str(&split.lead);
                }
                None => {
                    let leading = prev.is_some() || !prefix.is_empty();
                    if leading && !prefix.ends_with("\n\n") {
                        prefix.push('\n');
                    }
                }
            }
            // Keys above the first table, with nothing between: a blank line.
            if prev.is_none() && root_values && prefix.is_empty() {
                prefix.push('\n');
            }
            headers.push((id, prefix));
            prev = Some(id);
        }
        let mut trailing = own_trailing.get(&prev).cloned().unwrap_or_default();
        trailing.push_str(&pending);
        trailing.push_str(&end_free);
        Some(Prefixes { headers, trailing })
    }
}

fn mark_id(table: &Table) -> Option<usize> {
    table
        .decor()
        .prefix()?
        .as_str()?
        .strip_prefix(MARK)?
        .strip_suffix(">>\n")?
        .parse()
        .ok()
}

/// The comments inside a value, not counting its own decor.
fn inner_comments(value: &Value) -> usize {
    let decor = |d: &toml_edit::Decor| {
        let raw = |r: Option<&toml_edit::RawString>| {
            count_comments(r.and_then(|r| r.as_str()).unwrap_or(""))
        };
        raw(d.prefix()) + raw(d.suffix())
    };
    match value {
        Value::Array(array) => {
            array
                .iter()
                .map(|v| decor(v.decor()) + inner_comments(v))
                .sum::<usize>()
                + count_comments(array.trailing().as_str().unwrap_or(""))
        }
        Value::InlineTable(table) => table
            .iter()
            .map(|(k, v)| {
                table.key(k).map_or(0, |k| decor(k.leaf_decor()))
                    + decor(v.decor())
                    + inner_comments(v)
            })
            .sum(),
        _ => 0,
    }
}

/// The comments inside a table's keys and values, not its header's prefix.
fn table_comments(table: &Table) -> usize {
    let raw =
        |r: Option<&toml_edit::RawString>| count_comments(r.and_then(|r| r.as_str()).unwrap_or(""));
    table
        .iter()
        .map(|(k, item)| {
            let key = table.key(k).map_or(0, |k| {
                raw(k.leaf_decor().prefix()) + raw(k.leaf_decor().suffix())
            });
            key + match item {
                Item::Value(v) => {
                    raw(v.decor().prefix()) + raw(v.decor().suffix()) + inner_comments(v)
                }
                _ => 0,
            }
        })
        .sum()
}

/// `key = value` as `toml` prints it, as an item with no position, so a new
/// table lands after the one before it.
fn to_item(key: &str, value: &toml::Value) -> Option<Item> {
    let mut table = toml::Table::new();
    table.insert(key.to_owned(), value.clone());
    let mut doc: DocumentMut = toml::to_string(&table).ok()?.parse().ok()?;
    let mut item = doc.as_table_mut().remove(key)?;
    match &mut item {
        Item::Table(table) => unplace(table),
        Item::ArrayOfTables(array) => array.iter_mut().for_each(unplace),
        _ => {}
    }
    Some(item)
}

fn unplace(table: &mut Table) {
    table.set_position(None);
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(table) => unplace(table),
            Item::ArrayOfTables(array) => array.iter_mut().for_each(unplace),
            _ => {}
        }
    }
}
