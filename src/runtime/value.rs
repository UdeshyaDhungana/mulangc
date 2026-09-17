//! The values a running mulang program manipulates.
//!
//! Every value is copied on assignment. Strings are refcounted but immutable,
//! and a closure shares the scope it captured, so nothing else aliases.

use std::fmt;
use std::rc::Rc;

use crate::runtime::builtins::Builtin;
use crate::runtime::scope::ScopeHandle;
use crate::syntax::FunctionDefinition;

#[derive(Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Text(Rc<str>),
    Function(Rc<Closure>),
    Builtin(&'static Builtin),
    Null,
}

/// A `kaam_gar_muji` expression paired with the scope it was written in, so it
/// can still see those variables whenever and wherever it is later called.
pub struct Closure {
    pub definition: Rc<FunctionDefinition>,
    pub captured_scope: ScopeHandle,
}

impl Value {
    pub fn text(content: impl AsRef<str>) -> Self {
        Value::Text(Rc::from(content.as_ref()))
    }

    /// The name this value's type goes by in an error message.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "integer",
            Value::Float(_) => "decimal",
            Value::Boolean(_) => "boolean",
            Value::Text(_) => "string",
            Value::Function(_) => "function",
            Value::Builtin(_) => "builtin function",
            Value::Null => "khali_muji",
        }
    }

    /// Only `jhut_muji` and `khali_muji` are false. Zero and the empty string
    /// are both true.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Boolean(false) | Value::Null)
    }

    /// Structural equality, with integers and decimals compared by numeric
    /// value so that `1 == 1.0`.
    pub fn is_equal_to(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Boolean(left), Value::Boolean(right)) => left == right,
            (Value::Text(left), Value::Text(right)) => left == right,

            (Value::Function(left), Value::Function(right)) => Rc::ptr_eq(left, right),
            (Value::Builtin(left), Value::Builtin(right)) => std::ptr::eq(*left, *right),

            _ => match numeric_pair(self, other) {
                Some(NumericPair::Integers(left, right)) => left == right,
                Some(NumericPair::Decimals(left, right)) => left == right,
                None => false,
            },
        }
    }
}

/// Two numeric operands promoted to a common representation. Integers stay
/// integers unless one side is a decimal, which keeps integer arithmetic exact.
pub enum NumericPair {
    Integers(i64, i64),
    Decimals(f64, f64),
}

pub fn numeric_pair(left: &Value, right: &Value) -> Option<NumericPair> {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => Some(NumericPair::Integers(*left, *right)),
        (Value::Float(left), Value::Float(right)) => Some(NumericPair::Decimals(*left, *right)),
        (Value::Integer(left), Value::Float(right)) => {
            Some(NumericPair::Decimals(*left as f64, *right))
        }
        (Value::Float(left), Value::Integer(right)) => {
            Some(NumericPair::Decimals(*left, *right as f64))
        }
        _ => None,
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(value) => write!(formatter, "{value}"),
            Value::Float(value) => formatter.write_str(&format_decimal(*value)),
            Value::Boolean(true) => formatter.write_str("sacho_muji"),
            Value::Boolean(false) => formatter.write_str("jhut_muji"),
            Value::Null => formatter.write_str("khali_muji"),
            Value::Text(content) => formatter.write_str(content),

            Value::Function(closure) => {
                let parameters: Vec<&str> = closure
                    .definition
                    .parameters
                    .iter()
                    .map(|parameter| parameter.name.as_str())
                    .collect();
                write!(
                    formatter,
                    "kaam_gar_muji({}) {{ … }}",
                    parameters.join(", ")
                )
            }
            Value::Builtin(builtin) => write!(formatter, "{}(…)", builtin.name),
        }
    }
}

impl fmt::Debug for Value {
    /// Delegates to [`Display`] rather than deriving, which would follow a
    /// closure's captured scope back into the values that hold the closure.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {self}", self.type_name())
    }
}

/// Keeps a decimal visibly a decimal: `1.0` rather than `1`, which would be
/// indistinguishable from an integer.
fn format_decimal(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_false_and_null_are_falsy() {
        assert!(!Value::Boolean(false).is_truthy());
        assert!(!Value::Null.is_truthy());
        assert!(Value::Integer(0).is_truthy());
        assert!(Value::text("").is_truthy());
    }

    #[test]
    fn integers_and_decimals_compare_by_numeric_value() {
        assert!(Value::Integer(1).is_equal_to(&Value::Float(1.0)));
        assert!(!Value::Integer(1).is_equal_to(&Value::Float(1.5)));
    }

    #[test]
    fn strings_compare_by_content() {
        assert!(Value::text("muji").is_equal_to(&Value::text("muji")));
        assert!(!Value::text("muji").is_equal_to(&Value::text("lang")));
    }

    #[test]
    fn values_of_unrelated_types_are_never_equal() {
        assert!(!Value::Integer(1).is_equal_to(&Value::Boolean(true)));
        assert!(!Value::text("1").is_equal_to(&Value::Integer(1)));
    }

    #[test]
    fn decimals_stay_visibly_decimal() {
        assert_eq!(Value::Float(1.0).to_string(), "1.0");
        assert_eq!(Value::Float(69.69).to_string(), "69.69");
    }

    #[test]
    fn strings_print_without_quotes() {
        assert_eq!(Value::text("hi").to_string(), "hi");
    }
}
