//! The interactive session.
//!
//! One [`Interpreter`] outlives the whole session, so a function defined on one
//! line is callable on the next. Input that merely ran out — an unclosed block,
//! a half-written function — is not an error: the parser says so via
//! [`Diagnostic::is_caused_by_end_of_input`], and the session asks for another
//! line instead of rejecting what was typed.
//!
//! TODO: GET RID OF THIS FILE

use std::io::{self, BufRead, Write};

use crate::diagnostics::{Diagnostic, render_all};
use crate::runtime::{Interpreter, Value};
use crate::syntax;

const ORIGIN: &str = "<repl>";
const PROMPT: &str = "(lekh_muji) >> ";
const CONTINUATION_PROMPT: &str = "          ... ";

pub struct Repl {
    interpreter: Interpreter,
    /// Lines of a construct that is not finished yet.
    pending_lines: Vec<String>,
}

impl Repl {
    pub fn new() -> Self {
        Self {
            interpreter: Interpreter::new(),
            pending_lines: Vec::new(),
        }
    }

    /// Reads, evaluates and prints until `input` runs out.
    pub fn run(&mut self, input: &mut impl BufRead, output: &mut impl Write) -> io::Result<()> {
        let mut line = String::new();

        loop {
            let prompt = if self.pending_lines.is_empty() {
                PROMPT
            } else {
                CONTINUATION_PROMPT
            };
            write!(output, "{prompt}")?;
            output.flush()?;

            line.clear();
            if input.read_line(&mut line)? == 0 {
                writeln!(output)?;
                return Ok(());
            }

            if let Some(report) = self.consume(&line) {
                write!(output, "{report}")?;
                output.flush()?;
            }
        }
    }

    /// Feeds one line to the session.
    ///
    /// Returns the text to show the user: a value, an error report, or nothing
    /// at all when the line left a construct unfinished.
    pub fn consume(&mut self, line: &str) -> Option<String> {
        self.pending_lines.push(line.to_owned());
        let source = self.pending_lines.join("");

        let program = match syntax::parse(&source) {
            Ok(program) => program,
            Err(diagnostics) => {
                if wants_more_input(&diagnostics) {
                    return None;
                }
                self.pending_lines.clear();
                return Some(render_all(&diagnostics, &source, ORIGIN));
            }
        };
        self.pending_lines.clear();

        match self.interpreter.run(&program) {
            Ok(Value::Null) => None,
            Ok(value) => Some(format!("{value}\n")),
            Err(diagnostic) => Some(diagnostic.render(&source, ORIGIN)),
        }
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new()
    }
}

/// True when the only thing wrong with the input is that there is not enough of
/// it yet. A line with a real mistake in it is reported straight away, even if
/// a later diagnostic also blames the end of the input.
fn wants_more_input(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .all(|diagnostic| diagnostic.is_caused_by_end_of_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_a_complete_line() {
        let mut repl = Repl::new();
        assert_eq!(repl.consume("1 + 1;").as_deref(), Some("2\n"));
    }

    #[test]
    fn remembers_declarations_between_lines() {
        let mut repl = Repl::new();
        repl.consume("thoos_muji x = 5;");
        assert_eq!(repl.consume("x * 2;").as_deref(), Some("10\n"));
    }

    #[test]
    fn waits_for_the_rest_of_a_multi_line_function() {
        let mut repl = Repl::new();

        assert_eq!(
            repl.consume("thoos_muji double = kaam_gar_muji(x) {\n"),
            None
        );
        assert_eq!(repl.consume("    patha_muji x * 2;\n"), None);
        assert_eq!(
            repl.consume("};\n").as_deref(),
            Some("kaam_gar_muji(x) { … }\n")
        );
        assert_eq!(repl.consume("double(21);").as_deref(), Some("42\n"));
    }

    #[test]
    fn reports_a_real_syntax_error_without_waiting() {
        let mut repl = Repl::new();
        let report = repl.consume("thoos_muji = 1;").expect("should report");
        assert!(report.contains("expected an identifier"));
    }

    #[test]
    fn a_reported_error_does_not_poison_the_next_line() {
        let mut repl = Repl::new();
        repl.consume("thoos_muji = 1;");
        assert_eq!(repl.consume("7;").as_deref(), Some("7\n"));
    }

    #[test]
    fn shows_nothing_when_a_statement_has_no_value() {
        let mut repl = Repl::new();
        assert_eq!(repl.consume("yedi_muji (jhut_muji) { 1; }"), None);
    }

    #[test]
    fn shows_jhut_muji_rather_than_treating_it_as_nothing() {
        let mut repl = Repl::new();
        assert_eq!(
            repl.consume("thoos_muji x = jhut_muji;").as_deref(),
            Some("jhut_muji\n")
        );
    }

    #[test]
    fn reports_a_runtime_error_against_the_line_that_caused_it() {
        let mut repl = Repl::new();
        let report = repl.consume("1 / 0;").expect("should report");
        assert!(report.contains("division by zero"));
    }
}
