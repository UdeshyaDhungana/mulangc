//! What mulang's operators mean.
//!
//! [`apply_unary`] and [`apply_binary`] are the only way to get at any of it.
//! Type promotion, the overloads of `+`, and every arithmetic edge case that
//! must be an error rather than a crash — division by zero, overflow — are
//! settled here rather than in the interpreter's walk over the tree.

use crate::diagnostics::{Diagnostic, Span};
use crate::runtime::value::{NumericPair, Value, numeric_pair};
use crate::syntax::{BinaryOperator, UnaryOperator};

pub fn apply_unary(
    operator: UnaryOperator,
    operand: &Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match (operator, operand) {
        (UnaryOperator::Not, value) => Ok(Value::Boolean(!value.is_truthy())),

        (UnaryOperator::Negate, Value::Integer(value)) => value
            .checked_neg()
            .map(Value::Integer)
            .ok_or_else(|| Diagnostic::error(span, "negating this integer overflows 64 bits")),
        (UnaryOperator::Negate, Value::Float(value)) => Ok(Value::Float(-value)),
        (UnaryOperator::Negate, other) => Err(Diagnostic::error(
            span,
            format!("cannot negate {}", other.type_name()),
        )),
    }
}

pub fn apply_binary(
    operator: BinaryOperator,
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match operator {
        BinaryOperator::Equal => Ok(Value::Boolean(left.is_equal_to(right))),
        BinaryOperator::NotEqual => Ok(Value::Boolean(!left.is_equal_to(right))),

        BinaryOperator::Less
        | BinaryOperator::LessOrEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterOrEqual => compare(operator, left, right, span),

        BinaryOperator::Add => match (left, right) {
            (Value::Text(left), Value::Text(right)) => Ok(Value::text(format!("{left}{right}"))),
            _ => arithmetic(operator, left, right, span),
        },

        BinaryOperator::Subtract
        | BinaryOperator::Multiply
        | BinaryOperator::Divide
        | BinaryOperator::Remainder => arithmetic(operator, left, right, span),
    }
}

fn compare(
    operator: BinaryOperator,
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let ordering = match numeric_pair(left, right) {
        Some(NumericPair::Integers(left, right)) => left.partial_cmp(&right),
        Some(NumericPair::Decimals(left, right)) => left.partial_cmp(&right),
        None => return Err(unsupported_operands(operator, left, right, span)),
    };

    // `partial_cmp` on decimals yields nothing when either side is NaN, and a
    // NaN compares false against everything, including itself.
    let Some(ordering) = ordering else {
        return Ok(Value::Boolean(false));
    };

    Ok(Value::Boolean(match operator {
        BinaryOperator::Less => ordering.is_lt(),
        BinaryOperator::LessOrEqual => ordering.is_le(),
        BinaryOperator::Greater => ordering.is_gt(),
        BinaryOperator::GreaterOrEqual => ordering.is_ge(),
        _ => unreachable!("compare is only reached for the four ordering operators"),
    }))
}

fn arithmetic(
    operator: BinaryOperator,
    left: &Value,
    right: &Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match numeric_pair(left, right) {
        Some(NumericPair::Integers(left, right)) => integer_arithmetic(operator, left, right, span),
        Some(NumericPair::Decimals(left, right)) => decimal_arithmetic(operator, left, right, span),
        None => Err(unsupported_operands(operator, left, right, span)),
    }
}

/// Integer arithmetic is exact: a result that will not fit is an error rather
/// than a silently wrapped number.
fn integer_arithmetic(
    operator: BinaryOperator,
    left: i64,
    right: i64,
    span: Span,
) -> Result<Value, Diagnostic> {
    if matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder) && right == 0 {
        return Err(Diagnostic::error(span, "division by zero"));
    }

    let result = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => left.checked_mul(right),
        BinaryOperator::Divide => left.checked_div(right),
        BinaryOperator::Remainder => left.checked_rem(right),
        _ => unreachable!("arithmetic is only reached for the five arithmetic operators"),
    };

    result.map(Value::Integer).ok_or_else(|| {
        Diagnostic::error(
            span,
            format!("`{left} {operator} {right}` overflows 64 bits"),
        )
    })
}

fn decimal_arithmetic(
    operator: BinaryOperator,
    left: f64,
    right: f64,
    span: Span,
) -> Result<Value, Diagnostic> {
    Ok(Value::Float(match operator {
        BinaryOperator::Add => left + right,
        BinaryOperator::Subtract => left - right,
        BinaryOperator::Multiply => left * right,
        BinaryOperator::Divide => left / right,
        BinaryOperator::Remainder => {
            return Err(Diagnostic::error(span, "`%` needs two integers")
                .with_help(format!("one of `{left}` and `{right}` is a decimal")));
        }
        _ => unreachable!("arithmetic is only reached for the five arithmetic operators"),
    }))
}

fn unsupported_operands(
    operator: BinaryOperator,
    left: &Value,
    right: &Value,
    span: Span,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(
        span,
        format!(
            "cannot apply `{operator}` to {} and {}",
            left.type_name(),
            right.type_name()
        ),
    );

    match (operator, left, right) {
        (BinaryOperator::Add, Value::Text(_), _) | (BinaryOperator::Add, _, Value::Text(_)) => {
            diagnostic.with_help("`+` joins two strings; convert the other side first")
        }
        _ => diagnostic,
    }
}

/// Kept out of the public interface but useful to construct in tests.
#[cfg(test)]
const ANYWHERE: Span = Span::new(0, 0);

#[cfg(test)]
mod tests {
    use super::*;

    fn binary(operator: BinaryOperator, left: Value, right: Value) -> Result<Value, Diagnostic> {
        apply_binary(operator, &left, &right, ANYWHERE)
    }

    #[test]
    fn integer_arithmetic_stays_integral() {
        let result = binary(BinaryOperator::Add, Value::Integer(2), Value::Integer(3));
        assert!(matches!(result, Ok(Value::Integer(5))));
    }

    #[test]
    fn mixing_an_integer_and_a_decimal_promotes_to_decimal() {
        let result = binary(
            BinaryOperator::Multiply,
            Value::Integer(2),
            Value::Float(1.5),
        );
        assert!(matches!(result, Ok(Value::Float(value)) if value == 3.0));
    }

    #[test]
    fn integer_division_by_zero_is_an_error_not_a_crash() {
        let error = binary(BinaryOperator::Divide, Value::Integer(1), Value::Integer(0))
            .expect_err("should fail");
        assert_eq!(error.message, "division by zero");
    }

    #[test]
    fn integer_overflow_is_an_error_not_a_wrap() {
        let error = binary(
            BinaryOperator::Add,
            Value::Integer(i64::MAX),
            Value::Integer(1),
        )
        .expect_err("should fail");
        assert!(error.message.contains("overflows"));
    }

    #[test]
    fn plus_joins_strings() {
        let joined =
            binary(BinaryOperator::Add, Value::text("mu"), Value::text("ji")).expect("should join");
        assert_eq!(joined.to_string(), "muji");
    }

    #[test]
    fn ordering_operators_compare_across_numeric_types() {
        let result = binary(
            BinaryOperator::LessOrEqual,
            Value::Integer(1),
            Value::Float(1.0),
        );
        assert!(matches!(result, Ok(Value::Boolean(true))));
    }

    #[test]
    fn ordering_a_string_against_a_number_is_an_error() {
        let error = binary(BinaryOperator::Less, Value::text("a"), Value::Integer(1))
            .expect_err("should fail");
        assert!(error.message.contains("cannot apply `<`"));
    }

    #[test]
    fn remainder_rejects_decimals() {
        let error = binary(
            BinaryOperator::Remainder,
            Value::Float(5.0),
            Value::Integer(2),
        )
        .expect_err("should fail");
        assert_eq!(error.message, "`%` needs two integers");
    }

    #[test]
    fn not_is_defined_for_every_value() {
        let result = apply_unary(UnaryOperator::Not, &Value::Integer(0), ANYWHERE);
        assert!(matches!(result, Ok(Value::Boolean(false))));
    }

    #[test]
    fn negating_a_string_is_an_error() {
        let error = apply_unary(UnaryOperator::Negate, &Value::text("a"), ANYWHERE)
            .expect_err("should fail");
        assert_eq!(error.message, "cannot negate string");
    }
}
