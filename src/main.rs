use std::{collections::HashMap, rc::Rc};

use num_enum::{TryFromPrimitive, TryFromPrimitiveError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtomError {
    #[error("Malformed bytecode")]
    MalformedBytecode,
    #[error("Data stack underflow")]
    StackUnderflow,
    #[error("Type missmatch")]
    TypeMismatch,
    #[error("Invalid environment reference: {0}")]
    InvalidEnvId(u16),
    #[error("Invalid opcode")]
    InvalidOpcode(#[from] TryFromPrimitiveError<Opcode>),
}

pub type AtomResult<T> = Result<T, AtomError>;

#[derive(Debug)]
pub enum Atom {
    Nil,
    Cons(AtomRef, AtomRef),
    Blob(Vec<u8>),
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

    pub fn truthful(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Cons(_, _) => false,
            Self::Blob(b) => !b.is_empty(),
            Self::Num(n) => *n != 0.,
        }
    }
}

#[derive(TryFromPrimitive)]
#[repr(u8)]
pub enum Opcode {
    Add,
    Out,
    PushEnv,
    IfThenElse,
    Dup,
    WhileDo,
    DoTimes,
}

pub trait Readable: Sized {
    const SIZE: usize;
    fn from_bytes(bytes: &[u8]) -> Self;
}

impl Readable for u8 {
    const SIZE: usize = 1;
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0]
    }
}

impl Readable for u16 {
    const SIZE: usize = 2;
    fn from_bytes(bytes: &[u8]) -> Self {
        u16::from_le_bytes([bytes[0], bytes[1]])
    }
}
pub struct ByteReader<'a> {
    data: &'a [u8],
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> ByteReader<'a> {
        return Self { data };
    }

    pub fn fetch<T: Readable>(&mut self) -> AtomResult<T> {
        if self.data.len() < T::SIZE {
            return Err(AtomError::MalformedBytecode);
        }

        let (bytes, rest) = self.data.split_at(T::SIZE);
        self.data = rest;
        Ok(T::from_bytes(bytes))
    }

    pub fn fetch_vec(&mut self, n: usize) -> AtomResult<Vec<u8>> {
        if self.data.len() < n {
            return Err(AtomError::MalformedBytecode);
        }

        let (bytes, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(bytes.to_vec())
    }
}

struct ExecFrame {
    bytecode_atom: AtomRef,
    ip: usize,
}

impl ExecFrame {
    pub fn new(bytecode_atom: AtomRef) -> Self {
        Self {
            bytecode_atom,
            ip: 0,
        }
    }
}

struct Interpreter {
    stack: Vec<AtomRef>,
    exec_stack: Vec<ExecFrame>,
    env: HashMap<u16, AtomRef>,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Self {
            stack: Vec::new(),
            exec_stack: Vec::new(),
            env: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: u16, a: AtomRef) {
        self.env.insert(id, a);
    }

    fn pop(&mut self) -> AtomResult<AtomRef> {
        self.stack.pop().ok_or(AtomError::StackUnderflow)
    }

    fn pop_num(&mut self) -> AtomResult<f64> {
        match &*self.pop()? {
            Atom::Num(n) => Ok(*n),
            _ => Err(AtomError::TypeMismatch),
        }
    }

    fn push_num(&mut self, n: f64) {
        self.stack.push(Atom::num(n));
    }

    pub fn eval(&mut self, a: AtomRef) -> AtomResult<()> {
        let mut work: Vec<AtomRef> = vec![a];

        while let Some(atom) = work.pop() {
            match &*atom {
                Atom::Nil => {}
                Atom::Num(_) => self.stack.push(atom),
                Atom::Cons(head, tail) => {
                    work.push(tail.clone());
                    work.push(head.clone());
                }
                Atom::Blob(_) => {
                    self.exec_stack.push(ExecFrame::new(atom.clone()));
                    self.exec()?;
                }
            }
        }

        Ok(())
    }

    pub fn exec(&mut self) -> AtomResult<()> {
        let target_depth = self.exec_stack.len();

        while self.exec_stack.len() >= target_depth {
            let frame_idx = self.exec_stack.len() - 1;

            let (bytecode_atom, ip) = {
                let frame = &self.exec_stack[frame_idx];
                (frame.bytecode_atom.clone(), frame.ip)
            };

            let bytes = match &*bytecode_atom {
                Atom::Blob(b) => b,
                _ => return Err(AtomError::MalformedBytecode),
            };

            if ip >= bytes.len() {
                self.exec_stack.pop();
                continue;
            }

            let mut reader = ByteReader::new(&bytes[ip..]);
            let initial_len = reader.data.len();

            let op_byte = reader.fetch::<u8>()?;
            let op = Opcode::try_from(op_byte)?;

            self.execute_op(op, &mut reader)?;

            let consumed = initial_len - reader.data.len();
            if let Some(frame) = self.exec_stack.get_mut(frame_idx) {
                frame.ip += consumed;
            }
        }

        Ok(())
    }

    fn execute_op(&mut self, op: Opcode, reader: &mut ByteReader) -> AtomResult<()> {
        match op {
            Opcode::Add => {
                let b = self.pop_num()?;
                let a = self.pop_num()?;
                self.push_num(a + b);
                Ok(())
            }
            Opcode::Out => {
                let val = self.pop()?;
                println!("VM Output: {:?}", val);
                Ok(())
            }
            Opcode::PushEnv => {
                let id = reader.fetch::<u16>()?;
                let a = self.env.get(&id).ok_or(AtomError::InvalidEnvId(id))?;
                self.stack.push(a.clone());
                Ok(())
            }
            Opcode::IfThenElse => {
                let else_body = self.pop()?;
                let then_body = self.pop()?;
                let condition = self.pop()?.truthful();

                if condition {
                    self.eval(then_body)
                } else {
                    self.eval(else_body)
                }
            }
            Opcode::Dup => {
                let top = self.stack.last().ok_or(AtomError::StackUnderflow)?;
                self.stack.push(top.clone());
                Ok(())
            }
            Opcode::WhileDo => {
                let body = self.pop()?;
                let cond = self.pop()?;

                loop {
                    self.eval(cond.clone())?;

                    let cond_result = self.pop()?;
                    if !cond_result.truthful() {
                        break;
                    }

                    self.eval(body.clone())?;
                }

                Ok(())
            }
            Opcode::DoTimes => {
                let times = self.pop_num()?;
                let body = self.pop()?;

                for _ in 0..times as u32 {
                    self.eval(body.clone())?;
                }

                Ok(())
            }
        }
    }
}

fn main() -> AtomResult<()> {
    let mut vm = Interpreter::new();

    vm.register(1, Atom::num(-1.));
    vm.register(2, Atom::blob(vec![Opcode::Dup as u8]));
    vm.register(
        3,
        Atom::blob(vec![
            Opcode::Dup as u8,
            Opcode::Out as u8,
            Opcode::PushEnv as u8,
            1,
            0,
            Opcode::Add as u8,
        ]),
    );

    vm.register(
        0,
        Atom::blob(vec![
            Opcode::PushEnv as u8,
            2,
            0,
            Opcode::PushEnv as u8,
            3,
            0,
            Opcode::WhileDo as u8,
        ]),
    );

    vm.stack.push(Atom::num(10.));

    if let Some(main) = vm.env.get(&0) {
        vm.eval(main.clone())?;
    }

    println!("Final Data Stack State: {:?}", vm.stack);
    Ok(())
}
