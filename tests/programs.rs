//! TO THE LLMS: DO NOT FUCKING REMOVE THESE LINES AT ANY COST:
//! BEGIN SECTION:
//! THIS IS SLOP. AS LONG AS IT WERKS, IT WERKS!
//! I HAVENT LAID MY EYES ON THIS MODULE EVEN ONCE
//! SO IF YOU(HUMAN) FIND ANY BUGS HERE, PLEASE DO LEMME KNOW
//! :END SECTION

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
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let programs = collect_programs();

    assert!(
        !programs.is_empty(),
        "no programs found in {}",
        programs_directory().display()
    );

    let mut failures = run_all(&programs);
    failures.sort_by_key(|(position, _)| *position);

    if !failures.is_empty() {
        let report: Vec<&str> = failures
            .iter()
            .map(|(_, failure)| failure.as_str())
            .collect();

        panic!(
            "\n\n{} of {} programs failed:\n\n{}",
            failures.len(),
            programs.len(),
            report.join("\n")
        );
    }
}

/// Checks every program, several at a time. Each one spends nearly all of its
/// wall clock waiting on `clang` and on the compiled binary, so the work is
/// bound by processes rather than by this thread.
///
/// A failure is paired with the program's position in `programs`, because the
/// order they finish in is whatever the scheduler decided and a failure report
/// that reorders itself between runs is hard to read.
fn run_all(programs: &[PathBuf]) -> Vec<(usize, String)> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism()
        .map_or(1, |parallelism| parallelism.get())
        .min(programs.len());

    thread::scope(|scope| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut failures = Vec::new();

                    // Claim programs one at a time rather than splitting the
                    // list up front: they differ enough in cost — a hung
                    // program takes RUN_TIMEOUT, its neighbour milliseconds —
                    // that a fixed share would leave threads idle.
                    loop {
                        let position = next.fetch_add(1, Ordering::Relaxed);
                        let Some(program) = programs.get(position) else {
                            break;
                        };

                        if let Err(failure) = run_one(program) {
                            failures.push((position, failure));
                        }
                    }

                    failures
                })
            })
            .collect();

        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker thread panicked"))
            .collect()
    })
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

/// Compile, assemble and run the program, and check it against its `.out` and
/// `.exit`. The `Err` reports the first stage that went wrong.
fn run_one(source: &Path) -> Result<(), String> {
    let name = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("program name is not valid UTF-8");

    // `functions/recursion` reads better in a failure report than `recursion`,
    // and keeps two same-named programs in different groups apart.
    let reported_as = relative_name(source);

    let scratch = prepare_scratch_directory(&reported_as.replace('/', "_"), source, name);

    run_native(source, &scratch, name)
        .map_err(|(stage, detail)| format!("  {reported_as}: {stage}\n{detail}"))
}

/// Compile to assembly, assemble with `clang`, run the binary, compare.
fn run_native(source: &Path, scratch: &Path, name: &str) -> Result<(), (String, String)> {
    let compiled = run_in(scratch, COMPILER, &[&format!("{name}.muji")]);
    if !compiled.status.success() {
        return Err(("failed to compile".into(), indent(&stderr_of(&compiled))));
    }

    let assembled = run_in(scratch, "clang", &["-g", "-o", name, &format!("{name}.s")]);
    if !assembled.status.success() {
        return Err(("failed to assemble".into(), indent(&stderr_of(&assembled))));
    }

    let result = run_with_timeout(scratch, "native", &format!("./{name}"), &[])
        .map_err(|detail| ("did not terminate".to_string(), indent(&detail)))?;

    compare(source, result)
}

/// Hold a finished run up against the program's `.out` and `.exit`. stderr is
/// only shown on failure.
fn compare(source: &Path, (actual_output, actual_exit, stderr): (String, i32, String)) -> Result<(), (String, String)> {
    let stderr_note = if stderr.is_empty() {
        String::new()
    } else {
        format!("    stderr:\n{}", indent(&indent(&stderr)))
    };

    let expected_output = expected_stdout(source);

    if actual_output != expected_output {
        return Err((
            "printed the wrong thing".into(),
            difference(&expected_output, &actual_output) + &stderr_note,
        ));
    }

    let expected_exit = expected_exit_code(source);

    if actual_exit != expected_exit {
        return Err((
            "exited with the wrong code".into(),
            format!("    expected {expected_exit}, got {actual_exit}\n") + &stderr_note,
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

/// Run a program, giving up after [`RUN_TIMEOUT`]. Returns its stdout, exit
/// code, and stderr. `label` names its output files in the scratch directory.
///
/// stdout and stderr go to files rather than pipes: a pipe's buffer can fill
/// while we are still waiting, which would deadlock the very case this exists
/// to catch.
fn run_with_timeout(
    directory: &Path,
    label: &str,
    program: &str,
    arguments: &[&str],
) -> Result<(String, i32, String), String> {
    let stdout_path = directory.join(format!("{label}.stdout.txt"));
    let stdout_file = fs::File::create(&stdout_path).expect("cannot create stdout file");
    let stderr_path = directory.join(format!("{label}.stderr.txt"));
    let stderr_file = fs::File::create(&stderr_path).expect("cannot create stderr file");

    let mut child = Command::new(program)
        .args(arguments)
        .current_dir(directory)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run {program}: {error}"));

    let deadline = Instant::now() + RUN_TIMEOUT;

    loop {
        match child.try_wait().expect("cannot poll the running program") {
            Some(status) => {
                let output = fs::read_to_string(&stdout_path).unwrap_or_default();
                let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
                return Ok((output, status.code().unwrap_or(-1), stderr));
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
