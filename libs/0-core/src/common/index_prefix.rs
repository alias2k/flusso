//! Validation for the deployment-wide index prefix.
//!
//! flusso can prepend a literal prefix to every index name it owns, so several
//! deployments (dev / staging / nightly) can share one OpenSearch cluster
//! without colliding. The prefix is a plain string (the caller includes any
//! separator — `dev_`, `staging-`), resolved at runtime from config, the
//! `FLUSSO_INDEX_PREFIX` env var, or the `--index-prefix` flag.
//!
//! It is prepended verbatim, so the *combined* name must be a legal OpenSearch
//! index name. [`validate_index_prefix`] enforces the part of that contract the
//! prefix controls: lowercase, no characters OpenSearch forbids, and a leading
//! character an index name may legally start with. An empty prefix is the
//! no-op default and always valid.
//!
//! ```
//! use schema_core::validate_index_prefix;
//! assert!(validate_index_prefix("dev_").is_ok());
//! assert!(validate_index_prefix("").is_ok());        // no prefix
//! assert!(validate_index_prefix("Dev_").is_err());   // uppercase
//! assert!(validate_index_prefix("_dev").is_err());   // illegal leading char
//! ```

/// Characters an index name (and therefore a prefix) may never contain — the
/// OpenSearch-forbidden set, plus the comma flusso uses to join indexes in a
/// combined search.
const FORBIDDEN: &[char] = &[
    ' ', ',', ':', '"', '*', '+', '/', '\\', '|', '?', '#', '<', '>',
];

/// Check that `prefix` is a legal leading fragment of an OpenSearch index name.
///
/// An empty prefix is valid (the default — no prefix). A non-empty prefix must
/// be lowercase, contain none of the OpenSearch-forbidden characters, and start
/// with an ASCII letter or digit (an index name may not begin with `_`/`-`/`+`).
/// Returns a human-readable reason on failure, suitable for surfacing at config
/// resolution time.
pub fn validate_index_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() {
        return Ok(());
    }
    if prefix.len() > 50 {
        return Err(format!(
            "index prefix {prefix:?} is too long ({} bytes); keep it under 50",
            prefix.len()
        ));
    }
    if let Some(bad) = prefix.chars().find(|c| FORBIDDEN.contains(c)) {
        return Err(format!(
            "index prefix {prefix:?} contains the illegal character {bad:?}"
        ));
    }
    if prefix.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(format!(
            "index prefix {prefix:?} must be lowercase (OpenSearch index names are lowercase)"
        ));
    }
    let first = prefix.chars().next().unwrap_or_default();
    if !first.is_ascii_alphanumeric() {
        return Err(format!(
            "index prefix {prefix:?} must start with a letter or digit, not {first:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
