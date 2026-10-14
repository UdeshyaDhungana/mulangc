use std::path::Path;
use std::process::ExitCode;

use mulang::codegen::macos_arm64::CodeGenerator;
use mulang::diagnostics::render_all;
use mulang::ir::instructions::format_program;
use mulang::ir::lowering::Lowerer;
use mulang::syntax;
use mulang::syntax::sema::Sema;

enum Mode {
    // AST -> assembly
    Assemble,
    // AST -> IR -> .irmu text file
    EmitIRText,
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    match arguments.as_slice() {
        [path] => compile_file(path, Mode::Assemble),
        [flag, path] if flag == "--irtext" => compile_file(path, Mode::EmitIRText),
        _ => {
            eprintln!("usage: mulang [--irtext] program.muji");
            ExitCode::FAILURE
        }
    }
}

fn compile_file(path: &str, mode: Mode) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("mulang: cannot read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };

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
        Ok(analysis) if matches!(mode, Mode::EmitIRText) => {
            let lowerer = Lowerer::new(analysis);
            let ir_program = lowerer.lower(ast);

            let output_path = format!("{file_name_without_extension}.irmu");
            if let Err(error) = std::fs::write(&output_path, format_program(&ir_program)) {
                eprintln!("mulang: cannot write {output_path}: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Ok(analysis) => {
            let lowerer = Lowerer::new(analysis);
            let ir_program = lowerer.lower(ast);
            let mut code_generator = CodeGenerator::new(file_name_without_extension);

            match code_generator.generate(ir_program) {
                Ok(_) => ExitCode::SUCCESS,
                Err(err) => {
                    eprint!("{}", err);
                    ExitCode::FAILURE
                }
            }
        }
    }
}
