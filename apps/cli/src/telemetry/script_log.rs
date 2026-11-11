//! The loud log for script failures: every one is logged at `error` with the
//! script line and a caret, rate-limited per (index, field) so a broken script
//! during a backfill of a million rows doesn't drown the log.
//!
//! The first failure of a field in each window is logged in full; the rest are
//! counted and reported with the next one logged. The failing document itself
//! goes to `debug` only, since it may hold personal data.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use daemon::{IndexName, Observer};
use source::document::BuildFailure;

/// How often one field's failure is logged in full.
const WINDOW: Duration = Duration::from_secs(60);

/// The field path with array positions folded (`variants[2].x` → `variants[].x`),
/// so one broken field is one log stream and one metric series.
pub(crate) fn field_label(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut in_brackets = false;
    for c in field.chars() {
        match c {
            '[' => {
                in_brackets = true;
                out.push('[');
            }
            ']' => {
                in_brackets = false;
                out.push(']');
            }
            _ if in_brackets => {}
            c => out.push(c),
        }
    }
    out
}

/// Logs script failures; see the module docs.
#[derive(Debug, Default)]
pub(crate) struct ScriptFailureLog {
    windows: Mutex<HashMap<(String, String), Window>>,
}

#[derive(Debug)]
struct Window {
    started: Instant,
    suppressed: u64,
}

impl ScriptFailureLog {
    /// Whether this failure is logged in full, and how many were suppressed
    /// since the last one that was.
    fn admit(&self, index: &str, field: &str, now: Instant) -> Option<u64> {
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (index.to_owned(), field.to_owned());
        match windows.get_mut(&key) {
            Some(window) if now.duration_since(window.started) < WINDOW => {
                window.suppressed += 1;
                None
            }
            Some(window) => {
                let suppressed = window.suppressed;
                *window = Window {
                    started: now,
                    suppressed: 0,
                };
                Some(suppressed)
            }
            None => {
                windows.insert(
                    key,
                    Window {
                        started: now,
                        suppressed: 0,
                    },
                );
                Some(0)
            }
        }
    }
}

impl Observer for ScriptFailureLog {
    fn on_document_failed(&self, index: &IndexName, id: &str, failure: &BuildFailure) {
        let field = field_label(&failure.field);
        match self.admit(index.as_ref(), &field, Instant::now()) {
            Some(suppressed) => tracing::error!(
                target: "script",
                index = %index,
                id,
                field = %failure.field,
                suppressed,
                "script failed; the document keeps its indexed version (on_error = skip)\n{failure}"
            ),
            None => tracing::debug!(
                target: "script",
                index = %index,
                id,
                field = %failure.field,
                "script failed (suppressed): {}",
                failure.message
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_positions_fold_into_one_label() {
        assert_eq!(field_label("variants[12].price"), "variants[].price");
        assert_eq!(field_label("a[0].b[3].c"), "a[].b[].c");
        assert_eq!(field_label("label"), "label");
    }

    #[test]
    fn one_field_is_logged_once_per_window_with_the_suppressed_count() {
        let log = ScriptFailureLog::default();
        let start = Instant::now();
        assert_eq!(log.admit("users", "label", start), Some(0));
        assert_eq!(log.admit("users", "label", start), None);
        assert_eq!(log.admit("users", "label", start), None);
        assert_eq!(
            log.admit("users", "other", start),
            Some(0),
            "another field has its own window"
        );
        assert_eq!(log.admit("users", "label", start + WINDOW), Some(2));
    }
}
