use std::collections::HashMap;

use strum_macros::EnumString;

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AssignmentTarget, BinaryOperator, Block, Conditional, Else, Expression,
        ExpressionKind::{self},
        Program, Statement, StatementKind, UnaryOperator,
    },
};

/// Stack a thread must have to run the compiler.
///
/// Code generation recurses over the AST, so deeply nested source needs room to
/// walk. Generous rather than measured: it costs address space, not memory.
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;

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

struct Frame {
    size: usize,
    offset_map: HashMap<String, i64>,
    // is_functional is true if the frame belongs to a
    // function and not a block statement
    is_functional: bool,
}

impl Frame {
    fn get_next_offset(&self) -> i64 {
        -i64::try_from(SLOT_SIZE * (1 + self.offset_map.len()))
            .expect("Failed converting frame offset")
    }
}

struct Section {
    lines: Vec<String>,
}

impl Default for Section {
    fn default() -> Self {
        Self { lines: Vec::new() }
    }
}

pub struct Compiler {
    // Static scope
    static_frames: Vec<Frame>,
    output_file: String,
    // For generating different text sections during codegen
    text_section_stack: Vec<Section>,
    registered_text_sections: Vec<Section>,
    // Easy for labelling
    counter: Counter,
}

impl Compiler {
    pub fn new(output_file: &str) -> Self {
        Self {
            output_file: output_file.to_string(),
            static_frames: Vec::new(),
            text_section_stack: vec![Default::default()],
            registered_text_sections: Vec::new(),
            counter: Counter::default(),
        }
    }

    fn begin_new_text_section(&mut self) {
        self.text_section_stack.push(Default::default());
    }

    fn get_current_text_section(&mut self) -> &mut Section {
        self.text_section_stack.last_mut().unwrap()
    }

    fn end_current_text_section_and_register(&mut self) {
        self.registered_text_sections
            .push(self.text_section_stack.pop().unwrap())
    }

    fn push_static_frame_and_emit_frame_push_assembly(
        &mut self,
        frame_size: usize,
        is_functional: bool,
    ) {
        self.static_frames.push({
            Frame {
                size: frame_size,
                offset_map: HashMap::new(),
                is_functional: is_functional,
            }
        });

        // Assembly for frame reservation
        // x29 is the frame pointer and x30 is the return address
        // So, for a new frame, we save the frame pointer and return address,
        // then move `stack pointer` to `x29` to set up the frame pointer
        // frame pointer is fixed because we'll need to calculate offsets
        // for locals
        self.emit_line("stp x29, x30, [sp, #-16]!");
        self.emit_line("mov x29, sp");
        if frame_size > 0 {
            self.emit_line(format!("sub sp, sp, #{frame_size}").as_str());
        }
        self.emit_newline();
    }

    fn pop_static_frame_and_emit_frame_release_assembly(&mut self) {
        self.static_frames.pop();
        self.emit_frame_release_assembly();
    }

    fn emit_frame_release_assembly(&mut self) {
        self.emit_newline();
        self.emit_line("mov sp, x29");
        self.emit_line("ldp x29, x30, [sp], #16");
    }

    /// Unwind every enclosing block frame and then the function's own, so a
    /// `de` buried in nested blocks still leaves through one epilogue.
    fn emit_essembly_to_destroy_all_frames_until_first_enclosing_function(&mut self) {
        let frames_to_release = self
            .static_frames
            .iter()
            .rev()
            .position(|frame| frame.is_functional)
            .expect("Expected an enclosing function frame")
            + 1;

        for _ in 0..frames_to_release {
            self.emit_frame_release_assembly();
        }
    }

    fn emit_line(&mut self, line: &str) {
        self.get_current_text_section()
            .lines
            .push(format!("    {line}\n"));
    }

    fn emit_newline(&mut self) {
        self.get_current_text_section().lines.push("\n".to_string());
    }

    fn emit_outer_level_line(&mut self, line: &str) {
        self.get_current_text_section()
            .lines
            .push(format!("{line}\n"));
    }

    fn emit_comparison_result_to_x0(&mut self, condition: &str) {
        self.emit_line("cmp x1, x0");
        self.emit_line(&format!("cset x0, {condition}"));
    }

    // In arm64, immediates can take only 2 bytes at a time
    // So, for 8 byte immediates, we need at max 4 operations
    fn emit_load_immediate_into_x0(&mut self, value: i64) {
        let bits = value as u64;
        let mut placed_first_halfword = false;

        for index in 0..4 {
            let shift = index * 16;
            let halfword = (bits >> shift) & 0xFFFF;

            // Prevent double-zeroing
            if halfword == 0 {
                continue;
            }

            let instruction = if placed_first_halfword {
                "movk"
            } else {
                "movz"
            };
            self.emit_line(&format!("{instruction} x0, #{halfword}, lsl #{shift}"));
            placed_first_halfword = true;
        }

        if !placed_first_halfword {
            self.emit_line("movz x0, #0");
        }
    }

    /// Push `x0` onto the stack.
    /// A value occupies 16 bytes rather than 8 because `sp` must stay 16-byte
    /// aligned at every instruction, so the upper half of each slot is padding.
    fn push_from_x0(&mut self) {
        self.emit_line("str x0, [sp, #-16]!");
    }

    /// Pop the top of the stack into `x0`.
    fn pop_to_x0(&mut self) {
        self.emit_line("ldr x0, [sp], #16");
    }

    /// Push the value of an expression that does not have one. Every
    /// expression must leave exactly one value on the stack, so the ones that
    /// produce nothing useful still push a placeholder for their consumer to discard.
    fn push_void(&mut self) {
        self.emit_line("mov x0, #0");
        self.push_from_x0();
    }

    /// Throw away the value on top of the stack, for an expression evaluated
    /// only for its side effect.
    fn pop_and_ignore_void(&mut self) {
        self.emit_line("add sp, sp, #16");
    }

    fn get_topmost_static_frame(&mut self) -> &mut Frame {
        self.static_frames.last_mut().unwrap()
    }

    pub fn compile(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.begin_new_text_section();
        self.emit_preamble();
        // Very much later in our lifetime, we'd witness linking, and include and everythang
        // For now, we just emit the builtins here
        self.emit_builtin_functions();

        self.emit_outer_level_line(".p2align 2");
        self.emit_outer_level_line("_main:");
        self.push_static_frame_and_emit_frame_push_assembly(
            frame_size_for_statements(&program.statements),
            true,
        );
        for statement in &program.statements {
            self.emit_statement_assembly(statement)?;
        }
        // return 0;
        self.emit_line("mov x0, #0");
        self.pop_static_frame_and_emit_frame_release_assembly();
        self.emit_line("ret");
        self.end_current_text_section_and_register();
        self.write_assembly_file()
    }

    fn emit_statement_assembly(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Declaration { identifier, value } => {
                if self
                    .get_topmost_static_frame()
                    .offset_map
                    .contains_key(&identifier.name)
                {
                    return Err(Diagnostic::error(
                        identifier.span,
                        "Redeclaration of variable in the same scope is not allowed",
                    ));
                }

                let next_offset = self.get_topmost_static_frame().get_next_offset();

                self.get_topmost_static_frame()
                    .offset_map
                    .insert(identifier.name.clone(), next_offset);

                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                self.pop_to_x0();
                self.emit_line(&format!("str x0, [x29, #{next_offset}]"));
                self.emit_newline();
                Ok(())
            }
            StatementKind::Return(return_expression) => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(return_expression)?;
                self.pop_to_x0();
                // Instead of this, what return should be responsible for
                // Is walking down the frames stack, and pop all non functional
                // frames
                self.emit_essembly_to_destroy_all_frames_until_first_enclosing_function();
                self.emit_line("ret");
                Ok(())
            }
            StatementKind::Expression(expression) => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(expression)?;
                self.pop_and_ignore_void();
                Ok(())
            }
            StatementKind::Function(definition) => {
                self.begin_new_text_section();
                self.emit_outer_level_line(&format!(
                    "// dhanda {}({})",
                    definition.name,
                    definition
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.as_str())
                        .collect::<Vec<&str>>()
                        .join(", ")
                ));
                self.emit_outer_level_line(".p2align 2");
                self.emit_outer_level_line(&format!("{}:", definition.name));
                let frame_size_for_function_body_statements =
                    frame_size_for_statements(&definition.body.statements);
                let frame_size_for_parameters = definition.parameters.len() * 16;
                // TODO: When we support more than 8 arguments, we'll run out of registers and
                // will have to use the stack, in that case, we need to find a way to preserve the locations
                // of the stack-living arguments so we can push them again later in the function body
                self.push_static_frame_and_emit_frame_push_assembly(
                    frame_size_for_function_body_statements + frame_size_for_parameters,
                    true,
                );
                for (idx, parameter) in definition.parameters.iter().enumerate() {
                    let next_offset = self.get_topmost_static_frame().get_next_offset();
                    self.get_topmost_static_frame()
                        .offset_map
                        .insert(parameter.name.clone(), next_offset);
                    // Get where that variable lives
                    // Later on we'll have to make this dynamic to fetch these from stack
                    let register_name = format!("x{}", idx);
                    self.emit_line(&format!("str {register_name}, [x29, #{next_offset}]"));
                }
                self.emit_newline();
                let mut has_return = false;
                for statement in definition.body.statements.iter() {
                    self.emit_statement_assembly(statement)?;
                    match statement.kind {
                        StatementKind::Return(..) => {
                            has_return = true;
                        }
                        _ => {}
                    }
                }
                // Return void if a function doesn't contain a return statement
                if !has_return {
                    self.emit_line("mov x0, #0");
                    self.emit_essembly_to_destroy_all_frames_until_first_enclosing_function();
                    self.emit_line("ret");
                }
                self.pop_static_frame_and_emit_frame_release_assembly();
                self.end_current_text_section_and_register();
                Ok(())
            }
            StatementKind::If(conditional) => self.emit_conditional_assembly(conditional),
            StatementKind::While { condition, body } => {
                let while_stmt_number = self.counter.next();
                // `L` marks a label the assembler resolves and then discards,
                // instead of exporting it into the object file's symbol table.
                let while_start_label = &format!("Lwhile_{}_start", while_stmt_number);
                let while_end_label = &format!("Lwhile_{}_end", while_stmt_number);

                self.emit_outer_level_line(&format!("{while_start_label}:"));
                self.emit_expression_evaluation_assembly_and_push_evaluated(condition)?;
                self.pop_to_x0();
                self.emit_line(&format!("cbz x0, {while_end_label}"));
                self.emit_block_assembly(body)?;
                self.emit_line(&format!("b {while_start_label}"));
                self.emit_outer_level_line(&format!("{while_end_label}:"));
                Ok(())
            }
            StatementKind::For {
                initialiser,
                condition,
                update,
                body,
            } => {
                // Option::iter yields one item or none, so a header without a
                // declaration collects to an empty Vec and reserves no slot.
                let initialiser_statements: Vec<Statement> = initialiser
                    .iter()
                    .map(|statement| (**statement).clone())
                    .collect();

                self.push_static_frame_and_emit_frame_push_assembly(
                    frame_size_for_statements(&initialiser_statements),
                    false,
                );

                let for_stmt_counter = self.counter.next();
                if let Some(initialiser) = initialiser {
                    self.emit_statement_assembly(initialiser)?;
                }
                let while_condition_label = format!("Lfor_{}_start", for_stmt_counter);
                let while_end_label = format!("Lfor_{}_end", for_stmt_counter);
                self.emit_outer_level_line(&format!("{while_condition_label}:"));
                self.emit_expression_evaluation_assembly_and_push_evaluated(condition)?;
                self.pop_to_x0();
                self.emit_line(&format!("cbz x0, {while_end_label}"));
                self.emit_block_assembly(body)?;
                self.emit_expression_evaluation_assembly_and_push_evaluated(update)?;
                // The update runs for its effect, once per iteration. Leaving
                // its value behind walks the stack downwards for as long as the
                // loop runs.
                self.pop_and_ignore_void();
                self.emit_line(&format!("b {while_condition_label}"));
                self.emit_outer_level_line(&format!("{while_end_label}:"));
                self.pop_static_frame_and_emit_frame_release_assembly();
                Ok(())
            }
        }
    }

    fn emit_conditional_assembly(&mut self, conditional: &Conditional) -> Result<(), Diagnostic> {
        let conditional_number = self.counter.next();
        let else_label = format!("Lif_{conditional_number}_else");
        let end_label = format!("Lif_{conditional_number}_end");

        self.emit_expression_evaluation_assembly_and_push_evaluated(&conditional.condition)?;
        self.pop_to_x0();
        self.emit_line(&format!("cbz x0, {else_label}"));

        self.emit_block_assembly(&conditional.then)?;
        self.emit_line(&format!("b {end_label}"));

        self.emit_outer_level_line(&format!("{else_label}:"));
        match conditional.otherwise.as_deref() {
            Some(Else::Block(block)) => self.emit_block_assembly(block)?,
            Some(Else::If(conditional)) => self.emit_conditional_assembly(conditional)?,
            None => {}
        }

        self.emit_outer_level_line(&format!("{end_label}:"));
        Ok(())
    }

    /// A block's statements, in a frame of their own so its locals go out of
    /// scope with it.
    fn emit_block_assembly(&mut self, block: &Block) -> Result<(), Diagnostic> {
        self.push_static_frame_and_emit_frame_push_assembly(
            frame_size_for_statements(&block.statements),
            false,
        );
        for statement in &block.statements {
            self.emit_statement_assembly(statement)?;
        }
        self.pop_static_frame_and_emit_frame_release_assembly();
        Ok(())
    }

    // Generate a sequence of statements to evaluate an expression and put the value on the top of the stack
    fn emit_expression_evaluation_assembly_and_push_evaluated(
        &mut self,
        expression: &Expression,
    ) -> Result<(), Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let mut sum_of_frame_sizes = 0;
                let mut containing_frame: Option<&Frame> = None;
                for (idx, frame) in self.static_frames.iter().rev().enumerate() {
                    if idx != 0 {
                        sum_of_frame_sizes += 16 + frame.size;
                    }

                    if frame.offset_map.contains_key(identifier) {
                        containing_frame = Some(frame);
                        break;
                    }
                }

                if containing_frame.is_none() {
                    return Err(Diagnostic::error(
                        expression.span,
                        format!("`{identifier}` is not a valid identifier"),
                    ));
                }

                let local_offset = containing_frame
                    .unwrap()
                    .offset_map
                    .get(identifier)
                    .unwrap();

                let offset = (TryInto::<i64>::try_into(sum_of_frame_sizes).unwrap()) + local_offset;

                self.emit_line(&format!("ldr x0, [x29, #{offset}]"));
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::Integer(integer) => {
                self.emit_load_immediate_into_x0(*integer);
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::Boolean(bool) => {
                // We internally represent booleans as 1 (true) or 0 (false).
                let bool_int = if *bool { 1 } else { 0 };
                self.emit_line(&format!("mov x0, #{bool_int}"));
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::UnaryOperation { operator, operand } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(operand)?;
                match operator {
                    UnaryOperator::Negate => {
                        self.pop_to_x0();
                        self.emit_line("neg x0, x0");
                        self.push_from_x0();
                        Ok(())
                    }
                    UnaryOperator::Not => {
                        self.pop_to_x0();
                        self.emit_line("cmp x0, #0");
                        self.emit_line("cset x0, eq");
                        self.push_from_x0();
                        Ok(())
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
                self.pop_to_x0();
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
                    BinaryOperator::Equal => self.emit_comparison_result_to_x0("eq"),
                    BinaryOperator::NotEqual => self.emit_comparison_result_to_x0("ne"),
                    BinaryOperator::Less => self.emit_comparison_result_to_x0("lt"),
                    BinaryOperator::LessOrEqual => self.emit_comparison_result_to_x0("le"),
                    BinaryOperator::Greater => self.emit_comparison_result_to_x0("gt"),
                    BinaryOperator::GreaterOrEqual => self.emit_comparison_result_to_x0("ge"),
                    BinaryOperator::BooleanAnd => {
                        self.emit_line("and x0, x0, x1");
                    }
                    BinaryOperator::BooleanOr => {
                        self.emit_line("orr x0, x0, x1");
                    }
                }

                // Push back onto the stack
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::Assignment { target, value } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                match target {
                    AssignmentTarget::Variable(identifier) => {
                        let mut sum_of_frame_sizes = 0;
                        let mut containing_frame: Option<&Frame> = None;
                        for (idx, frame) in self.static_frames.iter().rev().enumerate() {
                            if idx != 0 {
                                sum_of_frame_sizes += 16 + frame.size;
                            }

                            if frame.offset_map.contains_key(&identifier.name) {
                                containing_frame = Some(frame);
                                break;
                            }
                        }

                        if containing_frame.is_none() {
                            return Err(Diagnostic::error(
                                identifier.span,
                                format!("variable '{}' not found for assignment", identifier.name),
                            ));
                        }

                        let local_offset = containing_frame
                            .unwrap()
                            .offset_map
                            .get(&identifier.name)
                            .unwrap();

                        let offset =
                            (TryInto::<i64>::try_into(sum_of_frame_sizes).unwrap()) + local_offset;

                        self.pop_to_x0();
                        self.emit_line(&format!("str x0, [x29, #{offset}]"));
                        // An assignment is an expression whose value is the
                        // value assigned, so `x = y = 3` works and binds to the
                        // right. `x0` still holds it, so push it back.
                        self.push_from_x0();
                        Ok(())
                    }
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                // TODO: MOVE BUILTIN FUNCTIONS AS A SEPARATE LIBRARY
                // FUNCTION RATHER THAN A BUILTIN FUNCTION
                // THERE MUST BE NO BUILTIN FUNCTIONS
                #[derive(Debug, PartialEq, EnumString)]
                #[strum(serialize_all = "snake_case")]
                enum BuiltinFunctions {
                    Bol,
                    Abs,
                }

                // Match against the builtin functions first
                let Ok(builtin) = callee.name.parse::<BuiltinFunctions>() else {
                    if arguments.len() > 8 {
                        todo!("Not supported more than 8 arguments");
                    }
                    for argument in arguments.iter() {
                        self.emit_expression_evaluation_assembly_and_push_evaluated(argument)?;
                    }
                    for idx in (0..arguments.len()).rev() {
                        let register = format!("x{idx}");
                        self.emit_line(&format!("ldr {register}, [sp], #16"));
                    }
                    self.emit_line(&format!("bl {}", callee.name));
                    // Push the value onto the stack
                    self.push_from_x0();
                    return Ok(());
                };

                match builtin {
                    BuiltinFunctions::Bol => {
                        // This only supports one integer argument as of now
                        if arguments.len() != 1 {
                            return Err(Diagnostic::error(
                                expression.span,
                                "`bol` supports only one integer argument",
                            ));
                        }

                        self.emit_expression_evaluation_assembly_and_push_evaluated(&arguments[0])?;
                        self.pop_to_x0();
                        self.emit_line("bl _bol");
                        // `bol` prints and yields nothing, and the call
                        // has clobbered `x0` anyway.
                        self.push_void();
                        Ok(())
                    }
                    BuiltinFunctions::Abs => todo!("abs not implemented"),
                }
            }
        }
    }

    fn emit_preamble(&mut self) {
        self.emit_outer_level_line("// PREAMBLE");
        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_outer_level_line(".globl _main");
        self.emit_newline();
    }

    fn emit_builtin_functions(&mut self) {
        self.emit_outer_level_line("// BUILTINS");

        // bol()
        // Only supports one integer argument for now
        self.emit_outer_level_line(".p2align 2");
        self.emit_outer_level_line("_bol:");

        // Could use a frame here, but not necessary for a builtin
        self.push_static_frame_and_emit_frame_push_assembly(16, true);

        // because printf expects arguments on the stack
        self.emit_line("str x0, [sp]");
        self.emit_line("adrp x0, Lfmt_integer@PAGE");
        self.emit_line("add x0, x0, Lfmt_integer@PAGEOFF");
        self.emit_line("bl _printf");

        self.pop_static_frame_and_emit_frame_release_assembly();

        self.emit_line("ret");
        self.emit_newline();

        self.emit_outer_level_line(".section __TEXT,__cstring,cstring_literals");
        self.emit_outer_level_line("Lfmt_integer:");
        // asciz, not ascii: printf needs the trailing NUL to find the end.
        self.emit_line(".asciz \"%ld\\n\"");
        self.emit_newline();

        // Back to the code section so what follows is not emitted as data.
        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_newline();
    }

    fn write_assembly_file(&self) -> Result<(), Diagnostic> {
        let path = format!("{}.s", self.output_file);

        let contents = self
            .registered_text_sections
            .iter()
            // sections that end early go later
            .rev()
            .map(|section| section.lines.concat())
            .collect::<Vec<String>>()
            .join("\n\n");

        std::fs::write(&path, contents).map_err(|error| {
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
