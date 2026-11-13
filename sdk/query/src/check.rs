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
//!     map_values: None,
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
    /// A dynamic-key `map` field's value kind — the one thing that tells a `map`
    /// apart from a plain `object`. `None` for anything that is not a map.
    pub map_values: Option<KindTag>,
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

/// Whether a Rust type that is (`vec`) or is not a `Vec<…>` fits `name`'s
/// array shape, as a root checks it: a `nested` array wants a `Vec`, an
/// `object` with fields and a `map` want none, a leaf or a `json` field wants
/// one exactly when it is a flat array. Any other opaque `json` object and an
/// unmodeled kind take either.
/// Absent → `true`; the generated check reports the absence separately.
#[must_use]
pub const fn array_shape_ok(level: &[FieldSpec], name: &str, vec: bool) -> bool {
    let Some(field) = find(level, name) else {
        return true;
    };
    match field.kind {
        KindTag::Nested => vec,
        KindTag::Object if field.array => vec,
        KindTag::Object => (field.map_values.is_none() && field.children.is_empty()) || !vec,
        KindTag::Other => true,
        _ => field.array == vec,
    }
}

/// Whether `name` is a `keyword` or `text` field that is not a `map`. Absent →
/// `false`.
#[must_use]
pub const fn is_string(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => {
            field.map_values.is_none() && matches!(field.kind, KindTag::Keyword | KindTag::Text)
        }
        None => false,
    }
}

/// Whether `name` is an `object` or a `nested` array: a level, a `map` or an
/// opaque `json` object rather than a leaf. Absent → `false`.
#[must_use]
pub const fn is_container(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => matches!(field.kind, KindTag::Object | KindTag::Nested),
        None => false,
    }
}

/// Whether `name` is a level with fields of its own: a `nested` array, or an
/// `object` with declared children that is not a `map`. Absent → `false`.
#[must_use]
pub const fn has_fields(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => match field.kind {
            KindTag::Nested => true,
            KindTag::Object => field.map_values.is_none() && !field.children.is_empty(),
            _ => false,
        },
        None => false,
    }
}

/// Whether `name` is a `nested` array. Absent → `false`.
#[must_use]
pub const fn is_nested(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => matches!(field.kind, KindTag::Nested),
        None => false,
    }
}

/// Whether `name` is an opaque `json` object (an `object` with no declared
/// children that is not a `map`) or a kind this vocabulary does not model:
/// a field that takes any JSON. Absent → `false`.
#[must_use]
pub const fn is_json(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => match field.kind {
            KindTag::Other => true,
            KindTag::Object => field.map_values.is_none() && field.children.is_empty(),
            _ => false,
        },
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

/// Whether `name` declares any `variants:` at all — the precondition an
/// `exhaustive` type needs before [`variants_exhausted`] can verify anything.
/// Absent → `false`.
#[must_use]
pub const fn has_variants(level: &[FieldSpec], name: &str) -> bool {
    match find(level, name) {
        Some(field) => !field.variants.is_empty(),
        None => false,
    }
}

/// Whether every variant the schema declares for `name` also exists in `rust`.
///
/// The reverse direction of [`variants_covered`]: asserted together they make
/// the two sets equal. Read only for a type marked `#[flusso(…, exhaustive)]` —
/// a partial projection is legal, so nothing asserts this by default. A field
/// with no declared variants passes vacuously; [`has_variants`] owns that case,
/// with its own message.
#[must_use]
pub const fn variants_exhausted(level: &[FieldSpec], name: &str, rust: &[&str]) -> bool {
    let Some(field) = find(level, name) else {
        return false;
    };
    let mut i = 0;
    while i < field.variants.len() {
        #[allow(clippy::indexing_slicing)]
        if !contains(rust, field.variants[i]) {
            return false;
        }
        i += 1;
    }
    true
}

/// Whether `name` is a dynamic-key `map` whose values are one of `accepted`.
///
/// A field that is not a map fails — so a `HashMap<String, _>` declared against
/// a plain `object` is caught, not silently accepted. Two exceptions, matching
/// what a root does natively: an *opaque* object (a `json` field — `Object` with
/// no declared children) accepts anything, and an unmodeled kind
/// ([`Other`](KindTag::Other)) never rejects.
#[must_use]
pub const fn map_value_is(level: &[FieldSpec], name: &str, accepted: &[KindTag]) -> bool {
    let Some(field) = find(level, name) else {
        return false;
    };
    let Some(values) = field.map_values else {
        return matches!(field.kind, KindTag::Other)
            || (matches!(field.kind, KindTag::Object) && field.children.is_empty());
    };
    if accepted.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < accepted.len() {
        #[allow(clippy::indexing_slicing)]
        if accepted[i].accepts(values) {
            return true;
        }
        i += 1;
    }
    false
}

/// Whether `name` is compatible with a type's [`FlussoValueMeta::MAP_VALUES`].
///
/// A type that is not a map (`accepted` empty) has no opinion and passes; one
/// that is must land on a `map` field of a matching value kind — or on an
/// opaque object / unmodeled kind, which accept anything (see [`map_value_is`]).
#[must_use]
pub const fn map_kind_ok(level: &[FieldSpec], name: &str, accepted: &[KindTag]) -> bool {
    accepted.is_empty() || map_value_is(level, name, accepted)
}

/// The declared `values:` kind when `name` is a dynamic-key `map`; `None` for
/// anything else. A generated check reads this to *select* among pre-baked
/// panic messages (a const panic takes a literal, so the message can't be
/// composed from the level — but which literal fires can depend on it).
#[must_use]
pub const fn map_values_of(level: &[FieldSpec], name: &str) -> Option<KindTag> {
    match find(level, name) {
        Some(field) => field.map_values,
        None => None,
    }
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
/// Implemented by every flusso derive. Reading
/// `<Money as FlussoValueMeta>::KINDS` in a const fn needs no bound, because the
/// type is concrete — which is what keeps recursion into sub-fragments working.
///
/// # Implementing it by hand
///
/// A type with a hand-written [`FlussoValue`](crate::FlussoValue) implements
/// this trait by hand too, then calls [`embeddable!`](crate::embeddable) so a
/// `FlussoFragment` can embed it with its checks on. Set the constants as the
/// derive would: `KINDS` from the `FlussoValue<K>` impls, `VARIANTS` for an
/// enum.
///
/// ```
/// use flusso_query::{FlussoValue, FlussoValueMeta, KindTag, kind};
///
/// #[derive(serde::Serialize, serde::Deserialize)]
/// struct Count(i64);
///
/// impl FlussoValue<kind::Long> for Count {}
/// impl FlussoValueMeta for Count {
///     const KINDS: &'static [KindTag] = &[KindTag::Long];
///     const VARIANTS: &'static [&'static str] = &[];
/// }
/// flusso_query::embeddable!(Count);
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a document field type",
    label = "needs a flusso derive",
    note = "add `#[derive(FlussoValue)]` for a value type, `#[derive(FlussoMap)]` for a type standing in for a dynamic-key `map`, `#[derive(FlussoFragment)]` for a sub-document, or `#[flusso(opaque)]` on the field to skip checking it; a hand-written `FlussoValue` implements `FlussoValueMeta` and calls `flusso_query::embeddable!`"
)]
pub trait FlussoValueMeta {
    /// The mapping kinds this type may stand in for. Empty means "no opinion".
    const KINDS: &'static [KindTag];

    /// For an enum, its variants as the document spells them (serde renaming
    /// applied). Empty for anything that is not an enum.
    const VARIANTS: &'static [&'static str];

    /// For a type standing in for a dynamic-key `map`, the value kinds it can
    /// hold. Empty for anything that is not a map — which is why it defaults:
    /// only `#[derive(FlussoMap)]` sets it.
    ///
    /// Without this a map wrapper embedded in a fragment could only be checked
    /// as "the schema field here is object-ish", since a fragment can't see the
    /// mapping; with it, the value kind is checked too, matching what a root
    /// does natively.
    const MAP_VALUES: &'static [KindTag] = &[];

    /// Whether the type demands *full* coverage of the schema's declared
    /// variants — set by `#[flusso(…, exhaustive)]` on a `FlussoValue` enum,
    /// forwarded by an untagged newtype. Off by default: a Rust enum covering
    /// only some of the schema's variants is a legal partial projection.
    const EXHAUSTIVE: bool = false;

    /// Whether the type is an array newtype: it reads a JSON array, and a
    /// parent places it where it would place that `Vec<…>`. Two derives set
    /// it, for two kinds of array, told apart by [`KINDS`](Self::KINDS):
    ///
    /// - `#[derive(FlussoFragment)]` on a `#[serde(transparent)]` (or tuple)
    ///   newtype over a `Vec<…>` of a struct: an array of sub-documents. Its
    ///   kinds are the struct's, so it holds [`KindTag::Nested`], and it goes
    ///   at a `nested` field only.
    /// - `#[derive(FlussoValue)]` on an untagged newtype over a `Vec<…>` of a
    ///   leaf value: a flat array of values. Its kinds are the element's, never
    ///   `Nested` or `Object`, and it goes at an array leaf (`array: true`) or
    ///   a `json` array only ([`OpaqueProbe`]'s `LEAF_ARRAY`).
    ///
    /// Both derives forward it from a newtype over another array newtype of
    /// their own kind, and refuse, where the newtype is declared, any other
    /// newtype that would read an array: over `Option<Vec<…>>`, over a `Vec`
    /// of arrays, or over an array of the other kind. So a derived type reads
    /// a JSON array exactly when this is set. Off by default; leave it unset in
    /// a hand-written impl, where nothing checks it against what the type
    /// reads.
    const ARRAY: bool = false;
}

/// Lets a type with a hand-written [`FlussoValueMeta`] sit in a
/// `#[derive(FlussoFragment)]` struct with its checks on.
///
/// A fragment checks every custom field type the same way: it reads the type's
/// [`FlussoValueMeta`], then walks into the type with a check function every
/// flusso derive emits. A value type is a leaf, so its check does nothing; this
/// macro emits that function for a type no derive covers. Call it once per
/// type, in the crate that defines the type (it is an inherent impl). It also
/// asserts the type implements [`FlussoValueMeta`].
///
/// See [`FlussoValueMeta`] for a full example.
///
/// ```
/// # use flusso_query::{FlussoValueMeta, KindTag};
/// # struct Code(String);
/// # impl FlussoValueMeta for Code {
/// #     const KINDS: &'static [KindTag] = &[KindTag::Keyword];
/// #     const VARIANTS: &'static [&'static str] = &[];
/// # }
/// flusso_query::embeddable!(Code);
/// ```
#[macro_export]
macro_rules! embeddable {
    ($($ty:ty),+ $(,)?) => {$(
        impl $ty {
            #[doc(hidden)]
            pub const fn __flusso_check(_level: &[$crate::FieldSpec]) {
                let _ = <$ty as $crate::FlussoValueMeta>::KINDS;
            }
        }
    )+};
}

/// What `#[flusso(opaque)]` turns off for a type, as constants a generated
/// check reads to keep what `opaque` leaves on and to reject an `opaque` that
/// turns off nothing.
///
/// `opaque` skips the checks that read the type's [`FlussoValueMeta`]. Whether
/// a type implements that trait is not knowable at macro time, so this answers
/// it during const evaluation: the inherent constants below exist only for a
/// type with the metadata, and a type without it falls back to
/// [`OpaqueWithoutMeta`]'s, where `opaque` is what spares the field the read,
/// so it does turn something off.
///
/// ```
/// use flusso_query::{OpaqueProbe, OpaqueWithoutMeta as _};
///
/// struct HandWritten;
/// const _: () = assert!(OpaqueProbe::<HandWritten>::SKIPS_VARIANTS);
/// const _: () = assert!(!OpaqueProbe::<String>::SKIPS_VARIANTS);
/// const _: () = assert!(!OpaqueProbe::<HandWritten>::HAS_META);
/// const _: () = assert!(OpaqueProbe::<bool>::KINDS.len() == 1);
/// ```
#[doc(hidden)]
#[derive(Debug)]
pub struct OpaqueProbe<T: ?Sized>(core::marker::PhantomData<T>);

/// The fallback for [`OpaqueProbe`] on a type without [`FlussoValueMeta`].
#[doc(hidden)]
pub trait OpaqueWithoutMeta {
    /// Whether the type carries [`FlussoValueMeta`]: not this one.
    const HAS_META: bool = false;
    /// No metadata, so no opinion on the kind.
    const KINDS: &'static [KindTag] = &[];
    /// Whether `opaque` turns off a check at a field that declares variants.
    const SKIPS_VARIANTS: bool = true;
    /// Whether `opaque` turns off a check at a field that declares none.
    const SKIPS_EXHAUSTIVE: bool = true;
    /// No metadata, so not an array newtype.
    const ARRAY: bool = false;
    /// No metadata, so not an array newtype of values.
    const LEAF_ARRAY: bool = false;
    /// No metadata, so nothing says it cannot read a `nested` element.
    const AT_NESTED: bool = true;
    /// No metadata, so nothing says it cannot read an `object`.
    const AT_OBJECT: bool = true;
}

impl<T: ?Sized> OpaqueWithoutMeta for OpaqueProbe<T> {}

impl<T: ?Sized + FlussoValueMeta> OpaqueProbe<T> {
    /// The type carries [`FlussoValueMeta`], so its kinds can be checked.
    pub const HAS_META: bool = true;
    /// The kinds the type may stand in for.
    pub const KINDS: &'static [KindTag] = T::KINDS;
    /// At a field that declares variants: the type has variants to check.
    pub const SKIPS_VARIANTS: bool = !T::VARIANTS.is_empty() || T::EXHAUSTIVE;
    /// At a field that declares none: only the `exhaustive` check runs.
    pub const SKIPS_EXHAUSTIVE: bool = T::EXHAUSTIVE;
    /// Whether the type reads a JSON array (see [`FlussoValueMeta::ARRAY`]).
    pub const ARRAY: bool = T::ARRAY;
    /// Whether the type is an array newtype of leaf values, the kind an array
    /// leaf or a `json` array takes: it reads an array, and its kinds hold
    /// neither a sub-document nor an object.
    pub const LEAF_ARRAY: bool = T::ARRAY
        && !holds_kind(T::KINDS, KindTag::Nested)
        && !holds_kind(T::KINDS, KindTag::Object);
    /// Whether the type may stand at a `nested` field, as a sub-document.
    pub const AT_NESTED: bool = holds_kind(T::KINDS, KindTag::Nested);
    /// Whether the type may stand at an `object` field.
    pub const AT_OBJECT: bool = holds_kind(T::KINDS, KindTag::Object);
}

/// Whether a type of `kinds` may stand in for a field of kind `field`. An empty
/// `kinds` (a type without metadata) never does.
#[doc(hidden)]
#[must_use]
pub const fn accepts_kind(kinds: &[KindTag], field: KindTag) -> bool {
    let mut i = 0;
    while i < kinds.len() {
        #[allow(clippy::indexing_slicing)]
        if kinds[i].accepts(field) {
            return true;
        }
        i += 1;
    }
    false
}

const fn holds_kind(kinds: &[KindTag], wanted: KindTag) -> bool {
    let mut i = 0;
    while i < kinds.len() {
        #[allow(clippy::indexing_slicing)]
        if kinds[i] as u8 == wanted as u8 {
            return true;
        }
        i += 1;
    }
    false
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
    use super::{
        FieldSpec, KindTag, accepts_kind, array, array_shape_ok, children, exists, has_fields,
        has_variants, is_json, is_nested, kind_is, map_kind_ok, map_value_is, nullable,
        variants_covered, variants_exhausted,
    };

    const STATUS_VARIANTS: &[&str] = &["pending", "shipped", "delivered"];

    const LEVEL: &[FieldSpec] = &[
        FieldSpec {
            name: "city",
            kind: KindTag::Keyword,
            nullable: false,
            array: false,
            variants: &[],
            map_values: None,
            children: &[],
        },
        FieldSpec {
            name: "zip",
            kind: KindTag::Keyword,
            nullable: true,
            array: false,
            variants: &[],
            map_values: None,
            children: &[],
        },
        FieldSpec {
            name: "tags",
            kind: KindTag::Keyword,
            nullable: false,
            array: true,
            variants: &[],
            map_values: None,
            children: &[],
        },
        FieldSpec {
            name: "status",
            kind: KindTag::Keyword,
            nullable: false,
            array: false,
            variants: STATUS_VARIANTS,
            map_values: None,
            children: &[],
        },
        FieldSpec {
            name: "geo",
            kind: KindTag::Object,
            nullable: false,
            array: false,
            variants: &[],
            map_values: None,
            children: &[FieldSpec {
                name: "lat",
                kind: KindTag::Double,
                nullable: false,
                array: false,
                variants: &[],
                map_values: None,
                children: &[],
            }],
        },
        FieldSpec {
            name: "labels",
            kind: KindTag::Object,
            nullable: false,
            array: false,
            variants: &[],
            map_values: Some(KindTag::Text),
            children: &[],
        },
        // An opaque `json` field: an object with no declared children.
        FieldSpec {
            name: "meta",
            kind: KindTag::Object,
            nullable: false,
            array: false,
            variants: &[],
            map_values: None,
            children: &[],
        },
        // A `json` array column.
        FieldSpec {
            name: "notes",
            kind: KindTag::Object,
            nullable: true,
            array: true,
            variants: &[],
            map_values: None,
            children: &[],
        },
        // A `custom { opensearch }` field this vocabulary does not model.
        FieldSpec {
            name: "custom",
            kind: KindTag::Other,
            nullable: false,
            array: false,
            variants: &[],
            map_values: None,
            children: &[],
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
    fn a_json_array_wants_an_array_where_a_plain_json_takes_either() {
        assert!(array_shape_ok(LEVEL, "notes", true));
        assert!(!array_shape_ok(LEVEL, "notes", false));
        assert!(array_shape_ok(LEVEL, "meta", true));
        assert!(array_shape_ok(LEVEL, "meta", false));
    }

    #[test]
    fn a_map_holds_one_object_never_an_array() {
        assert!(array_shape_ok(LEVEL, "labels", false));
        assert!(!array_shape_ok(LEVEL, "labels", true));
    }

    #[test]
    fn a_type_stands_for_a_kind_only_when_it_names_one() {
        assert!(accepts_kind(
            &[KindTag::Keyword, KindTag::Text],
            KindTag::Keyword
        ));
        assert!(!accepts_kind(&[KindTag::Long], KindTag::Keyword));
        assert!(!accepts_kind(&[], KindTag::Keyword));
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
    fn exhaustion_requires_every_declared_variant() {
        assert!(variants_exhausted(
            LEVEL,
            "status",
            &["pending", "shipped", "delivered"]
        ));
        assert!(!variants_exhausted(
            LEVEL,
            "status",
            &["pending", "shipped"]
        ));
        assert!(!variants_exhausted(LEVEL, "town", &["pending"]));
    }

    #[test]
    fn extra_rust_variants_do_not_satisfy_exhaustion_alone() {
        // `variants_exhausted` only checks schema ⊆ rust — the reverse
        // direction stays `variants_covered`'s job, asserted alongside.
        assert!(variants_exhausted(
            LEVEL,
            "status",
            &["pending", "shipped", "delivered", "refunded"]
        ));
        assert!(!variants_covered(
            LEVEL,
            "status",
            &["pending", "shipped", "delivered", "refunded"]
        ));
    }

    #[test]
    fn a_field_without_declared_variants_is_vacuously_exhausted() {
        assert!(variants_exhausted(LEVEL, "city", &[]));
        assert!(!has_variants(LEVEL, "city"));
        assert!(has_variants(LEVEL, "status"));
        assert!(!has_variants(LEVEL, "town"));
    }

    #[test]
    fn walks_into_a_sub_level() {
        let geo = children(LEVEL, "geo");
        assert!(exists(geo, "lat"));
        assert!(kind_is(geo, "lat", &[KindTag::Double]));
    }

    #[test]
    fn a_map_field_checks_its_declared_value_kind() {
        assert!(map_value_is(LEVEL, "labels", &[KindTag::Text]));
        assert!(!map_value_is(LEVEL, "labels", &[KindTag::Keyword]));
        assert!(map_value_is(LEVEL, "labels", &[]));
    }

    #[test]
    fn a_structured_object_or_a_leaf_is_not_a_map() {
        assert!(!map_value_is(LEVEL, "geo", &[KindTag::Text]));
        assert!(!map_value_is(LEVEL, "city", &[KindTag::Text]));
        assert!(!map_value_is(LEVEL, "town", &[KindTag::Text]));
    }

    #[test]
    fn an_opaque_object_accepts_any_map() {
        // Parity with the root: a `json` field (object, no children) accepts
        // anything — a `HashMap` or a map wrapper of any value kind included.
        assert!(map_value_is(LEVEL, "meta", &[KindTag::Text]));
        assert!(map_value_is(LEVEL, "meta", &[KindTag::Double]));
        assert!(map_kind_ok(LEVEL, "meta", &[KindTag::Keyword]));
        // …and an unmodeled kind never rejects, in maps as everywhere else.
        assert!(map_value_is(LEVEL, "custom", &[KindTag::Text]));
    }

    #[test]
    fn a_level_with_fields_is_neither_json_nor_a_map() {
        assert!(has_fields(LEVEL, "geo"));
        assert!(!is_json(LEVEL, "geo"));
        assert!(is_json(LEVEL, "meta"));
        assert!(is_json(LEVEL, "custom"));
        assert!(!has_fields(LEVEL, "meta"));
        assert!(!has_fields(LEVEL, "labels"));
        assert!(!is_json(LEVEL, "labels"));
        assert!(!is_json(LEVEL, "city"));
        assert!(!is_json(LEVEL, "town"));
    }

    #[test]
    fn only_a_nested_field_is_nested() {
        const ORDERS: &[FieldSpec] = &[FieldSpec {
            name: "orders",
            kind: KindTag::Nested,
            nullable: false,
            array: false,
            variants: &[],
            map_values: None,
            children: &[],
        }];
        assert!(is_nested(ORDERS, "orders"));
        assert!(has_fields(ORDERS, "orders"));
        assert!(!is_nested(LEVEL, "geo"));
        assert!(!is_nested(LEVEL, "town"));
    }

    #[test]
    fn a_non_map_type_has_no_opinion_on_any_field() {
        assert!(map_kind_ok(LEVEL, "geo", &[]));
        assert!(map_kind_ok(LEVEL, "city", &[]));
    }

    #[test]
    fn a_map_type_rejects_a_structured_object() {
        assert!(!map_kind_ok(LEVEL, "geo", &[KindTag::Text]));
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
