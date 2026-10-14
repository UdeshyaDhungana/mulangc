// Semantic analyzer
use std::{collections::HashMap, ops::Deref};

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AST, Block, Conditional, Else, Expression, ExpressionKind, FunctionDefinition, Identifier,
        Statement,
        StatementKind::{self},
        ast::{
            SpannedVariableType,
            VariableType::{self, BooleanType, IntegerType, VoidType},
        },
    },
};

pub type MemoryUnits = usize;

pub const MAIN_FUNCTION_NODE_ID: &str = "_main";
pub const __DEBUG_BOL_FUNCTION_NODE_ID: &str = "__debug_bol";
pub const __DEBUG_BOL_BOOL_FUNCTION_NODE_ID: &str = "__debug_bolbool";

// Scope's symbol info
struct FunctionSymbolInfo {
    identifier: Identifier,
    parameters_types: Vec<SpannedVariableType>,
    return_type: SpannedVariableType,
}

struct VariableSymbolInfo {
    identifier: Identifier,
    var_type: SpannedVariableType,
}

impl VariableSymbolInfo {
    fn get_classification(&self) -> VariableType {
        self.var_type.var_type.clone()
    }
}

enum SymbolInfo {
    Function(FunctionSymbolInfo),
    Variable(VariableSymbolInfo),
}

#[derive(Debug)]
enum ScopeType {
    Function {
        node_id: String,
        memory_units_consumed: MemoryUnits,
        return_type_classification: VariableType,
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

    fn add_variable_identifier(&mut self, identifier: &Identifier, var_type: SpannedVariableType) {
        self.symbol_table.insert(
            identifier.name.clone(),
            SymbolInfo::Variable(VariableSymbolInfo {
                identifier: identifier.clone(),
                var_type,
            }),
        );
    }

    fn add_function_identifier(
        &mut self,
        identifier: &Identifier,
        fn_parameters_types: Vec<SpannedVariableType>,
        return_type: SpannedVariableType,
    ) {
        self.symbol_table.insert(
            identifier.name.clone(),
            SymbolInfo::Function(FunctionSymbolInfo {
                identifier: identifier.clone(),
                parameters_types: fn_parameters_types.clone(),
                return_type,
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
    pub var_type: SpannedVariableType,
}

#[derive(Debug)]
pub struct FunctionAttributes {
    // Depth of the frame the function is defined in, not of its own frame
    pub definition_depth: usize,
    pub memory_units_consumed: MemoryUnits,
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
    parameter_types: Vec<SpannedVariableType>,
    return_type: SpannedVariableType,
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

    fn get_current_frame(&mut self) -> &mut Scope {
        self.scopes_stack
            .iter_mut()
            .rfind(|scope| matches!(scope.scope_type, ScopeType::Function { .. }))
            .expect("At least one scope must exist during semantic analysis")
    }

    fn get_current_frame_depth(&self) -> usize {
        self.scopes_stack
            .iter()
            .filter(|scope| matches!(scope.scope_type, ScopeType::Function { .. }))
            .count()
            - 1
    }

    fn lookup_syminfo_for_identifier(&self, identifier_name: String) -> Option<&SymbolInfo> {
        for stack in self.scopes_stack.iter().rev() {
            if let Some(symbol_info) = stack.symbol_table.get(&identifier_name.clone()) {
                return Some(symbol_info);
            } else {
                continue;
            }
        }
        None
    }

    // node_id means the node id of the function identifier
    fn record_frame_location_in_analysis(&mut self, node_id: String, definition_depth: usize) {
        self.analysis.node_id_attributes.insert(
            node_id,
            IdentifierAttributes::Function(FunctionAttributes {
                definition_depth,
                // Counted once the function scope is popped
                memory_units_consumed: 0,
            }),
        );
    }

    fn allocate_slot_for_variable_in_current_frame(
        &mut self,
        identifier: &Identifier,
        var_type: SpannedVariableType,
    ) {
        let depth = self.get_current_frame_depth();
        let enclosing_frame = self.get_current_frame();
        let ScopeType::Function {
            memory_units_consumed,
            ..
        } = &mut enclosing_frame.scope_type
        else {
            panic!("Enclosing frame must be a function scope")
        };
        let previously_consumed = *memory_units_consumed;
        *memory_units_consumed += var_type.size();
        self.analysis.node_id_attributes.insert(
            identifier.node_id.clone(),
            IdentifierAttributes::Variable(VariableAttributes {
                frame_depth: depth,
                offset_from_frame_base: previously_consumed,
                var_type,
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
            memory_units_consumed: variable_slot_count,
            ..
        } = scope.scope_type
        else {
            return;
        };

        let popped_fn_attributes = self
            .analysis
            .must_get_function_attributes_from_node_id(&frame_id);
        popped_fn_attributes.memory_units_consumed = variable_slot_count;
    }

    fn add_debug_builtins(&mut self) {
        // DEBUG: Just add `bol` builtin so analyzer won't complain
        self.get_current_scope().add_function_identifier(
            &Identifier {
                name: String::from("bol"),
                node_id: String::from(__DEBUG_BOL_FUNCTION_NODE_ID),
                span: Span::new(0, 0),
            },
            vec![SpannedVariableType {
                span: Span::new(0, 0),
                var_type: IntegerType,
            }],
            SpannedVariableType {
                span: Span::new(0, 0),
                var_type: VariableType::VoidType,
            },
        );
        self.record_frame_location_in_analysis(String::from(__DEBUG_BOL_FUNCTION_NODE_ID), 0);

        // Add bol for bool type as well
        self.get_current_scope().add_function_identifier(
            &Identifier {
                name: String::from("bolbool"),
                node_id: String::from(__DEBUG_BOL_BOOL_FUNCTION_NODE_ID),
                span: Span::new(0, 0),
            },
            vec![SpannedVariableType {
                span: Span::new(0, 0),
                var_type: BooleanType,
            }],
            SpannedVariableType {
                span: Span::new(0, 0),
                var_type: VariableType::VoidType,
            },
        );
        self.record_frame_location_in_analysis(String::from(__DEBUG_BOL_BOOL_FUNCTION_NODE_ID), 0);
    }

    pub fn analyse(mut self, ast: &AST) -> Result<Analysis, Vec<Diagnostic>> {
        self.record_frame_location_in_analysis(String::from(MAIN_FUNCTION_NODE_ID), 0);
        self.push_scope(Scope::new(ScopeType::Function {
            node_id: String::from(MAIN_FUNCTION_NODE_ID),
            memory_units_consumed: 0,
            return_type_classification: VariableType::IntegerType,
        }));

        self.add_debug_builtins();

        // Pass 1
        // Process functions signatures first, so functions can be used before their definitions are set in
        for statement in &ast.block.statements {
            if let StatementKind::Function(FunctionDefinition {
                identifier,
                parameters,
                return_type,
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
                        parameter_types: parameters
                            .iter()
                            .map(|p| p.spanned_var_type.clone())
                            .collect(),
                        return_type: return_type.clone(),
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
            StatementKind::Declaration {
                identifier,
                value,
                spanned_var_type,
            } => {
                let expr_type = self.analyse_expression(value)?;
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

                let var_type_classification = spanned_var_type.var_type.clone();

                if var_type_classification == VoidType {
                    return Err(Diagnostic::error(
                        identifier.span,
                        "SEMA:: `void` is not assignable".to_string(),
                    ));
                }

                if !expr_type.is_convertible_to(&spanned_var_type.var_type) {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!(
                            "SEMA:: Cannot assign {expr_type} to a variable of type {var_type_classification}"
                        ),
                    ));
                }

                self.get_current_scope()
                    .add_variable_identifier(identifier, spanned_var_type.clone());
                self.allocate_slot_for_variable_in_current_frame(
                    identifier,
                    spanned_var_type.clone(),
                );
                Ok(())
            }
            StatementKind::Return(expression) => {
                if self.get_current_frame_depth() == 0 {
                    return Err(Diagnostic::error(
                        expression.span,
                        "`de` must be used inside a `dhanda`",
                    ));
                }

                let expr_classficiation = self.analyse_expression(expression)?;
                let enclosing_function_scope = self.get_current_frame();
                let ScopeType::Function {
                    return_type_classification,
                    ..
                } = &enclosing_function_scope.scope_type
                else {
                    panic!("Enclosing function scope must be a function scope");
                };

                if *return_type_classification != expr_classficiation {
                    return Err(Diagnostic::error(
                        expression.span,
                        format!(
                            "Expected return type to be {}, got {}",
                            return_type_classification, expr_classficiation,
                        ),
                    ));
                }
                Ok(())
            }
            StatementKind::Function(FunctionDefinition {
                identifier,
                body,
                parameters,
                return_type,
                ..
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

                self.get_current_scope().add_function_identifier(
                    identifier,
                    parameters
                        .iter()
                        .map(|p| p.spanned_var_type.clone())
                        .collect(),
                    return_type.clone(),
                );

                // We should allow recursive calls, so register early
                self.record_frame_location_in_analysis(
                    identifier.node_id.clone(),
                    self.get_current_frame_depth(),
                );

                self.push_scope(Scope::new(ScopeType::Function {
                    node_id: identifier.node_id.clone(),
                    memory_units_consumed: 0,
                    return_type_classification: return_type.var_type.clone(),
                }));
                for parameter in parameters {
                    let parameter_name = parameter.identifier.name.clone();
                    let param_type = parameter.spanned_var_type.clone();
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
                    self.get_current_scope()
                        .add_variable_identifier(&parameter.identifier, param_type.clone());
                    self.allocate_slot_for_variable_in_current_frame(
                        &parameter.identifier,
                        param_type,
                    );
                }
                self.get_current_scope().add_function_identifier(
                    identifier,
                    parameters
                        .iter()
                        .map(|t| t.spanned_var_type.clone())
                        .collect(),
                    return_type.clone(),
                );
                self.analyse_statements_collecting_diagnostics(&body.statements);

                // If a function has a non-void return type
                // but if any of the control flow paths does not produce
                // a return statement, emit an error
                // Type checking is not needed here since it's captured in the
                // analysis of each return statement
                if !self.does_block_definitely_return(body)
                    && return_type.var_type != VariableType::VoidType
                {
                    // C style error diagnostic
                    let body_end_span = body.span.end;
                    return Err(Diagnostic::error(
                        Span::new(body_end_span, body_end_span),
                        "non-void function does not return a value in all control paths",
                    ));
                }
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
            StatementKind::Expression(expression) => {
                self.analyse_expression(expression)?;
                Ok(())
            }
        }
    }

    fn analyse_expression(&mut self, expression: &Expression) -> Result<VariableType, Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let Some(symbol_info) = self.lookup_syminfo_for_identifier(identifier.name.clone())
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
                    SymbolInfo::Variable(var_symbol_info) => {
                        let declaration_node_id = var_symbol_info.identifier.node_id.clone();
                        let declaration_classification = var_symbol_info.get_classification();
                        self.analysis
                            .use_site_node_id_resolution_map
                            .insert(identifier.node_id.clone(), declaration_node_id);
                        Ok(declaration_classification)
                    }
                }
            }
            ExpressionKind::Integer(..) => Ok(IntegerType),
            ExpressionKind::UnaryOperation { operand, .. } => self.analyse_expression(operand),
            ExpressionKind::BinaryOperation { left, right, .. } => {
                let left_type = self.analyse_expression(left)?;
                let right_type = self.analyse_expression(right)?;
                if left_type != right_type {
                    panic!("Coercion is not supported at the moment")
                }
                Ok(left_type)
            }
            ExpressionKind::Assignment { identifier, value } => {
                let expr_classification = self.analyse_expression(value)?;
                let Some(symbol_info) = self.lookup_syminfo_for_identifier(identifier.name.clone())
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
                    SymbolInfo::Variable(variable_sym_info) => {
                        let resolved_identifier = variable_sym_info.identifier.clone();
                        let target_type = variable_sym_info.var_type.clone();

                        if variable_sym_info.var_type.var_type != expr_classification {
                            return Err(Diagnostic::error(
                                resolved_identifier.span,
                                format!(
                                    "Cannot assign expression of type {expr_classification} to variable {resolved_identifier} of type {target_type}"
                                ),
                            ));
                        }

                        self.analysis.use_site_node_id_resolution_map.insert(
                            identifier.node_id.clone(),
                            resolved_identifier.node_id.clone(),
                        );
                    }
                }
                Ok(expr_classification)
            }
            ExpressionKind::Call { callee, arguments } => {
                let function_name = callee.name.clone();
                let return_value_type: VariableType;

                // Scope stack takes priority over lookahead definitions
                let (definition_node_id, param_types) =
                    match self.lookup_syminfo_for_identifier(function_name.clone()) {
                        Some(SymbolInfo::Variable(..)) => {
                            return Err(Diagnostic::error(
                                expression.span,
                                "SEMA:: Called object is not a function",
                            ));
                        }
                        Some(SymbolInfo::Function(FunctionSymbolInfo {
                            identifier,
                            parameters_types,
                            return_type,
                        })) => {
                            return_value_type = return_type.var_type.clone();
                            (identifier.node_id.clone(), parameters_types.clone())
                        }
                        // Not in scope yet, so it must be defined further down
                        None => match self
                            .lookahead_function_definitions
                            .iter()
                            .find(|fn_definition| fn_definition.identifier.name == function_name)
                        {
                            Some(fn_definition) => {
                                return_value_type = fn_definition.return_type.var_type.clone();
                                (
                                    fn_definition.identifier.node_id.clone(),
                                    fn_definition.parameter_types.clone(),
                                )
                            }
                            None => {
                                return Err(Diagnostic::error(
                                    callee.span,
                                    format!("SEMA:: Usage of undefined function {function_name}"),
                                ));
                            }
                        },
                    };

                let fn_arity = param_types.len();
                if fn_arity != arguments.len() {
                    let few_many = if arguments.len() > fn_arity {
                        "many"
                    } else {
                        "few"
                    };

                    let error_pointer_span = expression.span.end - 1;
                    return Err(Diagnostic::error(
                        Span::new(error_pointer_span, error_pointer_span),
                        format!("SEMA:: too {few_many} arguments in call to `{function_name}`"),
                    ));
                }

                // Compare the types
                for i in 0..fn_arity {
                    let arg = &arguments[i];
                    let arg_type = self.analyse_expression(arg)?;
                    let param_type = &param_types[i];
                    if arg_type != param_type.var_type {
                        return Err(Diagnostic::error(
                            arg.span,
                            format!(
                                "SEMA:: Expected argument to be of type {}, got {}",
                                param_type, arg_type
                            ),
                        ));
                    }
                }

                self.analysis
                    .use_site_node_id_resolution_map
                    .insert(callee.node_id.clone(), definition_node_id);

                Ok(return_value_type)
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

    /* Return paths analysis */

    fn does_conditional_definitely_return(&mut self, c: &Conditional) -> bool {
        // An if statement without an else can't be distinguished as
        // definitely returnable
        let Some(else_block) = &c.otherwise else {
            return false;
        };

        match else_block.deref() {
            Else::If(conditional) => self.does_conditional_definitely_return(conditional),
            Else::Block(b) => self.does_block_definitely_return(b),
        }
    }

    fn does_stmt_definitely_return(&mut self, stmt: &Statement) -> bool {
        match &stmt.kind {
            StatementKind::Declaration { .. } => false,
            StatementKind::Return(..) => true,
            StatementKind::Expression(..) => false,
            StatementKind::Function(..) => false,
            StatementKind::If(conditional) => self.does_conditional_definitely_return(conditional),
            StatementKind::While { .. } => false,
            StatementKind::For { .. } => false,
        }
    }

    fn does_block_definitely_return(&mut self, block: &Block) -> bool {
        for statement in &block.statements {
            if self.does_stmt_definitely_return(statement) {
                return true;
            }
        }
        false
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
    fn a_program_that_is_only_a_runner_function() {
        accepts("dhanda run() { }");
    }

    #[test]
    fn a_variable_is_visible_after_its_declaration() {
        accepts("dhanda run() { thoos x: int = 1; x; }");
    }

    #[test]
    fn a_top_level_variable_is_visible_inside_a_function() {
        accepts("thoos g: int = 1; dhanda run() { g; }");
    }

    #[test]
    fn a_parameter_is_visible_inside_the_body() {
        accepts("dhanda f(a: int) { a; }");
    }

    #[test]
    fn a_call_whose_argument_count_matches() {
        accepts("dhanda add(a: int, b: int) int { de 1; } dhanda run() { add(1, 2); }");
    }

    #[test]
    fn a_function_may_call_itself() {
        accepts("dhanda fac(n: int) int { de fac(1); }");
    }

    #[test]
    fn a_function_may_call_one_defined_later() {
        accepts("dhanda a() int { de b(); } dhanda b() int { de 1; }");
    }

    #[test]
    fn two_top_level_functions_may_call_each_other() {
        accepts(
            "dhanda is_even(n: int) int { de is_odd(n - 1); } \
             dhanda is_odd(n: int) int { de is_even(n - 1); }",
        );
    }

    #[test]
    fn a_runner_may_call_a_function_defined_below_it() {
        accepts("dhanda run() { bol(later()); } dhanda later() int { de 1; }");
    }

    #[test]
    fn a_forward_call_still_has_its_arity_checked() {
        rejects(
            "dhanda run() { add(1); } dhanda add(a: int, b: int) int { de a + b; }",
            "too few arguments",
        );
    }

    #[test]
    fn a_builtin_is_callable_from_inside_a_function() {
        accepts("dhanda run() { bol(1); }");
    }

    #[test]
    fn an_inner_block_may_reuse_an_outer_name() {
        accepts("dhanda run() { thoos x: int = 1; yedi (1) { thoos x: int = 2; } }");
    }

    #[test]
    fn a_dhanda_may_be_declared_inside_a_dhanda() {
        accepts("dhanda run() { dhanda inner() int { de 1; } }");
    }

    #[test]
    fn a_dhanda_may_be_declared_inside_a_block() {
        accepts("dhanda run() { yedi (1) { dhanda inner() int { de 1; } } }");
        accepts("dhanda run() { jabasamma (1) { dhanda inner() int { de 1; } } }");
    }

    #[test]
    fn a_nested_dhanda_is_callable_after_its_definition() {
        accepts("dhanda run() { dhanda inner(a: int) int { de a; } bol(inner(1)); }");
    }

    #[test]
    fn a_nested_dhanda_may_call_itself() {
        accepts("dhanda run() { dhanda inner(n: int) int { de inner(n - 1); } }");
    }

    #[test]
    fn a_nested_dhanda_may_read_its_enclosing_dhandas_variables() {
        accepts("dhanda run() { thoos x: int = 1; dhanda inner() int { de x; } }");
        accepts("dhanda run() { dhanda outer(a: int) { dhanda inner() int { de a; } } }");
    }

    #[test]
    fn a_nested_dhanda_may_call_a_global_dhanda() {
        accepts("dhanda run() { dhanda inner() int { de later(); } } dhanda later() int { de 1; }");
    }

    #[test]
    fn a_nested_dhanda_may_shadow_a_global_dhanda() {
        accepts("dhanda f() int { de 1; } dhanda run() { dhanda f(a: int) int { de a; } f(1); }");
    }

    #[test]
    fn two_dhandas_may_each_nest_a_dhanda_of_the_same_name() {
        accepts(
            "dhanda run() { dhanda inner() int { de 1; } } \
             dhanda other() { dhanda inner() int { de 2; } }",
        );
    }

    #[test]
    fn a_void_function_may_fall_off_the_end() {
        accepts("dhanda nothing() { } nothing();");
        accepts(
            "dhanda after_while(n: int) { \
             thoos i: int = 0; jabasamma (i < n) { i = i + 1; } bol(i); } \
             after_while(4);",
        );
    }

    #[test]
    fn a_non_void_function_that_returns_on_every_path() {
        accepts("dhanda both_arms(n: int) int { yedi (n > 0) { de 1; } natra { de 2; } }");
        accepts(
            "dhanda sign(n: int) int { yedi (n > 0) { de 1; } natra yedi (n < 0) { de -1; } natra { de 0; } }",
        );
        accepts("dhanda only_positive(n: int) int { yedi (n > 0) { de n; } de 0; }");
    }

    /* ------------------------------------------------------------ rejected */

    #[test]
    fn a_nested_dhanda_is_not_visible_outside_its_enclosing_dhanda() {
        rejects(
            "dhanda run() { inner(); } dhanda other() { dhanda inner() int { de 1; } }",
            "inner",
        );
    }

    #[test]
    fn a_nested_dhanda_is_not_visible_outside_its_block() {
        rejects(
            "dhanda run() { yedi (1) { dhanda inner() int { de 1; } } inner(); }",
            "inner",
        );
    }

    #[test]
    fn two_nested_dhandas_with_the_same_name_in_one_scope() {
        rejects(
            "dhanda run() { dhanda inner() int { de 1; } dhanda inner() int { de 2; } }",
            "already been declared",
        );
    }

    #[test]
    fn a_nested_dhanda_and_a_variable_with_the_same_name_in_one_scope() {
        rejects(
            "dhanda run() { thoos inner: int = 1; dhanda inner() int { de 1; } }",
            "already been declared",
        );
    }

    #[test]
    fn a_nested_dhanda_still_has_its_arity_checked() {
        rejects(
            "dhanda run() { dhanda inner(a: int, b: int) int { de a; } inner(1); }",
            "too few arguments",
        );
    }

    #[test]
    fn a_program_without_a_runner_is_accepted() {
        accepts("thoos x: int = 1;");
    }

    #[test]
    fn two_declarations_of_one_name_in_the_same_scope() {
        rejects(
            "thoos x: int = 1; thoos x: int = 2;",
            "already been declared",
        );
    }

    #[test]
    fn two_functions_with_the_same_name() {
        rejects(
            "dhanda f() int { de 1; } dhanda f() int { de 2; }",
            "already been defined",
        );
    }

    #[test]
    fn a_parameter_name_repeated() {
        rejects(
            "dhanda f(a: int, a: int) int { de 1; }",
            "already been declared",
        );
    }

    #[test]
    fn an_undefined_name_on_its_own() {
        rejects("dhanda run() { nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_declaration_value() {
        rejects("dhanda run() { thoos x: int = nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_binary_operation() {
        rejects("dhanda run() { thoos x: int = nope + 1; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_unary_operation() {
        rejects("dhanda run() { thoos x: int = -nope; }", "nope");
    }

    #[test]
    fn a_global_may_be_initialised_from_another_global() {
        accepts("thoos a: int = 1; thoos b: int = a;");
        accepts("thoos a: int = 1; thoos b: int = a + 1;");
    }

    #[test]
    fn an_undefined_name_as_an_assignment_target() {
        rejects("dhanda run() { nope = 1; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_an_assigned_value() {
        rejects("dhanda run() { thoos x: int = 1; x = nope; }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_conditional() {
        rejects("dhanda run() { yedi (nope) { 1; } }", "nope");
        rejects("dhanda run() { yedi (1) { nope; } }", "nope");
        rejects("dhanda run() { yedi (1) { 1; } natra { nope; } }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_while_loop() {
        rejects("dhanda run() { jabasamma (nope) { 1; } }", "nope");
        rejects("dhanda run() { jabasamma (1) { nope; } }", "nope");
    }

    #[test]
    fn an_undefined_name_in_a_for_loop() {
        rejects(
            "dhanda run() { fanfani (thoos i: int = 0; 1; i) { nope; } }",
            "nope",
        );
        rejects(
            "dhanda run() { fanfani (thoos i: int = 0; nope; i) { 1; } }",
            "nope",
        );
    }

    #[test]
    fn a_call_to_an_undefined_function() {
        rejects("dhanda run() { nope(1); }", "nope");
    }

    #[test]
    fn a_call_with_too_few_arguments() {
        rejects(
            "dhanda add(a: int, b: int) int { de 1; } dhanda run() { add(1); }",
            "too few arguments",
        );
    }

    #[test]
    fn a_call_with_too_many_arguments() {
        rejects(
            "dhanda add(a: int, b: int) int { de 1; } dhanda run() { add(1, 2, 3); }",
            "too many arguments",
        );
    }

    #[test]
    fn a_call_to_something_that_is_not_a_function() {
        rejects("dhanda run() { thoos x: int = 1; x(); }", "not a function");
    }

    #[test]
    fn any_statement_is_allowed_at_the_top_level() {
        accepts("bol(1);");
        accepts("1 + 1;");
        accepts("yedi (1) { 1; }");
        accepts("jabasamma (1) { 1; }");
        accepts("fanfani (thoos i: int = 0; 1; i) { 1; }");
    }

    #[test]
    fn a_variable_used_before_the_line_that_declares_it() {
        rejects("dhanda run() { later; thoos later: int = 1; }", "later");
    }

    #[test]
    fn a_return_outside_any_function() {
        rejects("de 1;", "inside a `dhanda`");
    }

    #[test]
    fn a_for_loop_variable_does_not_outlive_the_loop() {
        rejects(
            "dhanda run() { fanfani (thoos i: int = 0; 1; i) { 1; } i; }",
            "i",
        );
    }

    #[test]
    fn a_void_call_passed_as_an_int_argument() {
        rejects("dhanda nothing() { } bol(nothing());", "got void");
        rejects(
            "dhanda prints_only(a: int) { bol(a); } bol(prints_only(3));",
            "got void",
        );
    }

    #[test]
    fn a_void_call_assigned_to_an_int_variable() {
        rejects(
            "dhanda nothing() { } thoos t: int = nothing();",
            "Cannot assign",
        );
    }

    #[test]
    fn a_non_void_function_with_no_return_at_all() {
        rejects(
            "dhanda nothing() int { }",
            "does not return a value in all control paths",
        );
    }

    #[test]
    fn a_non_void_function_that_returns_only_inside_a_conditional() {
        rejects(
            "dhanda only_positive(n: int) int { yedi (n > 0) { de n; } }",
            "does not return a value in all control paths",
        );
    }

    #[test]
    fn a_non_void_function_that_returns_only_inside_a_loop() {
        rejects(
            "dhanda first_match(limit: int) int { \
             fanfani (thoos i: int = 0; i < 10; i = i + 1) { yedi (i == limit) { de i; } } }",
            "does not return a value in all control paths",
        );
        rejects(
            "dhanda first_over(limit: int) int { \
             thoos i: int = 0; jabasamma (i < 100) { yedi (i > limit) { de i; } i = i + 1; } }",
            "does not return a value in all control paths",
        );
    }

    /* --------------------------------------------- more than one diagnostic */

    #[test]
    fn reports_every_broken_statement_in_one_pass() {
        let diagnostics = analyse("dhanda a() int { de x; } dhanda b() int { de y; }")
            .expect_err("should be rejected");

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("x"));
        assert!(diagnostics[1].message.contains("y"));
    }

    // A `fanfani` pushes the scope for its parentheses before analysing its
    // initialiser, so a failing initialiser leaves that scope behind. If it is
    // not cleaned up, every later statement is analysed one scope too deep, so
    // the `run` defined after it lands outside the global scope and is not found.
    #[test]
    fn a_failed_for_loop_does_not_leave_its_scope_behind() {
        let diagnostics = analyse(
            "dhanda broken() { fanfani (thoos i: int = missing; 1; i) { 1; } } dhanda run() { }",
        )
        .expect_err("should be rejected");

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing"));
    }
}
