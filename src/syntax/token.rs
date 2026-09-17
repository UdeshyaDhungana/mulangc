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
    Float(f64),
    Text(String),

    Declare,  // thoos_muji
    Function, // kaam_gar_muji
    Return,   // patha_muji
    If,       // yedi_muji
    ElseIf,   // nabhae_muji
    Else,     // nabhae_chikne
    True,     // sacho_muji
    False,    // jhut_muji
    While,    // jaba_samma_muji
    For,      // ghuma_muji

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,

    Assign,
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
        "thoos_muji" => Token::Declare,
        "kaam_gar_muji" => Token::Function,
        "patha_muji" => Token::Return,
        "yedi_muji" => Token::If,
        "nabhae_muji" => Token::ElseIf,
        "nabhae_chikne" => Token::Else,
        "sacho_muji" => Token::True,
        "jhut_muji" => Token::False,
        "jaba_samma_muji" => Token::While,
        "ghuma_muji" => Token::For,
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
            Token::Float(_) => "a decimal number".to_owned(),
            Token::Text(_) => "a string".to_owned(),
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
            Token::Float(value) => format!("`{value}`"),
            Token::Text(value) => format!("the string \"{value}\""),
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
            Token::Float(value) => return write!(formatter, "{value}"),
            Token::Text(value) => return write!(formatter, "\"{value}\""),

            Token::Declare => "thoos_muji",
            Token::Function => "kaam_gar_muji",
            Token::Return => "patha_muji",
            Token::If => "yedi_muji",
            Token::ElseIf => "nabhae_muji",
            Token::Else => "nabhae_chikne",
            Token::True => "sacho_muji",
            Token::False => "jhut_muji",
            Token::While => "jaba_samma_muji",
            Token::For => "ghuma_muji",

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
