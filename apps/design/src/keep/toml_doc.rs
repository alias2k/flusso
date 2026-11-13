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
//! **Sinks** are keyed by name. A renamed sink ([`SinkNames`]) is renamed in
//! place before the patch, in `[sinks]` and in every `[index.sinks]`, so it
//! keeps its position, its key order and its comments; a removed one goes
//! with its own comments. A sink key is matched by the name the loader reads
//! it as ([`SinkName`](kernel::common::SinkName) trims and lowercases), so
//! `[sinks.Primary]` is patched, renamed or removed where it stands and keeps
//! its spelling while its name does not change. A table naming one sink in
//! two spellings is the loader's last-wins guess, so the patch gives up.
//!
//! **Line endings.** `toml_edit` writes every line with `\n` and drops every
//! `\r` it holds, so while the file is patched each `\r` stands as the
//! private-use [`CR`] character, which a file the patcher takes never holds:
//! in comments and blank lines, at the end of each key and header line whose
//! line ended in CRLF, and inside string values (a multi-line string keeps
//! the CRLFs it holds and the bare `\n`s too). Every line the save writes
//! ends the way most of the file's lines do. After the render each [`CR`]
//! becomes `\r` again, so a line the save left alone keeps its bytes.
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
//!   between, is that table's: it moves and goes with it. At the end of the
//!   file too: only what a blank line separates from the last table stays
//!   last;
//! - any other comment is nobody's: it stays where it is, before whatever
//!   follows it.
//!
//! To place them once tables were removed, added or reordered, every header is
//! tagged before the patch with a marker in its prefix; the order the markers
//! come out in after the patch says what survived and where, and the prefixes
//! are rebuilt from the split pieces.

use std::collections::{HashMap, HashSet};

use toml_edit::{
    ArrayOfTables, Document, DocumentMut, InlineTable, Item, Key, Table, TableLike, Value,
};

use super::pairing::{Pairing, SinkNames};
use super::yaml::Patched;

const MARK: &str = "#<<flusso-keep ";

/// `text` patched from `old` to `new`, or `None` when it does not parse.
/// `indexes` pairs the `[[index]]` entries; without it they pair by name.
/// `sinks` says which sink each sink of the file becomes.
pub(super) fn patch(
    text: &str,
    old: &toml::Table,
    new: &toml::Table,
    indexes: Option<&Pairing>,
    sinks: &SinkNames,
) -> Option<Patched> {
    let stands_in = text.contains('\r');
    if text.contains(MARK) || (stands_in && text.contains(CR)) {
        return None;
    }
    let eol = marked(super::dominant_ending(text));
    let (source, swallowed) = mark_strings(text)?;
    let parsed = Document::parse(source.as_str()).ok()?;
    let ends = line_ends(parsed.as_table(), &source);
    let keys = key_texts(parsed.as_table(), &source);
    let mut doc = parsed.into_mut();
    mark_decor(&mut doc);
    keep_line_ends(doc.as_table_mut(), &ends, eol)?;
    let respelled = respelled_keys(&mut doc, keys)?;
    if !respelled.is_empty() && text.contains([TAG_OPEN, TAG_CLOSE]) {
        return None;
    }
    if spells_a_sink_twice(doc.as_table()) {
        return None;
    }
    let mut old = old.clone();
    if !sinks.renames.is_empty() {
        rename_sinks(doc.as_table_mut(), &sinks.renames)?;
        rename_sinks_in_model(&mut old, &sinks.renames);
    }
    let old = &old;
    let header = take_header(&mut doc)?;
    let mut stash = Vec::new();
    mark(doc.as_table_mut(), &mut stash)?;
    let before = order(&doc.to_string());
    let trailing = doc.trailing().as_str()?.to_owned();

    let mut patcher = Patcher {
        tails: HashMap::new(),
        doubtful: HashMap::new(),
        doubtful_sinks: &sinks.doubtful,
        dropped: 0,
        eol,
    };
    patcher.table(doc.as_table_mut(), None, old, new, Level::Root, indexes)?;

    mark(doc.as_table_mut(), &mut stash)?;
    let after = order(&doc.to_string());
    let root_values = doc
        .as_table()
        .iter()
        .any(|(_, item)| writes_body_line(item));
    let prefixes = patcher.place(&stash, &before, &after, &trailing, root_values)?;
    for (id, prefix) in prefixes.headers {
        set_prefix(doc.as_table_mut(), id, &prefix);
    }
    doc.set_trailing(prefixes.trailing);
    let text = restore_keys(format!("{header}{doc}"), &respelled)?;
    Some(Patched {
        text: if stands_in {
            restore_swallowed(text.replace(CR, "\r"), &swallowed)?
        } else {
            text
        },
        dropped: patcher.dropped,
    })
}

/// Remove and return the file's header: everything before the first key or
/// header. Without it a new top-level key would be written above it.
fn take_header(doc: &mut DocumentMut) -> Option<String> {
    let first_key = on_first_key_decor(doc.as_table_mut(), &mut |decor| {
        let header = decor.prefix().map_or(Some(""), |p| p.as_str())?.to_owned();
        decor.set_prefix("");
        Some(header)
    });
    if let Some(header) = first_key {
        return header;
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

/// `f` on the decor that starts the first line of `table`'s body: its first key's,
/// or, when that is a dotted key (`sinks.primary.url = …`), the leaf's.
/// `None` when the body has no line.
fn on_first_key_decor<R>(
    table: &mut Table,
    f: &mut dyn FnMut(&mut toml_edit::Decor) -> R,
) -> Option<R> {
    let key = table
        .iter()
        .find(|(_, item)| writes_body_line(item))
        .map(|(k, _)| k.to_owned())?;
    if table.get(&key).is_some_and(Item::is_value) {
        return Some(f(table.key_mut(&key)?.leaf_decor_mut()));
    }
    on_first_key_decor(table.get_mut(&key)?.as_table_mut()?, f)
}

/// Whether `item` writes a line in its table's body: a value, or a dotted
/// table holding one.
fn writes_body_line(item: &Item) -> bool {
    match item {
        Item::Value(_) => true,
        Item::Table(table) => {
            table.is_dotted() && table.iter().any(|(_, item)| writes_body_line(item))
        }
        _ => false,
    }
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
    let blank = |l: &&str| blank(l);
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

/// The document's trailing text split by owner: the comment lines directly
/// below the last table are its own, the rest stays at the end.
fn split_end(trailing: &str, eol: &str) -> Split {
    let lines: Vec<&str> = trailing.split_inclusive('\n').collect();
    let below_len = lines.iter().take_while(|l| !blank(l)).count();
    let (below, free) = lines.split_at(below_len);
    let mut below = below.concat();
    // A file with no final newline: the comment still ends its line, so the
    // table that comes after it starts on its own.
    if !below.is_empty() && !below.ends_with('\n') {
        below.push_str(eol);
    }
    Split {
        below,
        free: free.concat(),
        lead: String::new(),
    }
}

/// Rename the sinks `renames` names, in `[sinks]` and in every entry's
/// `[index.sinks]`, keeping each table's place, keys and comments.
fn rename_sinks(root: &mut Table, renames: &HashMap<String, String>) -> Option<()> {
    if let Some(sinks) = root.get_mut("sinks") {
        rename_keys(sinks, renames)?;
    }
    if let Some(Item::ArrayOfTables(entries)) = root.get_mut("index") {
        for entry in entries.iter_mut() {
            if let Some(sinks) = entry.get_mut("sinks") {
                rename_keys(sinks, renames)?;
            }
        }
    }
    Some(())
}

fn rename_keys(item: &mut Item, renames: &HashMap<String, String>) -> Option<()> {
    let renamed = |key: Key| match renames.get(&sink_name(key.get())) {
        Some(name) if *name != sink_name(key.get()) => Key::new(name.as_str())
            .with_leaf_decor(key.leaf_decor().clone())
            .with_dotted_decor(key.dotted_decor().clone()),
        _ => key,
    };
    match item {
        Item::Table(table) => {
            let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
            let entries: Vec<(Key, Item)> = keys
                .iter()
                .map(|k| table.remove_entry(k))
                .collect::<Option<_>>()?;
            for (key, child) in entries {
                table.insert_formatted(&renamed(key), child);
            }
        }
        Item::Value(Value::InlineTable(table)) => rename_inline(table, &renamed)?,
        _ => {}
    }
    Some(())
}

fn rename_inline(table: &mut InlineTable, renamed: &dyn Fn(Key) -> Key) -> Option<()> {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    let entries: Vec<(Key, Value)> = keys
        .iter()
        .map(|k| table.remove_entry(k))
        .collect::<Option<_>>()?;
    for (key, value) in entries {
        table.insert_formatted(&renamed(key), value);
    }
    Some(())
}

/// The same renames in the model the file reads as.
fn rename_sinks_in_model(config: &mut toml::Table, renames: &HashMap<String, String>) {
    let rename = |table: &mut toml::Table| {
        for (key, value) in std::mem::take(table) {
            table.insert(renames.get(&key).cloned().unwrap_or(key), value);
        }
    };
    if let Some(toml::Value::Table(sinks)) = config.get_mut("sinks") {
        rename(sinks);
    }
    if let Some(toml::Value::Array(entries)) = config.get_mut("index") {
        for entry in entries {
            if let Some(toml::Value::Table(sinks)) = entry.get_mut("sinks") {
                rename(sinks);
            }
        }
    }
}

/// `free` appended to `pending` without doubling the blank line between them.
fn join_free(pending: &mut String, free: &str) {
    let mut skipping = ends_blank(pending);
    for line in free.split_inclusive('\n') {
        skipping &= blank(line);
        if !skipping {
            pending.push_str(line);
        }
    }
}

/// Whether `text` ends with a blank line, in either line ending.
fn ends_blank(text: &str) -> bool {
    ["\n", "\u{e00d}\n"].contains(&text)
        || ["\n\n", "\n\u{e00d}\n"]
            .iter()
            .any(|end| text.ends_with(end))
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

/// Where a table is in the config: what its keys name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Root,
    /// An `[[index]]` entry.
    Index,
    /// `[sinks]` or an entry's `[index.sinks]`: keyed by sink name.
    Sinks,
    /// An inline table.
    Inline,
    /// An inline table keyed by sink name: `sinks = { primary = { … } }`.
    InlineSinks,
    Other,
}

/// How an inline table spaces its entries: `{ a = 1, b = 2 }` or `{a=1,b=2}`.
struct InlineSpacing {
    /// Around the key: before it (after `{` or `,`) and before its `=`.
    key: toml_edit::Decor,
    /// Between `=` and the value.
    value_prefix: String,
    /// After a value that a `,` follows.
    before_comma: String,
    /// After the last value, before `}`.
    before_close: String,
}

impl InlineSpacing {
    /// The spacing of `table`, whose keys are `keys`, read off its last entry.
    fn of(table: &dyn TableLike, keys: &[String]) -> Option<Self> {
        let raw =
            |r: Option<&toml_edit::RawString>| r.and_then(|r| r.as_str()).unwrap_or("").to_owned();
        let value = |key: &String| table.get(key).and_then(Item::as_value).map(Value::decor);
        let last = keys.last()?;
        let before_comma = keys
            .len()
            .checked_sub(2)
            .and_then(|i| keys.get(i))
            .and_then(value)
            .map_or_else(String::new, |d| raw(d.suffix()));
        let decor = value(last)?;
        let key = table.key(last)?.leaf_decor();
        Some(Self {
            key: toml_edit::Decor::new(raw(key.prefix()), raw(key.suffix())),
            value_prefix: raw(decor.prefix()),
            before_comma,
            before_close: raw(decor.suffix()),
        })
    }

    /// Space the entries the patch added (keys not in `was`) like the rest,
    /// and keep the space before `}` after whichever entry is last now.
    fn apply(self, table: &mut dyn TableLike, was: &[String]) {
        let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
        let (Some(last), Some(was_last)) = (keys.last(), was.last()) else {
            return;
        };
        for key in &keys {
            let added = !was.contains(key);
            if added && let Some(mut handle) = table.key_mut(key) {
                *handle.leaf_decor_mut() = self.key.clone();
            }
            let Some(decor) = table
                .get_mut(key)
                .and_then(Item::as_value_mut)
                .map(Value::decor_mut)
            else {
                continue;
            };
            if added {
                decor.set_prefix(self.value_prefix.as_str());
            }
            if key == last && (added || key != was_last) {
                decor.set_suffix(self.before_close.as_str());
            } else if key != last && (added || key == was_last) {
                decor.set_suffix(self.before_comma.as_str());
            }
        }
    }
}

struct Patcher<'a> {
    /// Free comments of removed keys with no key after them, per table (`None`
    /// is the root): they go after the table's last key.
    tails: HashMap<Option<usize>, String>,
    /// Removed tables that may still exist (an `[[index]]` entry, a sink),
    /// with the comments inside them.
    doubtful: HashMap<usize, usize>,
    /// Removed sinks that may still exist under another name.
    doubtful_sinks: &'a HashSet<String>,
    dropped: usize,
    /// The file's line ending, for every line the save writes.
    eol: &'static str,
}

impl Patcher<'_> {
    fn table(
        &mut self,
        doc: &mut dyn TableLike,
        id: Option<usize>,
        old: &toml::Table,
        new: &toml::Table,
        level: Level,
        indexes: Option<&Pairing>,
    ) -> Option<()> {
        let inline = matches!(level, Level::Inline | Level::InlineSinks);
        let by_sink = matches!(level, Level::Sinks | Level::InlineSinks);
        let keys: Vec<String> = doc.iter().map(|(k, _)| k.to_owned()).collect();
        let named = |key: &str| {
            if by_sink {
                sink_name(key)
            } else {
                key.to_owned()
            }
        };
        let names: Vec<String> = keys.iter().map(|k| named(k)).collect();
        let spacing = inline.then(|| InlineSpacing::of(doc, &keys)).flatten();
        let mut pending = String::new();
        for (key, name) in keys.iter().zip(&names) {
            let before = old.get(name);
            let removed = new.get(name).is_none() && before.is_some();
            let doubtful = removed && by_sink && self.doubtful_sinks.contains(name);
            if doubtful {
                let item = doc.get(key)?;
                self.forget(item);
            }
            if !inline && doc.get(key).is_some_and(Item::is_value) {
                let mut handle = doc.key_mut(key)?;
                let decor = handle.leaf_decor_mut();
                let prefix = decor.prefix().map_or(Some(""), |p| p.as_str())?.to_owned();
                let parts = split(&prefix);
                if doubtful {
                    self.dropped += count_comments(&parts.lead);
                }
                if removed {
                    if has_comment(&parts.free) || has_comment(&parts.below) {
                        pending.push_str(&parts.below);
                        pending.push_str(&parts.free);
                    }
                } else if !pending.is_empty() {
                    decor.set_prefix(format!("{}{prefix}", std::mem::take(&mut pending)));
                }
            }
            match new.get(name) {
                Some(after) if before == Some(after) => {}
                Some(after) => {
                    let sub = doc.get_mut(key)?;
                    let below = match (level, name.as_str()) {
                        (Level::Root, "index") => Level::Index,
                        (Level::Root | Level::Index, "sinks") => Level::Sinks,
                        _ => Level::Other,
                    };
                    let indexes = indexes.filter(|_| level == Level::Root && key == "index");
                    self.item(sub, key, before, after, below, indexes)?;
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
            if !names.contains(key) && old.get(key) != Some(after) {
                let item = to_item(key, after)?;
                let item = if inline {
                    let mut value = item.into_value().ok()?;
                    if self.eol != "\n" {
                        single_line(&mut value)?;
                    }
                    Item::Value(value)
                } else {
                    let mut item = item;
                    use_ending(&mut item, self.eol)?;
                    item
                };
                doc.insert(key, item);
            }
        }
        if let Some(spacing) = spacing {
            spacing.apply(doc, &keys);
        }
        Some(())
    }

    /// A removed item that may still exist: its comments are reported.
    fn forget(&mut self, item: &Item) {
        match item {
            Item::Table(table) => {
                match mark_id(table) {
                    Some(id) => {
                        self.doubtful.insert(id, table_comments(table));
                    }
                    None => self.dropped += table_comments(table),
                }
                for (_, child) in table.iter() {
                    if !child.is_value() {
                        self.forget(child);
                    }
                }
            }
            Item::ArrayOfTables(array) => {
                for table in array.iter() {
                    if let Some(id) = mark_id(table) {
                        self.doubtful.insert(id, table_comments(table));
                    }
                }
            }
            Item::Value(value) => {
                let suffix = value
                    .decor()
                    .suffix()
                    .and_then(|r| r.as_str())
                    .unwrap_or("");
                self.dropped += count_comments(suffix) + inner_comments(value);
            }
            Item::None => {}
        }
    }

    fn item(
        &mut self,
        item: &mut Item,
        key: &str,
        before: Option<&toml::Value>,
        after: &toml::Value,
        level: Level,
        indexes: Option<&Pairing>,
    ) -> Option<()> {
        match (item, before, after) {
            (Item::Table(table), Some(toml::Value::Table(was)), toml::Value::Table(is)) => {
                let id = mark_id(table);
                self.table(table, id, was, is, level, None)
            }
            (
                Item::Value(Value::InlineTable(table)),
                Some(toml::Value::Table(was)),
                toml::Value::Table(is),
            ) => {
                let level = if level == Level::Sinks {
                    Level::InlineSinks
                } else {
                    Level::Inline
                };
                self.table(table, None, was, is, level, None)
            }
            (Item::ArrayOfTables(array), Some(toml::Value::Array(was)), toml::Value::Array(is)) => {
                self.array(array, key, was, is, indexes)
            }
            (item, _, after) => {
                let mut next = to_item(key, after)?;
                use_ending(&mut next, self.eol)?;
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
                        let level = if key == "index" {
                            Level::Index
                        } else {
                            Level::Other
                        };
                        self.table(&mut table, id, was.as_table()?, is.as_table()?, level, None)?;
                    }
                    table
                }
                None => {
                    let mut fresh = to_item(key, &toml::Value::Array(vec![is.clone()]))?;
                    use_ending(&mut fresh, self.eol)?;
                    match fresh {
                        Item::ArrayOfTables(fresh) => fresh.get(0)?.clone(),
                        _ => return None,
                    }
                }
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
        let end_trailing = trailing;
        let end = split_end(trailing, self.eol);
        let end_free = end.free;
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
                    if leading && !ends_blank(&prefix) {
                        prefix.push_str(self.eol);
                    }
                }
            }
            // Keys above the first table, with nothing between: a blank line.
            if prev.is_none() && root_values && prefix.is_empty() {
                prefix.push_str(self.eol);
            }
            headers.push((id, prefix));
            prev = Some(id);
        }
        let mut trailing = own_trailing.get(&prev).cloned().unwrap_or_default();
        trailing.push_str(&pending);
        trailing.push_str(&end_free);
        let unterminated = !end_trailing.is_empty() && !end_trailing.ends_with('\n');
        if unterminated && let Some(cut) = trailing.strip_suffix('\n') {
            let cut = cut.strip_suffix(CR).unwrap_or(cut).len();
            trailing.truncate(cut);
        }
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
/// Stands for a `\r` while the file is in `toml_edit`'s hands: its encoder
/// writes every raw string without its `\r`s, a CRLF line ending, a CRLF in
/// a comment block and one inside a multi-line string alike. A private-use
/// character no file is allowed to hold (see [`patch`]), so turning each one
/// back into `\r` after the render restores exactly the `\r`s the file had
/// and the ones the save chose.
const CR: char = '\u{e00d}';

/// The end of a line in `eol`, with its `\r` as [`CR`].
fn marked(eol: &str) -> &'static str {
    if eol == "\r\n" { "\u{e00d}\n" } else { "\n" }
}

/// Whether `line` holds nothing but whitespace.
fn blank(line: &str) -> bool {
    line.chars().all(|c| c.is_whitespace() || c == CR)
}

/// `text` with the `\r` inside every string value as [`CR`], so it outlives
/// the render: a multi-line string keeps the CRLF it holds on purpose, and a
/// bare `\n` it holds stays bare. `None` when `text` does not parse.
///
/// A `\r` in the whitespace a line-ending backslash swallows can't be a
/// [`CR`]: `\` + [`CR`] is not an escape. That whitespace is replaced whole
/// by a [`Swallowed`] stand-in instead, which [`restore_swallowed`] turns
/// back into the file's bytes.
fn mark_strings(text: &str) -> Option<(String, Vec<Swallowed>)> {
    fn walk(value: &Value, out: &mut Vec<std::ops::Range<usize>>) {
        match value {
            Value::String(_) => out.extend(value.span()),
            Value::Array(array) => array.iter().for_each(|v| walk(v, out)),
            Value::InlineTable(table) => table.iter().for_each(|(_, v)| walk(v, out)),
            _ => {}
        }
    }
    fn strings(table: &Table, out: &mut Vec<std::ops::Range<usize>>) {
        for (_, item) in table.iter() {
            match item {
                Item::Value(value) => walk(value, out),
                Item::Table(table) => strings(table, out),
                Item::ArrayOfTables(array) => array.iter().for_each(|t| strings(t, out)),
                Item::None => {}
            }
        }
    }
    if !text.contains('\r') {
        return Some((text.to_owned(), Vec::new()));
    }
    let parsed = Document::parse(text).ok()?;
    let mut spans = Vec::new();
    strings(parsed.as_table(), &mut spans);
    spans.sort_by_key(|s| s.start);
    let longest_tabs = text.split(|c| c != '\t').map(str::len).max().unwrap_or(0);
    let mut swallowed = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for span in spans {
        out.push_str(text.get(at..span.start)?);
        let mut raw = text.get(span.clone())?.to_owned();
        if raw.starts_with("\"\"\"") {
            for region in swallowed_regions(&raw).into_iter().rev() {
                let tabs = longest_tabs + 1 + swallowed.len();
                let code = format!("\n{}", "\t".repeat(tabs));
                swallowed.push(Swallowed {
                    text: raw.get(region.clone())?.to_owned(),
                    code: code.clone(),
                });
                raw.replace_range(region, &code);
            }
        }
        out.push_str(&raw.replace('\r', &CR.to_string()));
        at = span.end;
    }
    out.push_str(text.get(at..)?);
    Some((out, swallowed))
}

/// The whitespace a line-ending backslash swallows, holding a `\r`, while
/// it stands in the file as `code`: a line break, then a run of tabs longer
/// than any in the file and than every other stand-in's.
#[derive(Debug)]
struct Swallowed {
    code: String,
    text: String,
}

/// In the raw multi-line basic string `raw`, the whitespace each
/// line-ending backslash swallows (spaces and tabs after it, the line break,
/// and the whitespace up to the next character), where it holds a `\r`.
fn swallowed_regions(raw: &str) -> Vec<std::ops::Range<usize>> {
    let ws = |b: &u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r');
    let bytes = raw.as_bytes();
    let mut regions = Vec::new();
    let mut i = 3;
    while let Some(&b) = bytes.get(i) {
        if b != b'\\' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let line = bytes
            .get(start..)
            .unwrap_or(&[])
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        let after = bytes.get(start + line..).unwrap_or(&[]);
        if after.starts_with(b"\n") || after.starts_with(b"\r\n") {
            let end = start
                + bytes
                    .get(start..)
                    .unwrap_or(&[])
                    .iter()
                    .take_while(|b| ws(b))
                    .count();
            if bytes.get(start..end).is_some_and(|r| r.contains(&b'\r')) {
                regions.push(start..end);
            }
            i = end;
        } else {
            i += 2;
        }
    }
    regions
}

/// `text` with each [`Swallowed`] stand-in back as the file's whitespace.
/// A stand-in the save no longer writes (its string changed) is gone; one
/// written twice is a guess, so the patch gives up.
fn restore_swallowed(mut text: String, swallowed: &[Swallowed]) -> Option<String> {
    for region in swallowed {
        let found: Vec<usize> = text
            .match_indices(&region.code)
            .map(|(at, _)| at)
            .filter(|&at| {
                text.get(..at).is_some_and(|b| b.ends_with('\\'))
                    && !text
                        .get(at + region.code.len()..)
                        .is_some_and(|rest| rest.starts_with([' ', '\t', '\n', '\r']))
            })
            .collect();
        match found.as_slice() {
            [] => {}
            [at] => text.replace_range(*at..*at + region.code.len(), &region.text),
            _ => return None,
        }
    }
    Some(text)
}

/// Every `\r` in the document's whitespace and comments as [`CR`].
fn mark_decor(doc: &mut DocumentMut) {
    fn raw(raw: Option<&toml_edit::RawString>) -> Option<String> {
        raw.and_then(|r| r.as_str())
            .filter(|s| s.contains('\r'))
            .map(|s| s.replace('\r', &CR.to_string()))
    }
    fn decor(decor: &mut toml_edit::Decor) {
        if let Some(prefix) = raw(decor.prefix()) {
            decor.set_prefix(prefix);
        }
        if let Some(suffix) = raw(decor.suffix()) {
            decor.set_suffix(suffix);
        }
    }
    fn in_value(value: &mut Value) {
        decor(value.decor_mut());
        match value {
            Value::Array(array) => {
                array.iter_mut().for_each(in_value);
                if let Some(trailing) = raw(Some(array.trailing())) {
                    array.set_trailing(trailing);
                }
            }
            Value::InlineTable(table) => {
                for (mut key, item) in table.iter_mut() {
                    decor(key.leaf_decor_mut());
                    decor(key.dotted_decor_mut());
                    in_value(item);
                }
                if let Some(trailing) = raw(Some(table.trailing())) {
                    table.set_trailing(trailing);
                }
            }
            _ => {}
        }
    }
    fn in_table(table: &mut Table) {
        decor(table.decor_mut());
        for (mut key, item) in table.iter_mut() {
            decor(key.leaf_decor_mut());
            decor(key.dotted_decor_mut());
            match item {
                Item::Value(value) => in_value(value),
                Item::Table(table) => in_table(table),
                Item::ArrayOfTables(array) => array.iter_mut().for_each(in_table),
                Item::None => {}
            }
        }
    }
    in_table(doc.as_table_mut());
    if let Some(trailing) = raw(Some(doc.trailing())) {
        doc.set_trailing(trailing);
    }
}

/// Open and close a tag ([`tag`]) in a value's decor. Private-use
/// characters: a file holding either is not patched where a tag is needed.
const TAG_OPEN: char = '\u{e00e}';
const TAG_CLOSE: char = '\u{e00f}';

/// The tag of the value [`Site`] `at`, written at the start of its decor.
fn tag(at: usize) -> String {
    format!("{TAG_OPEN}{at}{TAG_CLOSE}")
}

/// For every [`Site`] of the parsed `root`, in order, the text of its line
/// in `source` before its value: indentation, key and `=` (`None` for a
/// header).
fn key_texts(root: &Table, source: &str) -> Vec<Option<String>> {
    let mut found = Vec::new();
    sites(root, false, &mut found);
    found
        .iter()
        .map(|site| {
            let Site::Value(value) = site else {
                return None;
            };
            let end = value
                .decor()
                .prefix()
                .and_then(|p| p.span())
                .or_else(|| value.span())?
                .start;
            let before = source.get(..end)?;
            let start = before.rfind('\n').map_or(0, |n| n + 1);
            before.get(start..).map(str::to_owned)
        })
        .collect()
}

/// A value line `toml_edit` writes differently from the file: what the file
/// says before the value, and what `toml_edit` writes there.
struct Respelled {
    at: usize,
    written: String,
    file: String,
}

/// The value lines of `doc` whose key `toml_edit` does not write back as the
/// file spells it, each tagged in `doc` so [`restore_keys`] can find it.
///
/// A dotted key shares its leading parts with every other key under the same
/// dotted table, so `toml_edit` keeps one spelling of them: `x . z = 1` after
/// `x.y = 2` would be written `x. z = 1`.
fn respelled_keys(doc: &mut DocumentMut, keys: Vec<Option<String>>) -> Option<Vec<Respelled>> {
    let mut probe = doc.clone();
    let mut at = 0;
    sites_mut(probe.as_table_mut(), false, &mut |site| {
        if let SiteMut::Value(value) = site {
            prepend_tag(value, at);
        }
        at += 1;
    });
    let written = probe.to_string();
    let mut respelled = Vec::new();
    for (at, file) in keys.into_iter().enumerate() {
        let Some(file) = file else { continue };
        let written = written_key(&written, at)?;
        if written != file {
            respelled.push(Respelled {
                at,
                written: written.to_owned(),
                file,
            });
        }
    }
    let mut at = 0;
    sites_mut(doc.as_table_mut(), false, &mut |site| {
        if let SiteMut::Value(value) = site
            && respelled.iter().any(|r| r.at == at)
        {
            prepend_tag(value, at);
        }
        at += 1;
    });
    Some(respelled)
}

fn prepend_tag(value: &mut Value, at: usize) {
    let decor = value.decor_mut();
    let prefix = decor.prefix().and_then(|p| p.as_str()).unwrap_or("");
    let prefix = format!("{}{prefix}", tag(at));
    decor.set_prefix(prefix);
}

/// The text before the tag of site `at` on its line in `written`.
fn written_key(written: &str, at: usize) -> Option<&str> {
    let end = written.find(&tag(at))?;
    let before = written.get(..end)?;
    before.get(before.rfind('\n').map_or(0, |n| n + 1)..)
}

/// `text` with every tag [`respelled_keys`] set removed, and the key of each
/// tagged line spelled as the file had it while the save still writes the
/// key `toml_edit` read: a renamed key keeps its new spelling.
fn restore_keys(mut text: String, respelled: &[Respelled]) -> Option<String> {
    for r in respelled {
        let tag = tag(r.at);
        let Some(end) = text.find(&tag) else { continue };
        if text.get(end + tag.len()..)?.contains(&tag) {
            return None;
        }
        let start = text.get(..end)?.rfind('\n').map_or(0, |n| n + 1);
        if text.get(start..end)? == r.written {
            text.replace_range(start..end + tag.len(), &r.file);
        } else {
            text.replace_range(end..end + tag.len(), "");
        }
    }
    Some(text)
}

/// One place `toml_edit` ends a line itself: a table's header or a key's
/// value. It drops the line ending the source had there and renders `\n`.
enum Site<'a> {
    Header(&'a Table),
    Value(&'a Value),
}

enum SiteMut<'a> {
    Header(&'a mut Table),
    Value(&'a mut Value),
}

/// Whether `table` has a header line of its own (the test `for_each_header`
/// makes, minus dotted keys, which have none).
fn has_header(table: &Table) -> bool {
    !table.is_implicit() && !table.is_dotted()
}

fn sites<'a>(table: &'a Table, header: bool, out: &mut Vec<Site<'a>>) {
    if header {
        out.push(Site::Header(table));
    }
    for (_, item) in table.iter() {
        match item {
            Item::Value(value) => out.push(Site::Value(value)),
            Item::Table(table) => sites(table, has_header(table), out),
            Item::ArrayOfTables(array) => array.iter().for_each(|t| sites(t, true, out)),
            Item::None => {}
        }
    }
}

fn sites_mut(table: &mut Table, header: bool, f: &mut dyn FnMut(SiteMut<'_>)) {
    if header {
        f(SiteMut::Header(table));
    }
    for (_, item) in table.iter_mut() {
        match item {
            Item::Value(value) => f(SiteMut::Value(value)),
            Item::Table(table) => {
                let header = has_header(table);
                sites_mut(table, header, f);
            }
            Item::ArrayOfTables(array) => array.iter_mut().for_each(|t| sites_mut(t, true, f)),
            Item::None => {}
        }
    }
}

/// For every [`Site`] of the parsed `root`, in order, whether its line ends
/// with CRLF in `source` (`None`: the file ends there, or no span says).
fn line_ends(root: &Table, source: &str) -> Vec<Option<bool>> {
    let mut found = Vec::new();
    sites(root, false, &mut found);
    found
        .iter()
        .map(|site| {
            let (decor, span) = match site {
                Site::Header(table) => (table.decor(), table.span()),
                Site::Value(value) => (value.decor(), value.span()),
            };
            let end = decor
                .suffix()
                .and_then(|s| s.span())
                .filter(|s| !s.is_empty())
                .or(span)?
                .end;
            let rest = source.get(end..)?;
            if rest.starts_with("\r\n") {
                Some(true)
            } else if rest.starts_with('\n') {
                Some(false)
            } else {
                None
            }
        })
        .collect()
}

/// Put back the line endings `ends` recorded: a CRLF line gets its `\r` (as
/// [`CR`]) at the end of its suffix, so `toml_edit`'s own `\n` completes it.
/// A line with no ending in the source (the last one) takes `eol`.
fn keep_line_ends(root: &mut Table, ends: &[Option<bool>], eol: &str) -> Option<()> {
    let mut at = 0;
    sites_mut(root, false, &mut |site| {
        let crlf = ends.get(at).copied().flatten().unwrap_or(eol != "\n");
        at += 1;
        if crlf {
            end_with_cr(site);
        }
    });
    (at == ends.len()).then_some(())
}

fn end_with_cr(site: SiteMut<'_>) {
    let decor = match site {
        SiteMut::Header(table) => table.decor_mut(),
        SiteMut::Value(value) => value.decor_mut(),
    };
    let suffix = decor.suffix().and_then(|s| s.as_str()).unwrap_or("");
    let suffix = format!("{suffix}{CR}");
    decor.set_suffix(suffix);
}

/// `item`, freshly rendered, with every line it writes ending in `eol` (a
/// [`marked`] ending): its lines' own endings, the blank lines in its decor,
/// and no line break left inside a value (a multi-line string is written as
/// one escaped line).
fn use_ending(item: &mut Item, eol: &str) -> Option<()> {
    if eol == "\n" {
        return Some(());
    }
    match item {
        Item::Value(value) => {
            single_line(value)?;
            end_with_cr(SiteMut::Value(value));
        }
        Item::Table(table) => table_ending(table, has_header(table), eol)?,
        Item::ArrayOfTables(array) => {
            for table in array.iter_mut() {
                table_ending(table, true, eol)?;
            }
        }
        Item::None => {}
    }
    Some(())
}

fn table_ending(table: &mut Table, header: bool, eol: &str) -> Option<()> {
    let decor = table.decor_mut();
    if let Some(prefix) = decor.prefix().and_then(|p| p.as_str()) {
        let prefix = prefix.replace('\n', eol);
        decor.set_prefix(prefix);
    }
    if header {
        end_with_cr(SiteMut::Header(table));
    }
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    for key in keys {
        if let Some(mut key) = table.key_mut(&key) {
            let decor = key.leaf_decor_mut();
            if let Some(prefix) = decor.prefix().and_then(|p| p.as_str()) {
                let prefix = prefix.replace('\n', eol);
                decor.set_prefix(prefix);
            }
        }
        use_ending(table.get_mut(&key)?, eol)?;
    }
    Some(())
}

/// `value` written on one line: a string holding a line break becomes an
/// escaped basic string, an array or inline table loses the breaks between
/// its items.
fn single_line(value: &mut Value) -> Option<()> {
    match value {
        Value::String(text) if text.display_repr().contains('\n') => {
            let decor = text.decor().clone();
            let escaped = serde_json::to_string(text.value()).ok()?;
            let mut line: Value = escaped.parse().ok()?;
            *line.decor_mut() = decor;
            *value = line;
        }
        Value::Array(array) => {
            for item in array.iter_mut() {
                flatten_decor(item.decor_mut());
                single_line(item)?;
            }
            array.set_trailing("");
        }
        Value::InlineTable(table) => {
            for (_, item) in table.iter_mut() {
                flatten_decor(item.decor_mut());
                single_line(item)?;
            }
        }
        _ => {}
    }
    Some(())
}

/// A fresh value's decor holds only whitespace: a line break in it becomes a
/// space.
fn flatten_decor(decor: &mut toml_edit::Decor) {
    let flat = |raw: Option<&toml_edit::RawString>| {
        raw.and_then(|r| r.as_str())
            .filter(|s| s.contains('\n'))
            .map(|s| s.replace('\n', " "))
    };
    if let Some(prefix) = flat(decor.prefix()) {
        decor.set_prefix(prefix);
    }
    if let Some(suffix) = flat(decor.suffix()) {
        decor.set_suffix(suffix);
    }
}

/// `canonical`, the designer's own form of a config, with every line ending
/// in `eol`; `None` when it cannot be (it then stays LF).
pub(super) fn with_ending(canonical: &str, eol: &str) -> Option<String> {
    if eol == "\n" || canonical.contains(CR) {
        return None;
    }
    let eol = marked(eol);
    let mut doc: DocumentMut = canonical.parse().ok()?;
    let root = doc.as_table_mut();
    let keys: Vec<String> = root.iter().map(|(k, _)| k.to_owned()).collect();
    for key in keys {
        if let Some(mut key) = root.key_mut(&key) {
            let decor = key.leaf_decor_mut();
            if let Some(prefix) = decor.prefix().and_then(|p| p.as_str()) {
                let prefix = prefix.replace('\n', eol);
                decor.set_prefix(prefix);
            }
        }
        use_ending(root.get_mut(&key)?, eol)?;
    }
    let trailing = doc.trailing().as_str()?.replace('\n', eol);
    doc.set_trailing(trailing);
    Some(doc.to_string().replace(CR, "\r"))
}

/// Whether a `[sinks]` or `[index.sinks]` table names one sink twice, in two
/// spellings the loader reads as the same name (`Primary` and `primary`).
/// Which one the file means is the loader's guess, not the save's.
fn spells_a_sink_twice(root: &Table) -> bool {
    let twice = |item: Option<&Item>| {
        let Some(table) = item.and_then(Item::as_table_like) else {
            return false;
        };
        let mut seen = HashSet::new();
        table.iter().any(|(key, _)| !seen.insert(sink_name(key)))
    };
    let entries = root
        .get("index")
        .and_then(Item::as_array_of_tables)
        .into_iter()
        .flat_map(ArrayOfTables::iter);
    twice(root.get("sinks")) || entries.into_iter().any(|entry| twice(entry.get("sinks")))
}

/// The name the loader reads a sink key as: trimmed and lowercased by
/// [`SinkName`](kernel::common::SinkName). A key it refuses stays as written.
fn sink_name(key: &str) -> String {
    kernel::common::SinkName::try_new(key).map_or_else(|_| key.to_owned(), |name| name.to_string())
}
