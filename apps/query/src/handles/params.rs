//! Closed enums for the query parameters that take a fixed set of tokens —
//! replacing the stringly-typed `operator("and")` / `score_mode("avg")` shape so
//! a typo is a compile error, not a 400 from OpenSearch.
//!
//! Genuinely open-ended params (`analyzer`, `format`, `time_zone`,
//! `minimum_should_match`, simple-query-string `flags`) stay `String` — they
//! aren't enumerable.

use serde_json::Value;

/// Boolean combinator for analyzed terms (`operator` / `default_operator`).
#[derive(Debug, Clone, Copy)]
pub enum Operator {
    /// Every term must match.
    And,
    /// Any term may match.
    Or,
}

impl Operator {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Operator::And => "AND",
            Operator::Or => "OR",
        }
    }
}

/// How a `function_score`'s functions combine into one score.
#[derive(Debug, Clone, Copy)]
pub enum ScoreMode {
    /// Multiply the function scores (default).
    Multiply,
    /// Sum them.
    Sum,
    /// Average them.
    Avg,
    /// Take the first matching function's score.
    First,
    /// Take the largest.
    Max,
    /// Take the smallest.
    Min,
}

impl ScoreMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ScoreMode::Multiply => "multiply",
            ScoreMode::Sum => "sum",
            ScoreMode::Avg => "avg",
            ScoreMode::First => "first",
            ScoreMode::Max => "max",
            ScoreMode::Min => "min",
        }
    }
}

/// How a `nested` query's matching-element scores combine into the parent score.
/// Distinct from [`ScoreMode`]: no `multiply`/`first`, plus `None` (the nested
/// clause acts as a pure filter, contributing no score).
#[derive(Debug, Clone, Copy)]
pub enum NestedScoreMode {
    /// Average the element scores (default).
    Avg,
    /// Sum them.
    Sum,
    /// Take the smallest.
    Min,
    /// Take the largest.
    Max,
    /// Don't contribute to the parent score (filter only).
    None,
}

impl NestedScoreMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NestedScoreMode::Avg => "avg",
            NestedScoreMode::Sum => "sum",
            NestedScoreMode::Min => "min",
            NestedScoreMode::Max => "max",
            NestedScoreMode::None => "none",
        }
    }
}

/// How a `function_score`'s combined function score merges with the query score.
#[derive(Debug, Clone, Copy)]
pub enum BoostMode {
    /// Multiply (default).
    Multiply,
    /// Replace the query score entirely.
    Replace,
    /// Sum them.
    Sum,
    /// Average them.
    Avg,
    /// Take the largest.
    Max,
    /// Take the smallest.
    Min,
}

impl BoostMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BoostMode::Multiply => "multiply",
            BoostMode::Replace => "replace",
            BoostMode::Sum => "sum",
            BoostMode::Avg => "avg",
            BoostMode::Max => "max",
            BoostMode::Min => "min",
        }
    }
}

/// What a `match` does when analysis yields no terms (`zero_terms_query`).
#[derive(Debug, Clone, Copy)]
pub enum ZeroTermsQuery {
    /// Match nothing (default).
    None,
    /// Match everything.
    All,
}

impl ZeroTermsQuery {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ZeroTermsQuery::None => "none",
            ZeroTermsQuery::All => "all",
        }
    }
}

/// How a `range` relates to range-typed field values (`relation`).
#[derive(Debug, Clone, Copy)]
pub enum RangeRelation {
    /// The ranges overlap (default).
    Intersects,
    /// The field range fully contains the query range.
    Contains,
    /// The field range falls entirely within the query range.
    Within,
}

impl RangeRelation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RangeRelation::Intersects => "INTERSECTS",
            RangeRelation::Contains => "CONTAINS",
            RangeRelation::Within => "WITHIN",
        }
    }
}

/// The scoring `type` of a `multi_match`.
#[derive(Debug, Clone, Copy)]
pub enum MultiMatchType {
    /// Score by the single best-matching field (default).
    BestFields,
    /// Sum the scores of every matching field.
    MostFields,
    /// Treat the fields as one big field, term-centric.
    CrossFields,
    /// Phrase match on each field.
    Phrase,
    /// Phrase-prefix match on each field.
    PhrasePrefix,
    /// Bool-prefix match on each field.
    BoolPrefix,
}

impl MultiMatchType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MultiMatchType::BestFields => "best_fields",
            MultiMatchType::MostFields => "most_fields",
            MultiMatchType::CrossFields => "cross_fields",
            MultiMatchType::Phrase => "phrase",
            MultiMatchType::PhrasePrefix => "phrase_prefix",
            MultiMatchType::BoolPrefix => "bool_prefix",
        }
    }
}

/// Allowed edit distance for fuzzy matching (`fuzziness`).
#[derive(Debug, Clone, Copy)]
pub enum Fuzziness {
    /// Distance scaled to term length (the usual choice).
    Auto,
    /// `AUTO` with explicit low/high length thresholds (`AUTO:lo:hi`).
    AutoBounds(u32, u32),
    /// A fixed number of edits.
    Edits(u32),
}

impl Fuzziness {
    pub(crate) fn to_value(self) -> Value {
        match self {
            Fuzziness::Auto => Value::String("AUTO".to_string()),
            Fuzziness::AutoBounds(lo, hi) => Value::String(format!("AUTO:{lo}:{hi}")),
            Fuzziness::Edits(edits) => Value::from(edits),
        }
    }
}
