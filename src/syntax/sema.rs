// Semantic analyzer
use std::collections::HashMap;

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        Expression, ExpressionKind, FunctionDefinition, Identifier, Program, Statement,
        StatementKind,
    },
};

#[derive(Debug, PartialEq)]
enum SymbolType {
    Function,
    Variable,
}

type ArgumentSize = usize;

// I have a hunch that I'll change this to include types, etc.
// after we incorporate them
enum SymbolInfo {
    Function(ArgumentSize),
    Variable,
}

struct Scope {
    symbol_table: HashMap<String, SymbolInfo>,
}

impl Scope {
    fn new() -> Self {
        Self {
            symbol_table: HashMap::new(),
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

pub struct Sema {
    // Stack of current scopes
    scopes_stack: Vec<Scope>,
    main_function_found: bool,
}

pub struct Resolution {}

impl Sema {
    pub fn new() -> Self {
        Self {
            scopes_stack: Vec::new(),
            main_function_found: false,
        }
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
                return None;
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
        self.scopes_stack.push(Scope::new());

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

    fn analyse_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Declaration { identifier, .. } => {
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
            StatementKind::Return(expression) => return self.analyse_expression(&expression),
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

                self.push_new_scope(Scope::new());
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
            StatementKind::If(..) => Ok(()),
            StatementKind::While { .. } => Ok(()),
            StatementKind::For { .. } => Ok(()),
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
            ExpressionKind::UnaryOperation { .. } => Ok(()),
            ExpressionKind::BinaryOperation { .. } => Ok(()),
            ExpressionKind::Assignment { .. } => Ok(()),
            ExpressionKind::Call { callee, arguments } => {
                let function_name = callee.name.clone();
                if let Some(symbol_info) = self.get_symbol_info(function_name.clone()) {
                    match symbol_info {
                        SymbolInfo::Variable => {
                            return Err(Diagnostic::error(
                                expression.span,
                                format!("Called object is not a function"),
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
                                return Err(Diagnostic::error(
                                    Span::new(error_pointer_span, error_pointer_span),
                                    format!("too {few_many} arguments in call to {function_name}"),
                                ));
                            })()?
                        }
                    }
                } else {
                    return Err(Diagnostic::error(
                        expression.span,
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
}
