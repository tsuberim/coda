use clap::{Parser, Subcommand};
use chumsky::Parser as ChumskyParser;
use lang::parser::file_parser;

#[derive(Parser)]
#[command(name = "coda", about = "The Coda language compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a file and print the AST
    Parse {
        /// Source file to parse
        file: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => {
            let src = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(1);
            });

            match file_parser().parse(src.as_str()) {
                Ok(ast) => println!("{:#?}", ast),
                Err(errs) => {
                    for e in errs {
                        eprintln!("parse error: {e}");
                    }
                    std::process::exit(1);
                }
            }
        }
    }
}
