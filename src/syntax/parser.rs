use std::mem::discriminant;

use crate::diagnostics::{Diagnostic, Span};
use crate::syntax::ast::*;
use crate::syntax::sema::Counter;
use crate::syntax::token::{SpannedToken, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest,
    Assignment,
    BooleanOr,
    BooleanAnd,
    Equality,
    Comparison,
    Sum,
    Product,
    Prefix,
    Call,
}

pub fn parse(tokens: &[SpannedToken]) -> Result<AST, Vec<Diagnostic>> {
    let mut parser = Parser {
        tokens,
        position: 0,
        diagnostics: Vec::new(),
        counter: Default::default(),
    };
    let statements = parser.parse_program();

    if parser.diagnostics.is_empty() {
        let span = match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => first.span.joined_with(last.span),
            _ => Span::new(0, 0),
        };
        Ok(AST {
            block: Block { statements, span },
        })
    } else {
        Err(parser.diagnostics)
    }
}

struct Parser<'tokens> {
    tokens: &'tokens [SpannedToken],
    position: usize,
    diagnostics: Vec<Diagnostic>,
    counter: Counter,
}

impl Parser<'_> {
    fn parse_program(&mut self) -> Vec<Statement> {
        let mut statements = Vec::new();

        while !self.is_at(&Token::EndOfInput) {
            let position_before_statement = self.position;
            match self.parse_statement() {
                Ok(statement) => statements.push(statement),
                Err(diagnostic) => {
                    self.diagnostics.push(diagnostic);
                    self.recover_at_statement_boundary();
                    if self.position == position_before_statement {
                        self.advance();
                    }
                }
            }
        }
        statements
    }

    fn get_node_id(&mut self, identifier_name: String) -> String {
        let next = self.counter.next();
        format!("{identifier_name}_{next}")
    }

    fn recover_at_statement_boundary(&mut self) {
        while !self.is_at(&Token::EndOfInput) {
            if matches!(self.current_token(), Token::Semicolon) {
                self.advance();
                return;
            }
            if matches!(
                self.current_token(),
                Token::Declare
                    | Token::Return
                    | Token::Function
                    | Token::If
                    | Token::While
                    | Token::For
            ) {
                return;
            }
            self.advance();
        }
    }

    /* ---------------------------------------------------------------- statements */

    fn parse_statement(&mut self) -> Result<Statement, Diagnostic> {
        match self.current_token() {
            Token::Declare => self.parse_declaration(),
            Token::Return => self.parse_return(),
            Token::Function => self.parse_function(),
            Token::If => self.parse_if(),
            Token::While => self.parse_while(),
            Token::For => self.parse_for(),
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_declaration(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::Declare)?;
        let name = self.expect_identifier()?;
        self.expect(Token::Assign)?;
        let value = self.parse_expression(Precedence::Lowest)?;
        let end = self.expect_statement_terminator()?;

        Ok(Statement {
            kind: StatementKind::Declaration {
                identifier: name,
                value,
            },
            span: start.joined_with(end),
        })
    }

    fn parse_return(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::Return)?;
        let value = self.parse_expression(Precedence::Lowest)?;
        let end = self.expect_statement_terminator()?;

        Ok(Statement {
            kind: StatementKind::Return(value),
            span: start.joined_with(end),
        })
    }

    fn parse_if(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::If)?;
        let conditional = self.parse_conditional()?;

        Ok(Statement {
            span: start.joined_with(self.conditional_end_span(&conditional)),
            kind: StatementKind::If(conditional),
        })
    }

    fn parse_conditional(&mut self) -> Result<Conditional, Diagnostic> {
        self.expect(Token::LeftParenthesis)?;
        let condition = self.parse_expression(Precedence::Lowest)?;
        self.expect(Token::RightParenthesis)?;
        let then = self.parse_block()?;

        let otherwise = if self.is_at(&Token::Else) {
            self.advance();
            Some(Box::new(if self.is_at(&Token::If) {
                self.advance();
                Else::If(self.parse_conditional()?)
            } else {
                Else::Block(self.parse_block()?)
            }))
        } else {
            None
        };

        let next_id = self.counter.next();
        Ok(Conditional {
            node_id: format!("yedi_{next_id}"),
            condition,
            then,
            otherwise,
        })
    }

    fn parse_while(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::While)?;
        self.expect(Token::LeftParenthesis)?;
        let condition = self.parse_expression(Precedence::Lowest)?;
        self.expect(Token::RightParenthesis)?;
        let body = self.parse_block()?;

        let node_id = self.counter.next();
        Ok(Statement {
            span: start.joined_with(body.span),
            kind: StatementKind::While {
                node_id: format!("jabasamma_{node_id}"),
                condition: Box::new(condition),
                body,
            },
        })
    }

    fn parse_for(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::For)?;
        self.expect(Token::LeftParenthesis)?;

        let initialiser = match self.current_token() {
            Token::Semicolon => {
                self.advance();
                None
            }
            Token::Declare => Some(Box::new(self.parse_declaration()?)),
            _ => {
                let expression = self.parse_expression(Precedence::Lowest)?;
                let end = self.expect(Token::Semicolon)?;
                Some(Box::new(Statement {
                    span: expression.span.joined_with(end),
                    kind: StatementKind::Expression(expression),
                }))
            }
        };

        let condition = self.parse_expression(Precedence::Lowest)?;
        self.expect(Token::Semicolon)?;
        let update = self.parse_expression(Precedence::Lowest)?;
        self.expect(Token::RightParenthesis)?;
        let body = self.parse_block()?;

        let next_id = self.counter.next();
        Ok(Statement {
            span: start.joined_with(body.span),
            kind: StatementKind::For {
                initialiser,
                condition: Box::new(condition),
                update: Box::new(update),
                body,
                node_id: format!("jabasamma_{next_id}"),
            },
        })
    }

    fn parse_expression_statement(&mut self) -> Result<Statement, Diagnostic> {
        let expression = self.parse_expression(Precedence::Lowest)?;
        let mut span = expression.span;

        if self.is_at(&Token::Semicolon) {
            span = span.joined_with(self.current_span());
            self.advance();
        }

        Ok(Statement {
            kind: StatementKind::Expression(expression),
            span,
        })
    }

    fn parse_block(&mut self) -> Result<Block, Diagnostic> {
        let start = self.expect(Token::LeftBrace)?;
        let mut statements = Vec::new();

        while !self.is_at(&Token::RightBrace) {
            if self.is_at(&Token::EndOfInput) {
                return Err(self
                    .error_at_current("expected `}` to close this block")
                    .with_help("the block opened here was never closed"));
            }
            statements.push(self.parse_statement()?);
        }

        let end = self.expect(Token::RightBrace)?;
        Ok(Block {
            statements,
            span: start.joined_with(end),
        })
    }

    /* --------------------------------------------------------------- expressions */

    fn parse_expression(&mut self, precedence: Precedence) -> Result<Expression, Diagnostic> {
        let mut left = self.parse_prefix()?;

        while precedence < self.precedence_of(self.current_token()) {
            left = self.parse_infix(left)?;
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expression, Diagnostic> {
        let span = self.current_span();

        let kind = match self.current_token().clone() {
            Token::Identifier(name) => {
                self.advance();
                ExpressionKind::Identifier(Identifier {
                    name: name.clone(),
                    node_id: self.get_node_id(name),
                    span,
                })
            }
            Token::Integer(value) => {
                self.advance();
                ExpressionKind::Integer(value)
            }

            Token::Minus | Token::Bang => return self.parse_unary(),
            Token::LeftParenthesis => return self.parse_grouping(),

            _ => return Err(self.error_at_current("expected an expression")),
        };

        Ok(Expression { kind, span })
    }

    fn parse_infix(&mut self, left: Expression) -> Result<Expression, Diagnostic> {
        match self.current_token() {
            Token::LeftParenthesis => self.parse_call(left),
            Token::Assign => self.parse_assignment(left),
            _ => self.parse_binary(left),
        }
    }

    fn parse_unary(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.current_span();
        let operator = match self.current_token() {
            Token::Minus => UnaryOperator::Negate,
            Token::Bang => UnaryOperator::Not,
            _ => return Err(self.error_at_current("expected a prefix operator")),
        };
        self.advance();

        let operand = self.parse_expression(Precedence::Prefix)?;
        Ok(Expression {
            span: start.joined_with(operand.span),
            kind: ExpressionKind::UnaryOperation {
                operator,
                operand: Box::new(operand),
            },
        })
    }

    fn parse_binary(&mut self, left: Expression) -> Result<Expression, Diagnostic> {
        let operator = self
            .binary_operator_of(self.current_token())
            .ok_or_else(|| self.error_at_current("expected a binary operator"))?;
        let precedence = self.precedence_of(self.current_token());
        self.advance();

        let right = self.parse_expression(precedence)?;
        Ok(Expression {
            span: left.span.joined_with(right.span),
            kind: ExpressionKind::BinaryOperation {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
        })
    }

    fn parse_assignment(&mut self, left: Expression) -> Result<Expression, Diagnostic> {
        self.expect(Token::Assign)?;
        let target = self.assignment_target_of(left)?;
        let value = self.parse_expression(Precedence::Lowest)?;

        Ok(Expression {
            span: self.target_span(&target).joined_with(value.span),
            kind: ExpressionKind::Assignment {
                target,
                value: Box::new(value),
            },
        })
    }

    fn parse_grouping(&mut self) -> Result<Expression, Diagnostic> {
        self.expect(Token::LeftParenthesis)?;
        let inner = self.parse_expression(Precedence::Lowest)?;
        self.expect(Token::RightParenthesis)?;
        Ok(inner)
    }

    fn parse_call(&mut self, callee: Expression) -> Result<Expression, Diagnostic> {
        let callee = self.callee_of(callee)?;
        self.expect(Token::LeftParenthesis)?;
        let arguments =
            self.parse_comma_separated(&Token::RightParenthesis, Self::parse_expression_at_lowest)?;
        let end = self.expect(Token::RightParenthesis)?;

        Ok(Expression {
            span: callee.span.joined_with(end),
            kind: ExpressionKind::Call { callee, arguments },
        })
    }

    fn parse_function(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::Function)?;
        let name = self.expect_identifier()?;
        self.expect(Token::LeftParenthesis)?;
        let parameters =
            self.parse_comma_separated(&Token::RightParenthesis, Self::expect_identifier)?;
        self.expect(Token::RightParenthesis)?;
        let body = self.parse_block()?;

        Ok(Statement {
            span: start.joined_with(body.span),
            kind: StatementKind::Function(FunctionDefinition {
                identifier: name,
                parameters,
                body,
            }),
        })
    }

    fn parse_expression_at_lowest(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_expression(Precedence::Lowest)
    }

    fn parse_comma_separated<T>(
        &mut self,
        closing: &Token,
        parse_item: impl Fn(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<Vec<T>, Diagnostic> {
        let mut items = Vec::new();
        if self.is_at(closing) {
            return Ok(items);
        }

        items.push(parse_item(self)?);
        while self.is_at(&Token::Comma) {
            self.advance();
            items.push(parse_item(self)?);
        }
        Ok(items)
    }

    /* -------------------------------------------------------------------- cursor */

    fn current(&self) -> &SpannedToken {
        &self.tokens[self.position.min(self.tokens.len() - 1)]
    }

    fn current_token(&self) -> &Token {
        &self.current().token
    }

    fn current_span(&self) -> Span {
        self.current().span
    }

    fn advance(&mut self) {
        if !self.is_at(&Token::EndOfInput) {
            self.position += 1;
        }
    }

    fn is_at(&self, expected: &Token) -> bool {
        discriminant(self.current_token()) == discriminant(expected)
    }

    fn expect(&mut self, expected: Token) -> Result<Span, Diagnostic> {
        if !self.is_at(&expected) {
            return Err(self.error_at_current(format!("expected {}", expected.describe())));
        }
        let span = self.current_span();
        self.advance();
        Ok(span)
    }

    fn expect_identifier(&mut self) -> Result<Identifier, Diagnostic> {
        let Token::Identifier(name) = self.current_token().clone() else {
            return Err(self.error_at_current("expected an identifier"));
        };
        let span = self.current_span();
        self.advance();
        Ok(Identifier {
            name: name.clone(),
            span,
            node_id: self.get_node_id(name),
        })
    }

    fn expect_statement_terminator(&mut self) -> Result<Span, Diagnostic> {
        self.expect(Token::Semicolon)
            .map_err(|diagnostic| diagnostic.with_help("this statement must end with `;`"))
    }

    fn error_at_current(&self, message: impl Into<String>) -> Diagnostic {
        let found = self.current_token().describe_as_found();
        let diagnostic = Diagnostic::error(
            self.current_span(),
            format!("{}, found {found}", message.into()),
        );

        if self.is_at(&Token::EndOfInput) {
            diagnostic.caused_by_end_of_input()
        } else {
            diagnostic
        }
    }

    fn callee_of(&mut self, expression: Expression) -> Result<Identifier, Diagnostic> {
        let span = expression.span;
        match expression.kind {
            ExpressionKind::Identifier(identifier) => Ok(Identifier {
                name: identifier.name.clone(),
                span,
                node_id: identifier.node_id,
            }),
            _ => Err(Diagnostic::error(span, "cannot call this expression")
                .with_help("only a named function can be called")),
        }
    }

    fn assignment_target_of(
        &mut self,
        expression: Expression,
    ) -> Result<AssignmentTarget, Diagnostic> {
        let span = expression.span;
        match expression.kind {
            ExpressionKind::Identifier(identifier) => Ok(AssignmentTarget::Variable(Identifier {
                name: identifier.name.clone(),
                span,
                node_id: identifier.node_id,
            })),
            _ => Err(Diagnostic::error(span, "cannot assign to this expression")
                .with_help("the left of `=` must be a variable")),
        }
    }

    fn target_span(&self, target: &AssignmentTarget) -> Span {
        match target {
            AssignmentTarget::Variable(name) => name.span,
        }
    }

    fn conditional_end_span(&self, conditional: &Conditional) -> Span {
        match conditional.otherwise.as_deref() {
            None => conditional.then.span,
            Some(Else::Block(block)) => block.span,
            Some(Else::If(nested)) => self.conditional_end_span(nested),
        }
    }

    fn precedence_of(&self, token: &Token) -> Precedence {
        match token {
            Token::Assign => Precedence::Assignment,
            Token::BooleanOr => Precedence::BooleanOr,
            Token::BooleanAnd => Precedence::BooleanAnd,
            Token::Equal | Token::NotEqual => Precedence::Equality,
            Token::Less | Token::LessOrEqual | Token::Greater | Token::GreaterOrEqual => {
                Precedence::Comparison
            }
            Token::Plus | Token::Minus => Precedence::Sum,
            Token::Star | Token::Slash | Token::Percent => Precedence::Product,
            Token::LeftParenthesis => Precedence::Call,
            _ => Precedence::Lowest,
        }
    }

    fn binary_operator_of(&self, token: &Token) -> Option<BinaryOperator> {
        Some(match token {
            Token::Plus => BinaryOperator::Add,
            Token::Minus => BinaryOperator::Subtract,
            Token::Star => BinaryOperator::Multiply,
            Token::Slash => BinaryOperator::Divide,
            Token::Percent => BinaryOperator::Remainder,
            Token::Equal => BinaryOperator::Equal,
            Token::NotEqual => BinaryOperator::NotEqual,
            Token::Less => BinaryOperator::Less,
            Token::LessOrEqual => BinaryOperator::LessOrEqual,
            Token::Greater => BinaryOperator::Greater,
            Token::GreaterOrEqual => BinaryOperator::GreaterOrEqual,
            Token::BooleanAnd => BinaryOperator::BooleanAnd,
            Token::BooleanOr => BinaryOperator::BooleanOr,
            _ => return None,
        })
    }
}
