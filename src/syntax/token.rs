//! The lexical vocabulary of mulang.
//!
//! Token variants carry conventional English names; the muji spelling of each
//! keyword lives in exactly two places in this file — [`keyword_or_identifier`]
//! and the [`Display`] impl — so the rest of the toolchain never has to know
//! how a construct is spelled on the page.

use std::fmt;

use crate::diagnostics::Span;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Identifier(String),
    Integer(i64),

    Declare,  // thoos
    Function, // dhanda
    Return,   // de
    If,       // yedi
    Else,     // natra
    True,     // ho
    False,    // haina
    While,    // jabasamma
    For,      // fanfani

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,

    Assign,
    BooleanAnd,
    BooleanOr,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,

    Comma,
    Semicolon,
    LeftParenthesis,
    RightParenthesis,
    LeftBrace,
    RightBrace,

    EndOfInput,
}

/// A token together with the stretch of source it was read from.
#[derive(Clone, Debug)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Classifies a word the lexer has just read. Reserved words become their own
/// token; anything else is a name the program chose.
pub fn keyword_or_identifier(word: &str) -> Token {
    match word {
        "thoos" => Token::Declare,
        "dhanda" => Token::Function,
        "de" => Token::Return,
        "yedi" => Token::If,
        "natra" => Token::Else,
        "ho" => Token::True,
        "haina" => Token::False,
        "jabasamma" => Token::While,
        "fanfani" => Token::For,
        "ra" => Token::BooleanAnd,
        "wa" => Token::BooleanOr,
        _ => Token::Identifier(word.to_owned()),
    }
}

impl Token {
    /// A noun phrase naming this token in an error message, e.g. `expected
    /// {expected}, found {found}`. Literals describe their category rather than
    /// their value, which reads better on the "expected" side of that sentence.
    pub fn describe(&self) -> String {
        match self {
            Token::Identifier(_) => "an identifier".to_owned(),
            Token::Integer(_) => "an integer".to_owned(),
            Token::EndOfInput => "end of input".to_owned(),
            spelled => format!("`{spelled}`"),
        }
    }

    /// The same as [`describe`](Token::describe), except that literals show the
    /// value that was actually found. Used on the "found" side of the message.
    pub fn describe_as_found(&self) -> String {
        match self {
            Token::Identifier(name) => format!("`{name}`"),
            Token::Integer(value) => format!("`{value}`"),
            other => other.describe(),
        }
    }
}

impl fmt::Display for Token {
    /// Writes the token back out as mulang source text.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let spelling = match self {
            Token::Identifier(name) => return formatter.write_str(name),
            Token::Integer(value) => return write!(formatter, "{value}"),

            Token::Declare => "thoos",
            Token::Function => "dhanda",
            Token::Return => "de",
            Token::If => "yedi",
            Token::Else => "natra",
            Token::True => "ho",
            Token::False => "haina",
            Token::While => "jabasamma",
            Token::For => "fanfani",

            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::Bang => "!",

            Token::Assign => "=",
            Token::Equal => "==",
            Token::NotEqual => "!=",
            Token::Less => "<",
            Token::LessOrEqual => "<=",
            Token::Greater => ">",
            Token::GreaterOrEqual => ">=",
            Token::BooleanAnd => "ra",
            Token::BooleanOr => "wa",

            Token::Comma => ",",
            Token::Semicolon => ";",
            Token::LeftParenthesis => "(",
            Token::RightParenthesis => ")",
            Token::LeftBrace => "{",
            Token::RightBrace => "}",

            Token::EndOfInput => "",
        };
        formatter.write_str(spelling)
    }
}
