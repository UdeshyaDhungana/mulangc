//! End-to-end tests: compile each program in `tests/programs`, assemble it,
//! run it, and compare what it printed against the matching `.out` file.
//!
//! Every program gets its own scratch directory under `target/muji-tests`,
//! because the compiler writes `{stem}.s` relative to the working directory
//! rather than next to the source. The artifacts are left behind on purpose —
//! when a test fails, the assembly that failed is sitting there to read.
//!
//! Requires `clang` on the path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long a compiled program gets before it is treated as hung. A codegen
/// bug can corrupt a loop counter and turn a terminating loop into an infinite
/// one, so "never finishes" is a failure mode the harness has to report rather
/// than reproduce.
const RUN_TIMEOUT: Duration = Duration::from_secs(5);

/// The compiler binary. Cargo builds it before the test runs and hands us the
/// path, so we never shell out to `cargo run` from inside a test.
const COMPILER: &str = env!("CARGO_BIN_EXE_mulang");

#[test]
fn programs_produce_their_expected_output() {
    let mut failures = Vec::new();

    for program in collect_programs() {
        if let Err(failure) = run_one(&program) {
            failures.push(failure);
        }
    }

    assert!(
        !failures.is_empty() || !collect_programs().is_empty(),
        "no programs found in {}",
        programs_directory().display()
    );

    if !failures.is_empty() {
        panic!(
            "\n\n{} of {} programs failed:\n\n{}",
            failures.len(),
            collect_programs().len(),
            failures.join("\n")
        );
    }
}

fn programs_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs")
}

/// Every `.muji` file under `tests/programs`, at any depth, sorted so the
/// failure report reads the same way twice. Subdirectories are how the corpus
/// is grouped — `functions/`, `control_flow/`, and so on — and carry no meaning
/// beyond that.
fn collect_programs() -> Vec<PathBuf> {
    let mut programs = Vec::new();
    collect_programs_under(&programs_directory(), &mut programs);
    programs.sort();
    programs
}

fn collect_programs_under(directory: &Path, programs: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));

    for entry in entries {
        let path = entry.expect("cannot read directory entry").path();

        if path.is_dir() {
            collect_programs_under(&path, programs);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "muji")
        {
            programs.push(path);
        }
    }
}

/// `functions/recursion` — what a failure report calls the program, and the
/// name of its scratch directory once the `/` is flattened.
fn relative_name(source: &Path) -> String {
    source
        .strip_prefix(programs_directory())
        .expect("program lives outside tests/programs")
        .with_extension("")
        .to_str()
        .expect("program name is not valid UTF-8")
        .to_owned()
}

/// Compile, assemble, run, compare. The `Err` is a human-readable report of
/// the first stage that went wrong.
fn run_one(source: &Path) -> Result<(), String> {
    let name = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("program name is not valid UTF-8");

    // `functions/recursion` reads better in a failure report than `recursion`,
    // and keeps two same-named programs in different groups apart.
    let reported_as = relative_name(source);
    let report = |stage: &str, detail: String| format!("  {reported_as}: {stage}\n{detail}");

    let scratch = prepare_scratch_directory(&reported_as.replace('/', "_"), source, name);

    let compiled = run_in(&scratch, COMPILER, &[&format!("{name}.muji")]);
    if !compiled.status.success() {
        return Err(report("failed to compile", indent(&stderr_of(&compiled))));
    }

    let assembled = run_in(&scratch, "clang", &["-g", "-o", name, &format!("{name}.s")]);
    if !assembled.status.success() {
        return Err(report("failed to assemble", indent(&stderr_of(&assembled))));
    }

    let (actual_output, actual_exit) = match run_with_timeout(&scratch, name) {
        Ok(result) => result,
        Err(detail) => return Err(report("did not terminate", indent(&detail))),
    };

    let expected_output = expected_stdout(source);

    if actual_output != expected_output {
        return Err(report(
            "printed the wrong thing",
            difference(&expected_output, &actual_output),
        ));
    }

    let expected_exit = expected_exit_code(source);

    if actual_exit != expected_exit {
        return Err(report(
            "exited with the wrong code",
            format!("    expected {expected_exit}, got {actual_exit}\n"),
        ));
    }

    Ok(())
}

/// A clean `target/muji-tests/{name}` holding just the source, so the
/// compiler's cwd-relative output lands somewhere harmless.
fn prepare_scratch_directory(directory_name: &str, source: &Path, stem: &str) -> PathBuf {
    let scratch = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/muji-tests")
        .join(directory_name);

    if scratch.exists() {
        fs::remove_dir_all(&scratch).expect("cannot clear scratch directory");
    }
    fs::create_dir_all(&scratch).expect("cannot create scratch directory");
    fs::copy(source, scratch.join(format!("{stem}.muji"))).expect("cannot copy program");

    scratch
}

/// Run the compiled program, giving up after [`RUN_TIMEOUT`]. Returns its
/// stdout and exit code.
///
/// stdout goes to a file rather than a pipe: a pipe's buffer can fill while we
/// are still waiting, which would deadlock the very case this exists to catch.
fn run_with_timeout(directory: &Path, name: &str) -> Result<(String, i32), String> {
    let stdout_path = directory.join("stdout.txt");
    let stdout_file = fs::File::create(&stdout_path).expect("cannot create stdout file");

    let mut child = Command::new(format!("./{name}"))
        .current_dir(directory)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run {name}: {error}"));

    let deadline = Instant::now() + RUN_TIMEOUT;

    loop {
        match child.try_wait().expect("cannot poll the running program") {
            Some(status) => {
                let output = fs::read_to_string(&stdout_path).unwrap_or_default();
                return Ok((output, status.code().unwrap_or(-1)));
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();

                let so_far = fs::read_to_string(&stdout_path).unwrap_or_default();
                return Err(format!(
                    "still running after {} seconds; killed it.\nprinted so far:\n{}",
                    RUN_TIMEOUT.as_secs(),
                    if so_far.is_empty() {
                        "<nothing>".to_string()
                    } else {
                        so_far
                    }
                ));
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn run_in(directory: &Path, program: &str, arguments: &[&str]) -> Output {
    Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .output()
        .unwrap_or_else(|error| panic!("cannot run {program}: {error}"))
}

/// The `.out` file beside the program. Absent means "expected to print
/// nothing".
fn expected_stdout(source: &Path) -> String {
    fs::read_to_string(source.with_extension("out")).unwrap_or_default()
}

/// The `.exit` file beside the program, defaulting to 0. Every program exits 0
/// today, so no program has one yet — but once `muji`'s return value becomes
/// the process exit code, this is already here.
fn expected_exit_code(source: &Path) -> i32 {
    fs::read_to_string(source.with_extension("exit"))
        .map(|contents| {
            contents
                .trim()
                .parse()
                .expect("`.exit` file is not an integer")
        })
        .unwrap_or(0)
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A line-by-line comparison, marking the lines that differ. Enough to see
/// what went wrong without pulling in a diff crate.
fn difference(expected: &str, actual: &str) -> String {
    let expected: Vec<&str> = expected.lines().collect();
    let actual: Vec<&str> = actual.lines().collect();

    let mut report = String::from("    line | expected | actual\n");

    for line in 0..expected.len().max(actual.len()) {
        let left = expected.get(line).copied().unwrap_or("<missing>");
        let right = actual.get(line).copied().unwrap_or("<missing>");
        let marker = if left == right { ' ' } else { '*' };

        report.push_str(&format!(
            "  {marker} {:>4} | {left:>8} | {right}\n",
            line + 1
        ));
    }

    report
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>()
}
