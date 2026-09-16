//! Executing a parsed program.
//!
//! Two things shape the code here. First, `patha_muji` and runtime errors both
//! have to abandon whatever is part-way through evaluating, so both travel as
//! the error arm of a `Result` ([`Interrupt`]) and `?` does the unwinding. A
//! `patha_muji` buried inside a loop inside a conditional leaves the function
//! because nothing between it and the call boundary catches it.
//!
//! Second, scoping is decided by whoever evaluates a block, never by the block
//! itself: a function call, a conditional branch and each turn of a loop all
//! nest a scope, so a `thoos_muji` inside them cannot leak outwards.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::rc::Rc;

use crate::diagnostics::{Diagnostic, Span};
use crate::runtime::builtins::{self, BuiltinContext};
use crate::runtime::operations::{apply_binary, apply_unary};
use crate::runtime::scope::{Scope, ScopeHandle};
use crate::runtime::value::{Closure, Value};
use crate::syntax::{
    AssignmentTarget, Block, ConditionalBranch, Expression, ExpressionKind, FunctionDefinition,
    Identifier, Program, Statement, StatementKind,
};

/// How deep mulang calls may nest before the interpreter gives up. The limit
/// exists so that runaway recursion in a program becomes a mulang error rather
/// than an exhausted host stack.
const MAXIMUM_CALL_DEPTH: usize = 1_000;

/// Stack a thread must have to run an interpreter.
///
/// Evaluation recurses on the host stack, so [`MAXIMUM_CALL_DEPTH`] is only
/// enforceable if there is enough stack left to reach it — on a thread with
/// less, a runaway program overflows before the interpreter can say so. This
/// is generous rather than measured: it costs address space, not memory.
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;

pub struct Interpreter {
    global_scope: ScopeHandle,
    output: Box<dyn Write>,
    call_depth: usize,
}

/// Everything that stops evaluation partway.
enum Interrupt {
    Return(Value),
    Failure(Diagnostic),
}

type Outcome<T> = Result<T, Interrupt>;

impl From<Diagnostic> for Interrupt {
    fn from(diagnostic: Diagnostic) -> Self {
        Interrupt::Failure(diagnostic)
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::writing_to(Box::new(io::stdout()))
    }

    /// An interpreter whose program output goes somewhere other than stdout.
    pub fn writing_to(output: Box<dyn Write>) -> Self {
        Self {
            global_scope: Scope::global(),
            output,
            call_depth: 0,
        }
    }

    /// Runs `program` and yields the value of its last statement.
    ///
    /// Global state persists across calls, so a REPL can feed one line at a
    /// time and have earlier declarations still be in scope.
    pub fn run(&mut self, program: &Program) -> Result<Value, Diagnostic> {
        let global_scope = Rc::clone(&self.global_scope);

        match self.evaluate_statements(&program.statements, &global_scope) {
            Ok(value) => Ok(value),
            // A top-level `patha_muji` ends the program with its value.
            Err(Interrupt::Return(value)) => Ok(value),
            Err(Interrupt::Failure(diagnostic)) => Err(diagnostic),
        }
    }

    /* ---------------------------------------------------------------- statements */

    /// Evaluates statements in the scope given, yielding the last one's value.
    fn evaluate_statements(
        &mut self,
        statements: &[Statement],
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        let mut last_value = Value::Null;
        for statement in statements {
            last_value = self.evaluate_statement(statement, scope)?;
        }
        Ok(last_value)
    }

    fn evaluate_statement(&mut self, statement: &Statement, scope: &ScopeHandle) -> Outcome<Value> {
        match &statement.kind {
            StatementKind::Declaration {
                identifier: name,
                value,
            } => {
                let value = self.evaluate_expression(value, scope)?;
                scope.borrow_mut().declare(&name.name, value.clone());
                Ok(value)
            }
            StatementKind::Return(value) => {
                let value = self.evaluate_expression(value, scope)?;
                Err(Interrupt::Return(value))
            }
            StatementKind::Expression(expression) => self.evaluate_expression(expression, scope),
        }
    }

    /* --------------------------------------------------------------- expressions */

    fn evaluate_expression(
        &mut self,
        expression: &Expression,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        let span = expression.span;

        match &expression.kind {
            ExpressionKind::Integer(value) => Ok(Value::Integer(*value)),
            ExpressionKind::Float(value) => Ok(Value::Float(*value)),
            ExpressionKind::Boolean(value) => Ok(Value::Boolean(*value)),
            ExpressionKind::Text(value) => Ok(Value::text(value)),
            ExpressionKind::Identifier(name) => Ok(resolve(name, span, scope)?),

            ExpressionKind::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.evaluate_expression(element, scope)?);
                }
                Ok(Value::array(values))
            }
            ExpressionKind::HashMap(pairs) => self.evaluate_hash_map(pairs, scope),

            ExpressionKind::UnaryOperation { operator, operand } => {
                let operand = self.evaluate_expression(operand, scope)?;
                Ok(apply_unary(*operator, &operand, span)?)
            }
            ExpressionKind::BinaryOperation {
                operator,
                left,
                right,
            } => {
                let left = self.evaluate_expression(left, scope)?;
                let right = self.evaluate_expression(right, scope)?;
                Ok(apply_binary(*operator, &left, &right, span)?)
            }
            ExpressionKind::Assignment { target, value } => {
                self.evaluate_assignment(target, value, span, scope)
            }

            ExpressionKind::Index { collection, index } => {
                let collection = self.evaluate_expression(collection, scope)?;
                let index = self.evaluate_expression(index, scope)?;
                Ok(read_element(&collection, &index, span)?)
            }
            ExpressionKind::Call { callee, arguments } => {
                self.evaluate_call(callee, arguments, span, scope)
            }
            ExpressionKind::Function(definition) => Ok(Value::Function(Rc::new(Closure {
                definition: Rc::clone(definition),
                captured_scope: Rc::clone(scope),
            }))),

            ExpressionKind::If { branches, fallback } => {
                self.evaluate_if(branches, fallback.as_ref(), scope)
            }
            ExpressionKind::While { condition, body } => {
                self.evaluate_while(condition, body, scope)
            }
            ExpressionKind::For {
                initialiser,
                condition,
                update,
                body,
            } => self.evaluate_for(initialiser.as_deref(), condition, update, body, scope),
        }
    }

    fn evaluate_hash_map(
        &mut self,
        pairs: &[(Expression, Expression)],
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        let mut entries = BTreeMap::new();

        for (key, value) in pairs {
            let evaluated_key = self.evaluate_expression(key, scope)?;
            let Value::Text(evaluated_key) = evaluated_key else {
                return Err(Diagnostic::error(
                    key.span,
                    format!(
                        "a hashmap key must be a string, not {}",
                        evaluated_key.type_name()
                    ),
                )
                .into());
            };
            let evaluated_value = self.evaluate_expression(value, scope)?;
            entries.insert(evaluated_key.to_string(), evaluated_value);
        }

        Ok(Value::hash_map(entries))
    }

    fn evaluate_assignment(
        &mut self,
        target: &AssignmentTarget,
        value: &Expression,
        span: Span,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        let assigned = self.evaluate_expression(value, scope)?;

        match target {
            AssignmentTarget::Variable(name) => {
                let was_bound = scope.borrow_mut().assign(&name.name, assigned.clone());
                if !was_bound {
                    return Err(undefined_name(name).into());
                }
                Ok(assigned)
            }
            AssignmentTarget::Element { collection, index } => {
                let collection = self.evaluate_expression(collection, scope)?;
                let index = self.evaluate_expression(index, scope)?;
                Ok(store_element(&collection, &index, assigned, span)?)
            }
        }
    }

    fn evaluate_if(
        &mut self,
        branches: &[ConditionalBranch],
        fallback: Option<&Block>,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        for branch in branches {
            if self
                .evaluate_expression(&branch.condition, scope)?
                .is_truthy()
            {
                return self.evaluate_block(&branch.consequence, scope);
            }
        }

        match fallback {
            Some(block) => self.evaluate_block(block, scope),
            None => Ok(Value::Null),
        }
    }

    fn evaluate_while(
        &mut self,
        condition: &Expression,
        body: &Block,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        while self.evaluate_expression(condition, scope)?.is_truthy() {
            self.evaluate_block(body, scope)?;
        }
        Ok(Value::Null)
    }

    fn evaluate_for(
        &mut self,
        initialiser: Option<&Statement>,
        condition: &Expression,
        update: &Expression,
        body: &Block,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        // The loop variable belongs to a scope of its own: visible to the
        // condition, the update and the body, gone once the loop ends.
        let header_scope = Scope::nested_in(scope);
        if let Some(initialiser) = initialiser {
            self.evaluate_statement(initialiser, &header_scope)?;
        }

        while self
            .evaluate_expression(condition, &header_scope)?
            .is_truthy()
        {
            self.evaluate_block(body, &header_scope)?;
            self.evaluate_expression(update, &header_scope)?;
        }
        Ok(Value::Null)
    }

    /// Runs a block in a scope of its own, so declarations inside it are gone
    /// when it ends.
    fn evaluate_block(&mut self, block: &Block, enclosing: &ScopeHandle) -> Outcome<Value> {
        let block_scope = Scope::nested_in(enclosing);
        self.evaluate_statements(&block.statements, &block_scope)
    }

    /* -------------------------------------------------------------------- calls */

    fn evaluate_call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        span: Span,
        scope: &ScopeHandle,
    ) -> Outcome<Value> {
        // The callee is any expression, so a function returned by another call
        // or pulled out of an array is callable on the spot.
        let called = self.evaluate_expression(callee, scope)?;

        let mut evaluated_arguments = Vec::with_capacity(arguments.len());
        for argument in arguments {
            evaluated_arguments.push(self.evaluate_expression(argument, scope)?);
        }

        match called {
            Value::Function(closure) => self.call_closure(&closure, evaluated_arguments, span),
            Value::Builtin(builtin) => {
                let mut context = BuiltinContext {
                    output: &mut *self.output,
                };
                Ok(builtin.call(&mut context, evaluated_arguments, span)?)
            }
            other => Err(Diagnostic::error(
                span,
                format!("cannot call {}; it is not a function", other.type_name()),
            )
            .into()),
        }
    }

    fn call_closure(
        &mut self,
        closure: &Rc<Closure>,
        arguments: Vec<Value>,
        span: Span,
    ) -> Outcome<Value> {
        let parameters = &closure.definition.parameters;
        if parameters.len() != arguments.len() {
            return Err(argument_count_mismatch(&closure.definition, arguments.len(), span).into());
        }

        // Arguments land in a scope nested inside the one the function was
        // written in, never inside the caller's — that is what makes closures
        // close over their definition site.
        let call_scope = Scope::nested_in(&closure.captured_scope);
        for (parameter, argument) in parameters.iter().zip(arguments) {
            call_scope.borrow_mut().declare(&parameter.name, argument);
        }

        if self.call_depth >= MAXIMUM_CALL_DEPTH {
            return Err(Diagnostic::error(
                span,
                format!("calls nested more than {MAXIMUM_CALL_DEPTH} deep"),
            )
            .with_help("a recursive function is probably missing its base case")
            .into());
        }

        self.call_depth += 1;
        let outcome = self.evaluate_statements(&closure.definition.body.statements, &call_scope);
        self.call_depth -= 1;

        match outcome {
            // `patha_muji` unwinds to here and no further.
            Err(Interrupt::Return(value)) => Ok(value),
            // Without one, a function yields its last statement's value.
            other => other,
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve(name: &str, span: Span, scope: &ScopeHandle) -> Result<Value, Diagnostic> {
    scope
        .borrow()
        .lookup(name)
        .or_else(|| builtins::lookup(name).map(Value::Builtin))
        .ok_or_else(|| Diagnostic::error(span, format!("`{name}` is not defined")))
}

fn read_element(collection: &Value, index: &Value, span: Span) -> Result<Value, Diagnostic> {
    match collection {
        Value::Array(elements) => {
            let elements = elements.borrow();
            let position = array_position(index, elements.len(), span)?;
            Ok(elements[position].clone())
        }
        Value::HashMap(entries) => {
            let key = hash_map_key(index, span)?;
            // A key that was never set reads as khali_muji, the same as a
            // hashmap that was never given that key at all.
            Ok(entries.borrow().get(key).cloned().unwrap_or(Value::Null))
        }
        other => Err(
            Diagnostic::error(span, format!("cannot index {}", other.type_name()))
                .with_help("only arrays and hashmaps can be indexed"),
        ),
    }
}

fn store_element(
    collection: &Value,
    index: &Value,
    value: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match collection {
        Value::Array(elements) => {
            let position = array_position(index, elements.borrow().len(), span)?;
            elements.borrow_mut()[position] = value.clone();
            Ok(value)
        }
        Value::HashMap(entries) => {
            let key = hash_map_key(index, span)?.to_owned();
            entries.borrow_mut().insert(key, value.clone());
            Ok(value)
        }
        other => Err(
            Diagnostic::error(span, format!("cannot assign into {}", other.type_name()))
                .with_help("only arrays and hashmaps can be indexed"),
        ),
    }
}

/// Turns an index value into a position that is known to be in range, so both
/// reading and writing an element get the same bounds check and the same message.
fn array_position(index: &Value, length: usize, span: Span) -> Result<usize, Diagnostic> {
    let Value::Integer(index) = index else {
        return Err(Diagnostic::error(
            span,
            format!(
                "an array index must be an integer, not {}",
                index.type_name()
            ),
        ));
    };

    usize::try_from(*index)
        .ok()
        .filter(|position| *position < length)
        .ok_or_else(|| {
            Diagnostic::error(
                span,
                format!("index {index} is outside this array of {length} element(s)"),
            )
        })
}

fn hash_map_key(index: &Value, span: Span) -> Result<&str, Diagnostic> {
    match index {
        Value::Text(key) => Ok(key),
        other => Err(Diagnostic::error(
            span,
            format!("a hashmap key must be a string, not {}", other.type_name()),
        )),
    }
}

fn undefined_name(name: &Identifier) -> Diagnostic {
    Diagnostic::error(name.span, format!("`{}` is not defined", name.name))
        .with_help(format!("declare it first: `thoos_muji {} = …;`", name.name))
}

fn argument_count_mismatch(
    definition: &FunctionDefinition,
    supplied: usize,
    span: Span,
) -> Diagnostic {
    let parameters: Vec<&str> = definition
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect();

    Diagnostic::error(
        span,
        format!(
            "this function takes {} argument(s), but was given {supplied}",
            parameters.len()
        ),
    )
    .with_help(format!("it is declared as ({})", parameters.join(", ")))
}
