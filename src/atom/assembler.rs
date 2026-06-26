use crate::atom::{Atom, AtomRef, Opcode, bytecode::Writer};

pub struct Assembler {
    writer: Writer,
}

impl Assembler {
    pub fn new() -> Self {
        Self { writer: Writer::new() }
    }

    pub fn op(mut self, op: Opcode) -> Self {
        self.writer.write(op);
        self
    }

    pub fn push_env(mut self, name: &str) -> Self {
        self.writer.write(Opcode::PushEnv);
        self.writer.write(name);
        self
    }

    pub fn to_ret(mut self, count: u16) -> Self {
        self.writer.write(Opcode::ToRet);
        self.writer.write(count);
        self
    }
    pub fn fetch_ret(mut self, id: u16) -> Self {
        self.writer.write(Opcode::FetchRet);
        self.writer.write(id);
        self
    }
    pub fn drop_ret(mut self, count: u16) -> Self {
        self.writer.write(Opcode::DropRet);
        self.writer.write(count);
        self
    }

    pub fn add(self) -> Self { self.op(Opcode::Add) }
    pub fn join(self) -> Self { self.op(Opcode::Join) }
    pub fn cons(self) -> Self { self.op(Opcode::Cons) }
    pub fn out(self) -> Self { self.op(Opcode::Out) }
    pub fn drop(self) -> Self { self.op(Opcode::Drop) }
    pub fn dup(self) -> Self { self.op(Opcode::Dup) }
    pub fn eval(self) -> Self { self.op(Opcode::Eval) }
    pub fn while_do(self) -> Self { self.op(Opcode::WhileDo) }
    pub fn do_times(self) -> Self { self.op(Opcode::DoTimes) }
    pub fn if_then_else(self) -> Self { self.op(Opcode::IfThenElse) }
    pub fn this(self) -> Self { self.op(Opcode::This) }
    pub fn halt(self) -> Self { self.op(Opcode::Halt) }

    pub fn block(self) -> AtomRef {
        Atom::blob(self.writer.finish())
    }
}
