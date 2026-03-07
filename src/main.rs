use std::path::PathBuf;

use chumsky::Parser as _;
use clap::Parser;
use colored::Colorize;
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

fn print_help() {
    println!();
    println!("  {}  {}", "Coda".bold().bright_magenta(), env!("CARGO_PKG_VERSION").dimmed());
    println!("  {}", "A purely functional, HM-typed language.".dimmed());
    println!();
    println!("  {}", "Syntax".bold().underline());
    println!("  {}  lambda          {}  \\x y -> x + y", "\\x ->".bright_cyan(), "—".dimmed());
    println!("  {}    application   {}  f(x, y)", "f(x)".bright_cyan(), "—".dimmed());
    println!("  {}    infix         {}  1 + 2", "a + b".bright_cyan(), "—".dimmed());
    println!("  {}  template str  {}  `hi {{name}}`", "`...`".bright_cyan(), "—".dimmed());
    println!("  {}    block         {}  (x = 1; x + 1)", "(x=e; e)".bright_cyan(), "—".dimmed());
    println!();
    println!("  {}", "Builtins".bold().underline());
    println!("  {}   string concat (strings only)", "++".bright_cyan());
    println!("  {}    numeric addition", "+".bright_cyan());
    println!();
    println!("  {}  {}    {}  {}", "Ctrl-D".bright_yellow(), "exit", "↑↓".bright_yellow(), "history");
    println!();
}

fn repl() {
    use rustyline::{error::ReadlineError, DefaultEditor};

    print_help();

    let env = std_env();
    let mut rl = DefaultEditor::new().expect("failed to init readline");
    let prompt = format!("{} ", "›".bright_magenta().bold());

    loop {
        match rl.readline(&prompt) {
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => {
                println!("{}", "bye".dimmed());
                break;
            }
            Err(e) => { eprintln!("{} {e}", "error:".red().bold()); break; }
            Ok(line) => {
                let src = line.trim();
                if src.is_empty() { continue; }
                rl.add_history_entry(src).ok();

                match repl_parser().parse(src) {
                    Err(errs) => {
                        for e in errs {
                            eprintln!("{} {e}", "parse error:".red().bold());
                        }
                    }
                    Ok(ReplInput::Binding(name, expr)) => {
                        match eval(&expr, &env) {
                            Ok(val) => {
                                println!("{} {} {}", name.bright_cyan(), "=".dimmed(), val.pretty());
                                env.set(&name, val);
                            }
                            Err(e) => eprintln!("{} {e}", "error:".red().bold()),
                        }
                    }
                    Ok(ReplInput::Expr(expr)) => {
                        match eval(&expr, &env) {
                            Ok(val) => println!("{}", val.pretty()),
                            Err(e) => eprintln!("{} {e}", "error:".red().bold()),
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
