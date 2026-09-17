//! Source text in, [`Program`] out.
//!
//! [`parse`] is the only entry point anything outside this module needs.
//! Tokens, operator precedence and the grammar itself stay behind it, which is
//! what lets the lexer and parser be reworked — for error recovery, for source
//! maps, for a different surface syntax — without a ripple anywhere else.

pub mod ast;
mod lexer;
mod parser;
mod token;

pub use ast::{
    AssignmentTarget, BinaryOperator, Block, ConditionalBranch, Expression, ExpressionKind,
    FunctionDefinition, Identifier, Program, Statement, StatementKind, UnaryOperator,
};

use crate::diagnostics::Diagnostic;

/// Parses a complete mulang program.
///
/// The returned diagnostics describe every problem found in one pass: all
/// lexical errors, or — when the source tokenises cleanly — every statement
/// the grammar rejected.
pub fn parse(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let tokens = lexer::tokenize(source)?;
    parser::parse(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses, then prints the result back out as source. Round-tripping is a
    /// compact way to assert on structure: the printer fully parenthesises
    /// operators, so precedence and associativity show up in the text.
    fn reprint(source: &str) -> String {
        parse(source)
            .unwrap_or_else(|diagnostics| panic!("should parse: {}", diagnostics[0].message))
            .to_string()
            .trim_end()
            .to_owned()
    }

    fn first_error(source: &str) -> String {
        parse(source).expect_err("should not parse")[0]
            .message
            .clone()
    }

    #[test]
    fn parses_a_declaration() {
        assert_eq!(reprint("thoos_muji x = 43;"), "thoos_muji x = 43;");
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(reprint("1 + 2 * 3;"), "(1 + (2 * 3));");
    }

    #[test]
    fn comparison_binds_tighter_than_equality() {
        assert_eq!(reprint("a < b == c > d;"), "((a < b) == (c > d));");
    }

    #[test]
    fn grouping_overrides_precedence() {
        assert_eq!(reprint("(1 + 2) * 3;"), "((1 + 2) * 3);");
    }

    #[test]
    fn arithmetic_associates_to_the_left() {
        assert_eq!(reprint("1 - 2 - 3;"), "((1 - 2) - 3);");
    }

    #[test]
    fn assignment_associates_to_the_right() {
        assert_eq!(reprint("a = b = c;"), "(a = (b = c));");
    }

    #[test]
    fn assignment_takes_the_whole_expression_to_its_right() {
        assert_eq!(reprint("a = 1 + 2;"), "(a = (1 + 2));");
    }

    #[test]
    fn calls_chain() {
        assert_eq!(reprint("f(1)(2)(3);"), "f(1)(2)(3);");
    }

    #[test]
    fn prefix_operators_bind_tighter_than_arithmetic() {
        assert_eq!(reprint("-a * b;"), "((-a) * b);");
    }

    /// Arrays and hash maps are gone, so `[`, `]` and `:` are no longer part of
    /// the language at all — they fail in the lexer rather than the parser.
    #[test]
    fn rejects_array_and_hash_map_syntax() {
        assert_eq!(first_error("[1, 2];"), "unrecognised character `[`");
        assert_eq!(first_error("a[0];"), "unrecognised character `[`");
        assert_eq!(first_error("{\"a\": 1};"), "unrecognised character `:`");
    }

    #[test]
    fn parses_a_conditional_chain_without_a_fallback() {
        let source = "yedi_muji (a) { 1; } nabhae_muji (b) { 2; }";
        assert!(reprint(source).contains("nabhae_muji"));
    }

    #[test]
    fn parses_a_function_with_no_parameters() {
        assert_eq!(
            reprint("kaam_gar_muji() { patha_muji 1; };"),
            "kaam_gar_muji() {\n    patha_muji 1;\n};"
        );
    }

    #[test]
    fn parses_both_for_loop_initialiser_forms() {
        assert!(
            reprint("ghuma_muji (thoos_muji i = 0; i < 3; i = i + 1) { i; }")
                .contains("thoos_muji i = 0;")
        );
        assert!(reprint("ghuma_muji (i = 0; i < 3; i = i + 1) { i; }").contains("(i = 0);"));
    }

    #[test]
    fn rejects_assignment_to_a_literal() {
        assert_eq!(first_error("1 = 2;"), "cannot assign to this expression");
    }

    #[test]
    fn requires_a_semicolon_after_a_declaration() {
        assert!(first_error("thoos_muji x = 1").starts_with("expected `;`"));
    }

    #[test]
    fn reports_every_broken_statement_in_one_pass() {
        let diagnostics = parse("thoos_muji = 1; thoos_muji = 2;").expect_err("should not parse");
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn flags_a_truncated_block_as_running_out_of_input() {
        let diagnostics = parse("yedi_muji (a) {").expect_err("should not parse");
        assert!(diagnostics[0].is_caused_by_end_of_input);
    }
}
