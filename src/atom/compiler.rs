use std::collections::HashMap;

use crate::atom::{Assembler, Atom, AtomError, AtomRef, AtomResult, Node, Opcode, Spanned};

pub struct Scope<'a> {
    pub parent: Option<&'a Scope<'a>>,
    pub bindings: Vec<String>,
}

pub struct Compiler {
    pub definitions: HashMap<String, AtomRef>,
    anon_counter: usize,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            anon_counter: 0,
        }
    }

    pub fn compile(mut self, program: &[Spanned<Node>]) -> AtomResult<HashMap<String, AtomRef>> {
        let mut main_asm = Assembler::new();
        let mut root_scope = Scope {
            parent: None,
            bindings: Vec::new(),
        };
        let mut has_loose_code = false;

        for spanned in program {
            match &spanned.node {
                Node::Define { name, body } => {
                    let atom = self.node_to_atom(&body.node, &root_scope)?;
                    self.definitions.insert(name.clone(), atom);
                }
                _ => {
                    has_loose_code = true;
                    main_asm = self.compile_node(&spanned.node, main_asm, &mut root_scope)?;
                }
            }
        }

        if has_loose_code {
            self.definitions
                .insert("main".to_string(), main_asm.block());
        }

        Ok(self.definitions)
    }

    fn node_to_atom(&mut self, node: &Node, scope: &Scope) -> AtomResult<AtomRef> {
        match node {
            Node::Nil => Ok(Atom::nil()),
            Node::Number(n) => Ok(Atom::num(*n)),
            Node::String(s) => Ok(Atom::string(s.clone())),
            Node::List(nodes) => {
                let mut current = Atom::nil();
                for spanned in nodes.iter().rev() {
                    let element = self.node_to_atom(&spanned.node, scope)?;
                    current = Atom::cons(element, current);
                }
                Ok(current)
            }
            Node::Block(nodes) => {
                let mut block_asm = Assembler::new();
                let mut block_scope = Scope {
                    parent: Some(scope),
                    bindings: Vec::new(),
                };

                for spanned in nodes {
                    block_asm = self.compile_node(&spanned.node, block_asm, &mut block_scope)?;
                }

                if !block_scope.bindings.is_empty() {
                    block_asm = block_asm.drop_ret(block_scope.bindings.len() as u16);
                }

                Ok(block_asm.block())
            }
            _ => Err(AtomError::UnmappableNode),
        }
    }

    fn compile_node(
        &mut self,
        node: &Node,
        mut asm: Assembler,
        scope: &mut Scope,
    ) -> AtomResult<Assembler> {
        match node {
            Node::Nil | Node::Number(_) | Node::String(_) | Node::List(_) | Node::Block(_) => {
                let atom = self.node_to_atom(node, scope)?;
                let key = self.get_or_create_env_key(&atom);
                asm = asm.push_env(&key);
            }

            Node::Builtin(builtin) => {
                asm = asm.op(Opcode::from(*builtin));
            }

            Node::Define { name, body } => {
                let atom = self.node_to_atom(&body.node, scope)?;
                self.definitions.insert(name.clone(), atom);
            }

            Node::WordRef(name) => {
                asm = asm.push_env(name);
            }

            Node::BindVar(name) => {
                scope.bindings.push(name.clone());
                asm = asm.to_ret(1);
            }

            Node::FetchVar(name) => {
                let index = self.resolve_variable(scope, name, 0)?;
                asm = asm.fetch_ret(index);
            }

            Node::If { then_br, else_br } => {
                let then_atom = self.node_to_atom(&then_br.node, scope)?;
                let then_key = self.get_or_create_env_key(&then_atom);

                let else_atom = self.node_to_atom(&else_br.node, scope)?;
                let else_key = self.get_or_create_env_key(&else_atom);

                asm = asm.push_env(&then_key).push_env(&else_key).if_then_else();
            }
        }

        Ok(asm)
    }

    fn get_or_create_env_key(&mut self, atom: &AtomRef) -> String {
        match &**atom {
            Atom::Nil => {
                let key = "nil".to_string();
                self.definitions
                    .entry(key.clone())
                    .or_insert_with(|| atom.clone());
                key
            }
            Atom::Num(n) => {
                let key = n.to_string();
                self.definitions
                    .entry(key.clone())
                    .or_insert_with(|| atom.clone());
                key
            }
            _ => {
                let key = format!("${}", self.anon_counter);
                self.anon_counter += 1;
                self.definitions.insert(key.clone(), atom.clone());
                key
            }
        }
    }

    fn resolve_variable(&self, scope: &Scope, name: &str, depth: usize) -> AtomResult<u16> {
        if let Some(pos) = scope.bindings.iter().rposition(|b| b == name) {
            return Ok(((scope.bindings.len() - 1 - pos) + depth) as u16);
        }
        match scope.parent {
            Some(parent) => self.resolve_variable(parent, name, depth + scope.bindings.len()),
            None => Err(AtomError::UnboundVariable(name.to_string())),
        }
    }
}
