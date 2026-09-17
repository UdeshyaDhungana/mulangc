//! Source locations and the error report they turn into.
//!
//! Every failure in the toolchain — a stray character, a missing brace, a
//! division by zero at run time — is reported as a [`Diagnostic`]: a message
//! plus the [`Span`] of source text it blames. Callers only ever hand a
//! diagnostic back the original source text and get a finished report;
//! resolving byte offsets to line/column, extracting the offending line and
//! drawing the underline all stay in here.

use std::fmt::Write as _;

/// A half-open range of byte offsets into the source text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both operands, used to give a composite node
    /// (a binary expression, a block) the extent of everything it contains.
    pub fn joined_with(self, other: Span) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
    /// Set when the input simply ran out mid-construct. The REPL reads this to
    /// decide between asking for a continuation line and reporting an error.
    pub is_caused_by_end_of_input: bool,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            help: None,
            is_caused_by_end_of_input: false,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn caused_by_end_of_input(mut self) -> Self {
        self.is_caused_by_end_of_input = true;
        self
    }

    /// Renders the diagnostic against the source it was produced from.
    ///
    /// `origin` labels where the source came from — a file path, or something
    /// like `<stdin>` for text that was never on disk.
    pub fn render(&self, source: &str, origin: &str) -> String {
        let location = SourceLocation::of(source, self.span.start);
        let gutter_width = location.line_number.to_string().len();
        let blank_gutter = " ".repeat(gutter_width);

        let mut report = String::new();
        let _ = writeln!(report, "error: {}", self.message);
        let _ = writeln!(
            report,
            "{blank_gutter}--> {origin}:{}:{}",
            location.line_number, location.column_number
        );
        let _ = writeln!(report, "{blank_gutter} |");
        let _ = writeln!(
            report,
            "{:>gutter_width$} | {}",
            location.line_number,
            location.text.replace('\t', "    ")
        );
        let _ = writeln!(
            report,
            "{blank_gutter} | {}{}",
            " ".repeat(location.column_number - 1),
            "^".repeat(self.underline_width(source, &location))
        );
        if let Some(help) = &self.help {
            let _ = writeln!(report, "{blank_gutter} = help: {help}");
        }
        report
    }

    /// How many carets to draw, in characters. A span that runs past the end of
    /// its first line is trimmed there so the underline stays under the text it
    /// was drawn beneath.
    fn underline_width(&self, source: &str, location: &SourceLocation<'_>) -> usize {
        let visible_end = self
            .span
            .end
            .min(location.line_end_offset)
            .max(self.span.start);
        source[self.span.start..visible_end].chars().count().max(1)
    }
}

/// Renders every diagnostic in order, separated by a blank line.
pub fn render_all(diagnostics: &[Diagnostic], source: &str, origin: &str) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render(source, origin))
        .collect::<Vec<_>>()
        .join("\n")
}

struct SourceLocation<'source> {
    line_number: usize,
    column_number: usize,
    text: &'source str,
    line_end_offset: usize,
}

impl<'source> SourceLocation<'source> {
    fn of(source: &'source str, offset: usize) -> Self {
        // A span pointing at end-of-input sits one past the last byte.
        let offset = offset.min(source.len());
        let line_start_offset = source[..offset].rfind('\n').map_or(0, |index| index + 1);
        let line_end_offset = source[offset..]
            .find('\n')
            .map_or(source.len(), |index| offset + index);

        Self {
            line_number: source[..line_start_offset].matches('\n').count() + 1,
            column_number: source[line_start_offset..offset].chars().count() + 1,
            text: &source[line_start_offset..line_end_offset],
            line_end_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_an_offset_on_the_third_line() {
        let source = "one\ntwo\nthree\n";
        let location = SourceLocation::of(source, 10);

        assert_eq!(location.line_number, 3);
        assert_eq!(location.column_number, 3);
        assert_eq!(location.text, "three");
    }

    #[test]
    fn locates_end_of_input() {
        let source = "abc";
        let location = SourceLocation::of(source, source.len());

        assert_eq!(location.line_number, 1);
        assert_eq!(location.column_number, 4);
    }

    #[test]
    fn counts_columns_in_characters_not_bytes() {
        let source = "थोस् x";
        let location = SourceLocation::of(source, source.find('x').unwrap());

        assert_eq!(location.column_number, 6);
    }

    #[test]
    fn underlines_only_the_first_line_of_a_multi_line_span() {
        let source = "abc\ndef";
        let diagnostic = Diagnostic::error(Span::new(0, source.len()), "bad");

        assert!(diagnostic.render(source, "<test>").contains("^^^\n"));
    }
}
