//! `mulang [file.muji]` — run a program, or start an interactive session.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::thread;

use mulang::diagnostics::render_all;
use mulang::repl::Repl;
use mulang::runtime::REQUIRED_STACK_BYTES;
use mulang::runtime::compiler::Compiler;
use mulang::syntax;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let work = move || match arguments.as_slice() {
        [] => start_session(),
        [path] => compile_file(path),
        _ => {
            eprintln!("usage: mulang [program.muji]");
            ExitCode::FAILURE
        }
    };

    // The main thread's stack is not ours to size, so evaluation gets one that is.
    thread::Builder::new()
        .stack_size(REQUIRED_STACK_BYTES)
        .spawn(work)
        .and_then(|interpreter_thread| {
            interpreter_thread
                .join()
                .map_err(|_| io::Error::other("the compiler panicked"))
        })
        .unwrap_or_else(|error| {
            eprintln!("mulang: {error}");
            ExitCode::FAILURE
        })
}

fn compile_file(path: &str) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("mulang: cannot read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    // Intermediate representation
    let program = match syntax::parse(&source) {
        Ok(program) => program,
        Err(diagnostics) => {
            eprint!("{}", render_all(&diagnostics, &source, path));
            return ExitCode::FAILURE;
        }
    };

    let file_path = Path::new(path);

    if file_path.extension().and_then(|ext| ext.to_str()) != Some("muji") {
        eprintln!("File should be in .muji format");
        return ExitCode::FAILURE;
    }

    let file_name_without_extension = match file_path.file_stem().and_then(|stem| stem.to_str()) {
        Some(name) => name,
        None => {
            eprintln!("File should be in .muji format");
            return ExitCode::FAILURE;
        }
    };

    let mut compiler = Compiler::new(file_name_without_extension);

    match compiler.compile(&program) {
        Ok(_) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            eprint!("{}", diagnostic.render(&source, path));
            return ExitCode::FAILURE;
        }
    }
}

fn start_session() -> ExitCode {
    let user = std::env::var("USER").unwrap_or_else(|_| "मुजी".to_owned());
    println!("नमस्कार {user} मुजी!");
    println!("यो \"मुजी\" भाषा हो। तल लेख् मुजी 👇");

    let mut input = io::stdin().lock();
    let mut output = io::stdout();

    match Repl::new().run(&mut input, &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "mulang: {error}");
            ExitCode::FAILURE
        }
    }
}
