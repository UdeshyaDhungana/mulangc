use std::fmt;

use crate::diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum VariableTypeToken {
    IntType,
    BooleanType,
    VoidType,
    // CustomType(Identifier),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Identifier(String),
    Integer(i64),

    Declare,  // thoos
    Function, // dhanda
    Return,   // de
    If,       // yedi
    Else,     // natra
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

    TypeSpecifier,
    VariableType(VariableTypeToken),

    EndOfInput,
}

#[derive(Clone, Debug)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

pub fn keyword_or_identifier(word: &str) -> Token {
    match word {
        "thoos" => Token::Declare,
        "dhanda" => Token::Function,
        "de" => Token::Return,
        "yedi" => Token::If,
        "natra" => Token::Else,
        "jabasamma" => Token::While,
        "fanfani" => Token::For,
        "ra" => Token::BooleanAnd,
        "wa" => Token::BooleanOr,
        // List all builtin types here and compare them on Token::from_type_str
        // WARN: If you add a new builtin, add the corresponding branch to from_type_str as well
        "int" => Token::from_type_str("int"),
        "bool" => Token::from_type_str("bool"),
        "void" => Token::from_type_str("void"),
        _ => Token::Identifier(word.to_owned()),
    }
}

impl Token {
    pub fn describe(&self) -> String {
        match self {
            Token::Identifier(_) => "an identifier".to_owned(),
            Token::Integer(_) => "an integer".to_owned(),
            Token::EndOfInput => "end of input".to_owned(),
            spelled => format!("`{spelled}`"),
        }
    }

    pub fn from_type_str(type_str: &str) -> Self {
        match type_str {
            "int" => Token::VariableType(VariableTypeToken::IntType),
            "bool" => Token::VariableType(VariableTypeToken::BooleanType),
            "void" => Token::VariableType(VariableTypeToken::VoidType),
            _ => todo!("TOKEN:: custom types not supported yet"),
        }
    }

    pub fn describe_as_found(&self) -> String {
        match self {
            Token::Identifier(name) => format!("`{name}`"),
            Token::Integer(value) => format!("`{value}`"),
            other => other.describe(),
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let spelling = match self {
            Token::Identifier(name) => return formatter.write_str(name),
            Token::Integer(value) => return write!(formatter, "{value}"),

            Token::Declare => "thoos",
            Token::Function => "dhanda",
            Token::Return => "de",
            Token::If => "yedi",
            Token::Else => "natra",
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

            Token::TypeSpecifier => ":",
            Token::VariableType(variable_type) => match variable_type {
                VariableTypeToken::IntType => "int",
                VariableTypeToken::BooleanType => "bool",
                VariableTypeToken::VoidType => "void",
            },

            Token::EndOfInput => "",
        };
        formatter.write_str(spelling)
    }
}
