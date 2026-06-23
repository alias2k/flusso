//! Document path metadata: where a scope sits relative to the index root.
//!
//! Every scope type (the [`Root`](crate::Root) marker and each `nested` element
//! struct) implements [`FlussoDocument`](crate::FlussoDocument), which carries a
//! `const PATH: &[Segment]` — the chain of container levels from the root down to
//! that scope. A nesting-aware sort reads it to render the right `nested` clause.
//!
//! Only the **kinds** a path level can be are modeled: an [`Object`](SegmentKind::Object)
//! (a group / to-one join — flattened, no query boundary) or a
//! [`Nested`](SegmentKind::Nested) array (a real `nested` boundary). The derive
//! translates the resolved mapping into these at codegen, so this crate needs no
//! dependency on the schema layer.
//!
//! ```
//! use flusso_query::{Segment, SegmentKind, nested_boundaries};
//!
//! // `orders.shipping.packages`: a nested array, an object hop, a nested array.
//! let path = &[
//!     Segment { name: "orders", kind: SegmentKind::Nested },
//!     Segment { name: "shipping", kind: SegmentKind::Object },
//!     Segment { name: "packages", kind: SegmentKind::Nested },
//! ];
//! assert_eq!(nested_boundaries(path), ["orders", "orders.shipping.packages"]);
//! ```

/// How one path level is stored — the only shapes a level can take.
///
/// Named `SegmentKind` to stay clear of the value-kind markers in
/// [`kind`](crate::kind). Non-exhaustive: more container kinds may be added.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// A group / to-one-join object: extends the dotted path but is *not* a
    /// `nested` query boundary (it flattens into the enclosing scope).
    Object,
    /// A `nested` array: a real query/sort boundary that must be wrapped.
    Nested,
}

/// One level of a document path — a field name plus how it's stored.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    /// The field name at this level (a single path segment, not dotted).
    pub name: &'static str,
    /// Whether this level is a flattened object or a `nested` boundary.
    pub kind: SegmentKind,
}

/// The `nested` boundaries along `path`: the cumulative dotted path of each
/// [`Nested`](SegmentKind::Nested) level, outermost first.
///
/// Object levels extend the running path but contribute no boundary. A pure
/// function of the path — identical for every document — so it lives here rather
/// than on [`FlussoDocument`](crate::FlussoDocument). An empty result (a root or
/// flattened-object field) means a plain, non-nested sort.
#[must_use]
pub fn nested_boundaries(path: &[Segment]) -> Vec<String> {
    let mut running = String::new();
    let mut boundaries = Vec::new();
    for segment in path {
        if !running.is_empty() {
            running.push('.');
        }
        running.push_str(segment.name);
        if segment.kind == SegmentKind::Nested {
            boundaries.push(running.clone());
        }
    }
    boundaries
}
