mod assembler;
pub mod bytecode;
pub mod interpreter;
pub use assembler::Assembler;
pub use interpreter::Interpreter;
mod lexer;
pub use lexer::{Lexer, Span, Token, TokenKind};
mod parser;
pub use parser::{Builtin, DisplayWithSrc, Node, Parser, Program, Spanned};
mod compiler;
pub use compiler::Compiler;

use std::{fmt, rc::Rc};

use num_enum::TryFromPrimitiveError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtomError {
    #[error("Malformed bytecode")]
    MalformedBytecode,
    #[error("Data stack underflow")]
    StackUnderflow,
    #[error("Return stack underflow")]
    RetStackUnderflow,
    #[error("Type missmatch")]
    TypeMismatch,
    #[error("Halted")]
    Halt,
    #[error("Invalid environment reference: {0}")]
    InvalidEnvId(Rc<str>),
    #[error("Invalid opcode")]
    InvalidOpcode(#[from] TryFromPrimitiveError<Opcode>),
    #[error("Invalid magic number")]
    InvalidMagic,
    #[error("Node could not be mapped to an atom")]
    UnmappableNode,
    #[error("Unbound variable: {0}")]
    UnboundVariable(String),
    #[error("Invalid escape sequence at index {index}: '\\{ch}'")]
    InvalidEscape { index: usize, ch: char },
    #[error("Trailing backslash in string literal")]
    TrailingBackslash,
    #[error("Syntax error at {line}:{column}: {message}")]
    SyntaxError {
        message: String,
        line: usize,
        column: usize,
    },
}

pub type AtomResult<T> = Result<T, AtomError>;

#[derive(Debug, PartialEq)]
pub enum Atom {
    Nil,
    Cons(AtomRef, AtomRef),
    Blob(Vec<u8>),
    Str(String),
    Num(f64),
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Atom::Nil => write!(f, "nil"),
            Atom::Cons(car, cdr) => write!(f, "({} . {})", car, cdr),
            Atom::Blob(bytes) => {
                write!(f, "#[")?;
                for (i, byte) in bytes.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{:02x}", byte)?;
                }
                write!(f, "]")
            }
            Atom::Str(s) => write!(f, "{}", s),
            Atom::Num(n) => write!(f, "{}", n),
        }
    }
}

pub type AtomRef = Rc<Atom>;

impl Atom {
    pub fn nil() -> AtomRef {
        Rc::new(Atom::Nil)
    }

    pub fn cons(head: AtomRef, tail: AtomRef) -> AtomRef {
        Rc::new(Atom::Cons(head, tail))
    }

    pub fn blob(blob: Vec<u8>) -> AtomRef {
        Rc::new(Atom::Blob(blob))
    }

    pub fn num(num: f64) -> AtomRef {
        Rc::new(Atom::Num(num))
    }

    pub fn string(str: String) -> AtomRef {
        Rc::new(Atom::Str(str))
    }

    pub fn str(s: &str) -> AtomRef {
        Rc::new(Atom::Str(s.to_string()))
    }

    pub fn boolean(b: bool) -> AtomRef {
        Rc::new(Atom::Num(b.into()))
    }

    pub fn truthful(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Cons(head, tail) => head.truthful() || tail.truthful(),
            Self::Blob(b) => !b.is_empty(),
            Self::Str(s) => !s.is_empty(),
            Self::Num(n) => *n != 0.,
        }
    }

    pub fn tag(&self) -> AtomTag {
        match self {
            Atom::Nil => AtomTag::Nil,
            Atom::Num(_) => AtomTag::Num,
            Atom::Str(_) => AtomTag::Str,
            Atom::Cons(_, _) => AtomTag::Cons,
            Atom::Blob(_) => AtomTag::Blob,
        }
    }
}

use num_enum::TryFromPrimitive;

#[derive(TryFromPrimitive, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AtomTag {
    Nil = 0,
    Num = 1,
    Str = 2,
    Cons = 3,
    Blob = 4,
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum Opcode {
    PushEnv,

    Add,
    Sub,
    Mult,
    Div,
    Mod,

    Join,
    Cons,
    Head,
    Tail,

    This,

    Out,

    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Not,

    Eval,
    IfThenElse,
    IfThen,
    WhileDo,
    DoTimes,
    Halt,

    ToRet,
    FetchRet,
    DropRet,

    StringCast,

    Dup,
    Swap,
    Over,
    Nip,
    Drop,
}

impl From<Builtin> for Opcode {
    fn from(value: Builtin) -> Self {
        match value {
            Builtin::Add => Opcode::Add,
            Builtin::Sub => Opcode::Sub,
            Builtin::Mult => Opcode::Mult,
            Builtin::Div => Opcode::Div,
            Builtin::Mod => Opcode::Mod,
            Builtin::Join => Opcode::Join,
            Builtin::Cons => Opcode::Cons,
            Builtin::Head => Opcode::Head,
            Builtin::Tail => Opcode::Tail,
            Builtin::This => Opcode::This,
            Builtin::Out => Opcode::Out,
            Builtin::Lt => Opcode::Lt,
            Builtin::Lte => Opcode::Lte,
            Builtin::Gt => Opcode::Gt,
            Builtin::Gte => Opcode::Gte,
            Builtin::Eq => Opcode::Eq,
            Builtin::Not => Opcode::Not,
            Builtin::Eval => Opcode::Eval,
            Builtin::IfThenElse => Opcode::IfThenElse,
            Builtin::IfThen => Opcode::IfThen,
            Builtin::WhileDo => Opcode::WhileDo,
            Builtin::Times => Opcode::DoTimes,
            // Builtin::Halt => Opcode::Halt,

            // Builtin::ToRet => Opcode::ToRet,
            // Builtin::FetchRet => Opcode::FetchRet,
            // Builtin::DropRet => Opcode::DropRet,

            Builtin::StringCast => Opcode::StringCast,

            Builtin::Dup => Opcode::Dup,
            Builtin::Swap => Opcode::Swap,
            Builtin::Over => Opcode::Over,
            Builtin::Nip => Opcode::Nip,
            Builtin::Drop => Opcode::Drop,

        }
    }
}
