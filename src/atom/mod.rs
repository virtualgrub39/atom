pub mod assembler;
pub mod bytecode;

use std::rc::Rc;

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
}

pub type AtomResult<T> = Result<T, AtomError>;

#[derive(Debug)]
pub enum Atom {
    Nil,
    Cons(AtomRef, AtomRef),
    Blob(Vec<u8>),
    Str(String),
    Num(f64),
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

    pub fn truthful(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Cons(head, tail) => head.truthful() || tail.truthful(),
            Self::Blob(b) => !b.is_empty(),
            Self::Str(s) => !s.is_empty(),
            Self::Num(n) => *n != 0.,
        }
    }
}

use num_enum::TryFromPrimitive;

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum Opcode {
    Add,
    Join,
    Cons,
    Out,
    PushEnv,
    IfThenElse,
    Dup,
    Eval,
    WhileDo,
    DoTimes,
    Drop,
    ToRet,
    FetchRet,
    DropRet,
    This,
    Halt
}
