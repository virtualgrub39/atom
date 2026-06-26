use std::{collections::HashMap, rc::Rc};

mod atom;

use atom::{Atom, AtomError, AtomRef, AtomResult, Opcode, assembler::Assembler, bytecode};

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
    ret_stack: Vec<AtomRef>,
    exec_stack: Vec<ExecFrame>,
    env: HashMap<Rc<str>, AtomRef>,
}

impl Interpreter {
    pub fn new() -> Interpreter {
        Self {
            stack: Vec::new(),
            ret_stack: Vec::new(),
            exec_stack: Vec::new(),
            env: HashMap::new(),
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

    /// executes the `Atom::Blob` contents as bytecode
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
            Opcode::Eval => {
                let a = self.pop()?;
                match &*a {
                    Atom::Blob(_) => {
                        self.exec_stack.push(ExecFrame::new(a));
                        Ok(())
                    }
                    _ => self.eval(a),
                }
            }
            Opcode::Add => {
                let b = self.pop_num()?;
                let a = self.pop_num()?;
                self.push_num(a + b);
                Ok(())
            }
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
                self.stack.push(Atom::cons(b.clone(), a.clone()));
                Ok(())
            }
            Opcode::Out => {
                let val = self.pop()?;
                println!("VM Output: {:?}", val);
                Ok(())
            }
            Opcode::PushEnv => {
                // let id = reader.fetch::<u16>()?;
                // let a = self.env.get(&id).ok_or(AtomError::InvalidEnvId(id))?;
                let id = reader.fetch_str()?;
                let id_ref: Rc<str> = Rc::from(id);
                let a = self
                    .env
                    .get(&id_ref)
                    .ok_or(AtomError::InvalidEnvId(id_ref))?;

                self.stack.push(a.clone());
                Ok(())
            }
            Opcode::IfThenElse => {
                let else_body = self.pop()?;
                let then_body = self.pop()?;
                let condition = self.pop()?.truthful();

                let target = if condition { then_body } else { else_body };
                match &*target {
                    Atom::Blob(_) => {
                        self.exec_stack.push(ExecFrame::new(target));
                        Ok(())
                    }
                    _ => self.eval(target),
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
            Opcode::Drop => self.pop().map(|_| ()),
            Opcode::ToRet => {
                let count = reader.fetch::<u16>()? as usize;
                for _ in 0..count {
                    let v = self.pop()?;
                    self.ret_stack.push(v);
                }
                Ok(())
            }
            Opcode::FetchRet => {
                let idx = reader.fetch::<u16>()? as usize;
                let idx = self.ret_stack.len() - idx - 1;
                let v = &self.ret_stack[idx];
                self.stack.push(v.clone());
                Ok(())
            }
            Opcode::DropRet => {
                let count = reader.fetch::<u16>()? as usize;
                for _ in 0..count {
                    self.ret_stack.pop().ok_or(AtomError::RetStackUnderflow)?;
                }
                Ok(())
            }
            Opcode::This => {
                let this = self.exec_stack.last().unwrap();
                let this = &this.bytecode_atom;
                self.stack.push(this.clone());
                Ok(())
            }
            Opcode::Halt => Err(AtomError::Halt),
        }
    }
}

fn main() -> AtomResult<()> {
    let mut vm = Interpreter::new();

    vm.register("-1", Atom::num(-1.));
    vm.register("n", Atom::num(1e6)); // not that big...
    vm.register("34", Atom::num(34.));
    vm.register("35", Atom::num(35.));
    vm.register("fn", Assembler::new().add().out().block());

    // (defun fn (a b) (out (+ a b)))
    // (fn (34 35))
    vm.register("lispy_msg", Atom::str("lispy:"));
    let lispy = Assembler::new()
        .push_env("lispy_msg")
        .out()
        .push_env("fn")
        .push_env("35")
        .push_env("34")
        .cons()
        .cons()
        .dup()
        .out()
        .eval()
        .block();
    vm.register("lispy", lispy);

    // : fn + out ;
    // 34 35 fn
    vm.register("forthy_msg", Atom::str("forthy:"));
    let forthy = Assembler::new()
        .push_env("forthy_msg")
        .out()
        .push_env("34")
        .push_env("35")
        .push_env("fn")
        .eval()
        .block();
    vm.register("forthy", forthy);

    let recursive_else = Assembler::new().drop_ret(1).block();
    let recursive_then = Assembler::new().fetch_ret(0).eval().drop_ret(1).block();
    vm.register("recursive_else", recursive_else);
    vm.register("recursive_then", recursive_then);

    let recursive = Assembler::new()
        .this()
        .to_ret(1)
        // .dup()
        // .out()
        .push_env("-1")
        .add()
        .dup()
        .push_env("recursive_then")
        .push_env("recursive_else")
        .if_then_else()
        .block();
    vm.register("recursive", recursive);

    vm.register(
        "main",
        Assembler::new()
            .push_env("lispy")
            .eval()
            .push_env("forthy")
            .eval()
            .push_env("n")
            .push_env("recursive")
            .eval()
            .drop()
            .block(),
    );

    if let Some(main) = vm.env.get("main") {
        vm.eval(main.clone())?;
    }

    Ok(())
}
