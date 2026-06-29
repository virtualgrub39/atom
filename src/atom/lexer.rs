#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn to_location(&self, src: &str) -> (usize, usize) {
        let mut line: usize = 1;
        let mut column: usize = 1;
        let mut prev = '\0';

        for (i, c) in src.chars().enumerate() {
            if i == self.start {
                break;
            }
            match c {
                '\n' if prev == '\r' => {}
                '\r' | '\n' => {
                    line += 1;
                    column = 1;
                }
                _ => {
                    column += 1;
                }
            }
            prev = c;
        }

        (line, column)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TokenKind {
    StringLiteral,
    NumberLiteral,
    Comment,
    Invalid,
    // Tagged Identifiers
    LessIdent,
    GreaterIdent,
    DollarIdent,
    HashIdent,
    // Keywords
    Out,
    Times,
    Dup,
    Over,
    If,
    Else,
    Ift,
    Ifte,
    Lt,
    Lte,
    Eq,
    Not,
    Nip,
    Drop,
    This,
    String,
    Nil,
    Head,
    Tail,
    Swap,
    While,
    WhileDo,
    // Symbols
    LQuote,
    RQuote,
    LList,
    RList,
    Join,
    Bang,
    Plus,
    Minus,
    Cons,
    Asterisk,
    Slash,
    Percent,
}

#[derive(Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub struct TokenDisplay<'a> {
    token: &'a Token,
    src: &'a str,
}

use std::fmt;

impl<'a> fmt::Display for TokenDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (line, col) = self.token.span.to_location(self.src);

        let lexeme: String = self
            .src
            .chars()
            .skip(self.token.span.start)
            .take(self.token.span.end - self.token.span.start)
            .collect();

        write!(f, "{:?}({:?}) @ {}:{}", self.token.kind, lexeme, line, col)
    }
}

impl Token {
    pub fn display<'a>(&'a self, src: &'a str) -> TokenDisplay<'a> {
        TokenDisplay { token: self, src }
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, pos: 0 }
    }

    pub fn next_token(&mut self) -> Option<Token> {
        self.skip_ws();

        if self.pos >= self.source.len() {
            return None;
        }

        let start = self.pos;

        if self.source[self.pos..].starts_with("--") {
            self.bump();
            self.bump();
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                self.bump();
            }
            return Some(self.make_token(start, TokenKind::Comment));
        }

        let ch = self.peek()?;

        let kind = match ch {
            '"' => self.lex_string(),

            '<' | '>' | '$' | '#' => self.lex_tagged_ident(ch),

            c if self.is_ident_start(c) => self.lex_keyword_or_invalid(),

            c if c.is_ascii_digit() => {
                self.consume_number();
                TokenKind::NumberLiteral
            }

            '+' => {
                self.bump();
                if self.match_next('+') {
                    TokenKind::Join
                } else {
                    TokenKind::Plus
                }
            }
            ':' => {
                self.bump();
                if self.match_next(':') {
                    TokenKind::Cons
                } else {
                    TokenKind::Invalid
                }
            }

            '!' => {
                self.bump();
                TokenKind::Bang
            }
            '-' => {
                self.bump();
                TokenKind::Minus
            }
            '*' => {
                self.bump();
                TokenKind::Asterisk
            }
            '/' => {
                self.bump();
                TokenKind::Slash
            }
            '%' => {
                self.bump();
                TokenKind::Percent
            }
            '[' => {
                self.bump();
                TokenKind::LQuote
            }
            ']' => {
                self.bump();
                TokenKind::RQuote
            }
            '(' => {
                self.bump();
                TokenKind::LList
            }
            ')' => {
                self.bump();
                TokenKind::RList
            }

            _ => {
                self.bump();
                TokenKind::Invalid
            }
        };

        Some(self.make_token(start, kind))
    }

    // --- Sub-Lexers ---

    fn lex_string(&mut self) -> TokenKind {
        self.bump(); // Consume opening quote
        let mut terminated = false;
        while let Some(c) = self.peek() {
            if c == '\\' {
                self.bump();
                if self.peek().is_some() {
                    self.bump();
                }
            } else if c == '"' {
                self.bump();
                terminated = true;
                break;
            } else {
                self.bump();
            }
        }
        if terminated {
            TokenKind::StringLiteral
        } else {
            TokenKind::Invalid
        }
    }

    fn lex_tagged_ident(&mut self, prefix: char) -> TokenKind {
        self.bump(); // Consume prefix character
        if let Some(first) = self.peek() {
            if self.is_ident_start(first) {
                self.consume_ident_body();
                return match prefix {
                    '<' => TokenKind::LessIdent,
                    '>' => TokenKind::GreaterIdent,
                    '$' => TokenKind::DollarIdent,
                    '#' => TokenKind::HashIdent,
                    _ => unreachable!(),
                };
            }
        }
        TokenKind::Invalid
    }

    fn lex_keyword_or_invalid(&mut self) -> TokenKind {
        let start = self.pos;
        self.consume_ident_body();
        let word = &self.source[start..self.pos];

        match word {
            "out" => TokenKind::Out,
            "over" => TokenKind::Over,
            "times" => TokenKind::Times,
            "dup" => TokenKind::Dup,
            "string" => TokenKind::String,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "ift" => TokenKind::Ift,
            "ifte" => TokenKind::Ifte,
            "lt" => TokenKind::Lt,
            "lte" => TokenKind::Lte,
            "nip" => TokenKind::Nip,
            "drop" => TokenKind::Drop,
            "this" => TokenKind::This,
            "nil" => TokenKind::Nil,
            "eq" => TokenKind::Eq,
            "head" => TokenKind::Head,
            "tail" => TokenKind::Tail,
            "swap" => TokenKind::Swap,
            "while" => TokenKind::While,
            "whiledo" => TokenKind::WhileDo,
            "not" => TokenKind::Not,
            _ => TokenKind::Invalid,
        }
    }

    // --- State Mutation Helpers ---

    fn match_next(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn make_token(&self, start: usize, kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span {
                start,
                end: self.pos,
            },
        }
    }

    fn consume_number(&mut self) {
        self.consume_digits();
        if self.peek() == Some('.') {
            if let Some(next) = self.peek_over() {
                if next.is_ascii_digit() {
                    self.bump();
                    self.consume_digits();
                }
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            let mut ahead = self.source[self.pos..].chars();
            ahead.next();
            let mut next_char = ahead.next();
            if matches!(next_char, Some('+' | '-')) {
                next_char = ahead.next();
            }

            if let Some(c) = next_char {
                if c.is_ascii_digit() {
                    self.bump();
                    if matches!(self.peek(), Some('+' | '-')) {
                        self.bump();
                    }
                    self.consume_digits();
                }
            }
        }
    }

    fn consume_digits(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn consume_ident_body(&mut self) {
        while let Some(c) = self.peek() {
            if self.is_ident_continue(c) {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn is_reserved(&self, c: char) -> bool {
        matches!(
            c,
            '<' | '>' | '$' | '#' | '!' | '+' | '-' | '[' | ']' | '(' | ')' | ':'
        )
    }

    fn is_ident_start(&self, c: char) -> bool {
        (c.is_alphabetic() || c == '_') && !self.is_reserved(c)
    }

    fn is_ident_continue(&self, c: char) -> bool {
        !self.is_reserved(c) && !c.is_whitespace()
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_over(&self) -> Option<char> {
        let mut chars = self.source[self.pos..].chars();
        chars.next();
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}
