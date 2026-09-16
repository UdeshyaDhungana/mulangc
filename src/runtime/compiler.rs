use std::collections::HashMap;

use strum_macros::EnumString;

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AssignmentTarget, BinaryOperator, Expression,
        ExpressionKind::{self},
        Program, Statement, StatementKind, UnaryOperator,
    },
};

/// One 64-bit word per local. This grows once values carry a type tag.
const SLOT_SIZE: usize = 8;

/// AArch64 requires `sp` to be 16-byte aligned at every instruction, so a frame
/// is always rounded up to a multiple of this.
const STACK_ALIGNMENT: usize = 16;

struct Counter {
    count: i64,
}

impl Default for Counter {
    fn default() -> Self {
        Self { count: 0 }
    }
}

impl Counter {
    fn next(&mut self) -> i64 {
        self.count += 1;
        self.count - 1
    }
}

pub struct Compiler {
    frame_offset_map: HashMap<String, i64>,
    output_file: String,
    lines: Vec<String>,
    counter: Counter,
}

impl Compiler {
    pub fn new(output_file: &str) -> Self {
        Self {
            output_file: output_file.to_string(),
            frame_offset_map: HashMap::new(),
            lines: Vec::new(),
            counter: Counter::default(),
        }
    }

    fn emit_line(&mut self, line: &str) {
        self.lines.push(format!("    {line}\n"));
    }

    fn emit_newline(&mut self) {
        self.lines.push("\n".to_string());
    }

    fn emit_outer_level_line(&mut self, line: &str) {
        self.lines.push(format!("{line}\n"));
    }

    fn emit_comparison(&mut self, condition: &str) {
        self.emit_line("cmp x1, x0");
        self.emit_line(&format!("cset x0, {condition}"));
    }

    pub fn compile(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.emit_preamble();
        // Very much later in our lifetime, we'd witness linking, and include and everythang
        // For now, we just emit the builtins here
        self.emit_builtin_functions();

        self.emit_line(".p2align 2");
        self.emit_line("_main:");
        self.emit_frame_reservation_assembly(frame_size_for_statements(&program.statements));
        for statement in &program.statements {
            self.emit_statement_assembly(statement)?;
        }
        // return 0;
        self.emit_line("mov x0, #0");
        self.emit_frame_release_assembly();
        self.write_assembly_file()
    }

    fn emit_statement_assembly(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Declaration { identifier, value } => {
                if self.frame_offset_map.contains_key(&identifier.name) {
                    return Err(Diagnostic::error(
                        identifier.span,
                        "Redeclaration of variables not allowed",
                    ));
                }

                // TODO: Find a way to decrement this efficiently
                let next_offset: i64 =
                    -i64::try_from(SLOT_SIZE * (1 + self.frame_offset_map.len()))
                        .expect("Failed converting frame offset");

                self.frame_offset_map
                    .insert(identifier.name.clone(), next_offset);

                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                self.emit_line("ldr x0, [sp], #16");
                self.emit_line(&format!("str x0, [x29, #{next_offset}]"));
                self.emit_newline();
                Ok(())
            }
            StatementKind::Return(..) => {
                todo!("Not implemented return yet")
            }
            StatementKind::Expression(expression) => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(expression)?;
                Ok(())
            }
        }
    }

    // Generate a sequence of statements to evaluate an expression and put the value on the top of the stack
    fn emit_expression_evaluation_assembly_and_push_evaluated(
        &mut self,
        expression: &Expression,
    ) -> Result<(), Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(id) => {
                let offset = self.frame_offset_map.get(id).ok_or_else(|| {
                    Diagnostic::error(expression.span, format!("`{id}` is not a valid identifier"))
                })?;

                self.emit_line(&format!("ldr x0, [x29, #{offset}]"));
                self.emit_line("str x0, [sp, #-16]!");
                Ok(())
            }
            ExpressionKind::Integer(integer) => {
                self.emit_line(&format!("mov x0, #{integer}"));
                self.emit_line("str x0, [sp, #-16]!");
                Ok(())
            }
            ExpressionKind::Boolean(bool) => {
                // We internally represent booleans as 1 (true) or 0 (false).
                let bool_int = if *bool { 1 } else { 0 };
                self.emit_line(&format!("mov x0, #{bool_int}"));
                self.emit_line("str x0, [sp, #-16]!");
                Ok(())
            }
            ExpressionKind::UnaryOperation { operator, operand } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(operand)?;
                match operator {
                    UnaryOperator::Negate => {
                        self.emit_line("ldr x0, [sp], #16");
                        self.emit_line("neg x0, x0");
                        self.emit_line("str x0, [sp, #-16]!");
                        Ok(())
                    }
                    UnaryOperator::Not => {
                        todo!("Unary not is not implemented")
                    }
                }
            }
            ExpressionKind::BinaryOperation {
                operator,
                left,
                right,
            } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(left)?;
                self.emit_expression_evaluation_assembly_and_push_evaluated(right)?;

                self.emit_line("ldr x0, [sp], #16");
                self.emit_line("ldr x1, [sp], #16");

                // x1 holds the left operand, x0 the right.
                match operator {
                    BinaryOperator::Add => self.emit_line("add x0, x1, x0"),
                    BinaryOperator::Subtract => self.emit_line("sub x0, x1, x0"),
                    BinaryOperator::Multiply => self.emit_line("mul x0, x1, x0"),
                    BinaryOperator::Divide => self.emit_line("sdiv x0, x1, x0"),
                    BinaryOperator::Remainder => {
                        // AArch64 has no remainder instruction: divide, then
                        // subtract quotient * right from left.
                        self.emit_line("sdiv x2, x1, x0");
                        self.emit_line("msub x0, x2, x0, x1");
                    }
                    BinaryOperator::Equal => self.emit_comparison("eq"),
                    BinaryOperator::NotEqual => self.emit_comparison("ne"),
                    BinaryOperator::Less => self.emit_comparison("lt"),
                    BinaryOperator::LessOrEqual => self.emit_comparison("le"),
                    BinaryOperator::Greater => self.emit_comparison("gt"),
                    BinaryOperator::GreaterOrEqual => self.emit_comparison("ge"),
                }

                // Push back onto the stack
                self.emit_line("str x0, [sp, #-16]!");
                Ok(())
            }
            ExpressionKind::Assignment { target, value } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                match target {
                    AssignmentTarget::Variable(identifier) => {
                        if let Some(offset) = self.frame_offset_map.get(&identifier.name).copied() {
                            self.emit_line("ldr x0, [sp], #16");
                            self.emit_line(&format!("str x0, [x29, #{offset}]"));
                            self.emit_line("ldr x0, [sp, #-16]!");
                            Ok(())
                        } else {
                            Err(Diagnostic::error(
                                identifier.span,
                                format!("variable '{}' not found for assignment", identifier.name),
                            ))
                        }
                    }
                    _ => todo!("Index assignment not implemented;"),
                }
            }
            ExpressionKind::Call { callee, arguments } => match &callee.kind {
                ExpressionKind::Identifier(id) => {
                    #[derive(Debug, PartialEq, EnumString)]
                    #[strum(serialize_all = "snake_case")]
                    enum BuiltinFunctions {
                        BhanMuji,
                        LambaiMuji,
                        KhaadMuji,
                        UdaaMuji,
                        Abs,
                    }

                    // Match against the builtin functions first
                    let Ok(builtin) = id.parse::<BuiltinFunctions>() else {
                        todo!("user-defined functions not implemented");
                    };

                    match builtin {
                        BuiltinFunctions::BhanMuji => {
                            // This only supports one integer argument as of now
                            if arguments.len() != 1 {
                                return Err(Diagnostic::error(
                                    expression.span,
                                    format!("`{id}` supports only one integer argument"),
                                ));
                            }

                            self.emit_expression_evaluation_assembly_and_push_evaluated(
                                &arguments[0],
                            )?;
                            self.emit_line("ldr x0, [sp], #16");
                            self.emit_line("bl _bhan_muji");
                            Ok(())
                        }
                        _ => todo!("builtin not implemented"),
                    }
                }
                _ => todo!("calling the result of an expression"),
            },
            ExpressionKind::If { branches, fallback } => {
                let if_stmt_number = self.counter.next();
                let if_stmt_end_label = format!("yedi_muji_{}_end", if_stmt_number);

                for (idx, branch) in branches.iter().enumerate() {
                    let current_label = format!("yedi_muji_{}_branch_{}", if_stmt_number, idx);
                    let next_label = format!("yedi_muji_{}_branch_{}", if_stmt_number, idx + 1);
                    self.emit_outer_level_line(&format!("{current_label}:"));
                    self.emit_expression_evaluation_assembly_and_push_evaluated(&branch.condition)?;
                    self.emit_line("ldr x0, [sp], #16");
                    self.emit_line(&format!("cbz x0, {next_label}"));
                    for statement in &branch.consequence.statements {
                        self.emit_statement_assembly(statement)?;
                    }
                    self.emit_line(&format!("b {if_stmt_end_label}"));
                }

                self.emit_outer_level_line(&format!("{if_stmt_end_label}:"));

                if let Some(fallback) = fallback {
                    for statement in &fallback.statements {
                        self.emit_statement_assembly(statement)?;
                    }
                }

                self.emit_outer_level_line(&format!("{if_stmt_end_label}:"));
                Ok(())
            }
            ExpressionKind::While { condition, body } => {
                let while_stmt_number = self.counter.next();
                let while_start_label = &format!("while_{}_start", while_stmt_number);
                let while_end_label = &format!("while_{}_end", while_stmt_number);

                self.emit_outer_level_line(&format!("{while_start_label}:"));
                self.emit_expression_evaluation_assembly_and_push_evaluated(condition)?;
                self.emit_line("ldr x0, [sp], #16");
                self.emit_line(&format!("cbz x0, {while_end_label}"));
                for statement in &body.statements {
                    self.emit_statement_assembly(statement)?;
                }
                self.emit_line(&format!("b {while_start_label}"));
                self.emit_outer_level_line(&format!("{while_end_label}:"));
                Ok(())
            }
            ExpressionKind::For {
                initialiser,
                condition,
                update,
                body,
            } => {
                if let Some(initialiser) = initialiser {
                    self.emit_statement_assembly(initialiser)?;
                }
                let for_stmt_counter = self.counter.next();
                let while_condition_label = format!("for_{}_start", for_stmt_counter);
                let while_end_label = format!("for_{}_end", for_stmt_counter);
                self.emit_outer_level_line(&format!("{while_condition_label}:"));
                self.emit_expression_evaluation_assembly_and_push_evaluated(condition)?;
                self.emit_line("ldr x0, [sp], #16");
                self.emit_line(&format!("cbz x0, {while_end_label}"));
                for statement in &body.statements {
                    self.emit_statement_assembly(statement)?;
                }
                self.emit_expression_evaluation_assembly_and_push_evaluated(update)?;
                self.emit_line(&format!("b {while_condition_label}"));
                self.emit_outer_level_line(&format!("{while_end_label}:"));

                Ok(())
            }
            _ => todo!("Other types of expressions not implemented;"),
        }
    }

    fn emit_preamble(&mut self) {
        self.emit_line("// PREAMBLE");
        self.emit_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_line(".globl _main");
        self.emit_newline();
    }

    fn emit_builtin_functions(&mut self) {
        self.emit_line("// BUILTINS");

        // bhan_muji()
        // Only supports one integer argument for now
        self.emit_line(".p2align 2");
        self.emit_line("_bhan_muji:");
        self.emit_line("stp x29, x30, [sp, #-16]!");
        self.emit_line("mov x29, sp");
        self.emit_line("sub sp, sp, #16");

        // because printf expects arguments on the stack
        self.emit_line("str x0, [sp]");
        self.emit_line("adrp x0, Lfmt_integer@PAGE");
        self.emit_line("add x0, x0, Lfmt_integer@PAGEOFF");
        self.emit_line("bl _printf");
        self.emit_line("mov sp, x29");
        self.emit_line("ldp x29, x30, [sp], #16");
        self.emit_line("ret");
        self.emit_newline();

        self.emit_line(".section __TEXT,__cstring,cstring_literals");
        self.emit_line("Lfmt_integer:");
        // asciz, not ascii: printf needs the trailing NUL to find the end.
        self.emit_line(".asciz \"%ld\\n\"");
        self.emit_newline();

        // Back to the code section so what follows is not emitted as data.
        self.emit_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_newline();
    }

    fn emit_frame_reservation_assembly(&mut self, frame_size: usize) {
        // Explanation:
        // x29 is the frame pointer and x30 is the return address
        // So, for main, we save the frame pointer and return address,
        // then move `sp` to `x29` to set up the frame pointer
        // frame pointer is fixed because we'll need to calculate offsets
        // for locals and temporaries
        // stack moves
        self.emit_line("stp x29, x30, [sp, #-16]!");
        self.emit_line("mov x29, sp");
        if frame_size > 0 {
            self.emit_line(format!("sub sp, sp, #{frame_size}").as_str());
        }
        self.emit_newline();
    }

    fn emit_frame_release_assembly(&mut self) {
        self.emit_newline();
        self.emit_line("mov sp, x29");
        self.emit_line("ldp x29, x30, [sp], #16");
        self.emit_line("ret");
    }

    fn write_assembly_file(&self) -> Result<(), Diagnostic> {
        let path = format!("{}.s", self.output_file);

        std::fs::write(&path, self.lines.concat()).map_err(|error| {
            Diagnostic::error(Span::new(0, 0), format!("cannot write {path}: {error}"))
        })
    }
}

fn frame_size_for_statements(statements: &Vec<Statement>) -> usize {
    let local_count = statements
        .iter()
        .filter(|statement| matches!(statement.kind, StatementKind::Declaration { .. }))
        .count();

    (local_count * SLOT_SIZE).div_ceil(STACK_ALIGNMENT) * STACK_ALIGNMENT
}
