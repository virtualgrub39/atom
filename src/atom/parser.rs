use std::iter::Peekable;

use crate::atom::{AtomError, AtomResult, Lexer, Span, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

type SpannedNode = Spanned<Node>;

#[derive(Debug)]
pub struct Program {
    pub nodes: Vec<SpannedNode>,
}

impl Program {
    pub fn parse<'a>(buf: &'a str) -> AtomResult<Self> {
        let mut parser = Parser::new(buf);
        let nodes = parser.parse_stream(None)?;

        Ok(Self { nodes })
    }
}

#[derive(Debug)]
pub enum Node {
    Number(f64),
    String(String),
    Nil,

    Builtin(Builtin), // pure "words" - postfix operations

    WordRef(String),
    BindVar(String),
    FetchVar(String),

    Block(Vec<SpannedNode>),
    List(Vec<SpannedNode>),

    Define {
        name: String,
        body: Box<SpannedNode>,
    },

    If {
        then_br: Box<SpannedNode>,
        else_br: Box<SpannedNode>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Out,
    Dup,
    Over,
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Not,
    Nip,
    Drop,
    This,
    StringCast,
    Head,
    Tail,
    Swap,
    Add,  // +
    Sub,  // -
    Mult, // *
    Div,  // /
    Mod,  // %
    Eval, // !
    Cons, // ::
    Join, // ++
    Times,
    WhileDo,
    IfThen,
    IfThenElse,
}

pub struct Parser<'a> {
    lexer: Peekable<Lexer<'a>>,
    src: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            lexer: Lexer::new(src).peekable(),
            src,
        }
    }

    fn unescape(input: &str) -> AtomResult<String> {
        let mut out = String::with_capacity(input.len());
        let mut chars = input.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }

            let Some((_, esc)) = chars.next() else {
                return Err(AtomError::TrailingBackslash);
            };

            match esc {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                '"' => out.push('"'),
                'a' => out.push('\x07'),
                'b' => out.push('\x08'),
                'f' => out.push('\x0c'),
                'v' => out.push('\x0b'),
                other => {
                    return Err(AtomError::InvalidEscape {
                        index: i,
                        ch: other,
                    });
                }
            }
        }

        Ok(out)
    }

    fn parse_token(&mut self, token: Token) -> AtomResult<SpannedNode> {
        let start_span = token.span;

        let node = match token.kind {
            TokenKind::StringLiteral => {
                let raw = self.lexeme(&token);
                let content = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                    &raw[1..raw.len() - 1]
                } else {
                    raw
                };
                Node::String(Self::unescape(content)?)
            }

            TokenKind::NumberLiteral => {
                let raw = self.lexeme(&token);
                let val = raw.parse::<f64>().map_err(|_| {
                    self.error(
                        token.span,
                        format!("Failed to parse number literal: {}", raw),
                    )
                })?;
                Node::Number(val)
            }

            TokenKind::Nil => Node::Nil,

            TokenKind::DollarIdent => {
                let name = self.lexeme(&token)[1..].to_string();
                let body = self.parse_next_single_node("definition body")?;
                Node::Define {
                    name,
                    body: Box::new(body),
                }
            }
            TokenKind::HashIdent => Node::WordRef(self.lexeme(&token)[1..].to_string()),
            TokenKind::GreaterIdent => Node::BindVar(self.lexeme(&token)[1..].to_string()),
            TokenKind::LessIdent => Node::FetchVar(self.lexeme(&token)[1..].to_string()),

            TokenKind::LQuote => {
                let block_nodes = self.parse_stream(Some(TokenKind::RQuote))?;
                self.next_token();
                Node::Block(block_nodes)
            }
            TokenKind::LList => {
                let list_nodes = self.parse_stream(Some(TokenKind::RList))?;
                self.next_token();
                Node::List(list_nodes)
            }

            // TODO: prefix while
            TokenKind::If => {
                let then_br = self.parse_next_single_node("`then` branch")?;

                let else_br = if self
                    .peek_token()
                    .map_or(false, |t| t.kind == TokenKind::Else)
                {
                    self.next_token();
                    self.parse_next_single_node("`else` branch")?
                } else {
                    Spanned {
                        node: Node::Block(vec![]),
                        span: token.span,
                    }
                };

                Node::If {
                    then_br: Box::new(then_br),
                    else_br: Box::new(else_br),
                }
            }

            other => {
                if let Some(builtin) = self.match_builtin(other) {
                    Node::Builtin(builtin)
                } else {
                    return Err(
                        self.error(token.span, format!("Unrecognized primitive: {:?}", other))
                    );
                }
            }
        };

        let end_pos = self.peek_token().map_or(token.span.end, |t| t.span.start);

        Ok(Spanned {
            node,
            span: Span {
                start: start_span.start,
                end: end_pos,
            },
        })
    }

    fn parse_next_single_node(&mut self, context: &str) -> AtomResult<SpannedNode> {
        let next_tok = self.next_token().ok_or_else(|| AtomError::SyntaxError {
            message: format!("Unexpected EOF: missing {}", context),
            line: 0,
            column: 0,
        })?;

        self.parse_token(next_tok)
    }

    fn match_builtin(&self, kind: TokenKind) -> Option<Builtin> {
        match kind {
            TokenKind::Out => Some(Builtin::Out),
            TokenKind::Dup => Some(Builtin::Dup),
            TokenKind::Over => Some(Builtin::Over),
            TokenKind::Lt => Some(Builtin::Lt),
            TokenKind::Lte => Some(Builtin::Lte),
            TokenKind::Eq => Some(Builtin::Eq),
            TokenKind::Not => Some(Builtin::Not),
            TokenKind::Nip => Some(Builtin::Nip),
            TokenKind::Drop => Some(Builtin::Drop),
            TokenKind::This => Some(Builtin::This),
            TokenKind::String => Some(Builtin::StringCast),
            TokenKind::Head => Some(Builtin::Head),
            TokenKind::Tail => Some(Builtin::Tail),
            TokenKind::Swap => Some(Builtin::Swap),
            TokenKind::Plus => Some(Builtin::Add),
            TokenKind::Asterisk => Some(Builtin::Mult),
            TokenKind::Slash => Some(Builtin::Div),
            TokenKind::Percent => Some(Builtin::Mod),
            TokenKind::Minus => Some(Builtin::Sub),
            TokenKind::Bang => Some(Builtin::Eval),
            TokenKind::Cons => Some(Builtin::Cons),
            TokenKind::Join => Some(Builtin::Join),
            TokenKind::WhileDo => Some(Builtin::WhileDo),
            TokenKind::Times => Some(Builtin::Times),
            TokenKind::Ift => Some(Builtin::IfThen),
            TokenKind::Ifte => Some(Builtin::IfThenElse),
            _ => None,
        }
    }

    fn lexeme(&self, token: &Token) -> &str {
        &self.src[token.span.start..token.span.end]
    }

    fn error(&self, span: Span, message: impl Into<String>) -> AtomError {
        let (line, column) = span.to_location(self.src);
        AtomError::SyntaxError {
            message: message.into(),
            line,
            column,
        }
    }

    fn peek_token(&mut self) -> Option<&Token> {
        while self
            .lexer
            .peek()
            .map_or(false, |t| t.kind == TokenKind::Comment)
        {
            let _ = self.lexer.next();
        }

        self.lexer.peek()
    }

    fn next_token(&mut self) -> Option<Token> {
        self.peek_token()?;
        self.lexer.next()
    }

    pub fn parse_stream(&mut self, until: Option<TokenKind>) -> AtomResult<Vec<SpannedNode>> {
        let mut nodes = Vec::new();

        while let Some(peeked) = self.peek_token() {
            if let Some(ref delim) = until {
                if peeked.kind == *delim {
                    break;
                }
            }

            let token = self.next_token().unwrap();
            let spanned_node = self.parse_token(token)?;
            nodes.push(spanned_node);
        }

        if let Some(delim) = until {
            if self.peek_token().is_none() {
                return Err(AtomError::SyntaxError {
                    message: format!("Unclosed block: expected closing {:?}", delim),
                    line: 0,
                    column: 0,
                });
            }
        }

        Ok(nodes)
    }
}

use std::fmt;

pub trait DisplayWithSrc {
    fn fmt_with_src(&self, f: &mut fmt::Formatter<'_>, src: &str, indent: usize) -> fmt::Result;

    fn display<'a>(&'a self, src: &'a str) -> WithSrc<'a, Self>
    where
        Self: Sized,
    {
        WithSrc { value: self, src }
    }
}

pub struct WithSrc<'a, T: ?Sized> {
    value: &'a T,
    src: &'a str,
}

impl<'a, T: DisplayWithSrc> fmt::Display for WithSrc<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt_with_src(f, self.src, 0)
    }
}

impl<T: DisplayWithSrc> DisplayWithSrc for Spanned<T> {
    fn fmt_with_src(&self, f: &mut fmt::Formatter<'_>, src: &str, indent: usize) -> fmt::Result {
        let (line, col) = self.span.to_location(src);
        write!(f, "@{line}:{col} ")?;
        self.node.fmt_with_src(f, src, indent)
    }
}

impl DisplayWithSrc for Node {
    fn fmt_with_src(&self, f: &mut fmt::Formatter<'_>, src: &str, indent: usize) -> fmt::Result {
        let pad = "  ".repeat(indent);
        match self {
            Node::Number(n) => write!(f, "Number({n})"),
            Node::String(s) => write!(f, "String({s:?})"),
            Node::Nil => write!(f, "Nil"),
            Node::Builtin(b) => write!(f, "{b:?}"),
            Node::WordRef(name) => write!(f, "WordRef({name})"),
            Node::BindVar(name) => write!(f, "BindVar(<{name})"),
            Node::FetchVar(name) => write!(f, "FetchVar(${name})"),

            Node::Block(nodes) => fmt_children(f, src, indent, &pad, "Block", nodes),
            Node::List(nodes) => fmt_children(f, src, indent, &pad, "List", nodes),

            Node::Define { name, body } => {
                writeln!(f, "Define({name}) {{")?;
                write!(f, "{pad}  body: ")?;
                body.fmt_with_src(f, src, indent + 1)?;
                write!(f, "\n{pad}}}")
            }
            Node::If { then_br, else_br } => {
                writeln!(f, "If {{")?;
                write!(f, "{pad}  then: ")?;
                then_br.fmt_with_src(f, src, indent + 1)?;
                writeln!(f)?;
                write!(f, "{pad}  else: ")?;
                else_br.fmt_with_src(f, src, indent + 1)?;
                write!(f, "\n{pad}}}")
            }
        }
    }
}

fn fmt_children(
    f: &mut fmt::Formatter<'_>,
    src: &str,
    indent: usize,
    pad: &str,
    label: &str,
    nodes: &[SpannedNode],
) -> fmt::Result {
    writeln!(f, "{label} [")?;
    for node in nodes {
        write!(f, "{pad}  ")?;
        node.fmt_with_src(f, src, indent + 1)?;
        writeln!(f)?;
    }
    write!(f, "{pad}]")
}

impl DisplayWithSrc for Program {
    fn fmt_with_src(&self, f: &mut fmt::Formatter<'_>, src: &str, indent: usize) -> fmt::Result {
        let pad = "  ".repeat(indent);
        writeln!(f, "Program [")?;
        for node in &self.nodes {
            write!(f, "{pad}  ")?;
            node.fmt_with_src(f, src, indent + 1)?;
            writeln!(f)?;
        }
        write!(f, "{pad}]")
    }
}
