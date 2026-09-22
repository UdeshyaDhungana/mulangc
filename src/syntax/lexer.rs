use crate::diagnostics::{Diagnostic, Span};
use crate::syntax::token::{SpannedToken, Token, keyword_or_identifier};

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
                '(' => self.push(Token::LeftParenthesis, start),
                ')' => self.push(Token::RightParenthesis, start),
                '{' => self.push(Token::LeftBrace, start),
                '}' => self.push(Token::RightBrace, start),

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

                digit if digit.is_ascii_digit() => self.scan_number(start),
                first if is_name_start(first) => self.scan_name(start),

                unrecognised => self.emit(Diagnostic::error(
                    Span::new(start, self.offset),
                    format!("unrecognised character `{unrecognised}`"),
                )),
            }
        }
    }

    fn skip_ignored_text(&mut self) {
        loop {
            match self.current() {
                Some(character) if character.is_whitespace() => {
                    self.advance();
                }
                Some('/') if self.source[self.offset..].starts_with("//") => {
                    self.advance_while(|character| character != '\n');
                }
                _ => return,
            }
        }
    }

    fn scan_number(&mut self, start: usize) {
        self.advance_while(|character| character.is_ascii_digit());

        if self.current() == Some('.') {
            self.advance();
            self.advance_while(|character| character.is_ascii_digit());
            self.emit(Diagnostic::error(
                Span::new(start, self.offset),
                "mulang has no decimal numbers",
            ));
            return;
        }

        let literal = &self.source[start..self.offset];
        match literal.parse::<i64>() {
            Ok(value) => self.push(Token::Integer(value), start),
            Err(_) => self.emit(Diagnostic::error(
                Span::new(start, self.offset),
                format!("integer `{literal}` does not fit in 64 bits"),
            )),
        }
    }

    fn scan_name(&mut self, start: usize) {
        self.advance_while(is_name_continuation);
        let word = &self.source[start..self.offset];
        self.push(keyword_or_identifier(word), start);
    }

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
            tokens_of("thoos x = 43;"),
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
            tokens_of("// a comment ; with tokens in it\n1"),
            vec![Token::Integer(1), Token::EndOfInput]
        );
    }

    #[test]
    fn a_comment_ends_at_the_newline() {
        assert_eq!(
            tokens_of("1 // two\n3"),
            vec![Token::Integer(1), Token::Integer(3), Token::EndOfInput]
        );
    }

    #[test]
    fn a_comment_may_run_to_the_end_of_input() {
        assert_eq!(
            tokens_of("1 // never followed by a newline"),
            vec![Token::Integer(1), Token::EndOfInput]
        );
    }

    #[test]
    fn a_lone_slash_is_still_division() {
        assert_eq!(
            tokens_of("6 / 2"),
            vec![
                Token::Integer(6),
                Token::Slash,
                Token::Integer(2),
                Token::EndOfInput
            ]
        );
    }

    #[test]
    fn two_separated_slashes_are_two_divisions() {
        assert_eq!(
            tokens_of("8 / 2 / 2"),
            vec![
                Token::Integer(8),
                Token::Slash,
                Token::Integer(2),
                Token::Slash,
                Token::Integer(2),
                Token::EndOfInput
            ]
        );
    }

    #[test]
    fn rejects_string_literals() {
        let diagnostics = tokenize("\"hello\"").expect_err("should fail");
        assert_eq!(diagnostics[0].message, "unrecognised character `\"`");
    }

    #[test]
    fn rejects_decimal_numbers() {
        let diagnostics = tokenize("42 69.69").expect_err("should fail");
        assert_eq!(diagnostics[0].message, "mulang has no decimal numbers");
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
        let tokens = tokenize("  thoos").expect("should lex");
        assert_eq!(tokens[0].span, Span::new(2, 7));
    }
}
