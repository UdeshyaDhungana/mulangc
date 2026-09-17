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
    fn arity_is_reported_before_the_builtin_runs() {
        let error = call("abs", vec![]).expect_err("should fail");
        assert_eq!(error.message, "`abs` takes 1 argument(s), but was given 0");
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
