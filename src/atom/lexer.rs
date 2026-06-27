#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenKind {
    StringLiteral,
    // Keywords
    Out,
    // Tagged Identifiers
    LessIdent,    // <name
    GreaterIdent, // >name
    DollarIdent,  // $name
    HashIdent,    // #name
    // Fallback/Errors
    Invalid,
}

#[derive(Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
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
        let ch = self.peek()?;

        match ch {
            '"' => {
                self.bump();
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
                Some(Token {
                    kind: if terminated {
                        TokenKind::StringLiteral
                    } else {
                        TokenKind::Invalid
                    },
                    span: Span {
                        start,
                        end: self.pos,
                    },
                })
            }

            '<' | '>' | '$' | '#' => {
                self.bump(); // Consume the tag prefix

                let kind = match ch {
                    '<' => TokenKind::LessIdent,
                    '>' => TokenKind::GreaterIdent,
                    '$' => TokenKind::DollarIdent,
                    '#' => TokenKind::HashIdent,
                    _ => unreachable!(),
                };

                if let Some(first_char) = self.peek() {
                    if self.is_ident_start(first_char) {
                        self.consume_ident_body();
                        Some(Token {
                            kind,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        })
                    } else {
                        Some(Token {
                            kind: TokenKind::Invalid,
                            span: Span {
                                start,
                                end: self.pos,
                            },
                        })
                    }
                } else {
                    Some(Token {
                        kind: TokenKind::Invalid,
                        span: Span {
                            start,
                            end: self.pos,
                        },
                    })
                }
            }

            c if self.is_ident_start(c) => {
                self.consume_ident_body();
                let span = Span {
                    start,
                    end: self.pos,
                };

                let kind = self.match_keyword(&span).unwrap_or(TokenKind::Invalid);

                Some(Token { kind, span })
            }

            _ => {
                self.bump();
                Some(Token {
                    kind: TokenKind::Invalid,
                    span: Span {
                        start,
                        end: self.pos,
                    },
                })
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

    fn match_keyword(&self, span: &Span) -> Option<TokenKind> {
        let word = &self.source[span.start..span.end];
        match word {
            "out" => Some(TokenKind::Out),
            _ => None,
        }
    }

    fn is_reserved(&self, c: char) -> bool {
        matches!(c, '<' | '>' | '$' | '#' | '!')
    }

    fn is_ident_start(&self, c: char) -> bool {
        (c.is_alphabetic() || c == '_') && !self.is_reserved(c)
    }

    fn is_ident_continue(&self, c: char) -> bool {
        (c.is_alphanumeric() || c == '_') && !self.is_reserved(c)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
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
