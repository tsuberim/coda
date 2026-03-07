use std::path::PathBuf;

use chumsky::Parser as _;
use clap::Parser;
use colored::Colorize;
use lang::{
    ast::BlockItem,
    eval::{eval, std_env},
    parser::{file_parser, repl_parser, ReplInput},
    types::{infer, infer_scheme, std_type_env},
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
    println!("  {}    lambda               {}  \\x y -> x + y", "\\x ->".bright_cyan(), "—".dimmed());
    println!("  {}      application        {}  f(x, y)", "f(x)".bright_cyan(), "—".dimmed());
    println!("  {}      infix              {}  1 + 2", "a + b".bright_cyan(), "—".dimmed());
    println!("  {}    template str       {}  `hi {{name}}`", "`...`".bright_cyan(), "—".dimmed());
    println!("  {}      block              {}  (x = 1; x + 1)", "(x=e; e)".bright_cyan(), "—".dimmed());
    println!("  {}      annotation         {}  f : Int -> Int", "x : T".bright_cyan(), "—".dimmed());
    println!("  {}  import module (cached) {}  math = import `math.coda`", "import `p`".bright_cyan(), "—".dimmed());
    println!("  {}      comment", "--".bright_cyan());
    println!("  {}  multiline comment", "--- ... ---".bright_cyan());
    println!();
    println!("  {}", "Builtins".bold().underline());
    println!("  {} {} {}   string concat", "++".bright_cyan(), ":".dimmed(), "Str Str -> Str".bright_blue());
    println!("  {} {} {}   integer addition", "+".bright_cyan(), ":".dimmed(), "Int Int -> Int".bright_blue());
    println!();
    println!("  {}  {}    {}  {}", "Ctrl-D".bright_yellow(), "exit", "↑↓".bright_yellow(), "history");
    println!("  {}   {}    {}  {}", ":clear".bright_yellow(), "clear screen", ":env".bright_yellow(), "show bindings");
    println!();
}

fn repl() {
    use rustyline::{error::ReadlineError, DefaultEditor};

    print_help();

    let env = std_env();
    let mut type_env = std_type_env();
    let mut rl = DefaultEditor::new().expect("failed to init readline");
    let prompt = format!("{} ", "›".bright_magenta().bold());

    let history_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("coda")
        .join("history");
    if let Some(parent) = history_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    rl.load_history(&history_path).ok();

    loop {
        match rl.readline(&prompt) {
            Err(ReadlineError::Eof | ReadlineError::Interrupted) => {
                rl.save_history(&history_path).ok();
                println!("{}", "bye".dimmed());
                break;
            }
            Err(e) => { eprintln!("{} {e}", "error:".red().bold()); break; }
            Ok(line) => {
                let src = line.trim();
                if src.is_empty() { continue; }
                rl.add_history_entry(src).ok();

                if src == ":clear" {
                    rl.clear_screen().ok();
                    continue;
                }

                if src == ":env" {
                    let mut bindings: Vec<_> = type_env.iter()
                        .filter(|(name, _)| !name.starts_with('#'))
                        .collect();
                    bindings.sort_by_key(|(name, _)| name.as_str());
                    if bindings.is_empty() {
                        println!("{}", "(empty)".dimmed());
                    } else {
                        for (name, scheme) in bindings {
                            let ty = lang::types::normalize_ty(scheme.ty.clone());
                            println!("{} {} {}", name.bright_cyan(), ":".dimmed(), ty.pretty());
                        }
                    }
                    continue;
                }

                match repl_parser().parse(src) {
                    Err(errs) => {
                        for e in errs {
                            eprintln!("{} {e}", "parse error:".red().bold());
                        }
                    }
                    Ok(ReplInput::Nop) => {}
                    Ok(ReplInput::Items(items)) => {
                        'items: for item in &items {
                            match item {
                                BlockItem::Ann(name, te) => {
                                    match lang::types::apply_ann(&type_env, name, te) {
                                        Ok(scheme) => {
                                            let display = lang::types::normalize_ty(scheme.ty.clone());
                                            println!(
                                                "{} {} {}",
                                                name.bright_cyan(),
                                                ":".dimmed(),
                                                display.pretty(),
                                            );
                                            type_env.insert(name.clone(), scheme);
                                        }
                                        Err(e) => {
                                            eprintln!("{} {e}", "type error:".red().bold());
                                            break 'items;
                                        }
                                    }
                                }
                                BlockItem::Bind(name, expr) => {
                                    let type_result = infer_scheme(&type_env, expr)
                                        .and_then(|s| lang::types::enforce_binding(&type_env, name, s));
                                    match type_result {
                                        Err(e) => {
                                            eprintln!("{} {e}", "type error:".red().bold());
                                            break 'items;
                                        }
                                        Ok(scheme) => match eval(expr, &env) {
                                            Ok(val) => {
                                                // Suppress display of internal tmp vars (#N).
                                                if !name.starts_with('#') {
                                                    let display_ty = lang::types::normalize_ty(scheme.ty.clone());
                                                    println!(
                                                        "{} {} {} {} {}",
                                                        name.bright_cyan(),
                                                        "=".dimmed(),
                                                        val.pretty(),
                                                        ":".dimmed(),
                                                        display_ty.pretty(),
                                                    );
                                                }
                                                type_env.insert(name.clone(), scheme);
                                                env.set(name, val);
                                            }
                                            Err(e) => {
                                                eprintln!("{} {e}", "error:".red().bold());
                                                break 'items;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(ReplInput::Expr(expr)) => {
                        match infer(&type_env, &expr) {
                            Err(e) => eprintln!("{} {e}", "type error:".red().bold()),
                            Ok(ty) => match eval(&expr, &env) {
                                Ok(val) => println!("{} {} {}", val.pretty(), ":".dimmed(), ty.pretty()),
                                Err(e) => eprintln!("{} {e}", "error:".red().bold()),
                            }
                        }
                    }
                }
            }
        }
    }
}

fn interpret(path: PathBuf) {
    match lang::module::load_module(&path.to_string_lossy()) {
        Ok(entry) => println!("{} : {}", entry.val, entry.ty),
        Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
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
