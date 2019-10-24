use std::env;

/// TODO: Move these to seperate file
use example;

enum Program {
    Help,
    Example,
}

fn name_to_program(name: &str) -> Program {
    match name {
        "example" => Program::Example,
        _ => Program::Help,
    }
}

fn program_to_fn(prog: Program) -> impl FnOnce(Vec<String>) {
    match prog {
        Program::Example => example::example,
        Program::Help => help,
    }
}

fn help(_args: Vec<String>) {
    println!("Help called");
}

fn main() {
    let mut args: Vec<String> = env::args().collect();
    let mut first_arg = args
        .first()
        .map(|a| a.to_owned())
        .unwrap_or_else(|| ("tc".to_owned()));
    if first_arg == "tc" {
        args.drain(0..1);
        if args.len() == 0 {
            help(args.to_vec());
            return;
        }
        first_arg = args.first().unwrap().to_owned();
        args.drain(0..1);
    }
    let prog = program_to_fn(name_to_program(&first_arg));
    prog(args);
}
