mod atom;

use atom::{Assembler, Atom, AtomResult, Interpreter, Lexer, TokenKind, Span};

use std::env;
use std::fs;

#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

#[derive(Debug)]
pub struct Program {
    pub nodes: Vec<Spanned<Node>>
}

#[derive(Debug)]
pub enum Node {
    Number(f64),
    String(String),
    Nil,

    Builtin(Builtin),
    
    WordRef(String),
    BindVar(String),
    FetchVar(String),

    Block(Vec<Spanned<Node>>),
    List(Vec<Spanned<Node>>),

    Define {
        name: String,
        body: Box<Spanned<Node>>,
    },

    If {
        then_br: Box<Spanned<Node>>,
        else_br: Box<Spanned<Node>>,
    },

    While {
        cond: Box<Spanned<Node>>,
        body: Box<Spanned<Node>>,
    },

    Times (Box<Spanned<Node>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Out, Times, Dup, Over, Lt, Lte, Eq, Not, Nip, Drop, This, StringCast, Head, Tail, Swap,
    Add,       // +
    Sub,       // -
    Eval,      // !
    Cons,      // ::
    Join,      // ++
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

    return Ok(());

    let mut vm = Interpreter::new();

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
