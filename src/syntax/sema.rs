// Semantic analyzer
use std::{collections::HashMap, ops::Deref};

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AssignmentTarget, Block, Conditional, Else, Expression, ExpressionKind, FunctionDefinition,
        Identifier, Program, Statement, StatementKind,
    },
};

type ArgumentSize = usize;

// I have a hunch that I'll change this to include types, etc.
// after we incorporate them
enum SymbolInfo {
    Function(ArgumentSize),
    Variable,
}

#[derive(Debug, PartialEq)]
enum ScopeType {
    Global,
    Function,
    Block,
}

struct Scope {
    symbol_table: HashMap<String, SymbolInfo>,
    scope_type: ScopeType,
}

impl Scope {
    fn new(scope_type: ScopeType) -> Self {
        Self {
            symbol_table: HashMap::new(),
            scope_type,
        }
    }

    fn add_variable_identifier(&mut self, identifier: &Identifier) {
        self.symbol_table
            .insert(identifier.name.clone(), SymbolInfo::Variable);
    }

    fn add_function_identifier(&mut self, identifier: &Identifier, num_of_arguments: usize) {
        self.symbol_table.insert(
            identifier.name.clone(),
            SymbolInfo::Function(num_of_arguments),
        );
    }

    fn contains_identifier(&self, identifier_name: String) -> bool {
        self.symbol_table.contains_key(&identifier_name)
    }
}

struct PendingFunctionDefinition {
    name: String,
    arity: usize,
}

pub struct Sema {
    // Stack of current scopes
    scopes_stack: Vec<Scope>,
    main_function_found: bool,
    lookahead_function_definitions: Vec<PendingFunctionDefinition>,
}

pub struct Resolution {}

impl Default for Sema {
    fn default() -> Self {
        Self::new()
    }
}

impl Sema {
    pub fn new() -> Self {
        Self {
            scopes_stack: Vec::new(),
            main_function_found: false,
            lookahead_function_definitions: Vec::new(),
        }
    }

    fn get_enclosing_functional_scope(&mut self) -> Option<&mut Scope> {
        for scope in self.scopes_stack.iter_mut().rev() {
            if scope.scope_type == ScopeType::Function {
                return Some(scope);
            }
        }
        None
    }

    fn get_current_scope(&mut self) -> &mut Scope {
        self.scopes_stack
            .last_mut()
            .expect("At least one scope must exist during semantic analysis")
    }

    fn is_identifier_valid(&self, identifier_name: String) -> bool {
        // I'm walking this up the stack and not down
        // just for the fun of it and i feel like this shouldn't be a problem
        // Let's see if this results in anything unexpected later
        for stack in self.scopes_stack.iter() {
            if stack.contains_identifier(identifier_name.clone()) {
                return true;
            }
        }
        false
    }

    fn get_symbol_info(&self, function_identifier_name: String) -> Option<&SymbolInfo> {
        for stack in self.scopes_stack.iter().rev() {
            if let Some(symbol_info) = stack.symbol_table.get(&function_identifier_name.clone()) {
                return Some(symbol_info);
            } else {
                continue;
            }
        }
        None
    }

    fn push_new_scope(&mut self, scope: Scope) {
        self.scopes_stack.push(scope);
    }

    fn pop_scope(&mut self) {
        self.scopes_stack.pop();
    }

    pub fn analyse(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.scopes_stack.push(Scope::new(ScopeType::Global));

        // DEBUG: Just add `bol` builtin so analyzer won't complain
        self.get_current_scope().add_function_identifier(
            &Identifier {
                name: String::from("bol"),
                id: None,
                span: Span::new(0, 0),
            },
            1,
        );

        for statement in &program.block.statements {
            // Only allow function and variable definitions in the outer scope
            // Process functions signatures first, so functions can be used before their definitions are set in
            match &statement.kind {
                StatementKind::Function(FunctionDefinition {
                    identifier,
                    parameters,
                    ..
                }) => {
                    let function_name = identifier.name.clone();

                    if self.lookahead_function_definitions.iter().any(
                        |pending_function_definition| {
                            pending_function_definition.name == identifier.name
                        },
                    ) {
                        return Err(Diagnostic::error(
                            identifier.span,
                            format!("SEMA:: dhanda `{function_name} has already been defined`"),
                        ));
                    }

                    self.lookahead_function_definitions
                        .push(PendingFunctionDefinition {
                            name: identifier.name.clone(),
                            arity: parameters.len(),
                        })
                }
                StatementKind::Declaration { .. } => {}
                _ => {
                    return Err(Diagnostic::error(
                        statement.span,
                        "SEMA:: Only functions and variables definitions are allowed in the global scope",
                    ));
                }
            }
        }

        // Process the rest of the statements
        for statement in &program.block.statements {
            self.analyse_statement(statement)?;
        }

        if !self.main_function_found {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                "SEMA:: No `muji` function found",
            ));
        }

        Ok(())
    }

    fn analyse_block(&mut self, block: &Block) -> Result<(), Diagnostic> {
        self.push_new_scope(Scope::new(ScopeType::Block));
        for statement in &block.statements {
            self.analyse_statement(statement)?;
        }
        self.pop_scope();
        Ok(())
    }

    fn analyse_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Declaration { identifier, value } => {
                self.analyse_expression(value)?;
                let identifier_name = identifier.name.clone();
                if self
                    .get_current_scope()
                    .contains_identifier(identifier.name.clone())
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Identifier {identifier_name} has already been declared"),
                    ));
                }
                self.get_current_scope().add_variable_identifier(identifier);
                Ok(())
            }
            StatementKind::Return(expression) => {
                let Some(_) = self.get_enclosing_functional_scope() else {
                    return Err(Diagnostic::error(
                        expression.span,
                        "`de` should be inside a `dhanda`",
                    ));
                };

                self.analyse_expression(expression)
            }
            StatementKind::Function(FunctionDefinition {
                identifier,
                body,
                parameters,
            }) => {
                let identifier_name = identifier.name.clone();
                if identifier_name == "muji" {
                    self.main_function_found = true;
                }

                if self
                    .get_current_scope()
                    .contains_identifier(identifier.name.clone())
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Identifier {identifier_name} has already been declared"),
                    ));
                }

                // We should allow recursive calls
                self.get_current_scope()
                    .add_function_identifier(identifier, parameters.len());

                self.push_new_scope(Scope::new(ScopeType::Function));
                for parameter in parameters {
                    let parameter_name = parameter.name.clone();
                    if self
                        .get_current_scope()
                        .contains_identifier(parameter_name.clone())
                    {
                        return Err(Diagnostic::error(
                            parameter.span,
                            format!("SEMA:: Identifier {parameter_name} has already been declared"),
                        ));
                    }
                    self.get_current_scope().add_variable_identifier(parameter);
                }
                for statement in &body.statements {
                    self.analyse_statement(statement)?;
                }
                self.pop_scope();
                Ok(())
            }
            StatementKind::If(conditional) => self.analyse_conditional(conditional),
            StatementKind::While { condition, body } => {
                self.analyse_expression(condition)?;
                self.analyse_block(body)?;
                Ok(())
            }
            StatementKind::For {
                initialiser,
                condition,
                update,
                body,
            } => {
                self.push_new_scope(Scope::new(ScopeType::Block));
                if let Some(initializer) = initialiser {
                    self.analyse_statement(initializer)?;
                }
                self.analyse_expression(condition)?;
                self.analyse_expression(update)?;
                self.analyse_block(body)?;
                self.pop_scope();
                Ok(())
            }
            StatementKind::Expression(expression) => self.analyse_expression(expression),
        }
    }

    fn analyse_expression(&mut self, expression: &Expression) -> Result<(), Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                if !self.is_identifier_valid(identifier.clone()) {
                    return Err(Diagnostic::error(
                        expression.span,
                        format!("SEMA:: Usage of undefined identifier {identifier}"),
                    ));
                }
                Ok(())
            }
            ExpressionKind::Integer(..) => Ok(()),
            ExpressionKind::Boolean(..) => Ok(()),
            ExpressionKind::UnaryOperation { operand, .. } => self.analyse_expression(operand),
            ExpressionKind::BinaryOperation { left, right, .. } => {
                self.analyse_expression(left)?;
                self.analyse_expression(right)?;
                Ok(())
            }
            ExpressionKind::Assignment { target, value } => {
                self.analyse_expression(value)?;
                let AssignmentTarget::Variable(identifier) = target;
                if !self.is_identifier_valid(identifier.name.clone()) {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Usage of undefined identifier {}", identifier.name),
                    ));
                }
                Ok(())
            }
            ExpressionKind::Call { callee, arguments } => {
                if self.get_current_scope().scope_type == ScopeType::Global {
                    return Err(Diagnostic::error(
                        expression.span,
                        "SEMA:: Function calls are not allowed in global scope",
                    ));
                }

                let function_name = callee.name.clone();

                // TODO: Refactor, this code
                // Try from the symbol table first
                if let Some(symbol_info) = self.get_symbol_info(function_name.clone()) {
                    match symbol_info {
                        SymbolInfo::Variable => {
                            return Err(Diagnostic::error(
                                expression.span,
                                "SEMA:: Called object is not a function".to_string(),
                            ));
                        }
                        SymbolInfo::Function(num_of_parameters) => {
                            (|| -> Result<(), Diagnostic> {
                                if *num_of_parameters == arguments.len() {
                                    return Ok(());
                                }
                                let few_many = if arguments.len() > *num_of_parameters {
                                    "many"
                                } else {
                                    "few"
                                };

                                let error_pointer_span = expression.span.end - 1;
                                Err(Diagnostic::error(
                                    Span::new(error_pointer_span, error_pointer_span),
                                    format!(
                                        "SEMA:: too {few_many} arguments in call to `{function_name}`"
                                    ),
                                ))
                            })()?
                        }
                    }
                } else {
                    // Look up pending definitions
                    if let Some(pending_function_definition) = self
                        .lookahead_function_definitions
                        .iter()
                        .find(|pending_function_definition| {
                            pending_function_definition.name == function_name
                        })
                    {
                        if pending_function_definition.arity == arguments.len() {
                            return Ok(());
                        }

                        let few_many = if arguments.len() > pending_function_definition.arity {
                            "many"
                        } else {
                            "few"
                        };

                        let error_pointer_span = expression.span.end;
                        return Err(Diagnostic::error(
                            Span::new(error_pointer_span, error_pointer_span),
                            format!(
                                "too {few_many} arguments to the function in call to `{function_name}`"
                            ),
                        ));
                    }
                    return Err(Diagnostic::error(
                        callee.span,
                        format!("SEMA:: Usage of undefined function {function_name}"),
                    ));
                }

                for argument in arguments {
                    self.analyse_expression(argument)?;
                }
                Ok(())
            }
        }
    }

    fn analyse_conditional(&mut self, conditional: &Conditional) -> Result<(), Diagnostic> {
        self.analyse_expression(&conditional.condition)?;
        self.analyse_block(&conditional.then)?;

        if let Some(otherwise) = &conditional.otherwise {
            match otherwise.deref() {
                Else::Block(block) => return self.analyse_block(block),
                Else::If(conditional) => return self.analyse_conditional(conditional),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    /// Every test source carries a `muji` function, since its absence is
    /// itself one of the things sema checks. `main_only` is the exception.
    fn analyse(source: &str) -> Result<(), Diagnostic> {
        let program = parse(source)
            .unwrap_or_else(|diagnostics| panic!("should parse: {}", diagnostics[0].message));
        Sema::new().analyse(&program)
    }

    /// Asserts the program is semantically valid.
    fn accepts(source: &str) {
        if let Err(diagnostic) = analyse(source) {
            panic!("should analyse cleanly, but got: {}", diagnostic.message);
        }
    }

    /// Asserts the program is rejected, and that the message mentions
    /// `expected_fragment`. Matching a fragment rather than the whole string
    /// keeps the `SEMA:: ` prefix and the exact wording free to change.
    fn rejects(source: &str, expected_fragment: &str) {
        let message = analyse(source)
            .expect_err("should be rejected")
            .message
            .clone();

        assert!(
            message.contains(expected_fragment),
            "expected a message containing {expected_fragment:?}, got {message:?}"
        );
    }

    /* ------------------------------------------------------------ accepted */

    #[test]
    fn a_program_that_is_only_a_main_function() {
        accepts("dhanda muji() { }");
    }

    #[test]
    fn a_variable_is_visible_after_its_declaration() {
        accepts("dhanda muji() { thoos x = 1; x; }");
    }

    /// A top-level `thoos` is one of the two things the global scope accepts,
    /// and a function body can still read it.
    #[test]
    fn a_top_level_variable_is_visible_inside_a_function() {
        accepts("thoos g = 1; dhanda muji() { g; }");
    }

    #[test]
    fn a_parameter_is_visible_inside_the_body() {
        accepts("dhanda muji() { } dhanda f(a) { a; }");
    }

    #[test]
    fn a_call_whose_argument_count_matches() {
        accepts("dhanda add(a, b) { de 1; } dhanda muji() { add(1, 2); }");
    }

    /// A function's own name is in scope inside its body.
    #[test]
    fn a_function_may_call_itself() {
        accepts("dhanda muji() { } dhanda fac(n) { de fac(1); }");
    }

    /// Top-level signatures are collected before any body is checked, so a
    /// call may name a function defined further down the file.
    #[test]
    fn a_function_may_call_one_defined_later() {
        accepts("dhanda muji() { } dhanda a() { de b(); } dhanda b() { de 1; }");
    }

    #[test]
    fn two_top_level_functions_may_call_each_other() {
        accepts(
            "dhanda muji() { } \
             dhanda is_even(n) { de is_odd(n - 1); } \
             dhanda is_odd(n) { de is_even(n - 1); }",
        );
    }

    /// `muji` itself is defined before everything it calls, which is the
    /// ordering every fixture relies on.
    #[test]
    fn main_may_call_a_function_defined_below_it() {
        accepts("dhanda muji() { bol(later()); } dhanda later() { de 1; }");
    }

    /// Collecting signatures early must not skip the arity check.
    #[test]
    fn a_forward_call_still_has_its_arity_checked() {
        rejects(
            "dhanda muji() { add(1); } dhanda add(a, b) { de a + b; }",
            "too few arguments",
        );
    }

    /// A nested `dhanda` defined before the call that uses it.
    #[test]
    fn a_nested_function_called_after_its_definition() {
        accepts("dhanda muji() { dhanda inner() { de 1; } bol(inner()); }");
    }

    /// Only the global scope collects signatures ahead of time, so a nested
    /// `dhanda` stays order-dependent like any other statement.
    #[test]
    fn a_nested_function_called_before_its_definition() {
        rejects(
            "dhanda muji() { bol(inner()); dhanda inner() { de 1; } }",
            "inner",
        );
    }

    /// Follows from the same rule: the first body is checked before the
    /// second name exists.
    #[test]
    fn two_nested_functions_cannot_call_each_other() {
        rejects(
            "dhanda muji() { dhanda a() { de b(); } dhanda b() { de a(); } }",
            "b",
        );
    }

    /// A nested `dhanda` can still reach a top-level one defined below it,
    /// because that name was collected in the global sweep.
    #[test]
    fn a_nested_function_may_call_a_later_top_level_function() {
        accepts("dhanda muji() { dhanda inner() { de outer(); } } dhanda outer() { de 1; }");
    }

    /// The global scope has to stay reachable from inside a function body.
    #[test]
    fn a_builtin_is_callable_from_inside_a_function() {
        accepts("dhanda muji() { bol(1); }");
    }

    #[test]
    fn an_inner_block_may_reuse_an_outer_name() {
        accepts("dhanda muji() { thoos x = 1; yedi (1) { thoos x = 2; } }");
    }

    /// `dhanda` nests.
    #[test]
    fn a_function_defined_inside_another_function() {
        accepts("dhanda muji() { } dhanda outer() { dhanda inner() { de 1; } de 1; }");
    }

    /* ------------------------------------------------------------ rejected */

    #[test]
    fn a_program_without_a_main_function() {
        rejects("thoos x = 1;", "muji");
    }

    #[test]
    fn two_declarations_of_one_name_in_the_same_scope() {
        rejects(
            "dhanda muji() { } thoos x = 1; thoos x = 2;",
            "already been declared",
        );
    }

    #[test]
    fn two_functions_with_the_same_name() {
        rejects(
            "dhanda muji() { } dhanda f() { de 1; } dhanda f() { de 2; }",
            "already been defined",
        );
    }

    #[test]
    fn a_parameter_name_repeated() {
        rejects(
            "dhanda muji() { } dhanda f(a, a) { de 1; }",
            "already been declared",
        );
    }

    #[test]
    fn an_undefined_name_on_its_own() {
        rejects("dhanda muji() { nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_declaration_value() {
        rejects("dhanda muji() { } thoos x = nope;", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_binary_operation() {
        rejects("dhanda muji() { } thoos x = nope + 1;", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_unary_operation() {
        rejects("dhanda muji() { } thoos x = -nope;", "nope");
    }

    #[test]
    fn an_undefined_name_as_an_assignment_target() {
        rejects("dhanda muji() { nope = 1; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_an_assigned_value() {
        rejects("dhanda muji() { thoos x = 1; x = nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_conditional() {
        rejects("dhanda muji() { yedi (nope) { 1; } }", "nope");
        rejects("dhanda muji() { yedi (1) { nope; } }", "nope");
        rejects("dhanda muji() { yedi (1) { 1; } natra { nope; } }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_while_loop() {
        rejects("dhanda muji() { jabasamma (nope) { 1; } }", "nope");
        rejects("dhanda muji() { jabasamma (1) { nope; } }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_for_loop() {
        rejects(
            "dhanda muji() { fanfani (thoos i = 0; 1; i) { nope; } }",
            "nope",
        );
        rejects(
            "dhanda muji() { fanfani (thoos i = 0; nope; i) { 1; } }",
            "nope",
        );
    }

    #[test]
    fn a_call_to_an_undefined_function() {
        rejects("dhanda muji() { nope(1); }", "nope");
    }

    #[test]
    fn a_call_with_too_few_arguments() {
        rejects(
            "dhanda add(a, b) { de 1; } dhanda muji() { add(1); }",
            "too few arguments",
        );
    }

    #[test]
    fn a_call_with_too_many_arguments() {
        rejects(
            "dhanda add(a, b) { de 1; } dhanda muji() { add(1, 2, 3); }",
            "too many arguments",
        );
    }

    #[test]
    fn a_call_to_something_that_is_not_a_function() {
        rejects("dhanda muji() { thoos x = 1; x(); }", "not a function");
    }

    /// The global scope holds definitions and nothing else, so everything
    /// executable has to sit inside a `dhanda`.
    #[test]
    fn only_definitions_at_the_top_level() {
        rejects("dhanda muji() { } bol(1);", "global scope");
        rejects("dhanda muji() { } 1 + 1;", "global scope");
        rejects("dhanda muji() { } yedi (1) { 1; }", "global scope");
        rejects("dhanda muji() { } jabasamma (1) { 1; }", "global scope");
        rejects(
            "dhanda muji() { } fanfani (thoos i = 0; 1; i) { 1; }",
            "global scope",
        );
    }

    #[test]
    fn a_variable_used_before_the_line_that_declares_it() {
        rejects("dhanda muji() { later; thoos later = 1; }", "later");
    }

    /// A `de` outside a `dhanda` is now caught one step earlier, by the rule
    /// that the global scope holds only definitions — there is nowhere left
    /// for a bare `de` to sit.
    #[test]
    fn a_return_outside_any_function() {
        rejects("dhanda muji() { } de 1;", "global scope");
    }

    /// The `fanfani` header owns the loop variable, which dies with the loop.
    #[test]
    fn a_for_loop_variable_does_not_outlive_the_loop() {
        rejects(
            "dhanda muji() { fanfani (thoos i = 0; 1; i) { 1; } i; }",
            "i",
        );
    }
}
