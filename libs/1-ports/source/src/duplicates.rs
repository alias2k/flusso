//! What a deployment does with two or more fields of one name at one level.
//!
//! Such a schema still loads (the file formats are frozen), so
//! [`validate_spec`](crate::validate_spec) warns with
//! [`DiagnosticKind::DuplicateFieldName`](crate::DiagnosticKind::DuplicateFieldName),
//! and the warning says what the pipeline does with them. That is decided in
//! this order:
//!
//! 1. The Postgres build reads the **last** one's value (the document query
//!    emits both keys; decoding keeps the last) and converts it through the
//!    **first** one's declaration (`coerce_document`). An `array: true` column
//!    of scalars refuses an object element there, which fails the document.
//!    A constant is the exception both ways: the query emits a scalar
//!    constant as text and a list or an object as `null`, and a first
//!    constant builds the value it holds, whatever the query read. A `binary`
//!    column's bytes are Postgres hex text until a `binary` declaration
//!    decodes them.
//! 2. Script fields write their output over it.
//! 3. An input-only (`emit: false`) namesake strips the name.
//! 4. The sink indexes what is left under the last emitted one's mapping, and
//!    may reject it.
//!
//! [`duplicate_outcome`] classifies a set of namesakes. The `duplicate_names`
//! live test (`apps/cli/tests/duplicate_names.rs`) builds every ordered pair
//! of field kinds through the real builder and OpenSearch sink and asserts the
//! classification. A pair it can't place with certainty is
//! [`DuplicateOutcome::Unknown`], whose wording claims nothing.
//!
//! ```
//! use source::{DuplicateOutcome, duplicate_outcome};
//!
//! let column = |column: &str| serde_json::json!({
//!     "field": "email",
//!     "source": { "column": { "column": column, "ty": "keyword", "nullable": true } },
//! });
//! let fields: Vec<kernel::Field> =
//!     serde_json::from_value(serde_json::json!([column("email"), column("full_name")])).unwrap();
//! let same: Vec<_> = fields.iter().collect();
//! assert_eq!(duplicate_outcome(&same), DuplicateOutcome::Holds(1));
//! ```

use kernel::{
    AggregateOp, Field, FieldName, FieldSource, FlussoType, GenericValue, MappingType, Relation,
    constant_is_stored_unindexed, constant_mapping_type,
};

/// What every document holds under a name its namesakes share.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DuplicateOutcome {
    /// The value the namesake at this position (0-based, among them) builds
    /// on its own: the last one's, or a script's. The sink does with it what
    /// it does with that field alone.
    Holds(usize),
    /// The last one's value, wrapped in a one-element array by the first one,
    /// an `array: true` column (an array value stays as it is).
    Wrapped,
    /// The last one's value cut to the child fields the first one declares
    /// (the keys of an object; of each object of an array). `kept` names the
    /// ones the last one has too; with none, every element (or the object) is
    /// empty.
    FirstsChildren {
        /// The child fields both declare.
        kept: Vec<FieldName>,
    },
    /// No document holds the name: an input-only namesake strips it.
    Nothing,
    /// Every document holds `null` under the name: the last one is a constant
    /// holding a list or an object, which the document query reads as `null`.
    Null,
    /// The last one's value as text, which the first one's declaration does
    /// not convert back.
    AsText {
        /// What the last one is, and so what the text looks like.
        text: Text,
        /// In a one-element array: the first one is an `array: true` column.
        wrapped: bool,
    },
    /// Some documents fail to build; `documents` says which.
    FailsToBuild {
        /// Which documents, in a few words.
        documents: &'static str,
    },
    /// OpenSearch rejects some documents; `reason` says which.
    Rejected {
        /// What the documents hold that the mapping refuses.
        reason: Rejection,
        /// When a document's value can fit the mapping (a script's output
        /// that reads as the last one's type), the namesake whose value the
        /// document then holds; `None` when no document is indexed.
        unless_it_fits: Option<usize>,
    },
    /// It depends on both declarations and on the data: either value, an
    /// empty one, a failed build or a rejection.
    Unknown,
}

/// A last namesake whose value reaches the build as text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Text {
    /// A number constant: the query emits every scalar constant as text
    /// (`"5"`), and only a `decimal` declaration parses it back.
    NumberConstant,
    /// A boolean constant, as `"true"` or `"false"`.
    BooleanConstant,
    /// A `binary` column, as Postgres hex (`"\x0102"`), which only a `binary`
    /// declaration decodes to base64.
    Binary,
}

/// What a document holds that the last namesake's mapping refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rejection {
    /// The output of the script at this position (0-based, among the
    /// namesakes), of type `produced`. `wanted` is the scalar type the
    /// mapping takes; `None` when it takes an object, so every output is
    /// refused.
    ScriptOutput {
        /// The script's place among the namesakes.
        at: usize,
        /// The type the script returns.
        produced: MappingType,
        /// The scalar type the mapping takes, if it takes one.
        wanted: Option<MappingType>,
    },
    /// A geo point cut to the first namesake's child fields, which name no
    /// `lat` and `lon`.
    PointWithoutLatLon,
    /// The first namesake's constant, which the last one's mapping does not
    /// take.
    FirstConstant,
}

/// What a deployment does with the namesakes `same`, in declaration order.
pub fn duplicate_outcome(same: &[&Field]) -> DuplicateOutcome {
    let (Some(first), Some(last)) = (same.first(), same.last()) else {
        return DuplicateOutcome::Unknown;
    };
    if let Some(documents) = fails_to_build(first, last) {
        return DuplicateOutcome::FailsToBuild { documents };
    }
    if same.iter().any(|field| !field.emit) {
        return DuplicateOutcome::Nothing;
    }
    let script = same
        .iter()
        .rposition(|field| matches!(field.source, FieldSource::Script(_)));
    if let Some(at) = script {
        return script_outcome(at, same, last);
    }
    if let FieldSource::Constant(value) = &first.source {
        return match constant_fits(value, last) {
            Some(true) => DuplicateOutcome::Holds(0),
            Some(false) => DuplicateOutcome::Rejected {
                reason: Rejection::FirstConstant,
                unless_it_fits: None,
            },
            None => DuplicateOutcome::Unknown,
        };
    }
    if let FieldSource::Constant(value) = &last.source
        && read_as_null(value)
    {
        // An `array: true` column turns a null into its `default`.
        let defaulted =
            matches!(&first.source, FieldSource::Column(c) if c.array && c.default.is_some());
        return if defaulted {
            DuplicateOutcome::Unknown
        } else {
            DuplicateOutcome::Null
        };
    }
    if let Some(text) = read_as_text(first, last) {
        return DuplicateOutcome::AsText {
            text,
            wrapped: array_column(first),
        };
    }
    // A `custom` column may hold anything: not pinned beside a namesake.
    if scalar_array(first) && is_custom(last) {
        return DuplicateOutcome::Unknown;
    }
    document_outcome(same.len() - 1, first, last)
}

/// Names every field of `name` by position at its level and says what a
/// deployment does with them.
pub(crate) fn duplicate_message(name: &FieldName, same: &[(usize, &Field)]) -> String {
    let listed = same
        .iter()
        .map(|(at, field)| format!("#{} ({})", at + 1, described(field)))
        .collect::<Vec<_>>()
        .join(", ");
    let fields: Vec<&Field> = same.iter().map(|(_, field)| *field).collect();
    let position = |i: usize| same.get(i).map_or(0, |(at, _)| at + 1);
    let kept = match duplicate_outcome(&fields) {
        DuplicateOutcome::Holds(i) => holds_message(i, &fields, position(i)),
        DuplicateOutcome::FirstsChildren { kept } => {
            let (first, last) = (position(0), position(fields.len() - 1));
            if kept.is_empty() {
                let empty = if fields.last().is_some_and(|f| shape(f) == Shape::Rows) {
                    "every element is empty"
                } else {
                    "the object is empty"
                };
                format!(
                    "the index maps the last one, but a document keeps none of its child \
                     fields: #{first} and #{last} declare none in common, so {empty}"
                )
            } else {
                let kept = kept
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "the index maps the last one, but a document holds its value cut to the \
                     child fields #{first} declares that #{last} has too ({kept}), read as \
                     #{first} declares them"
                )
            }
        }
        DuplicateOutcome::Nothing if fields.iter().all(|field| !field.emit) => {
            "none reaches the document".to_owned()
        }
        DuplicateOutcome::Null => format!(
            "the index maps the last one, but every document holds `null`: #{} is a constant \
             holding a list or an object, which the document query reads as `null`, and #{} \
             reads that",
            position(fields.len() - 1),
            position(0)
        ),
        DuplicateOutcome::Nothing => "the index maps the last emitted one, but the input-only \
                                      (`emit: false`) one strips the name from every document, \
                                      so no document holds it"
            .to_owned(),
        DuplicateOutcome::FailsToBuild { documents } => format!(
            "the index maps the last one, but {documents} fails to build: #{} is an `array: \
             true` column, which takes scalar elements, and the last one's value holds \
             objects. With `on_error = stop` (the default) the ingest engine stops and \
             restarts on it, which halts every index of the deployment; with `skip` such a \
             document keeps its previous indexed version (none on a first backfill)",
            position(0)
        ),
        DuplicateOutcome::AsText { text, wrapped } => {
            let (last, first) = (position(fields.len() - 1), position(0));
            let what = match text {
                Text::NumberConstant => format!(
                    "#{last} is a number constant, which reaches #{first} as a string (`\"5\"`), \
                     and only a `decimal` field reads a string as a number"
                ),
                Text::BooleanConstant => format!(
                    "#{last} is a boolean constant, which reaches #{first} as the string \
                     `\"true\"` or `\"false\"`"
                ),
                Text::Binary => format!(
                    "#{last} is a `binary` column, whose bytes reach #{first} as Postgres hex \
                     text (`\"\\x0102\"`), not base64: only a `binary` field decodes them"
                ),
            };
            let array = if wrapped {
                format!(", in a one-element array since #{first} is an `array: true` column")
            } else {
                String::new()
            };
            format!(
                "the index maps the last one, and a document holds the last one's value as \
                 text{array}: {what}"
            )
        }
        DuplicateOutcome::Wrapped => "the index maps the last one, and a document holds the last \
                                      one's value wrapped in a one-element array (the first one \
                                      is an `array: true` column)"
            .to_owned(),
        DuplicateOutcome::Rejected { reason, .. } => format!(
            "the index maps the last one, {}, but {}. With `on_error = stop` (the \
             default) the sink stops and restarts on it, which halts every index it writes \
             and pins the replication slot; with `skip` such a document is quarantined",
            fields.last().map_or_else(String::new, |f| described(f)),
            rejection_message(&reason, position)
        ),
        DuplicateOutcome::Unknown => "the index maps the last one; what a document holds \
                                      depends on both declarations: it may hold either value, \
                                      be empty, fail to build or be refused by the sink (run \
                                      `flusso script run` on a row to see)"
            .to_owned(),
    };
    format!(
        "`{name}` is declared {} times at this level: {listed}; {kept}. Rename all but one",
        same.len()
    )
}

fn rejection_message(reason: &Rejection, position: impl Fn(usize) -> usize) -> String {
    match reason {
        Rejection::ScriptOutput {
            at,
            produced,
            wanted: None,
        } => format!(
            "a document holds the last script's output (#{}, {}): OpenSearch rejects every \
             document where the script returns a value",
            position(*at),
            typed(produced, false)
        ),
        Rejection::ScriptOutput {
            at,
            produced,
            wanted: Some(wanted),
        } => format!(
            "a document holds the last script's output (#{}, {}): OpenSearch rejects every \
             document whose output does not read as {}",
            position(*at),
            typed(produced, false),
            typed(wanted, false)
        ),
        Rejection::FirstConstant => format!(
            "a document holds the first one's constant (#{}), which a constant builds \
             whatever the last one reads: OpenSearch rejects every document",
            position(0)
        ),
        Rejection::PointWithoutLatLon => format!(
            "a document holds its point cut to the child fields #{} declares, with no `lat` \
             and `lon`: OpenSearch rejects every document with a point",
            position(0)
        ),
    }
}

/// A mapping type with its article: "an `integer`", "an array of `keyword`".
fn typed(ty: &MappingType, array: bool) -> String {
    let name = ty.name();
    if array {
        return format!("an array of `{name}`");
    }
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} `{name}`")
}

fn holds_message(i: usize, fields: &[&Field], position: usize) -> String {
    let (Some(first), Some(field)) = (fields.first(), fields.get(i)) else {
        return String::new();
    };
    if let FieldSource::Script(script) = &field.source {
        return format!(
            "the index maps the last one, and a document holds the last script's output \
             (#{position}, {})",
            typed(&script.ty.opensearch(), script.array)
        );
    }
    if i == 0
        && let FieldSource::Constant(_) = &first.source
    {
        return format!(
            "the index maps the last one, and a document holds the first one's constant \
             (#{position}), which a constant builds whatever the last one reads"
        );
    }
    let raw = "as raw JSON, not converted to the declared types";
    let how = if array_column(first) {
        ", its elements read as the first one's type"
    } else {
        match (shape(first), shape(field)) {
            (Shape::Value, Shape::Value) => ", read as the first one declares it",
            (Shape::Object, Shape::Rows) if matches!(first.source, FieldSource::Group(_)) => raw,
            (Shape::Object | Shape::Rows, Shape::Object | Shape::Rows) => {
                ", its child fields read as the first one declares them"
            }
            _ => raw,
        }
    };
    let space = if how.starts_with(',') { "" } else { " " };
    format!("the index maps the last one, and a document holds the last one's value{space}{how}")
}

/// Whether the document query reads a constant as `null`: it emits only a
/// scalar as its literal.
fn read_as_null(value: &GenericValue) -> bool {
    matches!(value, GenericValue::Array(_) | GenericValue::Map(_))
}

/// How `last`'s value reaches the build as text `first` keeps, if it does: a
/// number constant unless `first` is `decimal` (the query emits every scalar
/// constant as text, and only `coerce_decimal` parses a string), a boolean
/// constant, or a `binary` column unless `first` is `binary`.
fn read_as_text(first: &Field, last: &Field) -> Option<Text> {
    let typed = |field: &Field, ty: FlussoType| match &field.source {
        FieldSource::Column(c) => c.ty == ty,
        FieldSource::Relation(Relation::Aggregate(a)) => {
            matches!(
                a.op,
                AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_)
            ) && a.value_type.as_ref() == Some(&ty)
        }
        _ => false,
    };
    match &last.source {
        FieldSource::Constant(GenericValue::Bool(_)) => Some(Text::BooleanConstant),
        FieldSource::Constant(
            GenericValue::SmallInt(_)
            | GenericValue::Int(_)
            | GenericValue::BigInt(_)
            | GenericValue::Float(_)
            | GenericValue::Double(_)
            | GenericValue::Decimal(_),
        ) if !typed(first, FlussoType::Decimal) => Some(Text::NumberConstant),
        FieldSource::Column(c)
            if c.ty == FlussoType::Binary && !typed(first, FlussoType::Binary) =>
        {
            Some(Text::Binary)
        }
        _ => None,
    }
}

/// Whether the last namesake's mapping takes the first one's constant
/// `value`; `None` where that is not pinned.
fn constant_fits(value: &GenericValue, last: &Field) -> Option<bool> {
    let mut leaves = Vec::new();
    let object = scalar_leaves(value, &mut leaves);
    match mapped_as(last) {
        Mapped::Unindexed | Mapped::Itself => Some(true),
        Mapped::Structured if object => None,
        Mapped::Structured => Some(leaves.is_empty()),
        _ if object => Some(false),
        Mapped::Anything => Some(true),
        Mapped::Scalar(MappingType::Other(name)) if name == "geo_point" => geo_fits(value),
        Mapped::Scalar(ty) => leaves
            .iter()
            .try_fold(true, |all, leaf| Some(all && scalar_fits(leaf, &ty)?)),
    }
}

/// Whether a `geo_point` takes the constant `value`: a `[lon, lat]` pair
/// does, a lone number or boolean doesn't; a string may be `"lat,lon"`.
fn geo_fits(value: &GenericValue) -> Option<bool> {
    let number = |v: &GenericValue| {
        matches!(
            v,
            GenericValue::SmallInt(_)
                | GenericValue::Int(_)
                | GenericValue::BigInt(_)
                | GenericValue::Float(_)
                | GenericValue::Double(_)
                | GenericValue::Decimal(_)
        )
    };
    match value {
        GenericValue::Array(items) if items.len() == 2 && items.iter().all(number) => Some(true),
        GenericValue::Bool(_) => Some(false),
        v if number(v) => Some(false),
        _ => None,
    }
}

/// Collects `value`'s scalars, lists flattened; `true` when it holds an object
/// (the scalars are then partial, and unused).
fn scalar_leaves<'a>(value: &'a GenericValue, leaves: &mut Vec<&'a GenericValue>) -> bool {
    match value {
        GenericValue::Map(_) => true,
        GenericValue::Array(items) => items.iter().any(|item| scalar_leaves(item, leaves)),
        GenericValue::Null => false,
        scalar => {
            leaves.push(scalar);
            false
        }
    }
}

/// Whether a field mapped as `ty` takes the scalar `value`; `None` where that
/// is not pinned.
fn scalar_fits(value: &GenericValue, ty: &MappingType) -> Option<bool> {
    use GenericValue as V;
    use MappingType as M;
    let numeric = matches!(
        ty,
        M::Byte
            | M::Short
            | M::Integer
            | M::Long
            | M::Float
            | M::Double
            | M::HalfFloat
            | M::ScaledFloat
    );
    match value {
        V::SmallInt(_) | V::Int(_) | V::BigInt(_) | V::Float(_) | V::Double(_) | V::Decimal(_) => {
            match ty {
                _ if numeric => Some(true),
                M::Date => Some(true),
                M::Boolean => Some(false),
                M::Other(name) if name == "geo_point" => Some(false),
                _ => None,
            }
        }
        V::Bool(_) => match ty {
            M::Boolean => Some(true),
            M::Date => Some(false),
            _ if numeric => Some(false),
            M::Other(name) if name == "geo_point" => Some(false),
            _ => None,
        },
        V::String(text) => match ty {
            _ if numeric => Some(text.trim().parse::<f64>().is_ok()),
            M::Boolean => Some(text == "true" || text == "false"),
            _ => None,
        },
        V::Date(_) | V::Timestamp(_) | V::TimestampTz(_) => match ty {
            M::Date => Some(true),
            M::Boolean => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Which documents fail to build when `first` is an `array: true` column of
/// scalars and `last`'s value can hold objects (`coerce_array` refuses an
/// object element), or `None` when none can.
fn fails_to_build(first: &Field, last: &Field) -> Option<&'static str> {
    let FieldSource::Column(column) = &first.source else {
        return None;
    };
    if !column.array || column.ty == FlussoType::Json {
        return None;
    }
    match &last.source {
        FieldSource::Relation(Relation::Join(_)) => Some("every document with a related row"),
        FieldSource::Group(_) => Some("every document"),
        FieldSource::Geo(_) => Some("every document with a point"),
        FieldSource::Column(column)
            if matches!(column.ty, FlussoType::Json | FlussoType::Map { .. }) =>
        {
            Some("every document whose value holds an object")
        }
        _ => None,
    }
}

/// An `array: true` column whose elements must be scalars (any type but
/// `json`).
fn scalar_array(field: &Field) -> bool {
    matches!(&field.source, FieldSource::Column(c) if c.array && c.ty != FlussoType::Json)
}

fn is_custom(field: &Field) -> bool {
    matches!(&field.source, FieldSource::Column(c) if matches!(c.ty, FlussoType::Custom { .. }))
}

/// An `array: true` column of any type.
fn array_column(field: &Field) -> bool {
    matches!(&field.source, FieldSource::Column(c) if c.array)
}

/// Whether a field's value is an array on its own: an `array: true` column,
/// a to-many join, an `ids` aggregate, a script returning an array.
fn builds_an_array(field: &Field) -> bool {
    match &field.source {
        FieldSource::Column(column) => column.array,
        FieldSource::Relation(Relation::Join(join)) => join.kind.is_to_many(),
        FieldSource::Relation(Relation::Aggregate(aggregate)) => {
            matches!(aggregate.op, AggregateOp::Ids { .. })
        }
        FieldSource::Script(script) => script.array,
        _ => false,
    }
}

/// The last script (at `at`) writes the value; the sink takes it only if the
/// last one's mapping does.
fn script_outcome(at: usize, same: &[&Field], last: &Field) -> DuplicateOutcome {
    let Some(FieldSource::Script(script)) = same.get(at).map(|f| &f.source) else {
        return DuplicateOutcome::Unknown;
    };
    let produced = script.ty.opensearch();
    match mapped_as(last) {
        Mapped::Itself | Mapped::Anything | Mapped::Unindexed => DuplicateOutcome::Holds(at),
        Mapped::Structured => DuplicateOutcome::Rejected {
            reason: Rejection::ScriptOutput {
                at,
                produced,
                wanted: None,
            },
            unless_it_fits: None,
        },
        Mapped::Scalar(ty) if ty == produced => DuplicateOutcome::Holds(at),
        Mapped::Scalar(ty) => DuplicateOutcome::Rejected {
            reason: Rejection::ScriptOutput {
                at,
                produced,
                wanted: Some(ty),
            },
            unless_it_fits: Some(at),
        },
    }
}

/// What the mapping of a field takes, for a value it did not build.
enum Mapped {
    /// The script whose output it is.
    Itself,
    /// Any scalar: a text-family field.
    Anything,
    /// Any value: a field that is not indexed.
    Unindexed,
    /// An object or nested array: a join, an object, a `json` or `map` column.
    Structured,
    /// A scalar of this type.
    Scalar(MappingType),
}

fn mapped_as(field: &Field) -> Mapped {
    let of = |ty: &FlussoType| match ty.opensearch() {
        MappingType::Text | MappingType::Keyword => Mapped::Anything,
        MappingType::Object | MappingType::Nested => Mapped::Structured,
        other => Mapped::Scalar(other),
    };
    match &field.source {
        FieldSource::Script(_) => Mapped::Itself,
        FieldSource::Constant(value) if constant_is_stored_unindexed(value) => Mapped::Unindexed,
        FieldSource::Constant(value) => match constant_mapping_type(value) {
            MappingType::Keyword | MappingType::Text => Mapped::Anything,
            MappingType::Object | MappingType::Nested => Mapped::Structured,
            other => Mapped::Scalar(other),
        },
        FieldSource::Column(column) if !column.indexed => Mapped::Unindexed,
        FieldSource::Column(column) => of(&column.ty),
        FieldSource::Relation(Relation::Join(join)) if !join.indexed => Mapped::Unindexed,
        FieldSource::Relation(Relation::Join(_)) | FieldSource::Group(_) => Mapped::Structured,
        FieldSource::Relation(Relation::Aggregate(aggregate)) => match &aggregate.op {
            AggregateOp::Count => Mapped::Scalar(MappingType::Long),
            AggregateOp::Exists => Mapped::Scalar(MappingType::Boolean),
            AggregateOp::Avg(_) => Mapped::Scalar(MappingType::Double),
            AggregateOp::Ids { element_type } => of(element_type),
            AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => aggregate
                .value_type
                .as_ref()
                .map_or(Mapped::Scalar(MappingType::Double), of),
        },
        FieldSource::Geo(_) => of(&FlussoType::GeoPoint),
    }
}

/// The shape a field's value has in a document.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// A leaf: a column, an aggregate, a geo point, a constant, a script.
    Value,
    /// An object: a group or a to-one join.
    Object,
    /// An array of objects: a to-many join.
    Rows,
}

fn shape(field: &Field) -> Shape {
    match &field.source {
        FieldSource::Group(_) => Shape::Object,
        FieldSource::Relation(Relation::Join(join)) if join.kind.is_to_many() => Shape::Rows,
        FieldSource::Relation(Relation::Join(_)) => Shape::Object,
        _ => Shape::Value,
    }
}

/// Whether a value-shaped field's value can be an object: a `json` or `map`
/// column (or array), a geo point, a custom type.
fn value_holds_objects(field: &Field) -> bool {
    match &field.source {
        FieldSource::Column(column) => matches!(
            column.ty,
            FlussoType::Json | FlussoType::Map { .. } | FlussoType::Custom { .. }
        ),
        FieldSource::Geo(_) => true,
        _ => false,
    }
}

/// Two emitted, script-free namesakes: the last one's value read through the
/// first one's declaration.
fn document_outcome(last_at: usize, first: &Field, last: &Field) -> DuplicateOutcome {
    if array_column(first) {
        if builds_an_array(last) {
            return DuplicateOutcome::Holds(last_at);
        }
        // An indexed `json` column holding keys is refused by the strict
        // mapping on its own; wrapped, what it does is not pinned.
        if is_indexed_json(last) {
            return DuplicateOutcome::Unknown;
        }
        return DuplicateOutcome::Wrapped;
    }
    let firsts = first.children();
    let lasts = last.children();
    match (shape(first), shape(last)) {
        (Shape::Value, _) => DuplicateOutcome::Holds(last_at),
        (Shape::Object, Shape::Rows) if matches!(first.source, FieldSource::Group(_)) => {
            DuplicateOutcome::Holds(last_at)
        }
        (_, Shape::Object | Shape::Rows) => {
            if lasts
                .iter()
                .all(|child| firsts.iter().any(|f| f.field == child.field))
            {
                return DuplicateOutcome::Holds(last_at);
            }
            let kept = firsts
                .iter()
                .filter(|child| lasts.iter().any(|o| o.field == child.field))
                .map(|child| child.field.clone())
                .collect();
            DuplicateOutcome::FirstsChildren { kept }
        }
        // An object value is cut to the keys named like the first one's child
        // fields: a dynamic `map` or an unindexed column takes what is left.
        (_, Shape::Value) if takes_any_keys(last) => DuplicateOutcome::FirstsChildren {
            kept: firsts.iter().map(|child| child.field.clone()).collect(),
        },
        // A point cut to child fields with no `lat` and `lon` is no point.
        (_, Shape::Value)
            if matches!(last.source, FieldSource::Geo(_))
                && !["lat", "lon"]
                    .iter()
                    .all(|key| firsts.iter().any(|f| f.field.as_ref() == *key)) =>
        {
            DuplicateOutcome::Rejected {
                reason: Rejection::PointWithoutLatLon,
                unless_it_fits: None,
            }
        }
        (_, Shape::Value) if value_holds_objects(last) => DuplicateOutcome::Unknown,
        (_, Shape::Value) => DuplicateOutcome::Holds(last_at),
    }
}

/// A `map` column, or an unindexed `json` one: OpenSearch takes any keys.
fn takes_any_keys(field: &Field) -> bool {
    matches!(&field.source, FieldSource::Column(c)
        if !c.array && (matches!(c.ty, FlussoType::Map { .. }) || (c.ty == FlussoType::Json && !c.indexed)))
}

fn is_indexed_json(field: &Field) -> bool {
    matches!(&field.source, FieldSource::Column(c) if c.ty == FlussoType::Json && c.indexed)
}

/// A field in a few words: what it reads, and `emit: false` when it is input-only.
pub(crate) fn described(field: &Field) -> String {
    let what = match &field.source {
        FieldSource::Column(column) => format!("column `{}`", column.column),
        FieldSource::Relation(Relation::Join(join)) => format!("join on `{}`", join.table),
        FieldSource::Relation(relation) => format!("aggregate on `{}`", relation.table()),
        FieldSource::Group(_) => "object".to_owned(),
        FieldSource::Geo(_) => "geo point".to_owned(),
        FieldSource::Constant(_) => "constant".to_owned(),
        FieldSource::Script(_) => "script".to_owned(),
    };
    if field.emit {
        what
    } else {
        format!("{what}, `emit: false`")
    }
}
