use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::thread;

use mulang::compiler::{CodeGenerator, REQUIRED_STACK_BYTES};
use mulang::diagnostics::render_all;
use mulang::ir::instructions::format_program;
use mulang::ir::interpreter::IRInterpreter;
use mulang::ir::lowering::Lowerer;
use mulang::syntax;
use mulang::syntax::sema::Sema;

enum Mode {
    // AST -> assembly
    Assemble,
    // AST -> IR -> interpreter
    InterpretIR,
    // AST -> IR -> .irmu text file
    EmitIRText,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let work = move || match arguments.as_slice() {
        [path] => compile_file(path, Mode::Assemble),
        [flag, path] if flag == "--ir" => compile_file(path, Mode::InterpretIR),
        [first, second, path]
            if matches!(
                (first.as_str(), second.as_str()),
                ("--ir", "--text") | ("--text", "--ir")
            ) =>
        {
            compile_file(path, Mode::EmitIRText)
        }
        _ => {
            eprintln!("usage: mulang [--ir [--text]] program.muji");
            ExitCode::FAILURE
        }
    };

    thread::Builder::new()
        .stack_size(REQUIRED_STACK_BYTES)
        .spawn(work)
        .and_then(|compiler_thread| {
            compiler_thread
                .join()
                .map_err(|_| io::Error::other("the compiler panicked"))
        })
        .unwrap_or_else(|error| {
            eprintln!("mulang: {error}");
            ExitCode::FAILURE
        })
}

fn compile_file(path: &str, mode: Mode) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("mulang: cannot read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    // AST
    let ast = match syntax::parse(&source) {
        Ok(ast) => ast,
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

    let sema = Sema::new();

    match sema.analyse(&ast) {
        Err(diagnostics) => {
            eprint!("{}", render_all(&diagnostics, &source, path));
            ExitCode::FAILURE
        }
        Ok(analysis) if matches!(mode, Mode::InterpretIR | Mode::EmitIRText) => {
            let mut lowerer = Lowerer::new(analysis);
            let ir_program = lowerer.lower(ast);

            if let Mode::EmitIRText = mode {
                let output_path = format!("{file_name_without_extension}.irmu");
                if let Err(error) = std::fs::write(&output_path, format_program(&ir_program)) {
                    eprintln!("mulang: cannot write {output_path}: {error}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }

            IRInterpreter::new(ir_program).run();
            ExitCode::SUCCESS
        }
        Ok(analysis) => {
            let mut code_generator = CodeGenerator::new(analysis, file_name_without_extension);

            match code_generator.generate(&ast) {
                Ok(_) => ExitCode::SUCCESS,
                Err(diagnostic) => {
                    eprint!("{}", diagnostic.render(&source, path));
                    ExitCode::FAILURE
                }
            }
        }
    }
}
