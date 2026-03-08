use std::path::PathBuf;

use chumsky::Parser as _;
use clap::Parser;
use colored::Colorize;
use lang::{
    ast::BlockItem,
    eval::{eval, run_task, std_env, Value},
    parser::{file_parser, repl_parser, ReplInput},
    types::{infer_scheme_with_map, infer_with_map, std_type_env},
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
                                    let type_result = infer_scheme_with_map(&type_env, expr)
                                        .and_then(|(s, tm)| lang::types::enforce_binding(&type_env, name, s).map(|s2| (s2, tm)).map_err(|e| {
                                            lang::types::InferError { kind: e, span: 0..0 }
                                        }));
                                    match type_result {
                                        Err(e) => {
                                            eprintln!("{}", e.render("<repl>", src));
                                            break 'items;
                                        }
                                        Ok((scheme, type_map)) => match eval(expr, &env.clone().with_type_map(type_map)) {
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
                                BlockItem::MonadicBind(name, expr) => {
                                    // Type-check: expr must be Task ok err. Extract ok type.
                                    let (scheme, type_map) = match infer_scheme_with_map(&type_env, expr) {
                                        Err(e) => { eprintln!("{}", e.render("<repl>", src)); break 'items; }
                                        Ok(s) => s,
                                    };
                                    let ok_scheme = match &scheme.ty {
                                        lang::types::Type::Shaped(lang::types::BaseType::Task(ok, _err), sh) if sh.is_empty() => {
                                            lang::types::Scheme::mono((**ok).clone())
                                        }
                                        _ => {
                                            eprintln!("{} expected Task, got {}", "type error:".red().bold(), lang::types::normalize_ty(scheme.ty.clone()).pretty());
                                            break 'items;
                                        }
                                    };
                                    // Eval and run the task.
                                    match eval(expr, &env.clone().with_type_map(type_map)) {
                                        Err(e) => { eprintln!("{} {e}", "error:".red().bold()); break 'items; }
                                        Ok(task_val) => match run_task(&task_val) {
                                            Err(err_val) => {
                                                eprintln!("{} {}", "task failed:".red().bold(), err_val);
                                                break 'items;
                                            }
                                            Ok(result) => {
                                                if !name.starts_with('#') {
                                                    let display_ty = lang::types::normalize_ty(ok_scheme.ty.clone());
                                                    println!(
                                                        "{} {} {} {} {}",
                                                        name.bright_cyan(),
                                                        "<-".dimmed(),
                                                        result.pretty(),
                                                        ":".dimmed(),
                                                        display_ty.pretty(),
                                                    );
                                                }
                                                type_env.insert(name.clone(), ok_scheme);
                                                env.set(name, result);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(ReplInput::Expr(expr)) => {
                        match infer_with_map(&type_env, &expr) {
                            Err(e) => eprintln!("{}", e.render("<repl>", src)),
                            Ok((ty, type_map)) => match eval(&expr, &env.clone().with_type_map(type_map)) {
                                Ok(Value::Task(ref f)) => {
                                    let (ok_ty, err_ty) = match &ty {
                                        lang::types::Type::Shaped(lang::types::BaseType::Task(ok, err), sh) if sh.is_empty() => {
                                            (lang::types::normalize_ty((**ok).clone()), lang::types::normalize_ty((**err).clone()))
                                        }
                                        _ => (lang::types::normalize_ty(ty.clone()), lang::types::normalize_ty(ty.clone())),
                                    };
                                    match f() {
                                        Ok(result) => {
                                            let unit = Value::Record(vec![]);
                                            if result != unit {
                                                println!("{} {} {}", result.pretty(), ":".dimmed(), ok_ty.pretty());
                                            }
                                        }
                                        Err(err_val) => eprintln!("{} {} {} {}", "task failed:".red().bold(), err_val, ":".dimmed(), err_ty.pretty()),
                                    }
                                }
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
    let src = read_file(&path);
    let filename = path.to_str().unwrap_or("?");

    let ast = match file_parser().parse(src.as_str()) {
        Ok(ast) => ast,
        Err(errs) => {
            for e in errs { eprintln!("{}:{}", filename, e); }
            std::process::exit(1);
        }
    };

    let (ty, type_map) = match infer_with_map(&std_type_env(), &ast) {
        Ok(result) => result,
        Err(e) => { eprintln!("{}", e.render(filename, &src)); std::process::exit(1); }
    };

    let val = match eval(&ast, &std_env().with_type_map(type_map)) {
        Ok(v) => v,
        Err(e) => { eprintln!("error: {e}"); std::process::exit(1); }
    };

    match val {
        Value::Task(_) => match run_task(&val) {
            Ok(result) => {
                if result != Value::Record(vec![]) { println!("{}", result); }
            }
            Err(err_val) => { eprintln!("error: {}", err_val); std::process::exit(1); }
        },
        _ => println!("{} : {}", val, ty),
    }
}

fn compile(path: PathBuf, output: Option<PathBuf>) {
    let out = output.unwrap_or_else(|| PathBuf::from(path.file_stem().unwrap()));
    let src = read_file(&path);
    let ast = match file_parser().parse(src.as_str()) {
        Ok(ast) => ast,
        Err(errs) => {
            for e in errs { eprintln!("{}:parse error: {e}", path.display()); }
            std::process::exit(1);
        }
    };
    let ty = match lang::types::infer(&lang::types::std_type_env(), &ast) {
        Ok(ty) => ty,
        Err(e) => {
            eprintln!("{}", e.render(path.to_str().unwrap_or("?"), &src));
            std::process::exit(1);
        }
    };
    let is_task = matches!(&ty, lang::types::Type::Shaped(lang::types::BaseType::Task(..), sh) if sh.is_empty());
    let ir = match lang::codegen::compile(&ast, is_task) {
        Ok(ir) => ir,
        Err(e) => { eprintln!("codegen error: {e}"); std::process::exit(1); }
    };
    // Write IR to a temp file, compile with clang, then clean up.
    let ir_path = std::env::temp_dir().join(format!("coda_{}.ll", std::process::id()));
    std::fs::write(&ir_path, &ir).unwrap_or_else(|e| {
        eprintln!("failed to write IR: {e}"); std::process::exit(1);
    });
    let runtime_c = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/runtime.c");
    let status = std::process::Command::new("clang")
        .args([
            ir_path.to_str().unwrap(),
            runtime_c.to_str().unwrap(),
            "-o", out.to_str().unwrap(),
            "-O1",
        ])
        .status()
        .unwrap_or_else(|e| { eprintln!("clang failed: {e}"); std::process::exit(1); });
    std::fs::remove_file(&ir_path).ok();
    if !status.success() {
        eprintln!("clang exited with {}", status);
        std::process::exit(1);
    }
    println!("compiled: {}", out.display());
}

fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: {}: {e}", path.display());
        std::process::exit(1);
    })
}
