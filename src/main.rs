mod atom;

use atom::{
    Assembler, Atom, AtomError, AtomResult, DisplayWithSrc, Interpreter, Lexer, Node, Program,
    Spanned, TokenKind,
};

use std::collections::HashMap;
use std::env;
use std::fs;

use crate::atom::AtomRef;
use crate::atom::Opcode;

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
                let key = format!("__anon_{}", self.anon_counter);
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

fn main() -> AtomResult<()> {
    let args: Vec<String> = env::args().collect();

    let path = args.get(1).expect("Usage: <program> <source_file>");

    assert!(
        std::path::Path::new(path).exists(),
        "File not found: {path}"
    );

    let src = fs::read_to_string(path).expect("Failed to read file");
    let mut lexer = Lexer::new(&src);

    let parser_tokens =
        std::iter::from_fn(|| lexer.next_token()).filter(|token| token.kind != TokenKind::Comment);

    for token in parser_tokens {
        println!("{}", token.display(&src));
    }

    let prog = atom::Program::parse(&src)?;
    println!("{}", prog.display(&src));

    let e = Compiler::new().compile(&prog.nodes)?;
    let e = e
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect::<HashMap<_, _>>();

    println!("Compiled environment: {:?}", e);

    let mut vm = Interpreter::new();
    vm.import(e);

    if let Some(main) = vm.env.get("main") {
        vm.eval(main.clone())?;
    }

    return Ok(());

    vm.register("-1", Atom::num(-1.));
    vm.register("n", Atom::num(1000.));
    vm.register("34", Atom::num(34.));
    vm.register("35", Atom::num(35.));
    vm.register("CRLF", Atom::str("\r\n"));
    vm.register(
        "println",
        Assembler::new().out().push_env("CRLF").out().block(),
    );
    vm.register(
        "fn",
        Assembler::new().add().push_env("println").eval().block(),
    );

    // (defun fn (a b) (out (+ a b)))
    // (fn (34 35))
    vm.register("lispy_msg", Atom::str("lispy:\n"));
    let lispy = Assembler::new()
        .push_env("lispy_msg")
        .out()
        .push_env("fn")
        .push_env("35")
        .push_env("34")
        .cons()
        .cons()
        .dup()
        // .out()
        .push_env("println")
        .eval()
        .eval()
        .block();
    vm.register("lispy", lispy);

    // : fn + out ;
    // 34 35 fn
    vm.register("forthy_msg", Atom::str("forthy:\n"));
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
