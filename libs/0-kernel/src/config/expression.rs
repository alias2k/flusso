//! The input of a `sum`/`avg`/`min`/`max` aggregate: one column, or arithmetic
//! over the columns of the aggregated table.
//!
//! An [`Expression`] is a tree: columns and numeric literals at the leaves,
//! `+ - * /` and unary minus above them. [`Expression::parse`] builds it with a
//! hand-written tokenizer and a recursive-descent parser (no regular
//! expression): usual precedence (`*` `/` over `+` `-`, unary minus over both),
//! left associativity, parentheses to group.
//!
//! - **A plain column is the [`Expression::Column`] leaf.** It serializes,
//!   hashes and displays exactly as the bare [`ColumnName`] the aggregate held
//!   before expressions existed, so an existing schema keeps its content hash
//!   and an existing `flusso.lock` keeps its bytes.
//! - **An expression is stored as its canonical text** ([`Display`](std::fmt::Display)): single
//!   spaces around binary operators, only the parentheses the tree needs. Two
//!   spellings of one tree (whitespace, redundant parentheses) serialize and
//!   hash the same.
//! - **Malformed input fails with a position** ([`ExpressionError`]): the
//!   1-based character the parser stopped at.
//! - **Depth is capped at [`MAX_DEPTH`]**, for the parser's recursion and the
//!   tree's height, so no input can overflow the stack.
//!
//! ```
//! use kernel::config::{BinaryOp, Expression};
//!
//! let total: Expression = " quantity*(unit_price - discount) ".parse().unwrap();
//! assert_eq!(total.to_string(), "quantity * (unit_price - discount)");
//! assert!(matches!(total, Expression::Binary { op: BinaryOp::Multiply, .. }));
//! let names: Vec<&str> = total.columns().into_iter().map(|c| c.as_ref()).collect();
//! assert_eq!(names, ["quantity", "unit_price", "discount"]);
//!
//! let plain: Expression = "amount".parse().unwrap();
//! assert_eq!(plain.as_column().map(|c| c.as_ref()), Some("amount"));
//!
//! let err = "quantity *".parse::<Expression>().unwrap_err();
//! assert_eq!(err.position, 11);
//! ```

mod parse;
#[cfg(test)]
mod tests;

use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::ColumnName;

pub use parse::{ExpressionError, ExpressionErrorKind, MAX_DEPTH};

/// Arithmetic over the columns of one table, evaluated per row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    /// A column of the table.
    Column(ColumnName),
    /// A numeric literal.
    Number(NumberLiteral),
    /// Unary minus.
    Negate(Box<Expression>),
    /// `left op right`.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        left: Box<Expression>,
        /// The right operand.
        right: Box<Expression>,
    },
}

/// A binary arithmetic operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
}

/// A numeric literal as written: ASCII digits, optionally a `.` and more digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NumberLiteral(String);

impl Expression {
    /// Parse `input`. A lone column name gives [`Expression::Column`].
    pub fn parse(input: &str) -> Result<Self, ExpressionError> {
        parse::parse(input)
    }

    /// The column, when the expression is a plain column.
    pub fn as_column(&self) -> Option<&ColumnName> {
        match self {
            Self::Column(column) => Some(column),
            _ => None,
        }
    }

    /// Every column the expression reads, once each, in order of appearance.
    pub fn columns(&self) -> Vec<&ColumnName> {
        let mut out = Vec::new();
        self.collect_columns(&mut out);
        out
    }

    fn collect_columns<'a>(&'a self, out: &mut Vec<&'a ColumnName>) {
        match self {
            Self::Column(column) => {
                if !out.contains(&column) {
                    out.push(column);
                }
            }
            Self::Number(_) => {}
            Self::Negate(inner) => inner.collect_columns(out),
            Self::Binary { left, right, .. } => {
                left.collect_columns(out);
                right.collect_columns(out);
            }
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Self::Binary { op, .. } => op.precedence(),
            Self::Negate(_) => 3,
            Self::Column(_) | Self::Number(_) => 4,
        }
    }
}

impl BinaryOp {
    /// The operator as written: `+`, `-`, `*` or `/`.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
        }
    }

    fn precedence(self) -> u8 {
        match self {
            Self::Add | Self::Subtract => 1,
            Self::Multiply | Self::Divide => 2,
        }
    }
}

impl NumberLiteral {
    /// The literal's text: digits, optionally a `.` and more digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<ColumnName> for Expression {
    fn from(column: ColumnName) -> Self {
        Self::Column(column)
    }
}

impl FromStr for Expression {
    type Err = ExpressionError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

/// The canonical text: single spaces around binary operators, and parentheses
/// only where the tree needs them (a right operand at the same precedence keeps
/// them, since the operators associate left).
impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Column(column) => f.write_str(column.as_ref()),
            Self::Number(number) => f.write_str(number.as_str()),
            Self::Negate(inner) if inner.precedence() <= 3 => write!(f, "-({inner})"),
            Self::Negate(inner) => write!(f, "-{inner}"),
            Self::Binary { op, left, right } => {
                write_operand(f, left, left.precedence() < op.precedence())?;
                write!(f, " {} ", op.symbol())?;
                write_operand(f, right, right.precedence() <= op.precedence())
            }
        }
    }
}

fn write_operand(f: &mut fmt::Formatter<'_>, operand: &Expression, group: bool) -> fmt::Result {
    if group {
        write!(f, "({operand})")
    } else {
        write!(f, "{operand}")
    }
}

/// A plain column hashes as the [`ColumnName`] it is, so an aggregate written
/// before expressions keeps its content hash. Anything else hashes its
/// canonical text, which is never a column name: it holds an operator, or is
/// a number, and a column name never starts with a digit.
impl Hash for Expression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Column(column) => column.hash(state),
            other => other.to_string().hash(state),
        }
    }
}

impl Serialize for Expression {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Expression {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text)
            .map_err(|e| serde::de::Error::custom(format!("invalid expression `{text}`: {e}")))
    }
}
