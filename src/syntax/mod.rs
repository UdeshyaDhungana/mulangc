pub mod ast;
mod lexer;
mod parser;
pub mod sema;
mod token;

pub use ast::{
    AST, AssignmentTarget, BinaryOperator, Block, Conditional, Else, Expression, ExpressionKind,
    FunctionDefinition, Identifier, Statement, StatementKind, UnaryOperator,
};

use crate::diagnostics::Diagnostic;

pub fn parse(source: &str) -> Result<AST, Vec<Diagnostic>> {
    let tokens = lexer::tokenize(source)?;
    parser::parse(&tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(reprint("thoos x = 43;"), "thoos x = 43;");
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
    fn calls_take_a_name_not_an_expression() {
        assert_eq!(reprint("f(1, 2);"), "f(1, 2);");
        assert_eq!(first_error("f(1)(2);"), "cannot call this expression");
        assert_eq!(first_error("(a + b)(1);"), "cannot call this expression");
    }

    #[test]
    fn prefix_operators_bind_tighter_than_arithmetic() {
        assert_eq!(reprint("-a * b;"), "((-a) * b);");
    }

    #[test]
    fn rejects_array_and_hash_map_syntax() {
        assert_eq!(first_error("[1, 2];"), "unrecognised character `[`");
        assert_eq!(first_error("a[0];"), "unrecognised character `[`");
        assert_eq!(first_error("{a: 1};"), "unrecognised character `:`");
    }

    #[test]
    fn parses_a_conditional_chain_without_a_fallback() {
        assert_eq!(
            reprint("yedi (a) { 1; } natra yedi (b) { 2; }"),
            "yedi (a) {\n    1;\n} natra yedi (b) {\n    2;\n}"
        );
    }

    #[test]
    fn parses_a_conditional_chain_with_a_fallback() {
        assert_eq!(
            reprint("yedi (a) { 1; } natra yedi (b) { 2; } natra { 3; }"),
            "yedi (a) {\n    1;\n} natra yedi (b) {\n    2;\n} natra {\n    3;\n}"
        );
    }

    #[test]
    fn a_natra_yedi_arm_nests_rather_than_flattening() {
        let ast =
            parse("yedi (a) { 1; } natra yedi (b) { 2; } natra { 3; }").expect("should parse");

        let StatementKind::If(outer) = &ast.block.statements[0].kind else {
            panic!("expected an if statement");
        };
        let Some(Else::If(middle)) = outer.otherwise.as_deref() else {
            panic!("expected `natra yedi` to nest a conditional");
        };
        assert!(matches!(middle.otherwise.as_deref(), Some(Else::Block(..))));
    }

    #[test]
    fn rejects_a_bare_statement_after_natra() {
        assert!(first_error("yedi (a) { 1; } natra bol(2);").starts_with("expected `{`"));
    }

    #[test]
    fn parses_a_function_with_no_parameters() {
        assert_eq!(
            reprint("dhanda one() { de 1; }"),
            "dhanda one() {\n    de 1;\n}"
        );
    }

    #[test]
    fn parses_a_function_with_parameters() {
        assert_eq!(
            reprint("dhanda add(a, b) { de a + b; }"),
            "dhanda add(a, b) {\n    de (a + b);\n}"
        );
    }

    #[test]
    fn a_function_definition_needs_a_name() {
        assert_eq!(
            first_error("dhanda() { de 1; }"),
            "expected an identifier, found `(`"
        );
    }

    #[test]
    fn a_function_is_no_longer_an_expression() {
        assert_eq!(
            first_error("thoos f = dhanda(x) { de x; };"),
            "expected an expression, found `dhanda`"
        );
    }

    #[test]
    fn parses_both_for_loop_initialiser_forms() {
        assert_eq!(
            reprint("fanfani (thoos i = 0; i < 3; i = i + 1) { i; }"),
            "fanfani (thoos i = 0; (i < 3); (i = (i + 1))) {\n    i;\n}"
        );
        assert_eq!(
            reprint("fanfani (i = 0; i < 3; i = i + 1) { i; }"),
            "fanfani ((i = 0); (i < 3); (i = (i + 1))) {\n    i;\n}"
        );
    }

    #[test]
    fn a_conditional_is_never_the_callee_of_a_call() {
        let ast = parse("yedi (a) { 1; } (2);").expect("should parse");

        assert_eq!(ast.block.statements.len(), 2);
        assert!(matches!(
            ast.block.statements[0].kind,
            StatementKind::If(..)
        ));
        assert!(matches!(
            ast.block.statements[1].kind,
            StatementKind::Expression(..)
        ));
    }

    #[test]
    fn rejects_a_loop_where_an_expression_is_expected() {
        assert_eq!(
            first_error("thoos x = jabasamma (a) {};"),
            "expected an expression, found `jabasamma`"
        );
        assert_eq!(
            first_error("jabasamma (a) { 1; } + 1;"),
            "expected an expression, found `+`"
        );
    }

    #[test]
    fn rejects_assignment_to_a_literal() {
        assert_eq!(first_error("1 = 2;"), "cannot assign to this expression");
    }

    #[test]
    fn requires_a_semicolon_after_a_declaration() {
        assert!(first_error("thoos x = 1").starts_with("expected `;`"));
    }

    #[test]
    fn reports_every_broken_statement_in_one_pass() {
        let diagnostics = parse("thoos = 1; thoos = 2;").expect_err("should not parse");
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn flags_a_truncated_block_as_running_out_of_input() {
        let diagnostics = parse("yedi (a) {").expect_err("should not parse");
        assert!(diagnostics[0].is_caused_by_end_of_input);
    }
}
