#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use proptest::prelude::*;

use super::{BinaryOp, Expression, ExpressionErrorKind, MAX_DEPTH, NumberLiteral};
use crate::common::ColumnName;

fn parse(input: &str) -> Expression {
    Expression::parse(input).unwrap()
}

fn fails(input: &str) -> (usize, ExpressionErrorKind) {
    let err = Expression::parse(input).unwrap_err();
    (err.position, err.kind)
}

fn col(name: &str) -> Expression {
    Expression::Column(ColumnName::try_new(name).unwrap())
}

fn num(text: &str) -> Expression {
    Expression::Number(NumberLiteral(text.to_owned()))
}

fn bin(op: BinaryOp, left: Expression, right: Expression) -> Expression {
    Expression::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn neg(inner: Expression) -> Expression {
    Expression::Negate(Box::new(inner))
}

#[test]
fn a_lone_name_is_a_plain_column() {
    assert_eq!(parse("amount"), col("amount"));
    assert_eq!(parse("  Unit_Price2 "), col("unit_price2"));
    assert_eq!(parse("(amount)"), col("amount"));
    assert_eq!(parse("_x").as_column().map(|c| c.as_ref()), Some("_x"));
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    use BinaryOp::*;
    assert_eq!(
        parse("a + b * c"),
        bin(Add, col("a"), bin(Multiply, col("b"), col("c")))
    );
    assert_eq!(
        parse("a * b + c"),
        bin(Add, bin(Multiply, col("a"), col("b")), col("c"))
    );
    assert_eq!(
        parse("(a + b) * c"),
        bin(Multiply, bin(Add, col("a"), col("b")), col("c"))
    );
}

#[test]
fn operators_associate_left() {
    use BinaryOp::*;
    assert_eq!(
        parse("a - b - c"),
        bin(Subtract, bin(Subtract, col("a"), col("b")), col("c"))
    );
    assert_eq!(
        parse("a / b / c"),
        bin(Divide, bin(Divide, col("a"), col("b")), col("c"))
    );
    assert_eq!(
        parse("a - (b - c)"),
        bin(Subtract, col("a"), bin(Subtract, col("b"), col("c")))
    );
}

#[test]
fn unary_minus_binds_tightest() {
    use BinaryOp::*;
    assert_eq!(parse("-a * b"), bin(Multiply, neg(col("a")), col("b")));
    assert_eq!(parse("a - -b"), bin(Subtract, col("a"), neg(col("b"))));
    assert_eq!(parse("--a"), neg(neg(col("a"))));
    assert_eq!(parse("-(a + b)"), neg(bin(Add, col("a"), col("b"))));
    assert_eq!(parse("-2"), neg(num("2")));
}

#[test]
fn numbers_are_integers_or_decimals_as_written() {
    use BinaryOp::*;
    assert_eq!(
        parse("quantity * 1.50"),
        bin(Multiply, col("quantity"), num("1.50"))
    );
    assert_eq!(parse("007 + 0.5"), bin(Add, num("007"), num("0.5")));
}

#[test]
fn display_is_canonical() {
    for (input, canonical) in [
        ("quantity*unit_price", "quantity * unit_price"),
        ("((a)) + (b * c)", "a + b * c"),
        ("(a + b) * c", "(a + b) * c"),
        ("a - (b - c)", "a - (b - c)"),
        ("(a - b) - c", "a - b - c"),
        ("a / (b * c)", "a / (b * c)"),
        ("-(a)", "-a"),
        ("--a", "- -a"),
        ("-(-a)", "- -a"),
        ("---(a + b)", "- - -(a + b)"),
        ("a - --b", "a - - -b"),
        ("-(a * b)", "-(a * b)"),
        ("a--b", "a - -b"),
        ("-2.5*x", "-2.5 * x"),
    ] {
        assert_eq!(parse(input).to_string(), canonical, "{input}");
    }
}

#[test]
fn rejections_name_the_position() {
    use ExpressionErrorKind::*;
    assert_eq!(fails(""), (1, Empty));
    assert_eq!(fails("   "), (1, Empty));
    assert_eq!(fails("quantity *"), (11, ExpectedOperand("the end".into())));
    assert_eq!(fails("a + * b"), (5, ExpectedOperand("`*`".into())));
    assert_eq!(fails("a b"), (3, ExpectedOperator("column `b`".into())));
    assert_eq!(fails("2x"), (2, ExpectedOperator("column `x`".into())));
    assert_eq!(fails("a % b"), (3, UnexpectedCharacter('%')));
    assert_eq!(fails("sum(a)"), (4, ExpectedOperator("`(`".into())));
    assert_eq!(fails("a + 1."), (6, IncompleteNumber));
    assert_eq!(fails("a + .5"), (5, UnexpectedCharacter('.')));
    assert_eq!(fails("(a + b"), (1, UnclosedParenthesis));
    assert_eq!(fails("a + (b"), (5, UnclosedParenthesis));
    assert_eq!(fails("(a b)"), (4, ExpectedOperator("column `b`".into())));
    assert_eq!(fails("a + b)"), (6, UnmatchedParenthesis));
    assert_eq!(fails("()"), (2, ExpectedOperand("`)`".into())));
    assert_eq!(fails("a = b"), (3, UnexpectedCharacter('=')));
    assert_eq!(fails("a; drop table x"), (2, UnexpectedCharacter(';')));
    assert_eq!(fails("é + a"), (1, UnexpectedCharacter('é')));
    assert_eq!(fails("é + 'a'"), (1, UnexpectedCharacter('é')));
    let long = "a".repeat(64);
    assert_eq!(fails(&format!("1 + {long}")), (5, InvalidColumn(long)));
}

#[test]
fn positions_count_characters_not_bytes() {
    assert_eq!(
        fails("a ž"),
        (3, ExpressionErrorKind::UnexpectedCharacter('ž'))
    );
    assert_eq!(
        fails("a +  ž"),
        (6, ExpressionErrorKind::UnexpectedCharacter('ž'))
    );
}

#[test]
fn the_message_reads_position_first() {
    let err = Expression::parse("quantity *").unwrap_err();
    assert_eq!(
        err.to_string(),
        "at position 11: expected a column, a number, `-` or `(`, found the end"
    );
}

#[test]
fn nesting_deeper_than_the_cap_is_refused() {
    let at_cap = format!("{}a{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    assert_eq!(parse(&at_cap), col("a"));
    let over = format!(
        "{}a{}",
        "(".repeat(MAX_DEPTH + 1),
        ")".repeat(MAX_DEPTH + 1)
    );
    assert_eq!(fails(&over), (MAX_DEPTH + 1, ExpressionErrorKind::TooDeep));
    let minus = format!("{}a", "-".repeat(MAX_DEPTH + 1));
    assert_eq!(fails(&minus).1, ExpressionErrorKind::TooDeep);
}

fn renders_back(input: &str) -> Expression {
    let tree = parse(input);
    let text = tree.to_string();
    assert!(!text.contains("--"), "{input} rendered {text}");
    assert_eq!(
        Expression::parse(&text).as_ref(),
        Ok(&tree),
        "{input} rendered {text}"
    );
    tree
}

fn chain(opening: impl Fn(usize) -> &'static str, n: usize) -> String {
    let open: String = (0..n).map(&opening).collect();
    let close = ")".repeat(open.matches('(').count());
    format!("{open}a{close}")
}

#[test]
fn every_chain_up_to_the_cap_renders_to_text_that_parses_back() {
    let shapes: [(&str, fn(usize) -> &'static str); 4] = [
        ("minus", |_| "-"),
        ("parenthesis", |_| "("),
        (
            "minus then parenthesis",
            |i| if i % 2 == 0 { "-" } else { "(" },
        ),
        (
            "parenthesis then minus",
            |i| if i % 2 == 0 { "(" } else { "-" },
        ),
    ];
    for (shape, opening) in shapes {
        for n in 1..=MAX_DEPTH {
            renders_back(&chain(opening, n));
        }
        assert_eq!(
            fails(&chain(opening, MAX_DEPTH + 1)).1,
            ExpressionErrorKind::TooDeep,
            "{shape}"
        );
    }
    let minuses = renders_back(&format!("{}a", "-".repeat(MAX_DEPTH)));
    assert_eq!(height(&minuses), MAX_DEPTH);
}

#[test]
fn a_tree_taller_than_the_cap_is_refused() {
    let chain = |terms: usize| vec!["a"; terms].join(" + ");
    assert!(Expression::parse(&chain(MAX_DEPTH + 1)).is_ok());
    assert_eq!(fails(&chain(MAX_DEPTH + 2)).1, ExpressionErrorKind::TooDeep);
}

#[test]
fn pathological_input_neither_panics_nor_overflows() {
    for input in [
        "(".repeat(100_000),
        ")".repeat(100_000),
        "-".repeat(100_000),
        "a+".repeat(100_000) + "a",
        "a*(".repeat(100_000),
    ] {
        assert!(Expression::parse(&input).is_err());
    }
}

#[test]
fn a_whole_column_name_keeps_the_column_name_rules() {
    let long = "a".repeat(64);
    let at_limit = "a".repeat(63);
    for input in [
        "amount",
        "Amount",
        "AMOUNT_2",
        "  amount  ",
        "\u{2003}amount\u{3000}",
        "\tamount\n",
        "\u{212A}ey",
        "\u{212A}",
        "_",
        at_limit.as_str(),
        long.as_str(),
        " a ",
        "2amount",
        "am ount",
        "",
        "é",
        "\u{0130}d",
    ] {
        let expected = ColumnName::try_new(input).ok();
        let parsed = Expression::parse(input);
        match &expected {
            Some(column) => assert_eq!(parsed, Ok(Expression::Column(column.clone())), "{input:?}"),
            None => assert!(parsed.is_err(), "{input:?} parsed as {parsed:?}"),
        }
    }
    assert_eq!(parse("\u{212A}ey"), col("key"));
    assert_eq!(parse(&format!(" {at_limit} ")), col(&at_limit));
}

#[test]
fn columns_lists_each_once_in_order() {
    let expr = parse("b * a + b / (c - a)");
    let names: Vec<&str> = expr.columns().into_iter().map(|c| c.as_ref()).collect();
    assert_eq!(names, ["b", "a", "c"]);
    assert!(parse("1 + 2").columns().is_empty());
}

#[test]
fn a_plain_column_serializes_and_hashes_as_its_name() {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    let column = ColumnName::try_new("amount").unwrap();
    let hasher = BuildHasherDefault::<DefaultHasher>::default();
    assert_eq!(
        hasher.hash_one(&column),
        hasher.hash_one(Expression::Column(column.clone()))
    );
    assert_eq!(
        serde_json::to_string(&Expression::Column(column.clone())).unwrap(),
        serde_json::to_string(&column).unwrap()
    );
}

#[test]
fn serde_round_trips_through_the_canonical_text() {
    let expr: Expression = serde_json::from_str(r#"" quantity*(price-1.5) ""#).unwrap();
    assert_eq!(
        serde_json::to_string(&expr).unwrap(),
        r#""quantity * (price - 1.5)""#
    );
    let err = serde_json::from_str::<Expression>(r#""a +""#).unwrap_err();
    assert!(
        err.to_string()
            .contains("invalid expression `a +`: at position 4"),
        "{err}"
    );
}

fn arb_expression() -> impl Strategy<Value = Expression> {
    let leaf = prop_oneof![
        "[a-z_][a-z0-9_]{0,8}".prop_map(|name| col(&name)),
        "[0-9]{1,4}(\\.[0-9]{1,3})?".prop_map(|text| num(&text)),
    ];
    leaf.prop_recursive(6, 48, 2, |inner| {
        prop_oneof![
            inner.clone().prop_map(neg),
            (
                prop_oneof![
                    Just(BinaryOp::Add),
                    Just(BinaryOp::Subtract),
                    Just(BinaryOp::Multiply),
                    Just(BinaryOp::Divide),
                ],
                inner.clone(),
                inner,
            )
                .prop_map(|(op, left, right)| bin(op, left, right)),
        ]
    })
}

fn height(expression: &Expression) -> usize {
    match expression {
        Expression::Column(_) | Expression::Number(_) => 0,
        Expression::Negate(inner) => 1 + height(inner),
        Expression::Binary { left, right, .. } => 1 + height(left).max(height(right)),
    }
}

#[derive(Debug, Clone)]
enum Wrap {
    Negate,
    Left(BinaryOp, Expression),
    Right(BinaryOp, Expression),
}

fn arb_op() -> impl Strategy<Value = BinaryOp> {
    prop_oneof![
        Just(BinaryOp::Add),
        Just(BinaryOp::Subtract),
        Just(BinaryOp::Multiply),
        Just(BinaryOp::Divide),
    ]
}

/// A small tree under a chain of negations and binary operators as tall as
/// the cap allows: the shapes that render deepest.
fn arb_deep_expression() -> impl Strategy<Value = Expression> {
    let wrap = prop_oneof![
        3 => Just(Wrap::Negate),
        1 => (arb_op(), "[a-c]").prop_map(|(op, name)| Wrap::Left(op, col(&name))),
        1 => (arb_op(), "[a-c]").prop_map(|(op, name)| Wrap::Right(op, col(&name))),
    ];
    (arb_expression(), prop::collection::vec(wrap, 0..=MAX_DEPTH)).prop_map(|(base, wraps)| {
        let room = MAX_DEPTH.saturating_sub(height(&base));
        wraps
            .into_iter()
            .take(room)
            .fold(base, |inner, wrap| match wrap {
                Wrap::Negate => neg(inner),
                Wrap::Left(op, other) => bin(op, inner, other),
                Wrap::Right(op, other) => bin(op, other, inner),
            })
    })
}

proptest! {
    #[test]
    fn a_tree_up_to_the_cap_renders_to_text_that_parses_back(expr in arb_deep_expression()) {
        prop_assert!(height(&expr) <= MAX_DEPTH);
        let text = expr.to_string();
        prop_assert!(!text.contains("--"), "{}", text);
        prop_assert_eq!(Expression::parse(&text).unwrap(), expr);
    }

    #[test]
    fn rendering_then_parsing_gives_back_the_tree(expr in arb_expression()) {
        let text = expr.to_string();
        prop_assert_eq!(Expression::parse(&text).unwrap(), expr);
    }

    #[test]
    fn whitespace_never_changes_the_tree(expr in arb_expression(), pad in "[ \t\n]{0,3}") {
        let text = expr.to_string();
        let spaced: String = text
            .split(' ')
            .collect::<Vec<_>>()
            .join(&format!(" {pad}"));
        prop_assert_eq!(Expression::parse(&format!("{pad}{spaced}{pad}")).unwrap(), expr);
    }

    #[test]
    fn arbitrary_input_never_panics(input in "\\PC{0,64}") {
        if let Ok(expr) = Expression::parse(&input) {
            let canonical = expr.to_string();
            prop_assert_eq!(Expression::parse(&canonical).unwrap().to_string(), canonical);
        }
    }

    #[test]
    fn arithmetic_soup_never_panics(input in "[a-c0-9_. ()+*/-]{0,64}") {
        if let Ok(expr) = Expression::parse(&input) {
            let canonical = expr.to_string();
            prop_assert_eq!(Expression::parse(&canonical).unwrap(), expr);
        }
    }
}
