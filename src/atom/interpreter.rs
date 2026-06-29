use std::{collections::HashMap, rc::Rc};

use crate::atom::{Atom, AtomError, AtomRef, AtomResult, AtomTag, Opcode, bytecode};

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

pub struct Interpreter {
    pub stack: Vec<AtomRef>,
    pub ret_stack: Vec<AtomRef>,
    pub env: HashMap<Rc<str>, AtomRef>,
    exec_stack: Vec<ExecFrame>,
}

impl Interpreter {
    pub fn load_atom_file(&mut self, bytes: &[u8]) -> AtomResult<()> {
        let mut reader = bytecode::Reader::new(bytes);

        let magic = reader.fetch_vec(4)?;
        if magic != b"ATOM" {
            return Err(AtomError::InvalidMagic);
        }

        let count = reader.fetch::<u32>()?;

        for _ in 0..count {
            let key: Rc<str> = Rc::from(reader.fetch_str()?);
            let value = self.deserialize_atom(&mut reader)?;
            self.env.insert(key, value);
        }

        Ok(())
    }

    pub fn deserialize_atom(&mut self, reader: &mut bytecode::Reader) -> AtomResult<AtomRef> {
        let tag = reader.fetch::<u8>()?;
        let tag = AtomTag::try_from(tag).map_err(|_| AtomError::MalformedBytecode)?;

        match tag {
            AtomTag::Nil => Ok(Atom::nil()),
            AtomTag::Num => {
                let num = reader.fetch::<f64>()?;
                Ok(Atom::num(num))
            }
            AtomTag::Str => {
                let s: Rc<str> = Rc::from(reader.fetch_str()?);
                Ok(Atom::string(s.to_string()))
            }
            AtomTag::Cons => {
                let head = self.deserialize_atom(reader)?;
                let tail = self.deserialize_atom(reader)?;
                Ok(Atom::cons(head, tail))
            }
            AtomTag::Blob => {
                let bytes = reader.fetch_vec(reader.len())?;
                Ok(Atom::blob(bytes))
            }
        }
    }

    pub fn import(&mut self, env: HashMap<Rc<str>, AtomRef>) {
        for (key, value) in env {
            self.env.insert(key, value);
        }
    }

    pub fn new() -> Interpreter {
        Self {
            stack: Vec::new(),
            ret_stack: Vec::new(),
            env: HashMap::new(),
            exec_stack: Vec::new(),
        }
    }

    pub fn register(&mut self, id: &str, a: AtomRef) {
        self.env.insert(id.into(), a);
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

    fn push_bool(&mut self, b: bool) {
        self.stack.push(Atom::boolean(b));
    }

    fn apply_num_op(&mut self, op: impl FnOnce(f64, f64) -> f64) -> AtomResult<()> {
        let b = self.pop_num()?;
        let a = self.pop_num()?;
        self.push_num(op(a, b));
        Ok(())
    }

    fn apply_cmp_op(&mut self, op: impl FnOnce(f64, f64) -> bool) -> AtomResult<()> {
        let b = self.pop_num()?;
        let a = self.pop_num()?;
        self.push_bool(op(a, b));
        Ok(())
    }

    fn run(&mut self, atom: AtomRef) -> AtomResult<()> {
        match &*atom {
            Atom::Blob(_) => {
                self.exec_stack.push(ExecFrame::new(atom));
                Ok(())
            }
            _ => self.eval(atom),
        }
    }

    pub fn eval(&mut self, a: AtomRef) -> AtomResult<()> {
        let mut work: Vec<AtomRef> = vec![a];

        while let Some(atom) = work.pop() {
            match &*atom {
                Atom::Nil => {}
                Atom::Num(_) | Atom::Str(_) => self.stack.push(atom),
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

            let mut reader = bytecode::Reader::new(&bytes[ip..]);
            let initial_len = reader.len();

            let op_byte = reader.fetch::<u8>()?;
            let op = Opcode::try_from(op_byte)?;

            self.execute_op(op, &mut reader)?;

            let consumed = initial_len - reader.len();
            if let Some(frame) = self.exec_stack.get_mut(frame_idx) {
                frame.ip += consumed;
            }
        }
        Ok(())
    }

    fn execute_op(&mut self, op: Opcode, reader: &mut bytecode::Reader) -> AtomResult<()> {
        match op {
            Opcode::Eval => self.pop().and_then(|a| self.run(a)),
            Opcode::StringCast => self
                .pop()
                .map(|a| self.stack.push(Atom::string(a.to_string()))),

            Opcode::Add => self.apply_num_op(|a, b| a + b),
            Opcode::Sub => self.apply_num_op(|a, b| a - b),
            Opcode::Lt => self.apply_cmp_op(|a, b| a < b),
            Opcode::Lte => self.apply_cmp_op(|a, b| a <= b),
            Opcode::Gt => self.apply_cmp_op(|a, b| a > b),
            Opcode::Gte => self.apply_cmp_op(|a, b| a >= b),

            Opcode::Eq => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push_bool(a == b);
                Ok(())
            }
            Opcode::Not => self.pop().map(|a| self.push_bool(!a.truthful())),

            Opcode::Join => {
                let b = self.pop()?;
                let a = self.pop()?;
                match (&*a, &*b) {
                    (Atom::Blob(a), Atom::Blob(b)) => {
                        let mut joined = Vec::with_capacity(a.len() + b.len());
                        joined.extend(a);
                        joined.extend(b);
                        self.stack.push(Atom::blob(joined));
                        Ok(())
                    }
                    (Atom::Str(a), Atom::Str(b)) => {
                        let mut joined = String::with_capacity(a.len() + b.len());
                        joined.push_str(a);
                        joined.push_str(b);
                        self.stack.push(Atom::string(joined));
                        Ok(())
                    }
                    _ => Err(AtomError::TypeMismatch),
                }
            }
            Opcode::Cons => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(Atom::cons(b, a));
                Ok(())
            }
            Opcode::Head => match &*self.pop()? {
                Atom::Cons(head, _) => {
                    self.stack.push(head.clone());
                    Ok(())
                }
                _ => Err(AtomError::TypeMismatch),
            },
            Opcode::Tail => match &*self.pop()? {
                Atom::Cons(_, tail) => {
                    self.stack.push(tail.clone());
                    Ok(())
                }
                _ => Err(AtomError::TypeMismatch),
            },
            Opcode::Out => self.pop().map(|val| print!("{val}")),
            Opcode::PushEnv => {
                let id_ref: Rc<str> = Rc::from(reader.fetch_str()?);
                let a = self
                    .env
                    .get(&id_ref)
                    .ok_or_else(|| AtomError::InvalidEnvId(id_ref))?;
                self.stack.push(a.clone());
                Ok(())
            }
            Opcode::IfThenElse => {
                let else_body = self.pop()?;
                let then_body = self.pop()?;
                let target = if self.pop()?.truthful() {
                    then_body
                } else {
                    else_body
                };
                self.run(target)
            }
            Opcode::IfThen => {
                let then_body = self.pop()?;
                if self.pop()?.truthful() {
                    self.run(then_body)
                } else {
                    Ok(())
                }
            }
            Opcode::Dup => {
                let top = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or(AtomError::StackUnderflow)?;
                self.stack.push(top);
                Ok(())
            }
            Opcode::Over => {
                let len = self.stack.len();
                if len < 2 {
                    return Err(AtomError::StackUnderflow);
                }
                let a = self.stack[len - 2].clone();
                self.stack.push(a);
                Ok(())
            }
            Opcode::Nip => {
                let b = self.pop()?;
                self.pop()?;
                self.stack.push(b);
                Ok(())
            }
            Opcode::Swap => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.stack.push(b);
                self.stack.push(a);
                Ok(())
            }
            Opcode::WhileDo => {
                let body = self.pop()?;
                let cond = self.pop()?;
                loop {
                    self.eval(cond.clone())?;
                    if !self.pop()?.truthful() {
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
            Opcode::Drop => self.pop().map(|_| ()),
            Opcode::ToRet => {
                for _ in 0..reader.fetch::<u16>()? {
                    let v = self.pop()?;
                    self.ret_stack.push(v);
                }
                Ok(())
            }
            Opcode::FetchRet => {
                let idx = reader.fetch::<u16>()? as usize;
                let idx = self
                    .ret_stack
                    .len()
                    .checked_sub(idx + 1)
                    .ok_or(AtomError::RetStackUnderflow)?;
                let v = self
                    .ret_stack
                    .get(idx)
                    .ok_or(AtomError::RetStackUnderflow)?
                    .clone();
                self.stack.push(v);
                Ok(())
            }
            Opcode::DropRet => {
                for _ in 0..reader.fetch::<u16>()? {
                    self.ret_stack.pop().ok_or(AtomError::RetStackUnderflow)?;
                }
                Ok(())
            }
            Opcode::This => {
                let this = self.exec_stack.last().ok_or(AtomError::MalformedBytecode)?;
                self.stack.push(this.bytecode_atom.clone());
                Ok(())
            }
            Opcode::Halt => Err(AtomError::Halt),
        }
    }
}
