use std::env;

/// TODO: Move these to seperate file
use example;

fn resolve_program(name: &str) -> impl FnOnce(Vec<String>) {
    example::example
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let prog = resolve_program(args.first().map(|x| x.as_ref()).unwrap_or(""));
    prog(args);
}
