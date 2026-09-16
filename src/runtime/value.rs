//! The values a running mulang program manipulates.
//!
//! Arrays and hash maps are shared, mutable handles: passing one to a function
//! and mutating it there is visible to the caller, which is what makes
//! `bubbleSort(myArray)` sort the caller's array. Numbers, booleans and strings
//! are copied, so nothing else aliases.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::runtime::builtins::Builtin;
use crate::runtime::scope::ScopeHandle;
use crate::syntax::FunctionDefinition;

pub type ArrayHandle = Rc<RefCell<Vec<Value>>>;
/// Ordered by key so that printing a hash map is deterministic.
pub type HashMapHandle = Rc<RefCell<BTreeMap<String, Value>>>;

#[derive(Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Text(Rc<str>),
    Array(ArrayHandle),
    HashMap(HashMapHandle),
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
    pub fn array(elements: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(elements)))
    }

    pub fn hash_map(entries: BTreeMap<String, Value>) -> Self {
        Value::HashMap(Rc::new(RefCell::new(entries)))
    }

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
            Value::Array(_) => "array",
            Value::HashMap(_) => "hashmap",
            Value::Function(_) => "function",
            Value::Builtin(_) => "builtin function",
            Value::Null => "khali_muji",
        }
    }

    /// Only `jhut_muji` and `khali_muji` are false. Zero, the empty string and
    /// the empty array are all true.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Boolean(false) | Value::Null)
    }

    /// Structural equality, with integers and decimals compared by numeric
    /// value so that `1 == 1.0`. Two arrays or hash maps are equal when their
    /// contents are, element by element.
    pub fn is_equal_to(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Boolean(left), Value::Boolean(right)) => left == right,
            (Value::Text(left), Value::Text(right)) => left == right,

            (Value::Array(left), Value::Array(right)) => {
                if Rc::ptr_eq(left, right) {
                    return true;
                }
                let (left, right) = (left.borrow(), right.borrow());
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left, right)| left.is_equal_to(right))
            }
            (Value::HashMap(left), Value::HashMap(right)) => {
                if Rc::ptr_eq(left, right) {
                    return true;
                }
                let (left, right) = (left.borrow(), right.borrow());
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right.iter())
                        .all(|(left, right)| left.0 == right.0 && left.1.is_equal_to(right.1))
            }

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

/// How far printing will descend into nested collections.
///
/// An array can hold itself — `a[0] = a` — and following that would never
/// finish. Rather than track which handles have already been visited, printing
/// stops at a depth no realistic value reaches and writes `…` instead.
const MAXIMUM_PRINTED_DEPTH: usize = 32;

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_value(formatter, self, Nesting::Outermost, 0)
    }
}

impl fmt::Debug for Value {
    /// Delegates to [`Display`] rather than deriving, which would follow a
    /// closure's captured scope back into the values that hold the closure.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {self}", self.type_name())
    }
}

/// Whether a value is being printed on its own or as part of a collection.
/// A string shows its quotes only inside one — otherwise `["a, b"]` and
/// `["a", "b"]` would print identically — while a bare `bhan_muji("hi")` still
/// prints `hi`, which is what a print statement should do.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nesting {
    Outermost,
    InsideCollection,
}

fn write_value(
    formatter: &mut fmt::Formatter<'_>,
    value: &Value,
    nesting: Nesting,
    depth: usize,
) -> fmt::Result {
    match value {
        Value::Integer(value) => write!(formatter, "{value}"),
        Value::Float(value) => formatter.write_str(&format_decimal(*value)),
        Value::Boolean(true) => formatter.write_str("sacho_muji"),
        Value::Boolean(false) => formatter.write_str("jhut_muji"),
        Value::Null => formatter.write_str("khali_muji"),

        Value::Text(content) => match nesting {
            Nesting::Outermost => formatter.write_str(content),
            Nesting::InsideCollection => write!(formatter, "\"{content}\""),
        },

        Value::Array(elements) if depth < MAXIMUM_PRINTED_DEPTH => {
            formatter.write_str("[")?;
            for (position, element) in elements.borrow().iter().enumerate() {
                if position > 0 {
                    formatter.write_str(", ")?;
                }
                write_value(formatter, element, Nesting::InsideCollection, depth + 1)?;
            }
            formatter.write_str("]")
        }
        Value::HashMap(entries) if depth < MAXIMUM_PRINTED_DEPTH => {
            formatter.write_str("{")?;
            for (position, (key, value)) in entries.borrow().iter().enumerate() {
                if position > 0 {
                    formatter.write_str(", ")?;
                }
                write!(formatter, "\"{key}\": ")?;
                write_value(formatter, value, Nesting::InsideCollection, depth + 1)?;
            }
            formatter.write_str("}")
        }
        Value::Array(_) => formatter.write_str("[…]"),
        Value::HashMap(_) => formatter.write_str("{…}"),

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
        assert!(Value::array(vec![]).is_truthy());
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
    fn arrays_compare_element_by_element() {
        let left = Value::array(vec![Value::Integer(1), Value::text("a")]);
        let right = Value::array(vec![Value::Integer(1), Value::text("a")]);
        assert!(left.is_equal_to(&right));
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
    fn strings_are_quoted_only_inside_a_collection() {
        assert_eq!(Value::text("hi").to_string(), "hi");
        assert_eq!(
            Value::array(vec![Value::text("hi")]).to_string(),
            "[\"hi\"]"
        );
    }

    #[test]
    fn printing_an_array_that_contains_itself_terminates() {
        let array = Value::array(vec![Value::Null]);
        let Value::Array(elements) = &array else {
            unreachable!("just built an array")
        };
        elements.borrow_mut()[0] = array.clone();

        let printed = array.to_string();
        assert!(printed.contains('…'), "{printed}");
        assert!(printed.chars().count() < 100, "{printed}");
    }
}
