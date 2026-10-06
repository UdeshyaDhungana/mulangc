// Semantic analyzer
use std::{collections::HashMap, ops::Deref};

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AST, AssignmentTarget, Block, Conditional, Else, Expression, ExpressionKind,
        FunctionDefinition, Identifier, Statement, StatementKind,
    },
};

pub type MemoryUnits = usize;

pub const MAIN_FUNCTION_NODE_ID: &str = "_main";
pub const __DEBUG_BOL_FUNCTION_NODE_ID: &str = "__debug_bol";

struct FunctionSymbolInfo {
    identifier: Identifier,
    arity: usize,
}

enum SymbolInfo {
    Function(FunctionSymbolInfo),
    Variable(Identifier),
}

#[derive(Debug)]
enum ScopeType {
    Function {
        node_id: String,
        accumulated_frame_size: MemoryUnits,
    },
    Block,
}

struct Scope {
    pub symbol_table: HashMap<String, SymbolInfo>,
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
        self.symbol_table.insert(
            identifier.name.clone(),
            SymbolInfo::Variable(identifier.clone()),
        );
    }

    fn add_function_identifier(&mut self, identifier: &Identifier, arity: usize) {
        self.symbol_table.insert(
            identifier.name.clone(),
            SymbolInfo::Function(FunctionSymbolInfo {
                identifier: identifier.clone(),
                arity,
            }),
        );
    }

    fn contains_identifier(&self, identifier_name: String) -> bool {
        self.symbol_table.contains_key(&identifier_name)
    }
}

#[derive(Default)]
pub struct Counter {
    count: usize,
}

impl Counter {
    pub fn advance(&mut self) -> usize {
        self.count += 1;
        self.count - 1
    }
}

#[derive(Debug)]
pub struct VariableAttributes {
    pub frame_depth: usize,
    pub offset_from_frame_base: MemoryUnits,
}

#[derive(Debug)]
pub struct FunctionAttributes {
    pub frame_depth: usize,
    pub frame_size: MemoryUnits,
}

#[derive(Debug)]
pub enum IdentifierAttributes {
    Variable(VariableAttributes),
    Function(FunctionAttributes),
}

#[derive(Debug, Default)]
pub struct Analysis {
    pub use_site_node_id_resolution_map: HashMap<String, String>,
    pub node_id_attributes: HashMap<String, IdentifierAttributes>,
}

impl Analysis {
    pub fn must_get_variable_attributes_from_node_id(&self, node_id: &str) -> &VariableAttributes {
        let Some(IdentifierAttributes::Variable(location)) = self.node_id_attributes.get(node_id)
        else {
            panic!("Variable with node id {node_id} not found");
        };

        location
    }

    pub fn must_get_function_attributes_from_node_id(
        &mut self,
        node_id: &str,
    ) -> &mut FunctionAttributes {
        let Some(IdentifierAttributes::Function(location)) =
            self.node_id_attributes.get_mut(node_id)
        else {
            panic!("Function with node id {node_id} not found");
        };

        location
    }

    pub fn resolve(&self, user_node_id: &String) -> String {
        self.use_site_node_id_resolution_map
            .get(user_node_id)
            .expect("Caller node id must have been resolved")
            .clone()
    }
}

struct LookaheadFunctionDefinition {
    identifier: Identifier,
    arity: usize,
}

pub struct Sema {
    scopes_stack: Vec<Scope>,
    analysis: Analysis,
    diagnostics: Vec<Diagnostic>,
    lookahead_function_definitions: Vec<LookaheadFunctionDefinition>,
}

impl Default for Sema {
    fn default() -> Self {
        Self::new()
    }
}

impl Sema {
    pub fn new() -> Self {
        Self {
            scopes_stack: Vec::new(),
            lookahead_function_definitions: Vec::new(),
            analysis: Default::default(),
            diagnostics: Vec::new(),
        }
    }

    fn get_current_scope(&mut self) -> &mut Scope {
        self.scopes_stack
            .last_mut()
            .expect("At least one scope must exist during semantic analysis")
    }

    fn get_symbol_info_for_identifier(&self, identifier_name: String) -> Option<&SymbolInfo> {
        for stack in self.scopes_stack.iter().rev() {
            if let Some(symbol_info) = stack.symbol_table.get(&identifier_name.clone()) {
                return Some(symbol_info);
            } else {
                continue;
            }
        }
        None
    }

    fn current_frame_mut(&mut self) -> (&String, &mut MemoryUnits) {
        for scope in self.scopes_stack.iter_mut().rev() {
            match &mut scope.scope_type {
                ScopeType::Function {
                    node_id: frame_id,
                    accumulated_frame_size: size,
                } => {
                    return (frame_id, size);
                }
                ScopeType::Block => continue,
            }
        }
        panic!("At least one frame must exist during semantic analysis");
    }

    fn frame_depth(&self) -> usize {
        self.scopes_stack
            .iter()
            .filter(|scope| matches!(scope.scope_type, ScopeType::Function { .. }))
            .count()
            - 1
    }

    // node_id means the node id of the function identifier
    fn record_frame_location_in_analysis(&mut self, node_id: String, depth: usize) {
        self.analysis.node_id_attributes.insert(
            node_id,
            IdentifierAttributes::Function(FunctionAttributes {
                frame_depth: depth,
                // We'll calculate the frame size once it ends
                frame_size: 0,
            }),
        );
    }

    fn allocate_slot_for_variable_in_current_frame(&mut self, identifier: &Identifier) {
        let depth = self.frame_depth();
        let (_, size) = self.current_frame_mut();
        let slot = *size;
        *size += 1;
        self.analysis.node_id_attributes.insert(
            identifier.node_id.clone(),
            IdentifierAttributes::Variable(VariableAttributes {
                frame_depth: depth,
                offset_from_frame_base: slot,
            }),
        );
    }

    fn push_scope(&mut self, scope: Scope) {
        self.scopes_stack.push(scope);
    }

    fn pop_scope(&mut self) {
        let scope = self
            .scopes_stack
            .pop()
            .expect("A scope must exist to be popped");

        let ScopeType::Function {
            node_id: frame_id,
            accumulated_frame_size,
        } = scope.scope_type
        else {
            return;
        };

        self.analysis
            .must_get_function_attributes_from_node_id(&frame_id)
            .frame_size = accumulated_frame_size;
    }

    pub fn analyse(mut self, ast: &AST) -> Result<Analysis, Vec<Diagnostic>> {
        self.record_frame_location_in_analysis(String::from(MAIN_FUNCTION_NODE_ID), 0);
        self.push_scope(Scope::new(ScopeType::Function {
            node_id: String::from(MAIN_FUNCTION_NODE_ID),
            accumulated_frame_size: 0,
        }));

        // DEBUG: Just add `bol` builtin so analyzer won't complain
        self.get_current_scope().add_function_identifier(
            &Identifier {
                name: String::from("bol"),
                node_id: String::from(__DEBUG_BOL_FUNCTION_NODE_ID),
                span: Span::new(0, 0),
            },
            1,
        );
        self.record_frame_location_in_analysis(String::from(__DEBUG_BOL_FUNCTION_NODE_ID), 0);
        // END DEBUG

        // Pass 1
        // Process functions signatures first, so functions can be used before their definitions are set in
        for statement in &ast.block.statements {
            if let StatementKind::Function(FunctionDefinition {
                identifier,
                parameters,
                ..
            }) = &statement.kind
            {
                let function_name = identifier.name.clone();

                if self
                    .lookahead_function_definitions
                    .iter()
                    .any(|pending_function_definition| {
                        pending_function_definition.identifier.name == identifier.name
                    })
                {
                    self.diagnostics.push(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: dhanda `{function_name} has already been defined`"),
                    ));
                    continue;
                }

                self.lookahead_function_definitions
                    .push(LookaheadFunctionDefinition {
                        identifier: identifier.clone(),
                        arity: parameters.len(),
                    })
            }
        }

        // Pass 2
        // Process the rest of the statements
        self.analyse_statements_collecting_diagnostics(&ast.block.statements);
        self.pop_scope();

        if self.diagnostics.is_empty() {
            Ok(self.analysis)
        } else {
            Err(self.diagnostics)
        }
    }

    fn analyse_statements_collecting_diagnostics(&mut self, statements: &[Statement]) {
        for statement in statements {
            let scope_depth_before_statement = self.scopes_stack.len();
            if let Err(diagnostic) = self.analyse_statement(statement) {
                self.diagnostics.push(diagnostic);
                self.scopes_stack.truncate(scope_depth_before_statement);
            }
        }
    }

    fn analyse_block(&mut self, block: &Block) -> Result<(), Diagnostic> {
        self.push_scope(Scope::new(ScopeType::Block));
        self.analyse_statements_collecting_diagnostics(&block.statements);
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
                self.allocate_slot_for_variable_in_current_frame(identifier);
                Ok(())
            }
            StatementKind::Return(expression) => {
                if self.frame_depth() == 0 {
                    return Err(Diagnostic::error(
                        expression.span,
                        "`de` must be used inside a `dhanda`",
                    ));
                }

                self.analyse_expression(expression)
            }
            StatementKind::Function(FunctionDefinition {
                identifier,
                body,
                parameters,
            }) => {
                let function_name = identifier.name.clone();

                if self
                    .get_current_scope()
                    .contains_identifier(function_name.clone())
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Identifier {function_name} has already been declared"),
                    ));
                }

                // We should allow recursive calls, so register early
                self.get_current_scope()
                    .add_function_identifier(identifier, parameters.len());
                self.record_frame_location_in_analysis(
                    identifier.node_id.clone(),
                    self.frame_depth(),
                );

                self.push_scope(Scope::new(ScopeType::Function {
                    node_id: identifier.node_id.clone(),
                    accumulated_frame_size: 0,
                }));
                for parameter in parameters {
                    let parameter_name = parameter.name.clone();
                    // Each parameter identifier must be unique
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
                    self.allocate_slot_for_variable_in_current_frame(parameter);
                }
                self.analyse_statements_collecting_diagnostics(&body.statements);
                self.pop_scope();
                Ok(())
            }
            StatementKind::If(conditional) => self.analyse_conditional(conditional),
            StatementKind::While {
                condition, body, ..
            } => {
                self.analyse_expression(condition)?;
                self.analyse_block(body)?;
                Ok(())
            }
            StatementKind::For {
                initialiser,
                condition,
                update,
                body,
                ..
            } => {
                // For's parenthesis get their own scope
                self.push_scope(Scope::new(ScopeType::Block));
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
                let Some(symbol_info) =
                    self.get_symbol_info_for_identifier(identifier.name.clone())
                else {
                    return Err(Diagnostic::error(
                        expression.span,
                        format!("SEMA:: Usage of undefined identifier {identifier}"),
                    ));
                };

                match symbol_info {
                    SymbolInfo::Function(..) => {
                        let identifier_name = identifier.name.clone();
                        Err(Diagnostic::error(
                            expression.span,
                            format!("Identifier {identifier_name} is a `dhanda`, not a `variable`"),
                        ))
                    }
                    SymbolInfo::Variable(Identifier {
                        node_id: resolved_node_id,
                        ..
                    }) => {
                        self.analysis
                            .use_site_node_id_resolution_map
                            .insert(identifier.node_id.clone(), resolved_node_id.clone());
                        Ok(())
                    }
                }
            }
            ExpressionKind::Integer(..) => Ok(()),
            ExpressionKind::UnaryOperation { operand, .. } => self.analyse_expression(operand),
            ExpressionKind::BinaryOperation { left, right, .. } => {
                self.analyse_expression(left)?;
                self.analyse_expression(right)?;
                Ok(())
            }
            ExpressionKind::Assignment { target, value } => {
                self.analyse_expression(value)?;
                let AssignmentTarget::Variable(identifier) = target;
                let Some(symbol_info) =
                    self.get_symbol_info_for_identifier(identifier.name.clone())
                else {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Usage of undefined identifier {}", identifier.name),
                    ));
                };
                let variable_name = identifier.name.clone();
                match symbol_info {
                    SymbolInfo::Function(..) => {
                        return Err(Diagnostic::error(
                            identifier.span,
                            format!("Cannot assign to {variable_name} of `dhanda` type"),
                        ));
                    }
                    SymbolInfo::Variable(Identifier {
                        node_id: resolved_node_id,
                        ..
                    }) => {
                        self.analysis
                            .use_site_node_id_resolution_map
                            .insert(identifier.node_id.clone(), resolved_node_id.clone());
                    }
                }
                Ok(())
            }
            ExpressionKind::Call { callee, arguments } => {
                let function_name = callee.name.clone();
                let mut was_callee_resolved_from_frame_stack = false;

                // Check within the scope stack
                // Scope stack takes priority over lookahead map
                if let Some(symbol_info) =
                    self.get_symbol_info_for_identifier(function_name.clone())
                {
                    match symbol_info {
                        SymbolInfo::Variable(..) => {
                            return Err(Diagnostic::error(
                                expression.span,
                                "SEMA:: Called object is not a function",
                            ));
                        }
                        SymbolInfo::Function(FunctionSymbolInfo { arity, identifier }) => {
                            if *arity != arguments.len() {
                                let few_many = if arguments.len() > *arity {
                                    "many"
                                } else {
                                    "few"
                                };

                                let error_pointer_span = expression.span.end - 1;
                                return Err(Diagnostic::error(
                                    Span::new(error_pointer_span, error_pointer_span),
                                    format!(
                                        "SEMA:: too {few_many} arguments in call to `{function_name}`"
                                    ),
                                ));
                            }
                            was_callee_resolved_from_frame_stack = true;
                            self.analysis
                                .use_site_node_id_resolution_map
                                .insert(callee.node_id.clone(), identifier.node_id.clone());
                        }
                    }
                }

                // Check within the lookahead definitions
                let mut was_callee_resolved_from_lookahead_definitions = false;
                if !was_callee_resolved_from_frame_stack
                    && let Some(fn_definition) = self
                        .lookahead_function_definitions
                        .iter()
                        .find(|fn_definition| fn_definition.identifier.name == function_name)
                {
                    if fn_definition.arity != arguments.len() {
                        let few_many = if arguments.len() > fn_definition.arity {
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
                    was_callee_resolved_from_lookahead_definitions = true;
                    self.analysis.use_site_node_id_resolution_map.insert(
                        callee.node_id.clone(),
                        fn_definition.identifier.node_id.clone(),
                    );
                }

                if !was_callee_resolved_from_frame_stack
                    && !was_callee_resolved_from_lookahead_definitions
                {
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

    fn analyse(source: &str) -> Result<Analysis, Vec<Diagnostic>> {
        let ast = parse(source)
            .unwrap_or_else(|diagnostics| panic!("should parse: {}", diagnostics[0].message));
        Sema::new().analyse(&ast)
    }

    fn accepts(source: &str) {
        if let Err(diagnostics) = analyse(source) {
            panic!(
                "should analyse cleanly, but got: {}",
                diagnostics[0].message
            );
        }
    }

    fn rejects(source: &str, expected_fragment: &str) {
        let message = analyse(source).expect_err("should be rejected")[0]
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

    #[test]
    fn a_function_may_call_itself() {
        accepts("dhanda muji() { } dhanda fac(n) { de fac(1); }");
    }

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

    #[test]
    fn main_may_call_a_function_defined_below_it() {
        accepts("dhanda muji() { bol(later()); } dhanda later() { de 1; }");
    }

    #[test]
    fn a_forward_call_still_has_its_arity_checked() {
        rejects(
            "dhanda muji() { add(1); } dhanda add(a, b) { de a + b; }",
            "too few arguments",
        );
    }

    #[test]
    fn a_builtin_is_callable_from_inside_a_function() {
        accepts("dhanda muji() { bol(1); }");
    }

    #[test]
    fn an_inner_block_may_reuse_an_outer_name() {
        accepts("dhanda muji() { thoos x = 1; yedi (1) { thoos x = 2; } }");
    }

    #[test]
    fn a_dhanda_may_be_declared_inside_a_dhanda() {
        accepts("dhanda muji() { dhanda inner() { de 1; } }");
    }

    #[test]
    fn a_dhanda_may_be_declared_inside_a_block() {
        accepts("dhanda muji() { yedi (1) { dhanda inner() { de 1; } } }");
        accepts("dhanda muji() { jabasamma (1) { dhanda inner() { de 1; } } }");
    }

    #[test]
    fn a_nested_dhanda_is_callable_after_its_definition() {
        accepts("dhanda muji() { dhanda inner(a) { de a; } bol(inner(1)); }");
    }

    #[test]
    fn a_nested_dhanda_may_call_itself() {
        accepts("dhanda muji() { dhanda inner(n) { de inner(n - 1); } }");
    }

    #[test]
    fn a_nested_dhanda_may_read_its_enclosing_dhandas_variables() {
        accepts("dhanda muji() { thoos x = 1; dhanda inner() { de x; } }");
        accepts("dhanda muji() { dhanda outer(a) { dhanda inner() { de a; } } }");
    }

    #[test]
    fn a_nested_dhanda_may_call_a_global_dhanda() {
        accepts("dhanda muji() { dhanda inner() { de later(); } } dhanda later() { de 1; }");
    }

    #[test]
    fn a_nested_dhanda_may_shadow_a_global_dhanda() {
        accepts("dhanda f() { de 1; } dhanda muji() { dhanda f(a) { de a; } f(1); }");
    }

    #[test]
    fn two_dhandas_may_each_nest_a_dhanda_of_the_same_name() {
        accepts(
            "dhanda muji() { dhanda inner() { de 1; } } \
             dhanda other() { dhanda inner() { de 2; } }",
        );
    }

    /* ------------------------------------------------------------ rejected */

    #[test]
    fn a_nested_dhanda_is_not_visible_outside_its_enclosing_dhanda() {
        rejects(
            "dhanda muji() { inner(); } dhanda other() { dhanda inner() { de 1; } }",
            "inner",
        );
    }

    #[test]
    fn a_nested_dhanda_is_not_visible_outside_its_block() {
        rejects(
            "dhanda muji() { yedi (1) { dhanda inner() { de 1; } } inner(); }",
            "inner",
        );
    }

    #[test]
    fn two_nested_dhandas_with_the_same_name_in_one_scope() {
        rejects(
            "dhanda muji() { dhanda inner() { de 1; } dhanda inner() { de 2; } }",
            "already been declared",
        );
    }

    #[test]
    fn a_nested_dhanda_and_a_variable_with_the_same_name_in_one_scope() {
        rejects(
            "dhanda muji() { thoos inner = 1; dhanda inner() { de 1; } }",
            "already been declared",
        );
    }

    #[test]
    fn a_nested_dhanda_still_has_its_arity_checked() {
        rejects(
            "dhanda muji() { dhanda inner(a, b) { de a; } inner(1); }",
            "too few arguments",
        );
    }

    #[test]
    fn a_program_without_a_muji_is_accepted() {
        accepts("thoos x = 1;");
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
        rejects("dhanda muji() { thoos x = nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_binary_operation() {
        rejects("dhanda muji() { thoos x = nope + 1; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_unary_operation() {
        rejects("dhanda muji() { thoos x = -nope; }", "nope");
    }

    #[test]
    fn a_global_may_be_initialised_from_another_global() {
        accepts("dhanda muji() { } thoos a = 1; thoos b = a;");
        accepts("dhanda muji() { } thoos a = 1; thoos b = a + 1;");
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

    #[test]
    fn any_statement_is_allowed_at_the_top_level() {
        accepts("dhanda muji() { } bol(1);");
        accepts("dhanda muji() { } 1 + 1;");
        accepts("dhanda muji() { } yedi (1) { 1; }");
        accepts("dhanda muji() { } jabasamma (1) { 1; }");
        accepts("dhanda muji() { } fanfani (thoos i = 0; 1; i) { 1; }");
    }

    #[test]
    fn a_variable_used_before_the_line_that_declares_it() {
        rejects("dhanda muji() { later; thoos later = 1; }", "later");
    }

    #[test]
    fn a_return_outside_any_function() {
        rejects("dhanda muji() { } de 1;", "inside a `dhanda`");
    }

    #[test]
    fn a_for_loop_variable_does_not_outlive_the_loop() {
        rejects(
            "dhanda muji() { fanfani (thoos i = 0; 1; i) { 1; } i; }",
            "i",
        );
    }

    /* --------------------------------------------- more than one diagnostic */

    #[test]
    fn reports_every_broken_statement_in_one_pass() {
        let diagnostics = analyse("dhanda muji() { } dhanda a() { de x; } dhanda b() { de y; }")
            .expect_err("should be rejected");

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("x"));
        assert!(diagnostics[1].message.contains("y"));
    }

    // A `fanfani` pushes the scope for its parentheses before analysing its
    // initialiser, so a failing initialiser leaves that scope behind. If it is
    // not cleaned up, every later statement is analysed one scope too deep, so
    // the `muji` defined after it lands outside the global scope and is not found.
    #[test]
    fn a_failed_for_loop_does_not_leave_its_scope_behind() {
        let diagnostics = analyse(
            "dhanda broken() { fanfani (thoos i = missing; 1; i) { 1; } } dhanda muji() { }",
        )
        .expect_err("should be rejected");

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing"));
    }
}
