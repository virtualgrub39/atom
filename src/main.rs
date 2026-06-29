mod atom;

use atom::{AtomResult, Compiler, DisplayWithSrc, Interpreter};

use std::collections::HashMap;
use std::env;
use std::fs;

fn main() -> AtomResult<()> {
    let args: Vec<String> = env::args().collect();

    let path = args.get(1)
        .map_or("examples/mega-example.a", |p| p.as_str());

    assert!(
        std::path::Path::new(path).exists(),
        "File not found: {path}"
    );

    let src = fs::read_to_string(path).expect("Failed to read file");
    let prog = atom::Program::parse(&src)?;
    // println!("{}", prog.display(&src));

    let e = Compiler::new().compile(&prog.nodes)?;
    let e = e
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect::<HashMap<_, _>>();

    // println!("Compiled environment: {:?}", e);

    let mut vm = Interpreter::new();
    vm.import(e);

    if let Some(main) = vm.env.get("main") {
        vm.eval(main.clone())?;
    }

    let atomc = vm.write_atom_file()?;
    fs::write("env.atomc", atomc).expect("Failed to write the atomc file");

    Ok(())
}
