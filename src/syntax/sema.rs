// Semantic analyzer
use std::{collections::HashMap, ops::Deref};

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AST, Block, Conditional, Else, Expression, ExpressionKind, FunctionDefinition, Identifier,
        Statement,
        StatementKind::{self},
        ast::{
            DataType::{self, IntegerType, VoidType},
            FunctionParameter, NodeId, SpannedDataType,
        },
    },
};

pub type MemoryUnits = usize;

pub const MAIN_FUNCTION_NODE_ID: &str = "_main";

/********* ANALYSIS ********* */

#[derive(Debug)]
pub struct VariableLayout {
    pub parent_frame_depth: usize,
    pub offset_from_parent_frame_base: MemoryUnits,
    pub data_type: SpannedDataType,
}

#[derive(Debug)]
pub struct FunctionLayout {
    // Depth of the frame the function is defined in, not of its own frame
    pub parent_frame_depth: usize,
    pub locals_occupied_stack_size: MemoryUnits,
}

// Layout describes where a particular variable or function sits
// These information should be enough to determine the starting point
// and ending point of a function/variable in the memory
#[derive(Debug)]
pub enum IdentifierLayout {
    Variable(VariableLayout),
    Function(FunctionLayout),
}

#[derive(Debug, Default)]
pub struct Analysis {
    pub node_id_resolution_map: HashMap<NodeId, NodeId>,
    pub node_id_layout: HashMap<NodeId, IdentifierLayout>,
}

impl Analysis {
    pub fn must_get_variable_layout(&self, node_id: &NodeId) -> &VariableLayout {
        let Some(IdentifierLayout::Variable(location)) = self.node_id_layout.get(node_id) else {
            panic!("Variable with node id {node_id} not found");
        };

        location
    }

    pub fn must_get_function_layout(&mut self, node_id: &NodeId) -> &mut FunctionLayout {
        let Some(IdentifierLayout::Function(location)) = self.node_id_layout.get_mut(node_id)
        else {
            panic!("Function with node id {node_id} not found");
        };

        location
    }

    pub fn must_resolve(&self, user_node_id: &NodeId) -> &NodeId {
        self.node_id_resolution_map
            .get(user_node_id)
            .expect("Caller node id must have been resolved")
    }
}

/******* Scope and symbol table ****** */
#[derive(Clone)]
struct FunctionSymbolAttribute {
    identifier: Identifier,
    param_types: Vec<SpannedDataType>,
    return_type: SpannedDataType,
}

#[derive(Clone)]
struct VariableSymbolAttribute {
    identifier: Identifier,
    data_type: SpannedDataType,
}

impl VariableSymbolAttribute {
    fn get_data_type(&self) -> &DataType {
        &self.data_type.data_type
    }
}

#[derive(Clone)]
enum SymbolAttribute {
    Function(FunctionSymbolAttribute),
    Variable(VariableSymbolAttribute),
}

#[derive(Debug)]
enum ScopeType {
    Function {
        node_id: NodeId,
        locals_occupied_stack_size: MemoryUnits,
        return_type: DataType,
    },
    Block,
}

struct Scope {
    // Symbol table for a given scope
    // maps the identifier name to its attributes
    pub sym_table: HashMap<String, SymbolAttribute>,
    scope_type: ScopeType,
}

impl Scope {
    fn new(scope_type: ScopeType) -> Self {
        Self {
            sym_table: HashMap::new(),
            scope_type,
        }
    }

    fn add_variable_identifier(&mut self, identifier: &Identifier, data_type: &SpannedDataType) {
        self.sym_table.insert(
            identifier.name.clone(),
            SymbolAttribute::Variable(VariableSymbolAttribute {
                identifier: identifier.clone(),
                data_type: data_type.clone(),
            }),
        );
    }

    fn add_function_identifier(
        &mut self,
        identifier: &Identifier,
        fn_parameters_types: &[SpannedDataType],
        return_type: &SpannedDataType,
    ) {
        self.sym_table.insert(
            identifier.name.clone(),
            SymbolAttribute::Function(FunctionSymbolAttribute {
                identifier: identifier.clone(),
                param_types: fn_parameters_types.to_vec(),
                return_type: return_type.clone(),
            }),
        );
    }

    fn contains_identifier(&self, identifier_name: &String) -> bool {
        self.sym_table.contains_key(identifier_name)
    }
}

pub struct Sema {
    scopes_stack: Vec<Scope>,
    analysis: Analysis,
    diagnostics: Vec<Diagnostic>,
    lookahead_function_attrs: Vec<FunctionSymbolAttribute>,
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
            lookahead_function_attrs: Vec::new(),
            analysis: Default::default(),
            diagnostics: Vec::new(),
        }
    }

    fn must_get_current_scope_mut(&mut self) -> &mut Scope {
        self.scopes_stack
            .last_mut()
            .expect("At least one scope must exist during semantic analysis")
    }

    fn must_get_current_frame_mut(&mut self) -> &mut Scope {
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

    fn lookup_symattr_in_scope_stack(
        &self,
        identifier: &Identifier,
    ) -> Result<&SymbolAttribute, Diagnostic> {
        for stack in self.scopes_stack.iter().rev() {
            if let Some(symbol_info) = stack.sym_table.get(&identifier.name) {
                return Ok(symbol_info);
            } else {
                continue;
            }
        }
        Err(Diagnostic::error(
            identifier.span,
            format!(
                "SEMA:: Usage of undefined identifier {}",
                identifier.name
            ),
        ))
    }

    // Adds the given variable (identifier + data type) to the current scope, adjusts the current
    // frame size, and records the variable layout in analysis
    fn record_variable_definition(
        &mut self,
        identifier: &Identifier,
        spanned_data_type: &SpannedDataType,
    ) {
        self.must_get_current_scope_mut()
            .add_variable_identifier(identifier, spanned_data_type);

        let enclosing_frame = self.must_get_current_frame_mut();
        let ScopeType::Function {
            locals_occupied_stack_size,
            ..
        } = &mut enclosing_frame.scope_type
        else {
            panic!("Enclosing frame must be a function scope")
        };
        let previously_occupied = *locals_occupied_stack_size;
        *locals_occupied_stack_size += spanned_data_type.size();

        let depth = self.get_current_frame_depth();
        self.analysis.node_id_layout.insert(
            identifier.node_id.clone(),
            IdentifierLayout::Variable(VariableLayout {
                parent_frame_depth: depth,
                offset_from_parent_frame_base: previously_occupied,
                data_type: spanned_data_type.clone(),
            }),
        );
    }

    fn record_function_definition(
        &mut self,
        identifier: &Identifier,
        parameters: &[FunctionParameter],
        return_type: &SpannedDataType,
    ) {
        self.must_get_current_scope_mut().add_function_identifier(
            identifier,
            &parameters
                .iter()
                .map(|p| p.spanned_data_type.clone())
                .collect::<Vec<_>>(),
            return_type,
        );

        self.analysis.node_id_layout.insert(
            identifier.node_id.clone(),
            IdentifierLayout::Function(FunctionLayout {
                parent_frame_depth: self.get_current_frame_depth(),
                // Counted once the function scope is popped
                locals_occupied_stack_size: 0,
            }),
        );
    }

    fn record_resolution(&mut self, usage_node_id: &NodeId, defn_node_id: &NodeId) {
        self.analysis
            .node_id_resolution_map
            .insert(usage_node_id.clone(), defn_node_id.clone());
    }

    fn push_scope(&mut self, scope: Scope) {
        self.scopes_stack.push(scope);
    }

    fn pop_scope(&mut self) {
        let popped_scope = self
            .scopes_stack
            .pop()
            .expect("A scope must exist to be popped");

        let ScopeType::Function {
            node_id,
            locals_occupied_stack_size,
            ..
        } = popped_scope.scope_type
        else {
            return;
        };

        let fn_layout_from_analysis = self.analysis.must_get_function_layout(&node_id);
        fn_layout_from_analysis.locals_occupied_stack_size = locals_occupied_stack_size;
    }

    pub fn analyse(mut self, ast: &AST) -> Result<Analysis, Vec<Diagnostic>> {
        // Manually instead of record_function_definition
        // because no scopes are setup at entry point
        let main_fn_node_id = String::from(MAIN_FUNCTION_NODE_ID);
        self.push_scope(Scope::new(ScopeType::Function {
            node_id: main_fn_node_id.clone(),
            locals_occupied_stack_size: 0,
            return_type: DataType::IntegerType,
        }));
        self.analysis.node_id_layout.insert(
            main_fn_node_id,
            IdentifierLayout::Function(FunctionLayout {
                parent_frame_depth: 0,
                locals_occupied_stack_size: 0,
            }),
        );

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
                    .lookahead_function_attrs
                    .iter()
                    .any(|defn| defn.identifier.name == identifier.name)
                {
                    self.diagnostics.push(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: dhanda `{function_name} has already been defined`"),
                    ));
                    continue;
                }

                self.lookahead_function_attrs.push(FunctionSymbolAttribute {
                    identifier: identifier.clone(),
                    param_types: parameters
                        .iter()
                        .map(|p| p.spanned_data_type.clone())
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
                spanned_data_type,
            } => {
                let expr_type = self.analyse_expression(value)?;
                let identifier_name = identifier.name.clone();

                if self
                    .must_get_current_scope_mut()
                    .contains_identifier(&identifier_name)
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Identifier {identifier_name} has already been declared"),
                    ));
                }

                let specified_type = &spanned_data_type.data_type;

                if *specified_type == VoidType {
                    return Err(Diagnostic::error(
                        identifier.span,
                        "SEMA:: `void` is not assignable".to_string(),
                    ));
                }

                if !expr_type.is_convertible_to(specified_type) {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!(
                            "SEMA:: Cannot assign {expr_type} to a variable of type {specified_type}"
                        ),
                    ));
                }

                self.record_variable_definition(identifier, spanned_data_type);
                Ok(())
            }
            StatementKind::Return(expression) => {
                if self.get_current_frame_depth() == 0 {
                    return Err(Diagnostic::error(
                        expression.span,
                        "`de` must be used inside a `dhanda`",
                    ));
                }

                let expr_data_type = self.analyse_expression(expression)?;
                let enclosing_frame = self.must_get_current_frame_mut();

                let ScopeType::Function {
                    return_type: return_type_classification,
                    ..
                } = &enclosing_frame.scope_type
                else {
                    panic!("Enclosing function scope must be a function scope");
                };

                if *return_type_classification != expr_data_type {
                    return Err(Diagnostic::error(
                        expression.span,
                        format!(
                            "Expected return type to be {}, got {}",
                            return_type_classification, expr_data_type,
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
                let function_name = &identifier.name;

                if self
                    .must_get_current_scope_mut()
                    .contains_identifier(function_name)
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        format!("SEMA:: Identifier {function_name} has already been declared"),
                    ));
                }

                self.record_function_definition(identifier, parameters, return_type);

                self.push_scope(Scope::new(ScopeType::Function {
                    node_id: identifier.node_id.clone(),
                    locals_occupied_stack_size: 0,
                    return_type: return_type.data_type.clone(),
                }));

                for param in parameters {
                    let param_identifier = &param.identifier;
                    let param_type = &param.spanned_data_type;
                    // Each parameter identifier must be unique
                    if self
                        .must_get_current_scope_mut()
                        .contains_identifier(&param_identifier.name)
                    {
                        return Err(Diagnostic::error(
                            param.span,
                            format!(
                                "SEMA:: Identifier {param_identifier} has already been declared",
                            ),
                        ));
                    }

                    self.record_variable_definition(param_identifier, param_type);
                }

                self.analyse_statements_collecting_diagnostics(&body.statements);

                // If a function has a non-void return type
                // but if any of the control flow paths does not produce
                // a return statement, emit an error
                // Type checking is not needed here since it's captured in the
                // analysis of each return statement
                if !self.does_block_definitely_return(body)
                    && return_type.data_type != DataType::VoidType
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

    fn analyse_expression(&mut self, expression: &Expression) -> Result<DataType, Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let symattr = self.lookup_symattr_in_scope_stack(identifier)?.clone();

                match symattr {
                    SymbolAttribute::Function(..) => Err(Diagnostic::error(
                        expression.span,
                        format!("Identifier {identifier} is a `dhanda`, not a `variable`"),
                    )),
                    SymbolAttribute::Variable(var_symbol_info) => {
                        let definition_datatype = var_symbol_info.get_data_type().clone();
                        self.record_resolution(
                            &identifier.node_id,
                            &var_symbol_info.identifier.node_id,
                        );
                        Ok(definition_datatype.clone())
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
                let expr_type = self.analyse_expression(value)?;
                let symattr = self.lookup_symattr_in_scope_stack(identifier)?;
                match symattr {
                    SymbolAttribute::Function(..) => {
                        return Err(Diagnostic::error(
                            identifier.span,
                            format!("Cannot assign to {identifier} of `dhanda` type"),
                        ));
                    }
                    SymbolAttribute::Variable(var_symattr) => {
                        let defn_identifier = var_symattr.identifier.clone();
                        let defn_datatype = var_symattr.data_type.clone();

                        if var_symattr.data_type.data_type != expr_type {
                            return Err(Diagnostic::error(
                                defn_identifier.span,
                                format!(
                                    "Cannot assign expression of type {expr_type} to variable {defn_identifier} of type {defn_datatype}"
                                ),
                            ));
                        }

                        self.record_resolution(&identifier.node_id, &defn_identifier.node_id);
                    }
                }
                Ok(expr_type)
            }
            ExpressionKind::Call { callee, arguments } => {
                let fn_defn_sym_attr: Option<FunctionSymbolAttribute>;

                // Search for the function symbol attribute
                // First, from the scope stack, and then from lookahead definitions
                let mut sym_attr_from_scope_stack: Option<&SymbolAttribute> = None;
                for stack in self.scopes_stack.iter().rev() {
                    if let Some(sym_attr) = stack.sym_table.get(&callee.name) {
                        sym_attr_from_scope_stack = Some(sym_attr);
                        break;
                    } else {
                        continue;
                    }
                }

                if let Some(sym_attr_from_scope_stack) = sym_attr_from_scope_stack {
                    match sym_attr_from_scope_stack {
                        SymbolAttribute::Variable(..) => {
                            return Err(Diagnostic::error(
                                callee.span,
                                "SEMA:: Called object is not a function",
                            ));
                        }
                        SymbolAttribute::Function(fn_attr) => {
                            fn_defn_sym_attr = Some(fn_attr.clone());
                        }
                    }
                } else {
                    let fn_attr = self
                        .lookahead_function_attrs
                        .iter()
                        .find(|fn_attr| fn_attr.identifier.name == callee.name);
                    match fn_attr {
                        Some(fn_attr) => {
                            fn_defn_sym_attr = Some(fn_attr.clone());
                        }
                        None => {
                            return Err(Diagnostic::error(
                                callee.span,
                                format!("SEMA:: Usage of undefined function {}", callee),
                            ));
                        }
                    }
                }

                let Some(fn_defn_sym_attr) = fn_defn_sym_attr else {
                    panic!("SEMA:: Should have been a Some() by now")
                };

                let FunctionSymbolAttribute {
                    identifier,
                    return_type,
                    param_types,
                } = fn_defn_sym_attr;

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
                        format!("SEMA:: too {few_many} arguments in call to `{callee}`"),
                    ));
                }

                for i in 0..fn_arity {
                    let arg = &arguments[i];
                    let arg_type = self.analyse_expression(arg)?;
                    let param_type = &param_types[i];
                    if arg_type != param_type.data_type {
                        return Err(Diagnostic::error(
                            arg.span,
                            format!(
                                "SEMA:: Expected argument to be of type {}, got {}",
                                param_type, arg_type
                            ),
                        ));
                    }
                }

                self.record_resolution(&callee.node_id, &identifier.node_id);

                Ok(return_type.data_type.clone())
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
