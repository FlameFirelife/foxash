mod ast;
mod cli;
mod lexer;
mod parser;
mod runtime;
mod value;

use cli::{parse_arguments, Command};
use lexer::Lexer;
use parser::Parser;
use runtime::Runtime;

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

const CLI_ERROR: u8 = 1;
const FILE_ERROR: u8 = 2;
const LEX_ERROR: u8 = 3;
const PARSE_ERROR: u8 = 4;
const RUNTIME_ERROR: u8 = 5;

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();

    let command = match parse_arguments(&arguments) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("error: {}", message);
            print_help();
            return ExitCode::from(CLI_ERROR);
        }
    };

    match command {
        Command::Help => {
            print_help();
            ExitCode::SUCCESS
        }

        Command::Version => {
            println!("Foxash 1.0.0");
            ExitCode::SUCCESS
        }

        Command::Run(path) => process_file(&path, true),

        Command::Check(path) => process_file(&path, false),
    }
}

fn process_file(path: &Path, run: bool) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,

        Err(error) => {
            eprintln!("File error: could not read '{}': {}", path.display(), error);

            return ExitCode::from(FILE_ERROR);
        }
    };

    let mut lexer = Lexer::new(&source);

    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,

        Err(error) => {
            eprintln!(
                "Lexer error in {} at {}:{}: {}",
                path.display(),
                error.line,
                error.column,
                error.message
            );

            return ExitCode::from(LEX_ERROR);
        }
    };

    let mut parser = Parser::new(tokens);

    let statements = match parser.parse() {
        Ok(statements) => statements,

        Err(error) => {
            eprintln!(
                "Parser error in {} at {}:{}: {}",
                path.display(),
                error.line,
                error.column,
                error.message
            );

            return ExitCode::from(PARSE_ERROR);
        }
    };

    if !run {
        println!("Checked '{}' successfully.", path.display());
        return ExitCode::SUCCESS;
    }

    let mut runtime = Runtime::new();

    if let Err(error) = runtime.execute(&statements) {
        eprintln!("Runtime error in {}: {}", path.display(), error.message);

        return ExitCode::from(RUNTIME_ERROR);
    }

    ExitCode::SUCCESS
}

fn print_help() {
    println!(
        "Foxash: programming language interpreter

Usage:
  foxash run <file.foxash>
  foxash check <file.foxash>
  foxash help
  foxash version

Options:
  -h, --help       Show this help message
  -V, --version    Show version information

Examples:
  foxash run examples/hello.foxash
  foxash check examples/hello.foxash"
    );
}
