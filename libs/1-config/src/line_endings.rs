//! Line endings a checkout converted, read as the ones flusso writes.
//!
//! git on Windows (`core.autocrlf`) or an editor may turn every `\n` of
//! `flusso.toml` or `flusso.lock` into `\r\n`. TOML keeps that `\r` inside a
//! multi-line string, so the converted file would hold other values than its
//! LF twin (issue #488). Both are read through [`lf_line_breaks`] first, so a
//! converted file is the same config. A `*.schema.yml` needs nothing: YAML
//! reads a CRLF line break as it reads an LF one.
//!
//! Only a `\r` directly before a `\n` goes. A carriage return a value holds is
//! written as the escape `\r`, which this never touches, and a bare `\r` byte
//! stays for the TOML parser to refuse, as it always did.

use std::borrow::Cow;

/// `text` with every CRLF line break as LF; borrowed when it holds none.
pub(crate) fn lf_line_breaks(text: &str) -> Cow<'_, str> {
    if text.contains("\r\n") {
        Cow::Owned(text.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(text)
    }
}
