//! End-to-end tests: mulang source in, printed output or an error message out.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use mulang::runtime::Interpreter;
use mulang::syntax;

/// Runs `source` and returns everything it printed.
fn output_of(source: &str) -> String {
    let printed = SharedBuffer::default();
    let program = syntax::parse(source)
        .unwrap_or_else(|diagnostics| panic!("should parse: {}", diagnostics[0].message));

    Interpreter::writing_to(Box::new(printed.clone()))
        .run(&program)
        .unwrap_or_else(|diagnostic| panic!("should run: {}", diagnostic.message));

    printed.contents()
}

/// Runs `source` and returns the message of the error it failed with.
fn runtime_error_of(source: &str) -> String {
    let program = syntax::parse(source)
        .unwrap_or_else(|diagnostics| panic!("should parse: {}", diagnostics[0].message));

    Interpreter::writing_to(Box::new(Vec::new()))
        .run(&program)
        .expect_err("should fail at run time")
        .message
}

fn run_example(file_name: &str) -> String {
    let path = format!("{}/examples/{file_name}", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&path).expect("example should exist");
    output_of(&source)
}

/* ------------------------------------------------------------------ language */

#[test]
fn declares_and_reads_variables() {
    assert_eq!(output_of("thoos_muji x = 43; bhan_muji(x);"), "43\n");
}

#[test]
fn arithmetic_follows_precedence() {
    assert_eq!(output_of("bhan_muji(2 + 3 * 4);"), "14\n");
    assert_eq!(output_of("bhan_muji((2 + 3) * 4);"), "20\n");
}

#[test]
fn integers_and_decimals_mix() {
    assert_eq!(output_of("bhan_muji(1 + 0.5);"), "1.5\n");
    assert_eq!(output_of("bhan_muji(7 / 2);"), "3\n");
    assert_eq!(output_of("bhan_muji(7 % 3);"), "1\n");
}

#[test]
fn strings_join_with_plus() {
    assert_eq!(
        output_of(r#"bhan_muji("mu" + "ji" + " lang");"#),
        "muji lang\n"
    );
}

#[test]
fn conditionals_pick_the_first_matching_branch() {
    let source = r#"
        thoos_muji describe = kaam_gar_muji(number) {
            yedi_muji (number > 0) {
                patha_muji "Positive";
            } nabhae_muji (number < 0) {
                patha_muji "Negative";
            } nabhae_chikne {
                patha_muji "Zero!";
            }
        };
        bhan_muji(describe(-1));
        bhan_muji(describe(0));
        bhan_muji(describe(1));
    "#;
    assert_eq!(output_of(source), "Negative\nZero!\nPositive\n");
}

#[test]
fn a_conditional_needs_no_fallback() {
    let source =
        "yedi_muji (1 > 2) { bhan_muji(\"no\"); } nabhae_muji (2 > 1) { bhan_muji(\"yes\"); }";
    assert_eq!(output_of(source), "yes\n");
}

#[test]
fn while_loops_accumulate() {
    let source = r#"
        thoos_muji i = 0;
        thoos_muji sum = 0;
        jaba_samma_muji (i < 5) {
            sum = sum + i;
            i = i + 1;
        }
        bhan_muji(sum);
    "#;
    assert_eq!(output_of(source), "10\n");
}

#[test]
fn for_loops_accumulate() {
    let source = r#"
        thoos_muji sum = 0;
        ghuma_muji (thoos_muji i = 0; i < 5; i = i + 1) {
            sum = sum + i;
        }
        bhan_muji(sum);
    "#;
    assert_eq!(output_of(source), "10\n");
}

#[test]
fn a_loop_variable_does_not_outlive_its_loop() {
    let source = "ghuma_muji (thoos_muji i = 0; i < 1; i = i + 1) { i; } i;";
    assert_eq!(runtime_error_of(source), "`i` is not defined");
}

#[test]
fn returning_from_inside_a_loop_leaves_the_function() {
    let source = r#"
        thoos_muji firstOver = kaam_gar_muji(limit) {
            thoos_muji i = 0;
            jaba_samma_muji (sacho_muji) {
                yedi_muji (i > limit) {
                    patha_muji i;
                }
                i = i + 1;
            }
        };
        bhan_muji(firstOver(3));
    "#;
    assert_eq!(output_of(source), "4\n");
}

#[test]
fn arrays_are_indexed_and_mutated_in_place() {
    let source = r#"
        thoos_muji values = [1, "two", 3.5];
        values[0] = 99;
        bhan_muji(values);
        bhan_muji(values[1]);
    "#;
    assert_eq!(output_of(source), "[99, \"two\", 3.5]\ntwo\n");
}

#[test]
fn hash_maps_are_indexed_and_extended() {
    let source = r#"
        thoos_muji codes = { "NP": "+977", "IN": "+91" };
        codes["US"] = "+1";
        bhan_muji(codes["NP"]);
        bhan_muji(lambai_muji(codes));
    "#;
    assert_eq!(output_of(source), "+977\n3\n");
}

#[test]
fn an_absent_hash_map_key_reads_as_khali_muji() {
    assert_eq!(output_of(r#"bhan_muji({"a": 1}["b"]);"#), "khali_muji\n");
}

#[test]
fn functions_are_values_that_can_be_passed_and_returned() {
    let source = r#"
        thoos_muji twice = kaam_gar_muji(f, x) { patha_muji f(f(x)); };
        thoos_muji increment = kaam_gar_muji(x) { patha_muji x + 1; };
        bhan_muji(twice(increment, 5));
    "#;
    assert_eq!(output_of(source), "7\n");
}

#[test]
fn a_function_returned_from_a_call_is_callable_on_the_spot() {
    let source = r#"
        thoos_muji adder = kaam_gar_muji(x) {
            patha_muji kaam_gar_muji(y) { patha_muji x + y; };
        };
        bhan_muji(adder(2)(3));
    "#;
    assert_eq!(output_of(source), "5\n");
}

#[test]
fn a_closure_keeps_its_own_captured_variable_per_call() {
    let source = r#"
        thoos_muji adder = kaam_gar_muji(x) {
            patha_muji kaam_gar_muji(y) { patha_muji x + y; };
        };
        thoos_muji addTwo = adder(2);
        thoos_muji addTen = adder(10);
        bhan_muji(addTwo(1));
        bhan_muji(addTen(1));
    "#;
    assert_eq!(output_of(source), "3\n11\n");
}

#[test]
fn recursion_works() {
    let source = r#"
        thoos_muji factorial = kaam_gar_muji(x) {
            yedi_muji (x == 0) {
                patha_muji 1;
            } nabhae_chikne {
                patha_muji x * factorial(x - 1);
            }
        };
        bhan_muji(factorial(10));
    "#;
    assert_eq!(output_of(source), "3628800\n");
}

#[test]
fn a_mutable_array_argument_is_shared_with_the_caller() {
    let source = r#"
        thoos_muji fill = kaam_gar_muji(target) { target[0] = "set"; };
        thoos_muji values = [0];
        fill(values);
        bhan_muji(values);
    "#;
    assert_eq!(output_of(source), "[\"set\"]\n");
}

#[test]
fn builtins_cover_length_append_and_remove() {
    let source = r#"
        thoos_muji values = [1, 2, 3];
        khaad_muji(values, 4);
        bhan_muji(lambai_muji(values));
        bhan_muji(udaa_muji(values, 0));
        bhan_muji(values);
        bhan_muji(abs(-69));
    "#;
    assert_eq!(output_of(source), "4\n1\n[2, 3, 4]\n69\n");
}

#[test]
fn comments_are_ignored() {
    assert_eq!(
        output_of("$ this is skipped; entirely $ bhan_muji(1);"),
        "1\n"
    );
}

/* --------------------------------------------------------------- diagnostics */

#[test]
fn dividing_by_zero_reports_rather_than_crashes() {
    assert_eq!(runtime_error_of("1 / 0;"), "division by zero");
}

#[test]
fn indexing_past_the_end_reports_rather_than_crashes() {
    assert_eq!(
        runtime_error_of("thoos_muji a = [1]; a[5];"),
        "index 5 is outside this array of 1 element(s)"
    );
}

#[test]
fn calling_a_non_function_reports_rather_than_crashes() {
    assert_eq!(
        runtime_error_of("thoos_muji x = 1; x(2);"),
        "cannot call integer; it is not a function"
    );
}

#[test]
fn assigning_to_an_undeclared_variable_is_an_error() {
    assert_eq!(runtime_error_of("missing = 1;"), "`missing` is not defined");
}

#[test]
fn passing_the_wrong_number_of_arguments_is_an_error() {
    assert_eq!(
        runtime_error_of("thoos_muji f = kaam_gar_muji(a, b) { a; }; f(1);"),
        "this function takes 2 argument(s), but was given 1"
    );
}

#[test]
fn unbounded_recursion_reports_rather_than_exhausting_the_stack() {
    // A default test thread has less stack than the interpreter's call limit
    // needs, so this one runs on a thread sized the way `main` sizes its own.
    let message = std::thread::Builder::new()
        .stack_size(mulang::runtime::REQUIRED_STACK_BYTES)
        .spawn(|| {
            runtime_error_of(
                "thoos_muji forever = kaam_gar_muji(x) { patha_muji forever(x); }; forever(1);",
            )
        })
        .expect("should spawn")
        .join()
        .expect("should not panic");

    assert!(message.contains("nested more than"), "got: {message}");
}

#[test]
fn an_error_points_at_the_source_that_caused_it() {
    let source = "thoos_muji a = 1;\nthoos_muji b = a / 0;\n";
    let program = syntax::parse(source).expect("should parse");
    let report = Interpreter::writing_to(Box::new(Vec::new()))
        .run(&program)
        .expect_err("should fail")
        .render(source, "test.muji");

    assert!(
        report.contains("test.muji:2:16"),
        "unexpected report:\n{report}"
    );
}

/* ------------------------------------------------------------------ examples */

#[test]
fn bubble_sort_example_sorts() {
    assert_eq!(run_example("bubble-sort.muji"), "[1, 3, 4, 5, 9]\n");
}

#[test]
fn recursion_example_computes_a_factorial() {
    assert_eq!(run_example("recursion.muji"), "24\n");
}

#[test]
fn positive_negative_example_classifies_each_number() {
    assert_eq!(
        run_example("positive_negative.muji"),
        "Negative\nZero!\nPositive\n"
    );
}

#[test]
fn newton_raphson_example_converges_on_a_root() {
    let printed = run_example("newton-rhapson.muji");
    let root: f64 = printed.trim().parse().expect("should print a number");
    assert!((root - 1.0).abs() < 1e-6, "converged to {root}");
}

#[test]
fn sum_example_runs_to_completion() {
    assert_eq!(run_example("sum.muji"), "");
}

/* --------------------------------------------------------------------- setup */

/// A writer the test can still read after the interpreter has taken ownership
/// of it.
#[derive(Clone, Default)]
struct SharedBuffer(Rc<RefCell<Vec<u8>>>);

impl SharedBuffer {
    fn contents(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).expect("output should be utf-8")
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
