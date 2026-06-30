use std::{collections::HashMap, io, rc::Rc};

use crate::atom::{Atom, AtomError, AtomRef, AtomResult, AtomTag, Opcode, bytecode};

use libffi::middle::{self, Cif, CodePtr, Type, arg};
use std::ffi::{CString, c_char, c_void};

fn ptr_to_atom(p: *mut c_void) -> AtomRef {
    Atom::num(f64::from_bits(p as u64))
}
fn atom_to_ptr(a: &AtomRef) -> AtomResult<*mut c_void> {
    match &**a {
        Atom::Num(n) => Ok(n.to_bits() as usize as *mut c_void),
        _ => Err(AtomError::TypeMismatch),
    }
}

fn ffi_type_of(tag: &str) -> AtomResult<Type> {
    Ok(match tag {
        "void" => Type::void(),
        "int" => Type::c_int(),
        "i64" => Type::i64(),
        "double" | "f64" => Type::f64(),
        "string" | "ptr" => Type::pointer(),
        other => return Err(AtomError::UnboundVariable(other.into())),
    })
}

enum Native {
    I32(i32),
    I64(i64),
    F64(f64),
    Ptr(*mut c_void),
    CStr { _owner: CString, ptr: *const c_char },
}

impl Native {
    fn from_atom(atom: &AtomRef, tag: &str) -> AtomResult<Native> {
        Ok(match tag {
            "int" => Native::I32(match &**atom {
                Atom::Num(n) => *n as i32,
                _ => return Err(AtomError::TypeMismatch),
            }),
            "i64" => Native::I64(match &**atom {
                Atom::Num(n) => *n as i64,
                _ => return Err(AtomError::TypeMismatch),
            }),
            "double" | "f64" => Native::F64(match &**atom {
                Atom::Num(n) => *n,
                _ => return Err(AtomError::TypeMismatch),
            }),
            "ptr" => Native::Ptr(atom_to_ptr(atom)?),
            "string" => {
                let s = match &**atom {
                    Atom::Str(s) => s.clone(),
                    _ => return Err(AtomError::TypeMismatch),
                };
                let owner = CString::new(s).map_err(|_| AtomError::InvalidCasts)?;
                let ptr = owner.as_ptr();
                Native::CStr { _owner: owner, ptr }
            }
            other => return Err(AtomError::UnboundVariable(other.into())),
        })
    }

    fn as_ffi_arg(&self) -> middle::Arg<'_> {
        match self {
            Native::I32(v) => arg(v),
            Native::I64(v) => arg(v),
            Native::F64(v) => arg(v),
            Native::Ptr(p) => arg(p),
            Native::CStr { ptr, .. } => arg(ptr),
        }
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

    fn deserialize_atom(&mut self, reader: &mut bytecode::Reader) -> AtomResult<AtomRef> {
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
                let len = reader.fetch::<u32>()? as usize;
                let bytes = reader.fetch_vec(len)?;
                Ok(Atom::blob(bytes))
            }
        }
    }

    pub fn write_atom_file(&self) -> AtomResult<Vec<u8>> {
        let mut writer = bytecode::Writer::new();
        writer.write_bytes(b"ATOM");
        writer.write::<u32>(self.env.len() as u32);

        for (k, v) in self.env.iter() {
            writer.write(k.as_ref());
            self.serialize_atom(v.clone(), &mut writer)?;
        }

        Ok(writer.finish())
    }

    pub fn serialize_atom(&self, atom: AtomRef, writer: &mut bytecode::Writer) -> AtomResult<()> {
        match &*atom {
            Atom::Nil => {
                writer.write::<AtomTag>(AtomTag::Nil);
            }
            Atom::Num(n) => {
                writer.write::<AtomTag>(AtomTag::Num);
                writer.write::<f64>(*n);
            }
            Atom::Blob(b) => {
                writer.write::<AtomTag>(AtomTag::Blob);
                writer.write::<u16>(b.len() as u16);
                writer.write_bytes(b);
            }
            Atom::Cons(head, tail) => {
                writer.write::<AtomTag>(AtomTag::Cons);
                self.serialize_atom(head.clone(), writer)?;
                self.serialize_atom(tail.clone(), writer)?;
            }
            Atom::Str(s) => {
                writer.write::<AtomTag>(AtomTag::Cons);
                writer.write(s.as_str());
            }
        }

        Ok(())
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

    fn pop_string(&mut self) -> AtomResult<String> {
        match &*self.pop()? {
            Atom::Str(n) => Ok(n.clone()),
            _ => Err(AtomError::TypeMismatch),
        }
    }

    fn pop_blob(&mut self) -> AtomResult<Vec<u8>> {
        match &*self.pop()? {
            Atom::Blob(n) => Ok(n.clone()),
            _ => Err(AtomError::TypeMismatch),
        }
    }

    fn flatten_list(atom: AtomRef, list: &mut Vec<AtomRef>) -> AtomResult<()> {
        match &*atom {
            Atom::Cons(head, tail) => {
                list.push(head.clone());
                Self::flatten_list(tail.clone(), list)?;
            }
            Atom::Nil => {}
            _ => {
                list.push(atom.clone());
            }
        }
        Ok(())
    }

    fn pop_string_list(&mut self) -> AtomResult<Vec<String>> {
        let mut list = Vec::new();
        Self::flatten_list(self.pop()?, &mut list)?;

        let list: Vec<String> = match list
            .iter()
            .map(|i| match &**i {
                Atom::Str(s) => Ok(s.clone()),
                _ => Err(AtomError::TypeMismatch),
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        Ok(list)
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
            Opcode::NumberCast => {
                let a = self.pop()?.to_string();
                let a = a.parse::<f64>().map_err(|_| AtomError::InvalidCasts)?;
                self.stack.push(Atom::num(a));
                Ok(())
            }

            Opcode::Add => self.apply_num_op(|a, b| a + b),
            Opcode::Sub => self.apply_num_op(|a, b| a - b),
            Opcode::Mult => self.apply_num_op(|a, b| a * b),
            Opcode::Div => self.apply_num_op(|a, b| a / b),
            Opcode::Mod => self.apply_num_op(|a, b| a % b),
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
            Opcode::And => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push_bool(a.truthful() && b.truthful());
                Ok(())
            }
            Opcode::Or => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push_bool(a.truthful() || b.truthful());

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
            Opcode::In => {
                let mut line = String::new();
                io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| AtomError::IOError(e))?;
                let line = line.trim();
                self.stack.push(Atom::str(line));
                Ok(())
            }
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
            Opcode::DLOpen => {
                let path = self.pop_string()?;
                let lib = unsafe { libloading::Library::new(&path) }?;
                let addr = Box::into_raw(Box::new(lib)) as usize as f64;
                self.push_num(addr);
                Ok(())
            }
            Opcode::DLSym => {
                let sym_name = self.pop_string()?;
                let lib_addr = self.pop_num()? as usize as *mut libloading::Library;

                let func_symbol = unsafe { (*lib_addr).get::<*const ()>(sym_name.as_bytes()) }
                    .map_err(|_| AtomError::UnboundVariable(sym_name))?;

                let raw_ptr = *func_symbol as usize;

                let func_addr = f64::from_bits(raw_ptr as u64);

                self.push_num(func_addr);
                Ok(())
            }
            Opcode::DLCall => {
                let fn_ptr = atom_to_ptr(&self.pop()?)?;
                let ret_tag = self.pop_string()?;
                let arg_tags = self.pop_string_list()?;

                let n = arg_tags.len();
                if self.stack.len() < n {
                    return Err(AtomError::StackUnderflow);
                }
                let arg_atoms = self.stack.split_off(self.stack.len() - n);

                let natives: Vec<Native> = arg_atoms
                    .iter()
                    .zip(arg_tags.iter())
                    .map(|(a, t)| Native::from_atom(a, t))
                    .collect::<AtomResult<_>>()?;
                let ffi_args: Vec<middle::Arg> = natives.iter().map(Native::as_ffi_arg).collect();
                let arg_types: Vec<Type> = arg_tags
                    .iter()
                    .map(|t| ffi_type_of(t))
                    .collect::<AtomResult<_>>()?;

                let cif = Cif::new(arg_types, ffi_type_of(&ret_tag)?);
                let code = CodePtr::from_ptr(fn_ptr as *const c_void);

                let result = unsafe {
                    match ret_tag.as_str() {
                        "void" => {
                            cif.call::<()>(code, &ffi_args);
                            Atom::nil()
                        }
                        "int" => Atom::num(cif.call::<i32>(code, &ffi_args) as f64),
                        "i64" => Atom::num(cif.call::<i64>(code, &ffi_args) as f64),
                        "double" | "f64" => Atom::num(cif.call::<f64>(code, &ffi_args)),
                        "string" => {
                            let p: *mut c_char = cif.call(code, &ffi_args);
                            if p.is_null() {
                                Atom::nil()
                            } else {
                                Atom::string(
                                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned(),
                                )
                            }
                        }
                        "ptr" => ptr_to_atom(cif.call(code, &ffi_args)),
                        other => return Err(AtomError::UnboundVariable(other.into())),
                    }
                };
                self.stack.push(result);
                Ok(())
            }
        }
    }
}
