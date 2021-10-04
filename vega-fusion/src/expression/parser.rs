use crate::error::{Result, ResultWithContext, VegaFusionError};
use crate::expression::ast::array::ArrayExpression;
use crate::expression::ast::base::{Expression, Span};
use crate::expression::ast::binary::{BinaryExpression, BinaryOperator};
use crate::expression::ast::call::{CallExpression, Callee};
use crate::expression::ast::conditional::ConditionalExpression;
use crate::expression::ast::identifier::Identifier;
use crate::expression::ast::literal::{Literal, LiteralValue};
use crate::expression::ast::logical::{LogicalExpression, LogicalOperator};
use crate::expression::ast::member::MemberExpression;
use crate::expression::ast::object::{ObjectExpression, Property};
use crate::expression::ast::unary::{UnaryExpression, UnaryOperator};
use crate::expression::lexer::{tokenize, Token};
use std::convert::TryFrom;

pub fn parse(expr: &str) -> Result<Expression> {
    let mut tokens = tokenize(expr)?;
    let result = perform_parse(&mut tokens, 0.0, expr)?;
    if !tokens.is_empty() {
        let (token, start, _) = &tokens[0];
        return Err(VegaFusionError::parse(&format!(
            "Unexpected token {} at position {} in expression: {}",
            token.to_string(),
            start,
            expr
        )));
    }

    Ok(result)
}

fn perform_parse(
    tokens: &mut Vec<(Token, usize, usize)>,
    min_bp: f64,
    full_expr: &str,
) -> Result<Expression> {
    if tokens.is_empty() {
        return Err(VegaFusionError::parse("Unexpected end of expression"));
    }

    // Pop leading token
    let (lhs_token, start, end) = tokens[0].clone();
    tokens.remove(0);

    // parse form that starts with lhs_token
    let lhs_result = if is_atom(&lhs_token) {
        parse_atom(&lhs_token, start, end)
    } else if let Ok(op) = UnaryOperator::try_from(&lhs_token) {
        // Unary expression
        parse_unary(tokens, op, start, full_expr)
    } else if lhs_token == Token::OpenParen {
        // Arbitrary expression inside parans
        parse_paren_grouping(tokens, full_expr)
    } else if lhs_token == Token::OpenSquare {
        // Array literal expression
        parse_array(tokens, start, full_expr)
    } else if lhs_token == Token::OpenCurly {
        // Object literal expression
        parse_object(tokens, start, full_expr)
    } else {
        Err(VegaFusionError::parse(&format!(
            "Unexpected token: {}",
            lhs_token
        )))
    };

    let mut lhs = lhs_result.with_context(|| {
        format!(
            "Failed to parse form starting at position {} in expression: {}",
            start, full_expr
        )
    })?;

    // pop tokens and add to lhs expression
    while !tokens.is_empty() {
        let (token, start, _) = &tokens[0];
        let start = *start;

        // Check for tokens that always close expressions. If found, break out of while loop
        match token {
            Token::CloseParen
            | Token::CloseCurly
            | Token::CloseSquare
            | Token::Comma
            | Token::Colon => break,
            _ => {}
        }

        let expr_result: Result<Expression> = if let Ok(op) = BinaryOperator::try_from(token) {
            if let Some(new_lhs_result) = parse_binary(tokens, op, &lhs, min_bp, start, full_expr) {
                new_lhs_result
            } else {
                break;
            }
        } else if let Ok(op) = LogicalOperator::try_from(token) {
            if let Some(new_lhs_result) = parse_logical(tokens, op, &lhs, min_bp, start, full_expr)
            {
                new_lhs_result
            } else {
                break;
            }
        } else if token == &Token::OpenParen {
            // Function call (e.g. foo(bar))
            if let Some(new_lhs_result) = parse_call(tokens, &lhs, min_bp, start, full_expr) {
                new_lhs_result
            } else {
                break;
            }
        } else if token == &Token::OpenSquare {
            // computed object/array membership (e.g. foo['bar'])
            if let Some(new_lhs_result) =
                parse_computed_member(tokens, &lhs, min_bp, start, full_expr)
            {
                new_lhs_result
            } else {
                break;
            }
        } else if token == &Token::Dot {
            // static property membership (e.g. foo.bar)
            if let Some(new_lhs_result) =
                parse_static_member(tokens, &lhs, min_bp, start, full_expr)
            {
                new_lhs_result
            } else {
                break;
            }
        } else if token == &Token::Question {
            // ternary operator (e.g. foo ? bar: baz)
            if let Some(new_lhs_result) = parse_ternary(tokens, &lhs, min_bp, start, full_expr) {
                new_lhs_result
            } else {
                break;
            }
        } else {
            Err(VegaFusionError::parse(&format!(
                "Unexpected token '{}'",
                token
            )))
        };

        lhs = expr_result.with_context(|| {
            format!(
                "Failed to parse form starting at position {} in expression: {}",
                start, full_expr
            )
        })?;
    }

    Ok(lhs)
}

pub fn expect_token(
    tokens: &mut Vec<(Token, usize, usize)>,
    expected: Token,
) -> Result<(Token, usize, usize)> {
    if tokens.is_empty() {
        return Err(VegaFusionError::parse(&format!(
            "Expected {}, reached end of expression",
            expected
        )));
    }
    let (token, start, end) = tokens[0].clone();
    if token != expected {
        return Err(VegaFusionError::parse(&format!(
            "Expected {}, received {}",
            expected, token
        )));
    }
    tokens.remove(0);
    Ok((token, start, end))
}

/// Check whether token is an atomic Expression
pub fn is_atom(token: &Token) -> bool {
    matches!(
        token,
        Token::Null
            | Token::Number { .. }
            | Token::Identifier { .. }
            | Token::String { .. }
            | Token::Bool { .. }
    )
}

/// Parse atom token to Expression
pub fn parse_atom(token: &Token, start: usize, end: usize) -> Result<Expression> {
    let span = Some(Span(start, end));

    Ok(match token {
        Token::Null => Expression::from(Literal {
            value: LiteralValue::Null,
            raw: "null".to_string(),
            span,
        }),
        Token::Bool { value, raw } => Expression::from(Literal {
            value: LiteralValue::Boolean(*value),
            raw: raw.clone(),
            span,
        }),
        Token::Number { value, raw } => Expression::from(Literal {
            value: LiteralValue::Number(*value),
            raw: raw.clone(),
            span,
        }),
        Token::String { value, raw } => Expression::from(Literal {
            value: LiteralValue::String(value.clone()),
            raw: raw.clone(),
            span,
        }),
        Token::Identifier { value } => Expression::from(Identifier {
            name: value.clone(),
            span,
        }),
        _ => {
            return Err(VegaFusionError::parse(&format!(
                "Token not an atom: {}",
                token
            )))
        }
    })
}

pub fn parse_unary(
    tokens: &mut Vec<(Token, usize, usize)>,
    op: UnaryOperator,
    start: usize,
    full_expr: &str,
) -> Result<Expression> {
    let unary_bp = op.unary_binding_power();
    let rhs = perform_parse(tokens, unary_bp, full_expr)?;
    let new_span = Span(start, rhs.span().unwrap().1);
    Ok(Expression::from(UnaryExpression {
        operator: op,
        prefix: true,
        argument: Box::new(rhs),
        span: Some(new_span),
    }))
}

pub fn parse_binary(
    tokens: &mut Vec<(Token, usize, usize)>,
    op: BinaryOperator,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    // Infix operator
    let (left_bp, right_bp) = op.infix_binding_power();
    if left_bp < min_bp {
        return None;
    }

    // Commit to processing operator token
    tokens.remove(0);

    Some(match perform_parse(tokens, right_bp, full_expr) {
        Ok(rhs) => {
            // Update lhs
            let new_span = Span(start, rhs.span().unwrap().1);

            Ok(Expression::from(BinaryExpression {
                left: Box::new(lhs.clone()),
                operator: op,
                right: Box::new(rhs),
                span: Some(new_span),
            }))
        }
        Err(err) => Err(err),
    })
}

pub fn parse_logical(
    tokens: &mut Vec<(Token, usize, usize)>,
    op: LogicalOperator,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    // Infix operator
    let (left_bp, right_bp) = op.infix_binding_power();
    if left_bp < min_bp {
        return None;
    }
    // Commit to processing operator token
    tokens.remove(0);

    Some(match perform_parse(tokens, right_bp, full_expr) {
        Ok(rhs) => {
            // Update lhs
            let new_span = Span(start, rhs.span().unwrap().1);

            Ok(Expression::from(LogicalExpression {
                left: Box::new(lhs.clone()),
                operator: op,
                right: Box::new(rhs),
                span: Some(new_span),
            }))
        }
        Err(err) => Err(err),
    })
}

pub fn parse_call(
    tokens: &mut Vec<(Token, usize, usize)>,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    let lhs = match lhs {
        Expression::Identifier(identifier) => identifier,
        _ => {
            return Some(Err(VegaFusionError::parse(
                "Only global functions are callable",
            )))
        }
    };

    // For precedence, see
    // https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Operator_Precedence
    let computed_member_bp = 20.0;
    if min_bp >= computed_member_bp {
        return None;
    }

    // Opening paren
    expect_token(tokens, Token::OpenParen).unwrap();

    // Parse arguments
    let mut arguments: Vec<Expression> = Vec::new();
    while !tokens.is_empty() && tokens[0].0 != Token::CloseParen {
        let parsed_arg = perform_parse(tokens, 1.0, full_expr);
        match parsed_arg {
            Ok(parsed_arg) => {
                arguments.push(parsed_arg);

                // Remove comma token, if any
                expect_token(tokens, Token::Comma).ok();
            }
            Err(err) => return Some(Err(err)),
        }
    }

    // Closing paren
    let (_, _, end) = expect_token(tokens, Token::CloseParen).unwrap();

    // Update span
    let new_span = Span(start, end);

    Some(Ok(Expression::from(CallExpression {
        callee: Callee::Identifier(lhs.clone()),
        arguments,
        span: Some(new_span),
    })))
}

pub fn parse_computed_member(
    tokens: &mut Vec<(Token, usize, usize)>,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    let computed_member_bp = 20.0;
    if min_bp >= computed_member_bp {
        return None;
    }

    // Opening bracket
    expect_token(tokens, Token::OpenSquare).unwrap();

    // Property expression
    Some(match perform_parse(tokens, 1.0, full_expr) {
        Ok(property) => {
            // Closing bracket
            let (_, _, end) = expect_token(tokens, Token::CloseSquare).unwrap();

            // Update span
            let new_span = Span(start, end);

            Ok(Expression::from(MemberExpression::new_computed(
                lhs.clone(),
                property,
                Some(new_span),
            )))
        }
        Err(err) => Err(err),
    })
}

pub fn parse_static_member(
    tokens: &mut Vec<(Token, usize, usize)>,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    let computed_member_bp = 20.0;
    if min_bp >= computed_member_bp {
        return None;
    }

    // Dot
    expect_token(tokens, Token::Dot).unwrap();

    // Property expression
    Some(match perform_parse(tokens, 1000.0, full_expr) {
        Ok(property) => {
            // Update span
            let new_span = Span(start, property.span().unwrap().1);

            match property {
                Expression::Identifier(ident) => Ok(Expression::from(
                    MemberExpression::new_static(lhs.clone(), ident, Some(new_span)),
                )),
                _ => Err(VegaFusionError::parse("Expected identifier")),
            }
        }
        Err(err) => Err(err),
    })
}

pub fn parse_ternary(
    tokens: &mut Vec<(Token, usize, usize)>,
    lhs: &Expression,
    min_bp: f64,
    start: usize,
    full_expr: &str,
) -> Option<Result<Expression>> {
    let (left_bp, middle_bp, right_bp) = ConditionalExpression::ternary_binding_power();
    if min_bp >= left_bp {
        return None;
    }

    // Question mark
    expect_token(tokens, Token::Question).unwrap();

    // Parse consequent
    let consequent = if let Ok(consequent) = perform_parse(tokens, middle_bp, full_expr) {
        consequent
    } else {
        return Some(Err(VegaFusionError::parse(
            "Failed to parse consequent of ternary operator",
        )));
    };

    // Colon
    expect_token(tokens, Token::Colon).unwrap();

    // Parse alternate
    let alternate = if let Ok(alternate) = perform_parse(tokens, right_bp, full_expr) {
        alternate
    } else {
        return Some(Err(VegaFusionError::parse(
            "Failed to parse alternate of ternary operator",
        )));
    };

    // Update span
    let new_span = Span(start, alternate.span().unwrap().1);

    Some(Ok(Expression::from(ConditionalExpression {
        test: Box::new(lhs.clone()),
        consequent: Box::new(consequent),
        alternate: Box::new(alternate),
        span: Some(new_span),
    })))
}

pub fn parse_paren_grouping(
    tokens: &mut Vec<(Token, usize, usize)>,
    full_expr: &str,
) -> Result<Expression> {
    perform_parse(tokens, 0.0, full_expr).and_then(|new_lhs| {
        expect_token(tokens, Token::CloseParen)?;
        Ok(new_lhs)
    })
}

pub fn parse_array(
    tokens: &mut Vec<(Token, usize, usize)>,
    start: usize,
    full_expr: &str,
) -> Result<Expression> {
    let mut elements: Vec<Expression> = Vec::new();

    while !tokens.is_empty() && tokens[0].0 != Token::CloseSquare {
        elements.push(perform_parse(tokens, 1.0, full_expr)?);

        // Remove single comma token, if any
        expect_token(tokens, Token::Comma).ok();
    }

    // Closing bracket
    let (_, _, end) = expect_token(tokens, Token::CloseSquare).unwrap();

    // Update span
    let new_span = Span(start, end);

    Ok(Expression::from(ArrayExpression {
        elements,
        span: Some(new_span),
    }))
}

pub fn parse_object(
    tokens: &mut Vec<(Token, usize, usize)>,
    start: usize,
    full_expr: &str,
) -> Result<Expression> {
    let mut properties: Vec<Property> = Vec::new();

    while !tokens.is_empty() && tokens[0].0 != Token::CloseCurly {
        let key = match perform_parse(tokens, 1.0, full_expr) {
            Ok(key) => key,
            Err(err) => return Err(err.with_context(|| "Failed to parse object key".to_string())),
        };

        expect_token(tokens, Token::Colon)?;

        let value = match perform_parse(tokens, 1.0, full_expr) {
            Ok(key) => key,
            Err(err) => {
                return Err(err.with_context(|| "Failed to parse object property value".to_string()))
            }
        };

        // Remove comma token, if any
        expect_token(tokens, Token::Comma).ok();

        let property = match key {
            Expression::Literal(key) => Property::new_literal(key, value),
            Expression::Identifier(key) => Property::new_identifier(key, value),
            _ => {
                return Err(VegaFusionError::parse(
                    "Object key must be an identifier or a literal value",
                ))
            }
        };

        properties.push(property);
    }

    // Closing bracket
    let (_, _, end) = expect_token(tokens, Token::CloseCurly).unwrap();

    // Update span
    let new_span = Span(start, end);

    Ok(Expression::from(ObjectExpression {
        properties,
        span: Some(new_span),
    }))
}

#[cfg(test)]
mod test_parse {
    use crate::expression::parser::parse;

    #[test]
    fn test_parse_atom() {
        let node = parse("23.500000").unwrap();
        assert_eq!(node.to_string(), "23.5");

        let node = parse("\"hello\"").unwrap();
        assert_eq!(node.to_string(), "\"hello\"");
    }

    #[test]
    fn test_parse_binary() {
        let node = parse("23.50 + foo * 87").unwrap();
        assert_eq!(node.to_string(), "23.5 + foo * 87");
    }

    #[test]
    fn test_parse_logical() {
        let node = parse("false || (foo && bar)").unwrap();
        assert_eq!(node.to_string(), "false || foo && bar");
    }

    #[test]
    fn test_parse_prefix() {
        let node = parse("-23.50 + +foo").unwrap();
        assert_eq!(node.to_string(), "-23.5 + +foo");
    }

    #[test]
    fn test_paren_grouping() {
        let node = parse("-(23.50 + foo)").unwrap();
        assert_eq!(node.to_string(), "-(23.5 + foo)");
    }

    #[test]
    fn test_call() {
        // One arg
        let node = parse("foo(19.0)").unwrap();
        assert_eq!(node.to_string(), "foo(19)");

        // Zero args
        let node = parse("foo()").unwrap();
        assert_eq!(node.to_string(), "foo()");

        // Two args
        let node = parse("foo('a', 21)").unwrap();
        assert_eq!(node.to_string(), "foo(\"a\", 21)");

        // Two args, trailing comma
        let node = parse("foo('a', 21,)").unwrap();
        assert_eq!(node.to_string(), "foo(\"a\", 21)");
    }

    #[test]
    fn test_computed_membership() {
        let node = parse("foo[19.0]").unwrap();
        assert_eq!(node.to_string(), "foo[19]");

        let node = parse("foo['bar']").unwrap();
        assert_eq!(node.to_string(), "foo[\"bar\"]");
    }

    #[test]
    fn test_static_membership() {
        let node = parse("foo.bar").unwrap();
        assert_eq!(node.to_string(), "foo.bar");

        let node = parse("foo.bar[2]").unwrap();
        assert_eq!(node.to_string(), "foo.bar[2]");
    }

    #[test]
    fn test_ternary() {
        let node = parse("foo ? 2 + 3: 27").unwrap();
        assert_eq!(node.to_string(), "foo ? 2 + 3: 27");

        let node = parse("foo ? 2 + 3: 27 || 17").unwrap();
        assert_eq!(node.to_string(), "foo ? 2 + 3: 27 || 17");

        let node = parse("foo ? 2 + 3: (27 || 17)").unwrap();
        assert_eq!(node.to_string(), "foo ? 2 + 3: 27 || 17");

        let node = parse("(foo ? 2 + 3: 27) || 17").unwrap();
        assert_eq!(node.to_string(), "(foo ? 2 + 3: 27) || 17");

        // Check right associativity
        let node = parse("c1 ? v1: c2 ? v2: c3 ? v3: v4").unwrap();
        assert_eq!(node.to_string(), "c1 ? v1: c2 ? v2: c3 ? v3: v4");

        let node = parse("c1 ? v1: (c2 ? v2: (c3 ? v3: v4))").unwrap();
        assert_eq!(node.to_string(), "c1 ? v1: c2 ? v2: c3 ? v3: v4");

        let node = parse("((c1 ? v1: c2) ? v2: c3)? v3: v4").unwrap();
        assert_eq!(node.to_string(), "((c1 ? v1: c2) ? v2: c3) ? v3: v4");
    }

    #[test]
    fn test_array() {
        let node = parse("[19.0]").unwrap();
        assert_eq!(node.to_string(), "[19]");

        let node = parse("['bar', 23]").unwrap();
        assert_eq!(node.to_string(), "[\"bar\", 23]");

        let node = parse("[]").unwrap();
        assert_eq!(node.to_string(), "[]");
    }

    #[test]
    fn test_object() {
        let node = parse("{a: 2, 'b': 2 + 2}").unwrap();
        assert_eq!(node.to_string(), r#"{a: 2, "b": 2 + 2}"#);
    }
}
