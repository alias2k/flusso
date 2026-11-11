//! Tokenizer and recursive-descent parser for [`Expression`].
//!
//! ```text
//! expression := term (("+" | "-") term)*
//! term       := unary (("*" | "/") unary)*
//! unary      := "-" unary | primary
//! primary    := column | number | "(" expression ")"
//! column     := [A-Za-z_][A-Za-z0-9_]*      (lowercased, at most 63 characters)
//! number     := [0-9]+ ("." [0-9]+)?
//! ```
//!
//! Whitespace between tokens is ignored. Positions are 1-based characters.
//!
//! ```
//! use kernel::config::{Expression, ExpressionErrorKind};
//!
//! let err = Expression::parse("(quantity * 2").unwrap_err();
//! assert_eq!(err.kind, ExpressionErrorKind::UnclosedParenthesis);
//! assert_eq!(err.position, 1);
//! ```

use std::fmt;

use super::{BinaryOp, Expression, NumberLiteral};
use crate::common::ColumnName;

/// How deep an expression may nest: parentheses and unary minus on the way
/// down, and the height of the resulting tree.
pub const MAX_DEPTH: usize = 64;

/// A malformed expression: what went wrong and where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("at position {position}: {kind}")]
pub struct ExpressionError {
    /// The 1-based character position the parser stopped at.
    pub position: usize,
    /// What went wrong.
    pub kind: ExpressionErrorKind,
}

/// What made an expression malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExpressionErrorKind {
    #[error("the expression is empty")]
    Empty,
    #[error(
        "unexpected character `{0}`: an expression holds column names, numbers, \
         `+ - * /` and parentheses"
    )]
    UnexpectedCharacter(char),
    #[error("a number needs a digit after its `.`")]
    IncompleteNumber,
    #[error("`{0}` is not a valid column name: at most 63 characters")]
    InvalidColumn(String),
    #[error("expected a column, a number, `-` or `(`, found {0}")]
    ExpectedOperand(String),
    #[error("expected an operator (`+ - * /`) or the end, found {0}")]
    ExpectedOperator(String),
    #[error("this `(` is never closed")]
    UnclosedParenthesis,
    #[error("this `)` has no matching `(`")]
    UnmatchedParenthesis,
    #[error("the expression nests deeper than {MAX_DEPTH} levels")]
    TooDeep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Column(String),
    Number(String),
    Plus,
    Minus,
    Star,
    Slash,
    Open,
    Close,
    End,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Column(name) => write!(f, "column `{name}`"),
            Self::Number(text) => write!(f, "number `{text}`"),
            Self::Plus => f.write_str("`+`"),
            Self::Minus => f.write_str("`-`"),
            Self::Star => f.write_str("`*`"),
            Self::Slash => f.write_str("`/`"),
            Self::Open => f.write_str("`(`"),
            Self::Close => f.write_str("`)`"),
            Self::End => f.write_str("the end"),
        }
    }
}

fn error(position: usize, kind: ExpressionErrorKind) -> ExpressionError {
    ExpressionError { position, kind }
}

/// Split `input` into tokens, each with its 1-based position, ending in
/// [`Token::End`].
fn tokenize(input: &str) -> Result<Vec<(Token, usize)>, ExpressionError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().zip(1..).peekable();
    let mut last = 0;
    while let Some((c, position)) = chars.next() {
        last = position;
        let token = match c {
            c if c.is_whitespace() => continue,
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '(' => Token::Open,
            ')' => Token::Close,
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::from(c.to_ascii_lowercase());
                while let Some(&(next, at)) = chars.peek() {
                    if !(next.is_ascii_alphanumeric() || next == '_') {
                        break;
                    }
                    name.push(next.to_ascii_lowercase());
                    last = at;
                    chars.next();
                }
                Token::Column(name)
            }
            c if c.is_ascii_digit() => {
                let mut text = String::from(c);
                take_digits(&mut chars, &mut text, &mut last);
                if let Some(&('.', dot)) = chars.peek() {
                    text.push('.');
                    last = dot;
                    chars.next();
                    let before = text.len();
                    take_digits(&mut chars, &mut text, &mut last);
                    if text.len() == before {
                        return Err(error(dot, ExpressionErrorKind::IncompleteNumber));
                    }
                }
                Token::Number(text)
            }
            other => {
                return Err(error(
                    position,
                    ExpressionErrorKind::UnexpectedCharacter(other),
                ));
            }
        };
        tokens.push((token, position));
    }
    tokens.push((Token::End, last + 1));
    Ok(tokens)
}

fn take_digits(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (char, usize)>>,
    text: &mut String,
    last: &mut usize,
) {
    while let Some(&(next, at)) = chars.peek() {
        if !next.is_ascii_digit() {
            break;
        }
        text.push(next);
        *last = at;
        chars.next();
    }
}

/// A parsed subtree and its height (a leaf is 0).
struct Node {
    expression: Expression,
    height: usize,
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    next: usize,
}

pub(super) fn parse(input: &str) -> Result<Expression, ExpressionError> {
    let mut parser = Parser {
        tokens: tokenize(input)?,
        next: 0,
    };
    if let (Token::End, _) = parser.peek() {
        return Err(error(1, ExpressionErrorKind::Empty));
    }
    let node = parser.expression(0)?;
    match parser.peek() {
        (Token::End, _) => Ok(node.expression),
        (Token::Close, position) => Err(error(position, ExpressionErrorKind::UnmatchedParenthesis)),
        (token, position) => Err(error(
            position,
            ExpressionErrorKind::ExpectedOperator(token.to_string()),
        )),
    }
}

impl Parser {
    fn peek(&self) -> (Token, usize) {
        self.tokens
            .get(self.next)
            .or_else(|| self.tokens.last())
            .cloned()
            .unwrap_or((Token::End, 1))
    }

    fn advance(&mut self) {
        self.next += 1;
    }

    fn expression(&mut self, depth: usize) -> Result<Node, ExpressionError> {
        let mut left = self.term(depth)?;
        loop {
            let (op, position) = match self.peek() {
                (Token::Plus, position) => (BinaryOp::Add, position),
                (Token::Minus, position) => (BinaryOp::Subtract, position),
                _ => return Ok(left),
            };
            self.advance();
            let right = self.term(depth)?;
            left = binary(op, left, right, position)?;
        }
    }

    fn term(&mut self, depth: usize) -> Result<Node, ExpressionError> {
        let mut left = self.unary(depth)?;
        loop {
            let (op, position) = match self.peek() {
                (Token::Star, position) => (BinaryOp::Multiply, position),
                (Token::Slash, position) => (BinaryOp::Divide, position),
                _ => return Ok(left),
            };
            self.advance();
            let right = self.unary(depth)?;
            left = binary(op, left, right, position)?;
        }
    }

    fn unary(&mut self, depth: usize) -> Result<Node, ExpressionError> {
        let (Token::Minus, position) = self.peek() else {
            return self.primary(depth);
        };
        let depth = deeper(depth, position)?;
        self.advance();
        let inner = self.unary(depth)?;
        let height = taller(inner.height, position)?;
        Ok(Node {
            expression: Expression::Negate(Box::new(inner.expression)),
            height,
        })
    }

    fn primary(&mut self, depth: usize) -> Result<Node, ExpressionError> {
        let (token, position) = self.peek();
        let expression = match token {
            Token::Column(name) => match ColumnName::try_new(&name) {
                Ok(column) => Expression::Column(column),
                Err(_) => {
                    return Err(error(position, ExpressionErrorKind::InvalidColumn(name)));
                }
            },
            Token::Number(text) => Expression::Number(NumberLiteral(text)),
            Token::Open => {
                let depth = deeper(depth, position)?;
                self.advance();
                let inner = self.expression(depth)?;
                return match self.peek() {
                    (Token::Close, _) => {
                        self.advance();
                        Ok(inner)
                    }
                    (Token::End, _) => {
                        Err(error(position, ExpressionErrorKind::UnclosedParenthesis))
                    }
                    (token, at) => Err(error(
                        at,
                        ExpressionErrorKind::ExpectedOperator(token.to_string()),
                    )),
                };
            }
            other => {
                return Err(error(
                    position,
                    ExpressionErrorKind::ExpectedOperand(other.to_string()),
                ));
            }
        };
        self.advance();
        Ok(Node {
            expression,
            height: 0,
        })
    }
}

fn binary(op: BinaryOp, left: Node, right: Node, position: usize) -> Result<Node, ExpressionError> {
    let height = taller(left.height.max(right.height), position)?;
    Ok(Node {
        expression: Expression::Binary {
            op,
            left: Box::new(left.expression),
            right: Box::new(right.expression),
        },
        height,
    })
}

fn deeper(depth: usize, position: usize) -> Result<usize, ExpressionError> {
    if depth >= MAX_DEPTH {
        return Err(error(position, ExpressionErrorKind::TooDeep));
    }
    Ok(depth + 1)
}

fn taller(height: usize, position: usize) -> Result<usize, ExpressionError> {
    if height >= MAX_DEPTH {
        return Err(error(position, ExpressionErrorKind::TooDeep));
    }
    Ok(height + 1)
}
