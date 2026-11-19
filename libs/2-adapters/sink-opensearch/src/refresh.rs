//! The indexes writes left unrefreshed, and which of them a caught-up write
//! must refresh itself.
//!
//! A write that isn't caught up forces no refresh and marks the indexes it
//! wrote. A caught-up write makes everything written by the writes that
//! returned before it searchable: its own bulk carries `?refresh=true`, and it
//! refreshes every marked index that bulk did not already cover.
//!
//! A `?refresh=true` bulk refreshes only the shards it touched, so it covers
//! a marked index only when it wrote that index, the index has one primary
//! shard, and the mark was made before the bulk was sent (a write that
//! returned later may have written after that refresh).
//!
//! ```text
//! mark users (seq 0)         an earlier unforced write returned
//! caught-up write: point 1   bulk users ?refresh=true, single shard
//! take_uncovered             users@0 < 1, written, single shard: covered
//! ```

use std::collections::BTreeMap;

/// The marked indexes, each with the sequence number of its latest mark.
#[derive(Debug, Default)]
pub(crate) struct Unrefreshed {
    entries: BTreeMap<String, u64>,
    next: u64,
}

impl Unrefreshed {
    /// Mark `indexes` as written by a write that forced no refresh.
    pub(crate) fn mark(&mut self, indexes: impl IntoIterator<Item = String>) {
        for index in indexes {
            let seq = self.next;
            self.next += 1;
            self.entries.insert(index, seq);
        }
    }

    /// The point a caught-up write notes before sending its bulk: a mark
    /// below it was made before.
    pub(crate) fn point(&self) -> u64 {
        self.next
    }

    /// Take the marks of the indexes the caught-up write covers (`in_scope`),
    /// and return the ones it must refresh itself: all but those its bulk,
    /// sent at `sent_at`, covered (`covered`: it wrote that index, which has
    /// one primary shard). Marks out of scope stay for a later caught-up
    /// write of their own index.
    pub(crate) fn take_uncovered(
        &mut self,
        sent_at: u64,
        in_scope: impl Fn(&str) -> bool,
        covered: impl Fn(&str) -> bool,
    ) -> BTreeMap<String, u64> {
        let (taken, kept): (BTreeMap<String, u64>, BTreeMap<String, u64>) =
            std::mem::take(&mut self.entries)
                .into_iter()
                .partition(|(index, _)| in_scope(index));
        self.entries = kept;
        taken
            .into_iter()
            .filter(|(index, seq)| !(*seq < sent_at && covered(index)))
            .collect()
    }

    /// Put back marks a failed refresh left, keeping any newer mark.
    pub(crate) fn restore(&mut self, entries: BTreeMap<String, u64>) {
        for (index, seq) in entries {
            let kept = self.entries.entry(index).or_insert(seq);
            *kept = (*kept).max(seq);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn names(entries: &BTreeMap<String, u64>) -> Vec<&str> {
        entries.keys().map(String::as_str).collect()
    }

    #[test]
    fn a_single_shard_index_the_bulk_wrote_is_covered() {
        let mut unrefreshed = Unrefreshed::default();
        unrefreshed.mark(["users".to_owned(), "orders".to_owned()]);
        let sent_at = unrefreshed.point();
        let left = unrefreshed.take_uncovered(sent_at, |_| true, |index| index == "users");
        assert_eq!(
            names(&left),
            ["orders"],
            "orders wasn't written by the bulk"
        );
    }

    #[test]
    fn a_mark_made_after_the_bulk_was_sent_is_refreshed() {
        let mut unrefreshed = Unrefreshed::default();
        let sent_at = unrefreshed.point();
        unrefreshed.mark(["users".to_owned()]);
        let left = unrefreshed.take_uncovered(sent_at, |_| true, |_| true);
        assert_eq!(
            names(&left),
            ["users"],
            "a write that returned after the bulk's refresh may have written after it"
        );
    }

    #[test]
    fn an_index_marked_again_after_the_bulk_was_sent_is_refreshed() {
        let mut unrefreshed = Unrefreshed::default();
        unrefreshed.mark(["users".to_owned()]);
        let sent_at = unrefreshed.point();
        unrefreshed.mark(["users".to_owned()]);
        let left = unrefreshed.take_uncovered(sent_at, |_| true, |_| true);
        assert_eq!(names(&left), ["users"]);
    }

    #[test]
    fn an_index_of_several_shards_is_never_covered() {
        let mut unrefreshed = Unrefreshed::default();
        unrefreshed.mark(["users".to_owned()]);
        let sent_at = unrefreshed.point();
        let left = unrefreshed.take_uncovered(sent_at, |_| true, |_| false);
        assert_eq!(names(&left), ["users"]);
    }

    #[test]
    fn a_mark_out_of_scope_stays_for_its_own_index() {
        let mut unrefreshed = Unrefreshed::default();
        unrefreshed.mark(["users".to_owned(), "orders".to_owned()]);
        let sent_at = unrefreshed.point();
        let left = unrefreshed.take_uncovered(sent_at, |index| index == "users", |_| false);
        assert_eq!(names(&left), ["users"], "only users is refreshed");
        let later = unrefreshed.take_uncovered(sent_at, |_| true, |_| false);
        assert_eq!(names(&later), ["orders"], "orders is still marked");
    }

    #[test]
    fn a_restored_mark_keeps_a_newer_one() {
        let mut unrefreshed = Unrefreshed::default();
        unrefreshed.mark(["users".to_owned()]);
        let taken = unrefreshed.take_uncovered(unrefreshed.point(), |_| true, |_| false);
        unrefreshed.mark(["users".to_owned()]);
        let newer = unrefreshed.point() - 1;
        unrefreshed.restore(taken);
        let left = unrefreshed.take_uncovered(newer, |_| true, |_| true);
        assert_eq!(
            names(&left),
            ["users"],
            "the newer mark still needs a refresh"
        );
    }
}
