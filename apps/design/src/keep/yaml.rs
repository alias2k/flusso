//! Patch a block-style YAML text towards a new value, keeping its comments.
//!
//! Three inputs: the file's `text`, the tree its model renders to (`old`), and
//! the tree the edited model renders to (`new`), plus how the items of its
//! `fields` lists pair up ([`Pairing`]). The text is scanned into its block
//! structure ([`Node`]); then the two trees are compared, and only where they
//! differ is the text touched.
//!
//! **Who owns a comment.** At each block level (the keys of a mapping, the
//! items of a list):
//!
//! - the comment lines directly above a child, with no blank line between, are
//!   that child's (its **lead**);
//! - a comment block a blank line separates from the child below is nobody's
//!   (**free**): it stays where it is, before whatever follows it;
//! - after a child, comment lines indented deeper than the child are inside it;
//!   after the last child, the rest belong to the level (its **tail**), so a
//!   comment after a list's last item, at its dash column or shallower, is the
//!   list's;
//! - a comment at the end of a line is that line's.
//!
//! **What an edit does.** A key or item whose rendering did not change keeps
//! its lines; a key the text writes that the canonical form leaves out (an
//! explicit default) stays, and one the canonical form writes that the text
//! leaves out stays out. A changed block mapping or list is patched
//! recursively, any other changed value is re-rendered in place keeping the
//! comment at the end of its first line and the comments after it. A removed
//! key or item goes with its lead and everything inside it; free comments
//! around it stay. An item that moved takes its lead and inner comments along,
//! and blank lines above it when nothing but blank lines is there.
//!
//! Every comment the patch drops while its owner may still exist (a re-rendered
//! value, an item paired with nothing in a list that also gained one) is
//! counted in [`Patched::dropped`].
//!
//! What the scanner does not understand (flow or quoted keys at a level it has
//! to change, anchors, tabs) makes [`patch`] return `None`, and the caller
//! falls back to the canonical form.

use serde_yaml::{Mapping, Value};

use super::pairing::Pairing;

/// A patched text and how many comments it could not keep.
#[derive(Debug)]
pub(super) struct Patched {
    pub(super) text: String,
    pub(super) dropped: usize,
}

/// `text` patched from `old` to `new`, or `None` when its layout is beyond the
/// scanner. `fields` pairs the root `fields` list (and, through it, every
/// nested one); without it, fields pair by name.
pub(super) fn patch(
    text: &str,
    old: &Value,
    new: &Value,
    fields: Option<&Pairing>,
) -> Option<Patched> {
    let (Value::Mapping(old), Value::Mapping(new)) = (old, new) else {
        return None;
    };
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
        .collect();
    let ends_with_eol = lines.last().is_some_and(String::is_empty);
    if ends_with_eol {
        lines.pop();
    }
    let header_len = lines
        .iter()
        .take_while(|l| is_trivia(l) || l.trim_end() == "---" || l.starts_with('%'))
        .count();
    let (header, body) = lines.split_at(header_len);
    let root = parse_map(body, 0)?;
    let mut patcher = Patcher { dropped: 0 };
    let mut out = header.to_vec();
    out.extend(patcher.map(
        &root,
        old,
        new,
        Ctx {
            retag: false,
            fields,
        },
    )?);
    let mut text = seal_kept_blocks(out).join(eol);
    if ends_with_eol {
        text.push_str(eol);
    }
    Some(Patched {
        text,
        dropped: patcher.dropped,
    })
}

/// Stands, in the patched lines, right after a body that ends inside a
/// keep-chomping block scalar (`|+`). Never a line of YAML.
const KEPT_OPEN: &str = "\u{0}kept-open";

/// `lines` without the [`KEPT_OPEN`] markers and the blank lines right after
/// each: a body ending in a `|+` scalar owns the blank lines that follow it
/// (they are its content), so a blank line the patch puts after it from
/// elsewhere would change its value.
fn seal_kept_blocks(lines: Vec<String>) -> Vec<String> {
    let mut sealed = false;
    lines
        .into_iter()
        .filter(|line| {
            if line == KEPT_OPEN {
                sealed = true;
                return false;
            }
            sealed &= line.trim().is_empty();
            !sealed
        })
        .collect()
}

/// `body`, then [`KEPT_OPEN`] when it ends inside a keep-chomping block scalar.
fn sealed(body: &[String]) -> impl Iterator<Item = String> + '_ {
    let open = ends_in_kept_block(body).then(|| KEPT_OPEN.to_owned());
    body.iter().cloned().chain(open)
}

/// Whether the last lines of `body` are inside a keep-chomping block scalar.
fn ends_in_kept_block(body: &[String]) -> bool {
    let mut block: Option<(usize, bool)> = None;
    for line in body {
        if let Some((at, _)) = block {
            if line.trim().is_empty() || indent(line) > at {
                continue;
            }
            block = None;
        }
        if let Some(indicator) = block_indicator(split_comment(line).0) {
            block = Some((block_parent_col(line), indicator.contains('+')));
        }
    }
    block.is_some_and(|(_, keep)| keep)
}

/// A block node as written, with the lines it owns.
#[derive(Debug)]
enum Node {
    Map(MapNode),
    Seq(SeqNode),
    /// A scalar, a flow collection, or a block the scanner leaves alone.
    Opaque,
}

#[derive(Debug)]
struct MapNode {
    entries: Vec<Entry>,
    /// The comments after the last entry that are the mapping's own.
    tail: Vec<String>,
}

/// The trivia above a child: what nobody owns, then the child's own comments.
#[derive(Debug, Default)]
struct Above {
    /// Blank lines and blank-separated comments: they stay where they are.
    free: Vec<String>,
    /// The comment block directly above the child: the child's.
    lead: Vec<String>,
}

#[derive(Debug)]
struct Entry {
    key: String,
    col: usize,
    above: Above,
    /// The key line, then every line of its value.
    body: Vec<String>,
    value: Node,
}

#[derive(Debug)]
struct SeqNode {
    col: usize,
    items: Vec<Item>,
    tail: Vec<String>,
}

#[derive(Debug)]
struct Item {
    above: Above,
    /// The dash line, then the rest of the item.
    body: Vec<String>,
    /// The item's block mapping, when it is one.
    map: Option<ItemMap>,
}

#[derive(Debug)]
struct ItemMap {
    map: MapNode,
    /// The dash line's text up to the mapping, when the mapping starts on it
    /// (scanned with the dash replaced by spaces); `None` when the dash stands
    /// on a line of its own and the mapping starts below.
    dash: Option<String>,
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_trivia(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn is_comment(line: &str) -> bool {
    line.trim_start_matches(' ').starts_with('#')
}

/// The plain key at `col` and the text after its colon. Spaces between the
/// key and the colon are allowed.
fn key_at(line: &str, col: usize) -> Option<(&str, &str)> {
    if indent(line) != col {
        return None;
    }
    let content = line.get(col..)?;
    let end = content
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(content.len());
    let key = content.get(..end)?;
    if key.is_empty() || key.starts_with('-') {
        return None;
    }
    let rest = content
        .get(end..)?
        .trim_start_matches(' ')
        .strip_prefix(':')?;
    (rest.is_empty() || rest.starts_with(' ')).then_some((key, rest))
}

/// Whether `line` holds a sequence dash at `col`.
fn dash_at(line: &str, col: usize) -> bool {
    indent(line) == col
        && line
            .get(col..)
            .is_some_and(|c| c == "-" || c.starts_with("- "))
}

/// The block scalar indicator ending `content` (`|`, `>-`, `|+2`), if any.
fn block_indicator(content: &str) -> Option<&str> {
    content
        .trim_end()
        .rsplit([' ', '-', ':'])
        .next()
        .filter(|last| {
            last.starts_with(['|', '>'])
                && last
                    .chars()
                    .skip(1)
                    .all(|c| c == '+' || c == '-' || c.is_ascii_digit())
        })
}

/// Whether `line` opens a block scalar with keep chomping (`|+`, `>2+`).
fn keeps_trailing_lines(line: &str) -> bool {
    block_indicator(split_comment(line).0).is_some_and(|ind| ind.contains('+'))
}

/// The column a line's content starts at, past its spaces and sequence dashes.
fn content_col(line: &str) -> usize {
    let mut col = indent(line);
    while line
        .get(col..)
        .is_some_and(|rest| rest == "-" || rest.starts_with("- "))
    {
        col += 1;
        col += indent(line.get(col..).unwrap_or(""));
    }
    col
}

/// The indentation a block scalar opened on `line` is relative to: its
/// content is indented deeper. The key's column (`key: |`), else the dash's
/// (`- |`, the item is the scalar).
fn block_parent_col(line: &str) -> usize {
    let content = split_comment(line).0.trim_end();
    let head = block_indicator(content)
        .and_then(|ind| content.get(..content.len() - ind.len()))
        .unwrap_or(content)
        .trim_end();
    match head.strip_suffix('-') {
        Some(before) if before.ends_with(' ') || before.is_empty() => before.len(),
        _ => content_col(line),
    }
}

/// How many comments `lines` hold, skipping the content of block scalars.
fn comments_in(lines: &[String]) -> usize {
    let mut count = 0;
    let mut block: Option<usize> = None;
    for line in lines {
        if let Some(at) = block {
            if line.trim().is_empty() || indent(line) > at {
                continue;
            }
            block = None;
        }
        let (content, comment) = split_comment(line);
        if !comment.is_empty() {
            count += 1;
        }
        if block_indicator(content).is_some() {
            block = Some(block_parent_col(line));
        }
    }
    count
}

/// The text of `s` before its trailing comment, and the comment with the space
/// before it.
fn split_comment(s: &str) -> (&str, &str) {
    let mut quote: Option<char> = None;
    let mut prev_space = true;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '#' && prev_space => {
                let start = s
                    .get(..i)
                    .map_or(i, |before| before.trim_end_matches(' ').len());
                return (s.get(..start).unwrap_or(s), s.get(start..).unwrap_or(""));
            }
            None if c == '\'' || c == '"' => quote = Some(c),
            None => {}
        }
        prev_space = c == ' ';
    }
    (s, "")
}

/// The comment ending a node's first line, if any.
fn first_line_comment(line: &str, col: usize) -> &str {
    split_comment(line.get(col..).unwrap_or("")).1
}

/// The trivia above a child split into free lines and its lead: the comment
/// lines directly above it.
fn above(run: &[String]) -> Above {
    let direct = run
        .iter()
        .rev()
        .take_while(|l| !l.trim().is_empty())
        .count();
    let (free, lead) = run.split_at(run.len() - direct);
    Above {
        free: free.to_vec(),
        lead: lead.to_vec(),
    }
}

/// One child of a block level: the trivia above it, then its lines.
type Piece = (Above, Vec<String>);

/// The children of the block level at `col` in `lines`, and the level's tail.
///
/// `starts` says whether a line opens a child; `compact` whether a child may
/// continue with dash lines at `col` (a mapping key whose value is a sequence
/// written at the key's own indentation).
fn segment(
    lines: &[String],
    col: usize,
    starts: impl Fn(&str) -> bool,
    compact: impl Fn(&str) -> bool,
) -> Option<(Vec<Piece>, Vec<String>)> {
    let mut i = lines.iter().take_while(|l| is_trivia(l)).count();
    let mut children = Vec::new();
    let mut run = lines.get(..i)?.to_vec();
    while i < lines.len() {
        let first = lines.get(i)?;
        if !starts(first) {
            return None;
        }
        let continues_compact = compact(first);
        let mut j = i + 1;
        while let Some(line) = lines.get(j) {
            let d = indent(line);
            if is_trivia(line) || d > col || (d == col && continues_compact && dash_at(line, col)) {
                j += 1;
            } else {
                break;
            }
        }
        let last = (i..j)
            .rev()
            .find(|&k| lines.get(k).is_some_and(|l| !is_trivia(l)))?;
        // Comments indented deeper than the child are inside it; a
        // keep-chomping block scalar (`|+`) owns the blank lines right after it.
        let blanks = if lines.get(i..j)?.iter().any(|l| keeps_trailing_lines(l)) {
            lines
                .get(last + 1..j)?
                .iter()
                .take_while(|l| l.trim().is_empty())
                .count()
        } else {
            0
        };
        // A list written at its key's column ends the level: a comment after
        // its last item, at the dash column, is the list's.
        let compact_list =
            continues_compact && lines.get(i + 1..j)?.iter().any(|l| dash_at(l, col));
        let inside = if compact_list && j == lines.len() {
            col
        } else {
            col + 1
        };
        let keep = (last + 1..j)
            .rev()
            .find(|&k| {
                lines
                    .get(k)
                    .is_some_and(|l| is_comment(l) && indent(l) >= inside)
            })
            .map_or(last + 1, |k| k + 1)
            .max(last + 1 + blanks);
        children.push((above(&run), lines.get(i..keep)?.to_vec()));
        run = lines.get(keep..j)?.to_vec();
        i = j;
    }
    Some((children, run))
}

fn parse_map(lines: &[String], col: usize) -> Option<MapNode> {
    let (children, tail) = segment(
        lines,
        col,
        |l| key_at(l, col).is_some(),
        |l| key_at(l, col).is_some_and(|(_, rest)| split_comment(rest).0.trim().is_empty()),
    )?;
    if children.is_empty() {
        return None;
    }
    let entries = children
        .into_iter()
        .map(|(above, body)| {
            let (key, rest) = key_at(body.first()?, col)?;
            let key = key.to_owned();
            let value = if split_comment(rest).0.trim().is_empty() {
                parse_value(body.get(1..)?, col)
            } else {
                Node::Opaque
            };
            Some(Entry {
                key,
                col,
                above,
                body,
                value,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(MapNode { entries, tail })
}

/// The block value under a key at `col`, from the lines after the key.
fn parse_value(lines: &[String], col: usize) -> Node {
    let Some(first) = lines.iter().find(|l| !is_trivia(l)) else {
        return Node::Opaque;
    };
    let at = indent(first);
    if at >= col && dash_at(first, at) {
        parse_seq(lines, at).map_or(Node::Opaque, Node::Seq)
    } else if at > col && key_at(first, at).is_some() {
        parse_map(lines, at).map_or(Node::Opaque, Node::Map)
    } else {
        Node::Opaque
    }
}

fn parse_seq(lines: &[String], col: usize) -> Option<SeqNode> {
    let (children, tail) = segment(lines, col, |l| dash_at(l, col), |_| false)?;
    let items = children
        .into_iter()
        .map(|(above, body)| {
            let map = item_map(&body, col);
            Item { above, body, map }
        })
        .collect();
    Some(SeqNode { col, items, tail })
}

/// The block mapping an item at dash column `col` holds, if it is one.
fn item_map(body: &[String], col: usize) -> Option<ItemMap> {
    let first = body.first()?;
    let after = first.get(col + 1..).unwrap_or("");
    if split_comment(after).0.trim().is_empty() {
        let rest = body.get(1..)?;
        let at = indent(rest.iter().find(|l| !is_trivia(l))?);
        return (at > col)
            .then(|| parse_map(rest, at))
            .flatten()
            .map(|map| ItemMap { map, dash: None });
    }
    let item_col = col + 1 + indent(after);
    let normalized = format!("{}{}", " ".repeat(item_col), first.get(item_col..)?);
    key_at(&normalized, item_col)?;
    let mut spaced = body.to_vec();
    *spaced.first_mut()? = normalized;
    Some(ItemMap {
        map: parse_map(&spaced, item_col)?,
        dash: Some(first.get(..item_col)?.to_owned()),
    })
}

/// `key: value` at `col`, as serde_yaml prints it.
fn render_entry(key: &str, value: &Value, col: usize) -> Option<Vec<String>> {
    let mut map = Mapping::new();
    map.insert(Value::from(key), value.clone());
    render(&Value::Mapping(map), col)
}

/// `- value` with its dash at `col`.
fn render_item(value: &Value, col: usize) -> Option<Vec<String>> {
    render(&Value::Sequence(vec![value.clone()]), col)
}

fn render(value: &Value, col: usize) -> Option<Vec<String>> {
    let text = serde_yaml::to_string(value).ok()?;
    let pad = " ".repeat(col);
    let lines: Vec<String> = quote_kept_blocks(text.lines().map(str::to_owned).collect())?
        .into_iter()
        .map(|l| format!("{pad}{l}"))
        .collect();
    Some(sealed(&lines).collect())
}

/// `lines` with every keep-chomping block scalar (`|+`) written as a
/// double-quoted string: written into a file, its trailing blank lines would
/// swallow the blank lines that follow it.
fn quote_kept_blocks(lines: Vec<String>) -> Option<Vec<String>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        i += 1;
        let Some(indicator) = block_indicator(line).filter(|ind| ind.contains('+')) else {
            out.push(line.clone());
            continue;
        };
        let head = line.get(..line.len() - indicator.len())?;
        let owner = head.get(content_col(head)..).unwrap_or("").trim_end();
        let plain_key = owner.strip_suffix(':').is_some_and(|key| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        });
        if !(owner.is_empty() || plain_key) {
            out.push(line.clone());
            continue;
        }
        let parent = content_col(line);
        let explicit = indicator
            .chars()
            .find(|c| c.is_ascii_digit())
            .and_then(|c| c.to_digit(10));
        let first = lines.get(i..)?.iter().find(|l| !l.trim().is_empty());
        let at = match explicit {
            Some(n) => parent + usize::try_from(n).ok()?,
            None => first.map_or(parent + 1, |l| indent(l)),
        };
        let mut content = String::new();
        while let Some(next) = lines.get(i) {
            if !next.trim().is_empty() && indent(next) < at {
                break;
            }
            content.push_str(next.get(at..).unwrap_or(""));
            content.push('\n');
            i += 1;
        }
        let quoted = serde_json::to_string(&content).ok()?;
        out.push(format!("{head}{quoted}"));
    }
    Some(out)
}

/// `lines` with `comment` put back at the end of the first one.
fn with_comment(mut lines: Vec<String>, comment: &str) -> Vec<String> {
    if let Some(first) = lines.first_mut() {
        first.push_str(comment);
    }
    lines
}

/// The comment lines ending `body`, after its last line of content.
fn trailing_comments(body: &[String]) -> &[String] {
    let content = body
        .iter()
        .rposition(|l| !is_trivia(l))
        .map_or(0, |i| i + 1);
    body.get(content..).unwrap_or(&[])
}

/// How the free trivia of a level's children is placed once some are dropped
/// or moved: each surviving child that stays put takes the free lines of the
/// dropped and moved ones above it, the rest goes before the tail.
struct Carry {
    before: Vec<Vec<String>>,
    end: Vec<String>,
}

impl Carry {
    /// `fate[i]`: `Some(true)` stays put, `Some(false)` moves (taking its free
    /// lines when they are only blank), `None` is dropped.
    fn new<'a>(free: impl Iterator<Item = &'a [String]>, fate: &[Option<bool>]) -> Self {
        let mut before = Vec::new();
        let mut pending = Vec::new();
        for (free, fate) in free.zip(fate) {
            let has_comment = free.iter().any(|l| is_comment(l));
            match fate {
                Some(true) => {
                    join_free(&mut pending, free);
                    before.push(std::mem::take(&mut pending));
                }
                _ if has_comment => {
                    join_free(&mut pending, free);
                    before.push(Vec::new());
                }
                _ => before.push(Vec::new()),
            }
        }
        Self {
            before,
            end: pending,
        }
    }
}

/// `free` appended to `pending` without doubling the blank line between them.
fn join_free(pending: &mut Vec<String>, free: &[String]) {
    let blank = |l: &String| l.trim().is_empty();
    let skip = if pending.last().is_some_and(blank) {
        free.iter().take_while(|l| blank(l)).count()
    } else {
        0
    };
    pending.extend(free.iter().skip(skip).cloned());
}

/// Where a map is in the schema: whether its first key is a field's type tag,
/// and how the items of its `fields` list pair.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    retag: bool,
    fields: Option<&'a Pairing>,
}

struct Patcher {
    dropped: usize,
}

impl Patcher {
    fn map(
        &mut self,
        node: &MapNode,
        old: &Mapping,
        new: &Mapping,
        ctx: Ctx,
    ) -> Option<Vec<String>> {
        if new.is_empty() {
            return None;
        }
        // A field's type tag is its first key: a type change renames that key
        // in place rather than removing one entry and adding another.
        let retag = match (old.keys().next(), new.keys().next()) {
            (Some(Value::String(was)), Some(Value::String(is))) if ctx.retag && was != is => {
                Some((was.as_str(), is.as_str()))
            }
            _ => None,
        };
        let fate: Vec<Option<bool>> = node
            .entries
            .iter()
            .map(|entry| {
                let key = entry.key.as_str();
                let removed = retag.is_none_or(|(was, _)| was != key)
                    && new.get(key).is_none()
                    && old.get(key).is_some();
                (!removed).then_some(true)
            })
            .collect();
        let carry = Carry::new(node.entries.iter().map(|e| &e.above.free[..]), &fate);
        let mut chunks: Vec<(String, Vec<String>)> = Vec::new();
        let mut present: Vec<&str> = Vec::new();
        for ((entry, fate), carried) in node.entries.iter().zip(&fate).zip(&carry.before) {
            let key = entry.key.as_str();
            present.push(key);
            if fate.is_none() {
                continue;
            }
            let mut lines = carried.clone();
            lines.extend(entry.above.lead.iter().cloned());
            if let Some((_, is)) = retag.filter(|(was, _)| *was == key) {
                let comment = first_line_comment(entry.body.first()?, entry.col);
                lines.extend(with_comment(
                    render_entry(is, new.get(is)?, entry.col)?,
                    comment,
                ));
                lines.extend(trailing_comments(&entry.body).iter().cloned());
                present.push(is);
                chunks.push((is.to_owned(), lines));
                continue;
            }
            let before = old.get(key);
            match new.get(key) {
                Some(after) if before != Some(after) => {
                    lines.extend(self.entry(entry, before, after, ctx)?);
                }
                _ => lines.extend(sealed(&entry.body)),
            }
            chunks.push((key.to_owned(), lines));
        }
        let col = node.entries.first()?.col;
        let mut preceding: Option<&str> = None;
        for (key, value) in new {
            let Value::String(key) = key else {
                return None;
            };
            // A default the text leaves out stays out, unless the type changed:
            // then it may not be the default any more.
            let default_kept = retag.is_none() && old.get(key.as_str()) == Some(value);
            if !present.contains(&key.as_str()) && !default_kept {
                let at = preceding
                    .and_then(|p| chunks.iter().position(|(k, _)| k == p))
                    .map_or(0, |i| i + 1);
                chunks.insert(at, (key.clone(), render_entry(key, value, col)?));
            }
            if chunks.iter().any(|(k, _)| k == key) {
                preceding = Some(key);
            }
        }
        let mut out: Vec<String> = chunks.into_iter().flat_map(|(_, lines)| lines).collect();
        out.extend(carry.end);
        out.extend(node.tail.iter().cloned());
        Some(out)
    }

    /// An entry whose value changed: its key line and value, patched.
    fn entry(
        &mut self,
        entry: &Entry,
        before: Option<&Value>,
        after: &Value,
        ctx: Ctx,
    ) -> Option<Vec<String>> {
        let key_line = entry.body.first()?;
        let comment = first_line_comment(key_line, entry.col);
        let nested = match (&entry.value, before, after) {
            (Node::Map(map), Some(Value::Mapping(was)), Value::Mapping(is)) => self
                .map(
                    map,
                    was,
                    is,
                    Ctx {
                        retag: false,
                        fields: None,
                    },
                )
                .map(|lines| (key_line.clone(), lines)),
            (Node::Seq(seq), Some(Value::Sequence(was)), Value::Sequence(is)) => {
                let fields = entry.key == "fields";
                let local;
                let pairing = match ctx.fields.filter(|_| fields) {
                    Some(pairing) => pairing,
                    None => {
                        local = pair_values(was, is, fields);
                        &local
                    }
                };
                self.seq(seq, was, is, pairing, fields).and_then(|lines| {
                    let key_line = if is.is_empty() {
                        with_comment(render_entry(&entry.key, after, entry.col)?, comment)
                            .into_iter()
                            .next()?
                    } else {
                        key_line.clone()
                    };
                    Some((key_line, lines))
                })
            }
            _ => None,
        };
        let mut lines = Vec::new();
        match nested {
            Some((key_line, value)) => {
                lines.push(key_line);
                lines.extend(value);
            }
            None => {
                let trailing = trailing_comments(&entry.body);
                let replaced = entry.body.get(..entry.body.len() - trailing.len())?;
                self.dropped += comments_in(replaced) - usize::from(!comment.is_empty());
                lines.extend(with_comment(
                    render_entry(&entry.key, after, entry.col)?,
                    comment,
                ));
                lines.extend(trailing.iter().cloned());
            }
        }
        Some(lines)
    }

    fn seq(
        &mut self,
        node: &SeqNode,
        old: &[Value],
        new: &[Value],
        pairing: &Pairing,
        fields: bool,
    ) -> Option<Vec<String>> {
        if node.items.len() != old.len() || pairing.pairs.len() != new.len() {
            return None;
        }
        let stays = stationary(&pairing.pairs, old.len());
        let fate: Vec<Option<bool>> = (0..old.len())
            .map(|i| {
                pairing
                    .pairs
                    .contains(&Some(i))
                    .then(|| stays.get(i).copied().unwrap_or(false))
            })
            .collect();
        for (i, item) in node.items.iter().enumerate() {
            if fate.get(i)?.is_none() && pairing.doubtful.get(i).copied().unwrap_or(false) {
                self.dropped += comments_in(&item.above.lead) + comments_in(&item.body);
            }
        }
        let carry = Carry::new(node.items.iter().map(|i| &i.above.free[..]), &fate);
        let mut out = Vec::new();
        for (j, (after, pair)) in new.iter().zip(&pairing.pairs).enumerate() {
            let Some(i) = *pair else {
                out.extend(render_item(after, node.col)?);
                continue;
            };
            let item = node.items.get(i)?;
            match fate.get(i)? {
                Some(true) => out.extend(carry.before.get(i)?.iter().cloned()),
                // Blank lines move with the item, except to the top of the list.
                _ if j > 0 && !item.above.free.iter().any(|l| is_comment(l)) => {
                    out.extend(item.above.free.iter().cloned());
                }
                _ => {}
            }
            out.extend(item.above.lead.iter().cloned());
            let before = old.get(i)?;
            if before == after {
                out.extend(sealed(&item.body));
                continue;
            }
            let ctx = Ctx {
                retag: fields,
                fields: pairing.child(j).filter(|_| fields),
            };
            let nested = match (&item.map, before, after) {
                (Some(map), Value::Mapping(was), Value::Mapping(is)) => self
                    .map(&map.map, was, is, ctx)
                    .and_then(|lines| match &map.dash {
                        Some(prefix) => with_dash(lines, prefix),
                        None => {
                            let mut out = vec![item.body.first()?.clone()];
                            out.extend(lines);
                            Some(out)
                        }
                    }),
                _ => None,
            };
            match nested {
                Some(lines) => out.extend(lines),
                None => {
                    let comment = first_line_comment(item.body.first()?, node.col);
                    let trailing = trailing_comments(&item.body);
                    let replaced = item.body.get(..item.body.len() - trailing.len())?;
                    self.dropped += comments_in(replaced) - usize::from(!comment.is_empty());
                    out.extend(with_comment(render_item(after, node.col)?, comment));
                    out.extend(trailing.iter().cloned());
                }
            }
        }
        out.extend(carry.end);
        out.extend(node.tail.iter().cloned());
        Some(out)
    }
}

/// Pair a list's items by field name (a `fields` list) or by value.
fn pair_values(old: &[Value], new: &[Value], fields: bool) -> Pairing {
    if fields {
        Pairing::matching(old, new, |was, is| {
            field_name(was).is_some() && field_name(was) == field_name(is)
        })
    } else {
        Pairing::matching(old, new, |was, is| was == is)
    }
}

/// A field item's document name: the value of its first (type) key.
fn field_name(value: &Value) -> Option<&str> {
    match value {
        Value::Mapping(map) => map.values().next()?.as_str(),
        _ => None,
    }
}

/// For each old item, whether it keeps its place: the paired items in the
/// longest run that keeps its old order do; the other paired ones moved.
pub(super) fn stationary(pairs: &[Option<usize>], old_len: usize) -> Vec<bool> {
    let order: Vec<usize> = pairs.iter().flatten().copied().collect();
    let n = order.len();
    // Longest increasing subsequence, O(n²): lists here are short. `runs[k]`
    // is the length of the longest one ending at `k` and the item before it.
    let mut runs: Vec<(usize, Option<usize>)> = Vec::with_capacity(n);
    for (k, item) in order.iter().enumerate() {
        let best = (0..k)
            .filter(|&m| order.get(m) < Some(item))
            .filter_map(|m| runs.get(m).map(|&(len, _)| (len, m)))
            .max_by_key(|&(len, m)| (len, std::cmp::Reverse(m)));
        runs.push(best.map_or((1, None), |(len, m)| (len + 1, Some(m))));
    }
    let mut stays = vec![false; old_len];
    let mut at = (0..n).max_by_key(|&k| (runs.get(k).map_or(0, |r| r.0), std::cmp::Reverse(k)));
    while let Some(k) = at {
        if let Some(flag) = order.get(k).and_then(|&i| stays.get_mut(i)) {
            *flag = true;
        }
        at = runs.get(k).and_then(|r| r.1);
    }
    stays
}

/// Put the item's dash back on the first line of its patched mapping.
fn with_dash(mut lines: Vec<String>, prefix: &str) -> Option<Vec<String>> {
    let first = lines.iter_mut().find(|l| !is_trivia(l))?;
    let rest = first.get(prefix.len()..)?;
    if !first.get(..prefix.len())?.chars().all(|c| c == ' ') {
        return None;
    }
    *first = format!("{prefix}{rest}");
    Some(lines)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_after_a_value_is_split_off() {
        assert_eq!(split_comment("a: b # c"), ("a: b", " # c"));
        assert_eq!(split_comment("a: 'b # c'"), ("a: 'b # c'", ""));
        assert_eq!(split_comment("a: b#c"), ("a: b#c", ""));
        assert_eq!(
            split_comment("a: \"x\\\" # y\" # z"),
            ("a: \"x\\\" # y\"", " # z")
        );
    }

    #[test]
    fn an_unchanged_tree_keeps_the_text_byte_for_byte() {
        let text = "# head\n\nversion: 1 # one\nfields:\n  # first\n  - keyword: a\n    required: true\n\n  - text: b\n";
        let value: Value = serde_yaml::from_str(text).unwrap();
        assert_eq!(patch(text, &value, &value, None).unwrap().text, text);
    }

    #[test]
    fn comments_in_block_scalars_are_content() {
        let lines: Vec<String> =
            "- keyword: a # one\n  script: |\n    # not a comment\n    x\n  # two\n"
                .lines()
                .map(str::to_owned)
                .collect();
        assert_eq!(comments_in(&lines), 2);
    }

    #[test]
    fn the_longest_ordered_run_stays_put() {
        assert_eq!(
            stationary(&[Some(2), Some(0), Some(1)], 3),
            [true, true, false]
        );
        assert_eq!(
            stationary(&[Some(0), None, Some(2)], 3),
            [true, false, true]
        );
    }
}
