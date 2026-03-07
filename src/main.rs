use std::path::PathBuf;

use chumsky::Parser as _;
use clap::Parser;
use lang::{
    eval::{eval, std_env},
    parser::{file_parser, repl_parser, ReplInput},
};

#[derive(Parser)]
#[command(name = "coda", about = "The Coda language — repl / interpreter / compiler")]
struct Cli {
    /// Source file to run or compile. Omit to start the REPL.
    file: Option<PathBuf>,

    /// Compile instead of interpret.
    #[arg(short = 'c', long, requires = "file")]
    compile: bool,

    /// Output path for compiled binary (default: input basename without extension).
    #[arg(short = 'o', long, requires = "compile")]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    match cli.file {
        None => repl(),
        Some(path) if cli.compile => compile(path, cli.output),
        Some(path) => interpret(path),
    }
}

fn repl() {
    use rustyline::{error::ReadlineError, DefaultEditor};

    println!("Coda REPL  (Ctrl-D to exit)");

    let env = std_env();
    let mut rl = DefaultEditor::new().expect("failed to init readline");

    loop {
        match rl.readline("> ") {
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => break,
            Err(e) => { eprintln!("error: {e}"); break; }
            Ok(line) => {
                let src = line.trim();
                if src.is_empty() { continue; }
                rl.add_history_entry(src).ok();

                match repl_parser().parse(src) {
                    Err(errs) => {
                        for e in errs { eprintln!("parse error: {e}"); }
                    }
                    Ok(ReplInput::Binding(name, expr)) => {
                        match eval(&expr, &env) {
                            Ok(val) => env.set(&name, val),
                            Err(e) => eprintln!("error: {e}"),
                        }
                    }
                    Ok(ReplInput::Expr(expr)) => {
                        match eval(&expr, &env) {
                            Ok(val) => println!("{}", val),
                            Err(e) => eprintln!("error: {e}"),
                        }
                    }
                }
            }
        }
    }
}

fn interpret(path: PathBuf) {
    let src = read_file(&path);
    match file_parser().parse(src.as_str()) {
        Ok(ast) => match eval(&ast, &std_env()) {
            Ok(val) => println!("{}", val),
            Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
        },
        Err(errs) => {
            for e in errs { eprintln!("{}:parse error: {e}", path.display()); }
            std::process::exit(1);
        }
    }
}

fn compile(path: PathBuf, output: Option<PathBuf>) {
    let out = output.unwrap_or_else(|| PathBuf::from(path.file_stem().unwrap()));
    let src = read_file(&path);
    match file_parser().parse(src.as_str()) {
        Ok(_ast) => {
            // TODO: codegen
            eprintln!("(compiler not yet implemented, would write to {})", out.display());
            std::process::exit(1);
        }
        Err(errs) => {
            for e in errs { eprintln!("{}:parse error: {e}", path.display()); }
            std::process::exit(1);
        }
    }
}

fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: {}: {e}", path.display());
        std::process::exit(1);
    })
}
