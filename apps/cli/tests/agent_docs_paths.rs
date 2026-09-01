#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

//! Drift guard for the agent-facing docs: every repo path they name must exist.
//!
//! `plugin/**` and `.claude/commands/**` are pointer-heavy by design (see
//! `plugin/ARCHITECTURE.md`): each meaning has one home and everything else
//! points at it. A pointer-heavy corpus rots mostly by **moved files** — the
//! target is renamed and the pointer silently dangles, so an agent follows it,
//! finds nothing, and answers from recollection instead.
//!
//! So this walks every markdown file under those trees, pulls out anything that
//! looks like a repo path, and asserts it resolves. It deliberately does **not**
//! check prose against behaviour; that stays a review concern.
//!
//! Paths are recognised in backtick spans and markdown link targets. A
//! candidate counts when it starts with a known top-level directory, or (for a
//! link target) resolves relative to the file holding it. `${CLAUDE_PLUGIN_ROOT}`
//! maps to `plugin/`, which is what it expands to in an install.

use std::path::{Path, PathBuf};

/// Top-level directories a repo-relative path can start with.
const ROOTS: &[&str] = &[
    "libs/", "apps/", "docs/", "dev/", "plugin/", "deploy/", ".claude/", ".github/", ".config/",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("apps/cli is two levels below the workspace root")
        .to_path_buf()
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// A candidate is unusable if it is a URL, a glob, a placeholder, or Rust/shell
/// syntax that merely looks path-shaped (`flusso_user_query::Orders`,
/// `sinks.<name>`, `plugin/skills/…`).
fn is_noise(candidate: &str) -> bool {
    candidate.is_empty()
        || candidate.contains("://")
        || candidate.contains('*')
        || candidate.contains('?')
        || candidate.contains('<')
        || candidate.contains('{')
        || candidate.contains('…')
        || candidate.contains("::")
        || candidate.contains(char::is_whitespace)
}

fn normalize(candidate: &str) -> String {
    candidate
        .trim_end_matches(|c| matches!(c, '.' | ',' | ')' | ':' | ';' | '`' | '/'))
        .replace("${CLAUDE_PLUGIN_ROOT}/", "plugin/")
        .replace("$CLAUDE_PLUGIN_ROOT/", "plugin/")
}

/// Backtick spans plus markdown link targets, as (candidate, is_link) pairs.
fn candidates(body: &str) -> Vec<(String, bool)> {
    let mut found = Vec::new();

    for span in body.split('`').skip(1).step_by(2) {
        found.push((normalize(span), false));
    }

    let mut rest = body;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        if let Some(close) = rest.find(')') {
            let target = &rest[..close];
            found.push((normalize(target.split('#').next().unwrap_or(target)), true));
            rest = &rest[close..];
        }
    }

    found
}

#[test]
fn every_repo_path_named_in_the_agent_docs_exists() {
    let root = workspace_root();
    let mut files = Vec::new();
    markdown_files(&root.join("plugin"), &mut files);
    markdown_files(&root.join(".claude/commands"), &mut files);

    assert!(
        files.len() > 5,
        "expected to find the agent docs under plugin/ and .claude/commands/, found {}",
        files.len()
    );

    let mut dangling = Vec::new();

    for file in &files {
        let body = std::fs::read_to_string(file).unwrap();
        let owner = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string();
        let dir = file.parent().unwrap();

        for (candidate, is_link) in candidates(&body) {
            if is_noise(&candidate) {
                continue;
            }

            let resolved = if ROOTS.iter().any(|r| candidate.starts_with(r)) {
                root.join(&candidate)
            } else if is_link && !candidate.starts_with('/') {
                // A sibling reference like `migration.md` or `examples/consumer.rs`.
                dir.join(&candidate)
            } else {
                continue;
            };

            if !resolved.exists() {
                dangling.push(format!("{owner}: {candidate}"));
            }
        }
    }

    assert!(
        dangling.is_empty(),
        "the agent docs point at paths that no longer exist. Fix the pointer or restore the \
         target:\n  {}",
        dangling.join("\n  ")
    );
}
