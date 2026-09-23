use std::fmt;

use crate::diagnostics::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub block: Block,
}

#[derive(Clone, Debug)]
pub struct Identifier {
    pub name: String,
    pub node_id: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    Declaration {
        identifier: Identifier,
        value: Expression,
    },
    Return(Expression),
    Expression(Expression),

    Function(FunctionDefinition),

    If(Conditional),
    While {
        node_id: String,
        condition: Box<Expression>,
        body: Block,
    },
    For {
        node_id: String,
        initialiser: Option<Box<Statement>>,
        condition: Box<Expression>,
        update: Box<Expression>,
        body: Block,
    },
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
    Identifier(Identifier),
    Integer(i64),
    Boolean(bool),

    UnaryOperation {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    BinaryOperation {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Assignment {
        target: AssignmentTarget,
        value: Box<Expression>,
    },
    Call {
        callee: Identifier,
        arguments: Vec<Expression>,
    },
}

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub identifier: Identifier,
    pub parameters: Vec<Identifier>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Conditional {
    pub node_id: String,
    pub condition: Expression,
    pub then: Block,
    pub otherwise: Option<Box<Else>>,
}

#[derive(Clone, Debug)]
pub enum Else {
    Block(Block),
    If(Conditional),
}

#[derive(Clone, Debug)]
pub enum AssignmentTarget {
    Variable(Identifier),
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
    BooleanAnd,
    BooleanOr,
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
            BinaryOperator::BooleanAnd => "ra",
            BinaryOperator::BooleanOr => "wa",
        })
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_statements(formatter, &self.block.statements, 0)
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
            write!(formatter, "thoos {name} = ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(";")
        }
        StatementKind::Return(value) => {
            formatter.write_str("de ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(";")
        }
        StatementKind::Expression(expression) => {
            write_expression(formatter, expression, depth)?;
            formatter.write_str(";")
        }

        StatementKind::Function(definition) => {
            write!(formatter, "dhanda {}(", definition.identifier)?;
            write_separated(
                formatter,
                &definition.parameters,
                depth,
                |formatter, parameter, _| write!(formatter, "{parameter}"),
            )?;
            formatter.write_str(") ")?;
            write_block(formatter, &definition.body, depth)
        }
        StatementKind::If(conditional) => {
            formatter.write_str("yedi ")?;
            write_conditional(formatter, conditional, depth)
        }
        StatementKind::While {
            condition, body, ..
        } => {
            formatter.write_str("jabasamma (")?;
            write_expression(formatter, condition, depth)?;
            formatter.write_str(") ")?;
            write_block(formatter, body, depth)
        }
        StatementKind::For {
            initialiser,
            condition,
            update,
            body,
            ..
        } => {
            formatter.write_str("fanfani (")?;
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

fn write_conditional(
    formatter: &mut fmt::Formatter<'_>,
    conditional: &Conditional,
    depth: usize,
) -> fmt::Result {
    formatter.write_str("(")?;
    write_expression(formatter, &conditional.condition, depth)?;
    formatter.write_str(") ")?;
    write_block(formatter, &conditional.then, depth)?;

    match conditional.otherwise.as_deref() {
        None => Ok(()),
        Some(Else::Block(block)) => {
            formatter.write_str(" natra ")?;
            write_block(formatter, block, depth)
        }
        Some(Else::If(nested)) => {
            formatter.write_str(" natra yedi ")?;
            write_conditional(formatter, nested, depth)
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
        ExpressionKind::Identifier(name) => formatter.write_str(&name.name),
        ExpressionKind::Integer(value) => write!(formatter, "{value}"),
        ExpressionKind::Boolean(true) => formatter.write_str("ho"),
        ExpressionKind::Boolean(false) => formatter.write_str("haina"),

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
            }
            formatter.write_str(" = ")?;
            write_expression(formatter, value, depth)?;
            formatter.write_str(")")
        }
        ExpressionKind::Call { callee, arguments } => {
            write!(formatter, "{callee}")?;
            formatter.write_str("(")?;
            write_separated(formatter, arguments, depth, write_expression)?;
            formatter.write_str(")")
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
