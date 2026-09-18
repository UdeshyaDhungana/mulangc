//! Tokens to a [`Program`], by Pratt parsing.
//!
//! Two conventions hold throughout, and everything else follows from them:
//!
//! 1. Every `parse_*` method consumes exactly the tokens of the construct it
//!    names and leaves the cursor on the first token after it. There is no
//!    "current versus peek" bookkeeping at call sites.
//! 2. A failed parse is a [`Diagnostic`] carried by `Result`, so a mistake
//!    deep in an expression cannot be mistaken for a successfully parsed node.
//!
//! After an error the parser resynchronises at the next statement boundary and
//! keeps going, so one run reports every broken statement in the file.

use std::mem::discriminant;
use std::rc::Rc;

use crate::diagnostics::{Diagnostic, Span};
use crate::syntax::ast::*;
use crate::syntax::token::{SpannedToken, Token};

/// Binding strength of an infix operator. Ordered weakest to strongest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest,
    Assignment,
    Equality,
    Comparison,
    Sum,
    Product,
    Prefix,
    Call,
}

pub fn parse(tokens: &[SpannedToken]) -> Result<Program, Vec<Diagnostic>> {
    let mut parser = Parser {
        tokens,
        position: 0,
        diagnostics: Vec::new(),
    };
    let statements = parser.parse_program();

    if parser.diagnostics.is_empty() {
        Ok(Program { statements })
    } else {
        Err(parser.diagnostics)
    }
}

struct Parser<'tokens> {
    tokens: &'tokens [SpannedToken],
    position: usize,
    diagnostics: Vec<Diagnostic>,
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
                    // A parse method that failed without consuming anything
                    // would otherwise leave us looping on the same token.
                    if self.position == position_before_statement {
                        self.advance();
                    }
                }
            }
        }
        statements
    }

    /// Skips ahead to somewhere a new statement can plausibly begin, so that
    /// one syntax error does not cascade into a page of follow-on complaints.
    fn recover_at_statement_boundary(&mut self) {
        while !self.is_at(&Token::EndOfInput) {
            if matches!(self.current_token(), Token::Semicolon) {
                self.advance();
                return;
            }
            if matches!(
                self.current_token(),
                Token::Declare | Token::Return | Token::If | Token::While | Token::For
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
            span: start.joined_with(conditional_end_span(&conditional)),
            kind: StatementKind::If(conditional),
        })
    }

    /// The `(condition) { … } [natra …]` that follows a `yedi`.
    ///
    /// `natra` is decided by one token of lookahead: a `yedi` after it means
    /// another conditional, and this method recurses; anything else must be the
    /// closing block. That lookahead is the whole of what used to be a separate
    /// `nabhae` keyword.
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

        Ok(Conditional {
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

        Ok(Statement {
            span: start.joined_with(body.span),
            kind: StatementKind::While {
                condition: Box::new(condition),
                body,
            },
        })
    }

    fn parse_for(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.expect(Token::For)?;
        self.expect(Token::LeftParenthesis)?;

        // The header's own `;` separators are mandatory, unlike the optional
        // terminator of a free-standing expression statement.
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

        Ok(Statement {
            span: start.joined_with(body.span),
            kind: StatementKind::For {
                initialiser,
                condition: Box::new(condition),
                update: Box::new(update),
                body,
            },
        })
    }

    /// An expression on its own. The trailing `;` is still optional here;
    /// making it mandatory is the next step.
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

        while precedence < precedence_of(self.current_token()) {
            left = self.parse_infix(left)?;
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expression, Diagnostic> {
        let span = self.current_span();

        let kind = match self.current_token().clone() {
            Token::Identifier(name) => {
                self.advance();
                ExpressionKind::Identifier(name)
            }
            Token::Integer(value) => {
                self.advance();
                ExpressionKind::Integer(value)
            }
            Token::True | Token::False => {
                let is_true = self.is_at(&Token::True);
                self.advance();
                ExpressionKind::Boolean(is_true)
            }

            Token::Minus | Token::Bang => return self.parse_unary(),
            Token::LeftParenthesis => return self.parse_grouping(),
            Token::Function => return self.parse_function(),

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
        let operator = binary_operator_of(self.current_token())
            .ok_or_else(|| self.error_at_current("expected a binary operator"))?;
        let precedence = precedence_of(self.current_token());
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

    /// `=` binds loosest of all operators and associates to the right, so the
    /// value is parsed at the lowest precedence: `a = b = c` groups as
    /// `a = (b = c)`, and `a = b + c` takes the whole sum.
    fn parse_assignment(&mut self, left: Expression) -> Result<Expression, Diagnostic> {
        self.expect(Token::Assign)?;
        let target = assignment_target_of(left)?;
        let value = self.parse_expression(Precedence::Lowest)?;

        Ok(Expression {
            span: target_span(&target).joined_with(value.span),
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
        self.expect(Token::LeftParenthesis)?;
        let arguments =
            self.parse_comma_separated(&Token::RightParenthesis, Self::parse_expression_at_lowest)?;
        let end = self.expect(Token::RightParenthesis)?;

        Ok(Expression {
            span: callee.span.joined_with(end),
            kind: ExpressionKind::Call {
                callee: Box::new(callee),
                arguments,
            },
        })
    }

    fn parse_function(&mut self) -> Result<Expression, Diagnostic> {
        let start = self.expect(Token::Function)?;
        self.expect(Token::LeftParenthesis)?;
        let parameters =
            self.parse_comma_separated(&Token::RightParenthesis, Self::expect_identifier)?;
        self.expect(Token::RightParenthesis)?;
        let body = self.parse_block()?;

        Ok(Expression {
            span: start.joined_with(body.span),
            kind: ExpressionKind::Function(Rc::new(FunctionDefinition { parameters, body })),
        })
    }

    fn parse_expression_at_lowest(&mut self) -> Result<Expression, Diagnostic> {
        self.parse_expression(Precedence::Lowest)
    }

    /// Reads `item (, item)*` up to but not including `closing`, which the
    /// caller consumes. Shared by parameters and arguments, which are otherwise
    /// identical and both permit being empty.
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
        // `tokenize` always terminates the stream with `EndOfInput`, and no
        // parse method advances past it.
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

    /// Compares token *kinds*, ignoring any value a literal carries.
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
        Ok(Identifier { name, span })
    }

    /// `thoos` and `de` both require a closing `;`, unlike a
    /// free-standing expression statement where it is optional.
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
}

/// Rejects assignment to anything that does not name a place a value can live.
fn assignment_target_of(expression: Expression) -> Result<AssignmentTarget, Diagnostic> {
    let span = expression.span;
    match expression.kind {
        ExpressionKind::Identifier(name) => {
            Ok(AssignmentTarget::Variable(Identifier { name, span }))
        }
        _ => Err(Diagnostic::error(span, "cannot assign to this expression")
            .with_help("the left of `=` must be a variable")),
    }
}

fn target_span(target: &AssignmentTarget) -> Span {
    match target {
        AssignmentTarget::Variable(name) => name.span,
    }
}

/// The span of the last block in a chain, wherever the chain ends.
fn conditional_end_span(conditional: &Conditional) -> Span {
    match conditional.otherwise.as_deref() {
        None => conditional.then.span,
        Some(Else::Block(block)) => block.span,
        Some(Else::If(nested)) => conditional_end_span(nested),
    }
}

fn precedence_of(token: &Token) -> Precedence {
    match token {
        Token::Assign => Precedence::Assignment,
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

fn binary_operator_of(token: &Token) -> Option<BinaryOperator> {
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
        _ => return None,
    })
}
