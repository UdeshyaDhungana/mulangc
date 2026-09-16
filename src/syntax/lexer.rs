//! Source text to tokens.
//!
//! [`tokenize`] is the whole interface: characters in, a token stream out, or
//! every lexical problem the source contains. Scanning does not stop at the
//! first bad character, so a file with three stray symbols reports three errors
//! instead of three compile-and-fix cycles.

use crate::diagnostics::{Diagnostic, Span};
use crate::syntax::token::{SpannedToken, Token, keyword_or_identifier};

/// Scans `source` into a token stream terminated by [`Token::EndOfInput`].
pub fn tokenize(source: &str) -> Result<Vec<SpannedToken>, Vec<Diagnostic>> {
    let mut lexer = Lexer {
        source,
        offset: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };
    lexer.run();

    if lexer.diagnostics.is_empty() {
        Ok(lexer.tokens)
    } else {
        Err(lexer.diagnostics)
    }
}

struct Lexer<'source> {
    source: &'source str,
    offset: usize,
    tokens: Vec<SpannedToken>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> Lexer<'source> {
    fn run(&mut self) {
        loop {
            self.skip_ignored_text();
            let start = self.offset;

            let Some(character) = self.advance() else {
                self.push(Token::EndOfInput, start);
                return;
            };

            match character {
                '+' => self.push(Token::Plus, start),
                '-' => self.push(Token::Minus, start),
                '*' => self.push(Token::Star, start),
                '/' => self.push(Token::Slash, start),
                '%' => self.push(Token::Percent, start),
                ',' => self.push(Token::Comma, start),
                ';' => self.push(Token::Semicolon, start),
                ':' => self.push(Token::Colon, start),
                '(' => self.push(Token::LeftParenthesis, start),
                ')' => self.push(Token::RightParenthesis, start),
                '{' => self.push(Token::LeftBrace, start),
                '}' => self.push(Token::RightBrace, start),
                '[' => self.push(Token::LeftBracket, start),
                ']' => self.push(Token::RightBracket, start),

                '=' => {
                    self.push_one_or_two_character_token(Token::Assign, '=', Token::Equal, start)
                }
                '!' => {
                    self.push_one_or_two_character_token(Token::Bang, '=', Token::NotEqual, start)
                }
                '<' => self.push_one_or_two_character_token(
                    Token::Less,
                    '=',
                    Token::LessOrEqual,
                    start,
                ),
                '>' => self.push_one_or_two_character_token(
                    Token::Greater,
                    '=',
                    Token::GreaterOrEqual,
                    start,
                ),

                '"' => self.scan_string(start),
                digit if digit.is_ascii_digit() => self.scan_number(start),
                first if is_name_start(first) => self.scan_name(start),

                unrecognised => self.emit(Diagnostic::error(
                    Span::new(start, self.offset),
                    format!("unrecognised character `{unrecognised}`"),
                )),
            }
        }
    }

    /// Consumes whitespace and `$ … $` comments until real content is next.
    fn skip_ignored_text(&mut self) {
        loop {
            match self.current() {
                Some(character) if character.is_whitespace() => {
                    self.advance();
                }
                Some('$') => {
                    let start = self.offset;
                    self.advance();
                    let is_terminated = loop {
                        match self.advance() {
                            Some('$') => break true,
                            Some(_) => continue,
                            None => break false,
                        }
                    };
                    if !is_terminated {
                        self.emit(
                            Diagnostic::error(Span::new(start, start + 1), "unterminated comment")
                                .with_help("comments open and close with `$`"),
                        );
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    fn scan_string(&mut self, start: usize) {
        let content_start = self.offset;
        loop {
            match self.advance() {
                Some('"') => break,
                Some(_) => continue,
                None => {
                    self.emit(Diagnostic::error(
                        Span::new(start, start + 1),
                        "unterminated string literal",
                    ));
                    return;
                }
            }
        }

        let content = self.source[content_start..self.offset - 1].to_owned();
        self.push(Token::Text(content), start);
    }

    /// Reads an integer, or a decimal number when a `.` follows the digits.
    fn scan_number(&mut self, start: usize) {
        self.advance_while(|character| character.is_ascii_digit());

        let has_fractional_part = self.current() == Some('.');
        if has_fractional_part {
            self.advance();
            let fraction_start = self.offset;
            self.advance_while(|character| character.is_ascii_digit());
            if self.offset == fraction_start {
                self.emit(Diagnostic::error(
                    Span::new(start, self.offset),
                    "decimal point must be followed by at least one digit",
                ));
                return;
            }
        }

        let literal = &self.source[start..self.offset];
        let span = Span::new(start, self.offset);

        if has_fractional_part {
            match literal.parse::<f64>() {
                Ok(value) => self.push(Token::Float(value), start),
                Err(_) => self.emit(Diagnostic::error(
                    span,
                    format!("`{literal}` is not a valid decimal number"),
                )),
            }
        } else {
            match literal.parse::<i64>() {
                Ok(value) => self.push(Token::Integer(value), start),
                Err(_) => self.emit(Diagnostic::error(
                    span,
                    format!("integer `{literal}` does not fit in 64 bits"),
                )),
            }
        }
    }

    fn scan_name(&mut self, start: usize) {
        self.advance_while(is_name_continuation);
        let word = &self.source[start..self.offset];
        self.push(keyword_or_identifier(word), start);
    }

    /// Emits `two_character` when `expected_second` follows, `one_character`
    /// otherwise. This is the `<` versus `<=` decision, shared by four operators.
    fn push_one_or_two_character_token(
        &mut self,
        one_character: Token,
        expected_second: char,
        two_character: Token,
        start: usize,
    ) {
        if self.current() == Some(expected_second) {
            self.advance();
            self.push(two_character, start);
        } else {
            self.push(one_character, start);
        }
    }

    fn push(&mut self, token: Token, start: usize) {
        self.tokens.push(SpannedToken {
            token,
            span: Span::new(start, self.offset),
        });
    }

    fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    fn current(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.current()?;
        self.offset += character.len_utf8();
        Some(character)
    }

    fn advance_while(&mut self, should_continue: impl Fn(char) -> bool) {
        while self.current().is_some_and(&should_continue) {
            self.advance();
        }
    }
}

fn is_name_start(character: char) -> bool {
    character.is_alphabetic() || character == '_'
}

fn is_name_continuation(character: char) -> bool {
    is_name_start(character) || character.is_numeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens_of(source: &str) -> Vec<Token> {
        tokenize(source)
            .expect("source should lex cleanly")
            .into_iter()
            .map(|spanned| spanned.token)
            .collect()
    }

    #[test]
    fn scans_a_declaration() {
        assert_eq!(
            tokens_of("thoos_muji x = 43;"),
            vec![
                Token::Declare,
                Token::Identifier("x".to_owned()),
                Token::Assign,
                Token::Integer(43),
                Token::Semicolon,
                Token::EndOfInput,
            ]
        );
    }

    #[test]
    fn prefers_the_two_character_operator() {
        assert_eq!(
            tokens_of("== != <= >= = ! < >"),
            vec![
                Token::Equal,
                Token::NotEqual,
                Token::LessOrEqual,
                Token::GreaterOrEqual,
                Token::Assign,
                Token::Bang,
                Token::Less,
                Token::Greater,
                Token::EndOfInput,
            ]
        );
    }

    #[test]
    fn skips_comments_entirely() {
        assert_eq!(
            tokens_of("$ a comment ; with tokens in it $ 1"),
            vec![Token::Integer(1), Token::EndOfInput]
        );
    }

    #[test]
    fn reports_an_unterminated_comment() {
        let diagnostics = tokenize("$ never closed").expect_err("should fail");
        assert_eq!(diagnostics[0].message, "unterminated comment");
    }

    #[test]
    fn reports_an_unterminated_string() {
        let diagnostics = tokenize("\"never closed").expect_err("should fail");
        assert_eq!(diagnostics[0].message, "unterminated string literal");
    }

    #[test]
    fn separates_integers_from_decimals() {
        assert_eq!(
            tokens_of("42 69.69"),
            vec![Token::Integer(42), Token::Float(69.69), Token::EndOfInput]
        );
    }

    #[test]
    fn allows_digits_inside_a_name() {
        assert_eq!(
            tokens_of("array2"),
            vec![Token::Identifier("array2".to_owned()), Token::EndOfInput]
        );
    }

    #[test]
    fn keeps_scanning_after_a_bad_character() {
        let diagnostics = tokenize("1 @ 2 # 3").expect_err("should fail");
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn spans_point_at_the_token_they_came_from() {
        let tokens = tokenize("  thoos_muji").expect("should lex");
        assert_eq!(tokens[0].span, Span::new(2, 12));
    }
}
