//! The compile-time channel a [`FlussoRoot`](crate::FlussoRoot) uses to validate
//! the fragments it embeds.
//!
//! # Why this exists
//!
//! A derive expanding `User` **cannot see** `Address`'s tokens — they are
//! separate macro expansions. So the root, which is the only type that resolves
//! the schema, has to hand what it knows *down* to the fragment as data, and the
//! fragment checks itself against it.
//!
//! The flow, per embedded field:
//!
//! 1. The root bakes the resolved mapping level into a `&'static [FieldSpec]`.
//! 2. The root emits `const _: () = Address::__flusso_check(LEVEL);`, spanned on
//!    the embedding field.
//! 3. `Address`'s own derive generated `__flusso_check` as a `const fn` holding
//!    one assertion per declared field, each with its message baked in at macro
//!    time — which is why the messages can name the field even though the root
//!    never saw it.
//! 4. A field that is itself a fragment recurses with a plain const fn call:
//!    `Geo::__flusso_check(children(level, "geo"))`.
//!
//! Everything here is `const fn` so the whole check runs during const
//! evaluation: a mismatch is a compile error whose primary span is the root's
//! embedding, with a note chain down to the offending field.
//!
//! # Constraints this design lives under
//!
//! - `panic!` in a const context takes a **literal** message, so nothing here
//!   formats. Every message is baked by the fragment's derive.
//! - There are no warnings: const evaluation either succeeds or fails.
//! - `slice::get` is not const-stable, so the helpers index directly (with
//!   bounds already proven by the surrounding `while`).
//!
//! ```
//! use flusso_query::{FieldSpec, KindTag, exists, kind_is, nullable};
//!
//! // What a root bakes for a level with `city` (keyword, required).
//! const LEVEL: &[FieldSpec] = &[FieldSpec {
//!     name: "city",
//!     kind: KindTag::Keyword,
//!     nullable: false,
//!     array: false,
//!     variants: &[],
//!     children: &[],
//! }];
//!
//! // What a fragment's generated `__flusso_check` asserts.
//! const _: () = assert!(exists(LEVEL, "city"));
//! const _: () = assert!(kind_is(LEVEL, "city", &[KindTag::Keyword, KindTag::Text]));
//! const _: () = assert!(!nullable(LEVEL, "city"));
//! ```

/// The mapping type of one schema field, reduced to a fieldless tag.
///
/// Mirrors the resolved mapping's type as far as a *check* needs to care:
/// enough to reject a Rust type that cannot hold the field, not enough to
/// rebuild the mapping. The derive translates the schema layer's `MappingType`
/// into this, so this crate keeps no dependency on the schema layer.
///
/// Fieldless on purpose — the const helpers compare with `as u8`, since
/// `PartialEq::eq` is not usable in a const context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindTag {
    /// A `keyword` field — an exact string.
    Keyword,
    /// A `text` field — an analyzed string.
    Text,
    /// A `boolean` field.
    Bool,
    /// A `byte` field (`i8`).
    Byte,
    /// A `short` field (`i16`).
    Short,
    /// An `integer` field (`i32`).
    Integer,
    /// A `long` field (`i64`).
    Long,
    /// A `float` / `half_float` field (`f32`).
    Float,
    /// A `double` field (`f64`).
    Double,
    /// A `decimal` / `scaled_float` field.
    Decimal,
    /// A `date` / `timestamp` field.
    Date,
    /// A `geo_point` field.
    GeoPoint,
    /// A `binary` field — base64, not searchable.
    Binary,
    /// An `object`: a group, a to-one join, a dynamic-key `map`, or opaque JSON.
    Object,
    /// A `nested` array of objects.
    Nested,
    /// Any mapping type this vocabulary does not model — never rejected.
    Other,
}

impl KindTag {
    /// Whether a Rust type of this kind may stand in for `other`.
    ///
    /// [`Other`](KindTag::Other) matches everything in both directions: an
    /// unmodeled mapping type must not turn into a spurious compile error.
    #[must_use]
    pub const fn accepts(self, other: KindTag) -> bool {
        matches!(self, KindTag::Other)
            || matches!(other, KindTag::Other)
            || self as u8 == other as u8
    }
}

/// One field of a resolved mapping level, as the root bakes it for a fragment.
///
/// A `&'static [FieldSpec]` *is* a level; [`children`] walks into a sub-level,
/// so one const carries a whole document subtree.
#[derive(Debug)]
pub struct FieldSpec {
    /// The document key — what the JSON actually uses, after schema renaming.
    pub name: &'static str,
    /// The field's mapping type.
    pub kind: KindTag,
    /// Whether the schema allows this field to be null (→ `Option<…>`).
    pub nullable: bool,
    /// Whether the field is a flat array (→ `Vec<…>`). A `nested` field is an
    /// array too, but is told apart by [`kind`](FieldSpec::kind).
    pub array: bool,
    /// A declared `enum` field's variants, in rank order; empty when the field
    /// is not an ordered enum.
    pub variants: &'static [&'static str],
    /// The sub-level of an `object` / `nested` field; empty for a leaf.
    pub children: &'static [FieldSpec],
}

/// Whether `name` exists at this level.
#[must_use]
pub const fn exists(level: &[FieldSpec], name: &str) -> bool {
    find(level, name).is_some()
}

/// Whether `name` is nullable. A field that does not exist reports `false` —
/// the generated check reports the absence separately, with a better message.
#[must_use]
pub const fn nullable(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => field.nullable,
        None => false,
    }
}

/// Whether `name` is a flat array. Absent → `false`, as for [`nullable`].
#[must_use]
pub const fn array(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => field.array,
        None => false,
    }
}

/// Whether `name`'s mapping kind is one `accepted` Rust type can hold.
///
/// `accepted` comes from the fragment's Rust field type — either inferred from a
/// primitive leaf, or read off a custom type's [`FlussoValueMeta::KINDS`]. An
/// empty `accepted` means "no opinion" and passes.
#[must_use]
pub const fn kind_is(level: &[FieldSpec], name: &str, accepted: &[KindTag]) -> bool {
    let Some(field) = find(level, name) else {
        return false;
    };
    if accepted.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < accepted.len() {
        #[allow(clippy::indexing_slicing)]
        if accepted[i].accepts(field.kind) {
            return true;
        }
        i += 1;
    }
    false
}

/// Whether every variant `rust` declares also exists in the schema's set.
///
/// Deliberately one-directional. A Rust enum covering only *some* of the
/// schema's variants is a legal partial projection and passes; a Rust variant
/// the schema never declares can never match a document, so it fails. A field
/// with no declared variants (a plain keyword) passes.
#[must_use]
pub const fn variants_covered(level: &[FieldSpec], name: &str, rust: &[&str]) -> bool {
    let Some(field) = find(level, name) else {
        return false;
    };
    if field.variants.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < rust.len() {
        #[allow(clippy::indexing_slicing)]
        if !contains(field.variants, rust[i]) {
            return false;
        }
        i += 1;
    }
    true
}

/// The sub-level under `name`, or an empty level when absent or a leaf.
#[must_use]
pub const fn children<'a>(level: &'a [FieldSpec], name: &str) -> &'a [FieldSpec] {
    match find(level, name) {
        Some(field) => field.children,
        None => &[],
    }
}

const fn find<'a>(level: &'a [FieldSpec], name: &str) -> Option<&'a FieldSpec> {
    let mut i = 0;
    while i < level.len() {
        #[allow(clippy::indexing_slicing)]
        if str_eq(level[i].name, name) {
            #[allow(clippy::indexing_slicing)]
            return Some(&level[i]);
        }
        i += 1;
    }
    None
}

const fn contains(haystack: &[&str], needle: &str) -> bool {
    let mut i = 0;
    while i < haystack.len() {
        #[allow(clippy::indexing_slicing)]
        if str_eq(haystack[i], needle) {
            return true;
        }
        i += 1;
    }
    false
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        #[allow(clippy::indexing_slicing)]
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// What a custom value type can hold, as constants a `const fn` can read.
///
/// [`FlussoValue`](crate::FlussoValue) answers the same question in the *type*
/// system, but a fragment's check runs in const evaluation, where the schema
/// kind is only a value — so it cannot name `FlussoValue<K>` for a `K` it does
/// not know. This trait carries the same information as data instead, which is
/// what lets a fragment field typed `Decimal` (or a `#[derive(FlussoValue)]`
/// enum) be checked as precisely as one typed `String`.
///
/// Implemented by `#[derive(FlussoValue)]`. Reading
/// `<Money as FlussoValueMeta>::KINDS` in a const fn needs no bound, because the
/// type is concrete — which is what keeps recursion into sub-fragments working.
pub trait FlussoValueMeta {
    /// The mapping kinds this type may stand in for. Empty means "no opinion".
    const KINDS: &'static [KindTag];

    /// For an enum, its variants as the document spells them (serde renaming
    /// applied). Empty for anything that is not an enum.
    const VARIANTS: &'static [&'static str];
}

/// Mirrors the [`FlussoValue`](crate::FlussoValue) impls above, so a
/// `#[derive(FlussoValue)]` newtype with no explicit kind can forward its inner
/// type's kinds as *data* (`const KINDS = <Inner as FlussoValueMeta>::KINDS`)
/// exactly as it forwards them as a bound.
macro_rules! value_meta {
    ($($ty:ty => [$($kind:ident),* $(,)?]),+ $(,)?) => {$(
        impl FlussoValueMeta for $ty {
            const KINDS: &'static [KindTag] = &[$(KindTag::$kind),*];
            const VARIANTS: &'static [&'static str] = &[];
        }
    )+};
}

value_meta! {
    String => [Keyword, Text, Date],
    bool => [Bool],
    i8 => [Byte, Short, Integer, Long, Float, Double, Decimal],
    i16 => [Short, Integer, Long, Float, Double, Decimal],
    i32 => [Integer, Long, Double, Decimal],
    i64 => [Long, Decimal],
    f32 => [Float, Double],
    f64 => [Double],
}

#[cfg(feature = "decimal")]
value_meta!(crate::Decimal => [Decimal]);

#[cfg(feature = "uuid")]
value_meta!(uuid::Uuid => [Keyword]);

#[cfg(feature = "chrono")]
value_meta! {
    chrono::NaiveDate => [Date],
    chrono::NaiveDateTime => [Date],
    chrono::DateTime<chrono::Utc> => [Date],
}

#[cfg(test)]
mod tests {
    use super::{FieldSpec, KindTag, array, children, exists, kind_is, nullable, variants_covered};

    const STATUS_VARIANTS: &[&str] = &["pending", "shipped", "delivered"];

    const LEVEL: &[FieldSpec] = &[
        FieldSpec {
            name: "city",
            kind: KindTag::Keyword,
            nullable: false,
            array: false,
            variants: &[],
            children: &[],
        },
        FieldSpec {
            name: "zip",
            kind: KindTag::Keyword,
            nullable: true,
            array: false,
            variants: &[],
            children: &[],
        },
        FieldSpec {
            name: "tags",
            kind: KindTag::Keyword,
            nullable: false,
            array: true,
            variants: &[],
            children: &[],
        },
        FieldSpec {
            name: "status",
            kind: KindTag::Keyword,
            nullable: false,
            array: false,
            variants: STATUS_VARIANTS,
            children: &[],
        },
        FieldSpec {
            name: "geo",
            kind: KindTag::Object,
            nullable: false,
            array: false,
            variants: &[],
            children: &[FieldSpec {
                name: "lat",
                kind: KindTag::Double,
                nullable: false,
                array: false,
                variants: &[],
                children: &[],
            }],
        },
    ];

    #[test]
    fn finds_a_field_by_its_document_key() {
        assert!(exists(LEVEL, "city"));
        assert!(!exists(LEVEL, "town"));
    }

    #[test]
    fn reads_nullability_and_array_shape() {
        assert!(!nullable(LEVEL, "city"));
        assert!(nullable(LEVEL, "zip"));
        assert!(array(LEVEL, "tags"));
        assert!(!array(LEVEL, "city"));
    }

    #[test]
    fn an_absent_field_reports_no_shape() {
        assert!(!nullable(LEVEL, "town"));
        assert!(!array(LEVEL, "town"));
        assert!(children(LEVEL, "town").is_empty());
    }

    #[test]
    fn accepts_any_kind_the_rust_type_can_hold() {
        assert!(kind_is(LEVEL, "city", &[KindTag::Keyword, KindTag::Text]));
        assert!(!kind_is(LEVEL, "city", &[KindTag::Long]));
    }

    #[test]
    fn an_empty_kind_set_has_no_opinion() {
        assert!(kind_is(LEVEL, "city", &[]));
    }

    #[test]
    fn an_unmodeled_kind_never_rejects() {
        assert!(KindTag::Other.accepts(KindTag::Keyword));
        assert!(KindTag::Keyword.accepts(KindTag::Other));
    }

    #[test]
    fn a_missing_field_fails_every_shape_check() {
        assert!(!kind_is(LEVEL, "town", &[KindTag::Keyword]));
        assert!(!variants_covered(LEVEL, "town", &["pending"]));
    }

    #[test]
    fn a_subset_of_the_declared_variants_is_allowed() {
        assert!(variants_covered(LEVEL, "status", &["pending", "shipped"]));
        assert!(variants_covered(LEVEL, "status", &[]));
    }

    #[test]
    fn a_variant_the_schema_never_declares_is_rejected() {
        assert!(!variants_covered(LEVEL, "status", &["pending", "refunded"]));
    }

    #[test]
    fn a_field_without_declared_variants_accepts_any() {
        assert!(variants_covered(LEVEL, "city", &["anything"]));
    }

    #[test]
    fn walks_into_a_sub_level() {
        let geo = children(LEVEL, "geo");
        assert!(exists(geo, "lat"));
        assert!(kind_is(geo, "lat", &[KindTag::Double]));
    }

    #[test]
    fn the_whole_check_runs_in_const_evaluation() {
        const _: () = assert!(exists(LEVEL, "city"));
        const _: () = assert!(!nullable(LEVEL, "city"));
        const _: () = assert!(kind_is(LEVEL, "city", &[KindTag::Keyword]));
        const _: () = assert!(variants_covered(LEVEL, "status", &["pending"]));
        const _: () = assert!(exists(children(LEVEL, "geo"), "lat"));
    }
}
