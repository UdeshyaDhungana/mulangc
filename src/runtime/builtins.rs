//! The functions every mulang program can call without defining them.
//!
//! [`Builtin::call`] checks arity against the declared range before dispatching,
//! so no individual builtin counts its own arguments or writes its own "wrong
//! number of arguments" message.

use std::io::Write;

use crate::diagnostics::{Diagnostic, Span};
use crate::runtime::value::Value;

/// What a builtin is allowed to reach outside itself: nothing but the stream
/// the program's output goes to. Routing printing through here rather than
/// straight to stdout is what makes program output testable.
pub struct BuiltinContext<'output> {
    pub output: &'output mut dyn Write,
}

pub struct Builtin {
    pub name: &'static str,
    minimum_arguments: usize,
    /// `None` when the builtin takes any number of arguments.
    maximum_arguments: Option<usize>,
    apply: fn(&mut BuiltinContext<'_>, Vec<Value>, Span) -> Result<Value, Diagnostic>,
}

impl Builtin {
    pub fn call(
        &self,
        context: &mut BuiltinContext<'_>,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.check_arity(arguments.len(), span)?;
        (self.apply)(context, arguments, span)
    }

    fn check_arity(&self, supplied: usize, span: Span) -> Result<(), Diagnostic> {
        let is_within_range = supplied >= self.minimum_arguments
            && self
                .maximum_arguments
                .is_none_or(|maximum| supplied <= maximum);
        if is_within_range {
            return Ok(());
        }

        let expected = match self.maximum_arguments {
            Some(maximum) if maximum == self.minimum_arguments => {
                format!("{}", self.minimum_arguments)
            }
            Some(maximum) => format!("{} or {maximum}", self.minimum_arguments),
            None => format!("at least {}", self.minimum_arguments),
        };

        Err(Diagnostic::error(
            span,
            format!(
                "`{}` takes {expected} argument(s), but was given {supplied}",
                self.name
            ),
        ))
    }
}

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.name == name)
}

static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "lambai_muji",
        minimum_arguments: 1,
        maximum_arguments: Some(1),
        apply: |_, arguments, span| match &arguments[0] {
            Value::Text(content) => Ok(Value::Integer(content.chars().count() as i64)),
            Value::Array(elements) => Ok(Value::Integer(elements.borrow().len() as i64)),
            Value::HashMap(entries) => Ok(Value::Integer(entries.borrow().len() as i64)),
            other => Err(Diagnostic::error(
                span,
                format!(
                    "`lambai_muji` needs a string, an array or a hashmap, not {}",
                    other.type_name()
                ),
            )),
        },
    },
    Builtin {
        name: "khaad_muji",
        minimum_arguments: 2,
        maximum_arguments: Some(2),
        apply: |_, mut arguments, span| {
            let element = arguments.pop().expect("arity checked");
            let Value::Array(elements) = arguments.pop().expect("arity checked") else {
                return Err(Diagnostic::error(span, "`khaad_muji` appends to an array"));
            };
            elements.borrow_mut().push(element);
            Ok(Value::Null)
        },
    },
    Builtin {
        name: "udaa_muji",
        minimum_arguments: 1,
        maximum_arguments: Some(2),
        apply: |_, arguments, span| {
            let Value::Array(elements) = &arguments[0] else {
                return Err(Diagnostic::error(span, "`udaa_muji` removes from an array"));
            };
            let mut elements = elements.borrow_mut();

            if elements.is_empty() {
                return Err(Diagnostic::error(
                    span,
                    "`udaa_muji` cannot remove from an empty array",
                ));
            }

            // With no index given, remove the last element.
            let position = match arguments.get(1) {
                None => elements.len() - 1,
                Some(Value::Integer(index)) => usize::try_from(*index)
                    .ok()
                    .filter(|index| *index < elements.len())
                    .ok_or_else(|| {
                        Diagnostic::error(
                            span,
                            format!(
                                "index {index} is outside this array of {} element(s)",
                                elements.len()
                            ),
                        )
                    })?,
                Some(other) => {
                    return Err(Diagnostic::error(
                        span,
                        format!(
                            "`udaa_muji` needs an integer index, not {}",
                            other.type_name()
                        ),
                    ));
                }
            };

            Ok(elements.remove(position))
        },
    },
    Builtin {
        name: "bhan_muji",
        minimum_arguments: 0,
        maximum_arguments: None,
        apply: |context, arguments, span| {
            let line = arguments
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("");

            writeln!(context.output, "{line}").map_err(|error| {
                Diagnostic::error(span, format!("could not write output: {error}"))
            })?;
            Ok(Value::Null)
        },
    },
    Builtin {
        name: "abs",
        minimum_arguments: 1,
        maximum_arguments: Some(1),
        apply: |_, arguments, span| match &arguments[0] {
            Value::Integer(value) => value.checked_abs().map(Value::Integer).ok_or_else(|| {
                Diagnostic::error(
                    span,
                    format!("the absolute value of {value} overflows 64 bits"),
                )
            }),
            Value::Float(value) => Ok(Value::Float(value.abs())),
            other => Err(Diagnostic::error(
                span,
                format!("`abs` needs a number, not {}", other.type_name()),
            )),
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const ANYWHERE: Span = Span::new(0, 0);

    fn call(name: &str, arguments: Vec<Value>) -> Result<Value, Diagnostic> {
        let mut discarded = Vec::new();
        let mut context = BuiltinContext {
            output: &mut discarded,
        };
        lookup(name)
            .expect("builtin should exist")
            .call(&mut context, arguments, ANYWHERE)
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        let result = call("lambai_muji", vec![Value::text("नमस्कार")]);
        assert!(matches!(result, Ok(Value::Integer(7))));
    }

    #[test]
    fn appending_is_visible_through_every_handle_to_the_array() {
        let array = Value::array(vec![Value::Integer(1)]);
        call("khaad_muji", vec![array.clone(), Value::Integer(2)]).expect("should append");
        assert_eq!(array.to_string(), "[1, 2]");
    }

    #[test]
    fn removing_without_an_index_takes_the_last_element() {
        let array = Value::array(vec![Value::Integer(1), Value::Integer(2)]);
        let removed = call("udaa_muji", vec![array.clone()]).expect("should remove");

        assert!(matches!(removed, Value::Integer(2)));
        assert_eq!(array.to_string(), "[1]");
    }

    #[test]
    fn removing_from_an_empty_array_is_an_error_not_a_crash() {
        let error = call("udaa_muji", vec![Value::array(vec![])]).expect_err("should fail");
        assert!(error.message.contains("empty array"));
    }

    #[test]
    fn removing_past_the_end_is_an_error_not_a_crash() {
        let array = Value::array(vec![Value::Integer(1)]);
        let error = call("udaa_muji", vec![array, Value::Integer(9)]).expect_err("should fail");
        assert!(error.message.contains("outside this array"));
    }

    #[test]
    fn removing_at_a_negative_index_is_an_error() {
        let array = Value::array(vec![Value::Integer(1)]);
        let error = call("udaa_muji", vec![array, Value::Integer(-1)]).expect_err("should fail");
        assert!(error.message.contains("outside this array"));
    }

    #[test]
    fn arity_is_reported_before_the_builtin_runs() {
        let error = call("abs", vec![]).expect_err("should fail");
        assert_eq!(error.message, "`abs` takes 1 argument(s), but was given 0");
    }

    #[test]
    fn a_builtin_with_a_range_reports_both_ends() {
        let error = call("udaa_muji", vec![]).expect_err("should fail");
        assert!(error.message.contains("takes 1 or 2 argument(s)"));
    }

    #[test]
    fn printing_writes_a_line_to_the_context() {
        let mut printed = Vec::new();
        let mut context = BuiltinContext {
            output: &mut printed,
        };
        lookup("bhan_muji")
            .expect("builtin should exist")
            .call(
                &mut context,
                vec![Value::text("Hello, "), Value::text("world!")],
                ANYWHERE,
            )
            .expect("should print");

        assert_eq!(
            String::from_utf8(printed).expect("utf-8"),
            "Hello, world!\n"
        );
    }
}
