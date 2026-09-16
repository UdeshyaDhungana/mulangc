//! The shape of a parsed mulang program.
//!
//! Every node carries the [`Span`] it was parsed from, so a back end — the
//! interpreter today, a code generator later — can blame precise source text
//! for anything it rejects.
//!
//! [`Display`] writes any node back out as mulang source. That keeps the
//! printing rules in one place and gives parser tests an exact, readable
//! assertion target.

use std::fmt;
use std::rc::Rc;

use crate::diagnostics::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    /// `thoos_muji name = value;`
    Declaration {
        identifier: Identifier,
        value: Expression,
    },
    /// `patha_muji value;`
    Return(Expression),
    /// An expression evaluated for its value, its effect, or both.
    Expression(Expression),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Identifier(String),
    Integer(i64),
    Float(f64),
    Text(String),
    Boolean(bool),

    Array(Vec<Expression>),
    /// Key/value pairs in source order. A `Vec` rather than a map because two
    /// syntactically distinct keys can still evaluate to the same string, and
    /// deciding what that means belongs to evaluation, not parsing.
    HashMap(Vec<(Expression, Expression)>),

    UnaryOperation {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    BinaryOperation {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `target = value`, where `target` was validated at parse time.
    Assignment {
        target: AssignmentTarget,
        value: Box<Expression>,
    },
    Index {
        collection: Box<Expression>,
        index: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    /// Shared behind an `Rc` so that building a closure at run time costs a
    /// refcount bump instead of a deep copy of the body.
    Function(Rc<FunctionDefinition>),

    /// `yedi_muji` with its `nabhae_muji` chain and optional `nabhae_chikne`.
    If {
        /// The `yedi_muji` branch first, then each `nabhae_muji` in order.
        branches: Vec<ConditionalBranch>,
        fallback: Option<Block>,
    },
    While {
        condition: Box<Expression>,
        body: Block,
    },
    For {
        initialiser: Option<Box<Statement>>,
        condition: Box<Expression>,
        update: Box<Expression>,
        body: Block,
    },
}

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub parameters: Vec<Identifier>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct ConditionalBranch {
    pub condition: Expression,
    pub consequence: Block,
}

/// The left side of an assignment. Restricting this to the two forms that can
/// actually name a storage location means the interpreter never has to ask
/// whether `2 + 2 = 5` is meaningful.
#[derive(Clone, Debug)]
pub enum AssignmentTarget {
    Variable(Identifier),
    Element {
        collection: Box<Expression>,
        index: Box<Expression>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl fmt::Display for UnaryOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            UnaryOperator::Negate => "-",
            UnaryOperator::Not => "!",
        })
    }
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
            BinaryOperator::Remainder => "%",
            BinaryOperator::Equal => "==",
            BinaryOperator::NotEqual => "!=",
            BinaryOperator::Less => "<",
            BinaryOperator::LessOrEqual => "<=",
            BinaryOperator::Greater => ">",
            BinaryOperator::GreaterOrEqual => ">=",
        })
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_statements(formatter, &self.statements, 0)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_statement(formatter, self, 0)
    }
}

impl fmt::Display for Block {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_block(formatter, self, 0)
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_expression(formatter, self, 0)
    }
}

/// Nested constructs need to know how far in they sit, which `Display` alone
/// cannot carry. These functions thread that depth through.
fn write_statements(
    formatter: &mut fmt::Formatter<'_>,
    statements: &[Statement],
    depth: usize,
) -> fmt::Result {
    for statement in statements {
        write_indent(formatter, depth)?;
        write_statement(formatter, statement, depth)?;
        writeln!(formatter)?;
    }
    Ok(())
}

fn write_statement(
    formatter: &mut fmt::Formatter<'_>,
    statement: &Statement,
    depth: usize,
) -> fmt::Result {
    match &statement.kind {
        StatementKind::Declaration {
            identifier: name,
            value,
        } => {
            write!(formatter, "thoos_muji {name} = ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(";")
        }
        StatementKind::Return(value) => {
            formatter.write_str("patha_muji ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(";")
        }
        StatementKind::Expression(expression) => {
            write_expression(formatter, expression, depth)?;
            formatter.write_str(";")
        }
    }
}

fn write_block(formatter: &mut fmt::Formatter<'_>, block: &Block, depth: usize) -> fmt::Result {
    formatter.write_str("{\n")?;
    write_statements(formatter, &block.statements, depth + 1)?;
    write_indent(formatter, depth)?;
    formatter.write_str("}")
}

fn write_expression(
    formatter: &mut fmt::Formatter<'_>,
    expression: &Expression,
    depth: usize,
) -> fmt::Result {
    match &expression.kind {
        ExpressionKind::Identifier(name) => formatter.write_str(name),
        ExpressionKind::Integer(value) => write!(formatter, "{value}"),
        ExpressionKind::Float(value) => write!(formatter, "{value}"),
        ExpressionKind::Text(value) => write!(formatter, "\"{value}\""),
        ExpressionKind::Boolean(true) => formatter.write_str("sacho_muji"),
        ExpressionKind::Boolean(false) => formatter.write_str("jhut_muji"),

        ExpressionKind::Array(elements) => {
            formatter.write_str("[")?;
            write_separated(formatter, elements, depth, write_expression)?;
            formatter.write_str("]")
        }
        ExpressionKind::HashMap(pairs) => {
            formatter.write_str("{")?;
            for (position, (key, value)) in pairs.iter().enumerate() {
                if position > 0 {
                    formatter.write_str(", ")?;
                }
                write_expression(formatter, key, depth)?;
                formatter.write_str(": ")?;
                write_expression(formatter, value, depth)?;
            }
            formatter.write_str("}")
        }

        ExpressionKind::UnaryOperation { operator, operand } => {
            write!(formatter, "({operator}")?;
            write_expression(formatter, operand, depth)?;
            formatter.write_str(")")
        }
        ExpressionKind::BinaryOperation {
            operator,
            left,
            right,
        } => {
            formatter.write_str("(")?;
            write_expression(formatter, left, depth)?;
            write!(formatter, " {operator} ")?;
            write_expression(formatter, right, depth)?;
            formatter.write_str(")")
        }
        ExpressionKind::Assignment { target, value } => {
            formatter.write_str("(")?;
            match target {
                AssignmentTarget::Variable(name) => write!(formatter, "{name}")?,
                AssignmentTarget::Element { collection, index } => {
                    write_expression(formatter, collection, depth)?;
                    formatter.write_str("[")?;
                    write_expression(formatter, index, depth)?;
                    formatter.write_str("]")?;
                }
            }
            formatter.write_str(" = ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(")")
        }
        ExpressionKind::Index { collection, index } => {
            write_expression(formatter, collection, depth)?;
            formatter.write_str("[")?;
            write_expression(formatter, index, depth)?;
            formatter.write_str("]")
        }
        ExpressionKind::Call { callee, arguments } => {
            write_expression(formatter, callee, depth)?;
            formatter.write_str("(")?;
            write_separated(formatter, arguments, depth, write_expression)?;
            formatter.write_str(")")
        }
        ExpressionKind::Function(definition) => {
            formatter.write_str("kaam_gar_muji(")?;
            write_separated(
                formatter,
                &definition.parameters,
                depth,
                |formatter, parameter, _| write!(formatter, "{parameter}"),
            )?;
            formatter.write_str(") ")?;
            write_block(formatter, &definition.body, depth)
        }

        ExpressionKind::If { branches, fallback } => {
            for (position, branch) in branches.iter().enumerate() {
                let keyword = if position == 0 {
                    "yedi_muji"
                } else {
                    " nabhae_muji"
                };
                write!(formatter, "{keyword} (")?;
                write_expression(formatter, &branch.condition, depth)?;
                formatter.write_str(") ")?;
                write_block(formatter, &branch.consequence, depth)?;
            }
            match fallback {
                Some(block) => {
                    formatter.write_str(" nabhae_chikne ")?;
                    write_block(formatter, block, depth)
                }
                None => Ok(()),
            }
        }
        ExpressionKind::While { condition, body } => {
            formatter.write_str("jaba_samma_muji (")?;
            write_expression(formatter, condition, depth)?;
            formatter.write_str(") ")?;
            write_block(formatter, body, depth)
        }
        ExpressionKind::For {
            initialiser,
            condition,
            update,
            body,
        } => {
            formatter.write_str("ghuma_muji (")?;
            match initialiser {
                Some(statement) => write_statement(formatter, statement, depth)?,
                None => formatter.write_str(";")?,
            }
            formatter.write_str(" ")?;
            write_expression(formatter, condition, depth)?;
            formatter.write_str("; ")?;
            write_expression(formatter, update, depth)?;
            formatter.write_str(") ")?;
            write_block(formatter, body, depth)
        }
    }
}

fn write_separated<T>(
    formatter: &mut fmt::Formatter<'_>,
    items: &[T],
    depth: usize,
    write_item: impl Fn(&mut fmt::Formatter<'_>, &T, usize) -> fmt::Result,
) -> fmt::Result {
    for (position, item) in items.iter().enumerate() {
        if position > 0 {
            formatter.write_str(", ")?;
        }
        write_item(formatter, item, depth)?;
    }
    Ok(())
}

fn write_indent(formatter: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        formatter.write_str("    ")?;
    }
    Ok(())
}
