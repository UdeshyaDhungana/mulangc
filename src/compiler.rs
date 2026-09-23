use std::{collections::HashMap, ops::Deref};

use crate::{
    diagnostics::{Diagnostic, Span},
    syntax::{
        AssignmentTarget, BinaryOperator, Block, Conditional, Else, Expression,
        ExpressionKind::{self},
        Program, Statement, StatementKind, UnaryOperator,
        sema::Resolution,
    },
};

type MemoryUnit = usize;
const MEMORY_UNIT_SCALE: usize = 8;
pub const REQUIRED_STACK_BYTES: usize = 64 * 1024 * 1024;
const STACK_ALIGNMENT: usize = 16;

struct Frame {
    capacity_in_bytes: usize,
    offset_map: HashMap<String, i32>,
}

impl Frame {
    fn new(capacity_in_bytes: usize) -> Self {
        // arm64 requirement
        if capacity_in_bytes % 16 != 0 {
            panic!("Frame size must be a multiple of 16");
        }
        Self {
            capacity_in_bytes,
            offset_map: HashMap::new(),
        }
    }

    fn get_next_offset(&self) -> i32 {
        -i32::try_from(MEMORY_UNIT_SCALE * (1 + self.offset_map.len())).expect("Failed to convert ")
    }

    fn add_node_id_and_offset(&mut self, node_id: String, offset: i32) {
        // We assume that each entry will take up one memory unit
        // which is true for the moment, but will break later when structs are introduced
        if self.capacity_in_bytes < (self.offset_map.len() * MEMORY_UNIT_SCALE) + 1 {
            panic!("Adding more entries to frame will exceed its capacity");
        }
        self.offset_map.insert(node_id, offset);
    }
}

#[derive(Default)]
struct Section {
    lines: Vec<String>,
}

pub struct CodeGenerator {
    output_file: String,

    // Static scoping
    resolution: Resolution,
    static_frames: Vec<Frame>,

    // For generating different text sections during codegen
    text_section_stack: Vec<Section>,
    registered_text_sections: Vec<Section>,
    global_map: HashMap<String, i64>,
}

impl CodeGenerator {
    pub fn new(resolution: Resolution, output_file: &str) -> Self {
        Self {
            output_file: output_file.to_string(),
            static_frames: Vec::new(),
            text_section_stack: vec![Default::default()],
            registered_text_sections: Vec::new(),
            resolution: resolution,
            global_map: HashMap::new(),
        }
    }

    fn is_global(&self, node_id: &String) -> bool {
        self.resolution.globals.contains(node_id)
    }

    fn begin_new_text_section(&mut self) {
        self.text_section_stack.push(Default::default());
    }

    fn get_current_text_section(&mut self) -> &mut Section {
        self.text_section_stack.last_mut().unwrap()
    }

    fn end_and_register_current_text_section(&mut self) {
        self.registered_text_sections
            .push(self.text_section_stack.pop().unwrap())
    }

    fn get_current_frame(&mut self) -> &mut Frame {
        self.static_frames.last_mut().unwrap()
    }

    fn get_frame_pointer_offset_for_node_id(&self, resolved_node_id: &String) -> i32 {
        let mut sum_of_frame_sizes = 0;

        for (idx, frame) in self.static_frames.iter().rev().enumerate() {
            if idx != 0 {
                sum_of_frame_sizes += 16 + frame.capacity_in_bytes;
            }

            if let Some(local_offset) = frame.offset_map.get(resolved_node_id) {
                return TryInto::<i32>::try_into(sum_of_frame_sizes).unwrap() + local_offset;
            }
        }

        panic!("CODEGEN:: No frame contains node with id {resolved_node_id}");
    }

    fn push_frame_and_emit_assembly(&mut self, frame: Frame) {
        let frame_capacity = frame.capacity_in_bytes;
        self.static_frames.push(frame);
        self.emit_line("stp x29, x30, [sp, #-16]!");
        self.emit_line("mov x29, sp");

        if frame_capacity > 0 {
            self.emit_line(format!("sub sp, sp, #{frame_capacity}").as_str());
        }
        self.emit_newline();
    }

    fn pop_frame_and_emit_release_assembly(&mut self) {
        self.pop_frame();
        self.emit_frame_release_assembly();
    }

    fn pop_frame(&mut self) {
        self.static_frames.pop();
    }

    fn emit_frame_release_assembly(&mut self) {
        self.emit_newline();
        self.emit_line("mov sp, x29");
        self.emit_line("ldp x29, x30, [sp], #16");
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

    fn push_from_x0(&mut self) {
        self.emit_line("str x0, [sp, #-16]!");
    }

    fn pop_to_x0(&mut self) {
        self.pop_to_xn(0);
    }

    // Pops from stack to register xn
    fn pop_to_xn(&mut self, n: usize) {
        self.emit_line(&format!("ldr x{n}, [sp], #16"));
    }

    fn pop_and_ignore_void(&mut self) {
        self.emit_line("add sp, sp, #16");
    }

    pub fn generate(&mut self, program: &Program) -> Result<(), Diagnostic> {
        self.begin_new_text_section();
        self.emit_preamble();

        // DEBUG
        self.emit_builtin_debug_function();
        // END DEBUG

        self.emit_outer_level_line(".p2align 2");
        self.emit_outer_level_line("_main:");

        self.push_frame_and_emit_assembly(Frame::new(frame_size_for_block_in_bytes(
            &program.block,
        )));

        for statement in &program.block.statements {
            self.emit_statement_assembly(statement)?;
        }

        // Defense check
        let Some(entry_node) = &self.resolution.entry_point_node_id else {
            panic!("CODEGEN:: No entry node found");
        };

        self.emit_line(&format!("bl {entry_node}"));

        // return 0;
        // later on we should change this once we get to exit codes
        self.emit_line("mov x0, #0");
        self.pop_frame_and_emit_release_assembly();
        self.emit_line("ret");
        self.end_and_register_current_text_section();

        self.begin_new_text_section();
        self.emit_globals();
        self.end_and_register_current_text_section();
        self.write_assembly_file()
    }

    fn emit_statement_assembly(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match &statement.kind {
            StatementKind::Declaration { identifier, value } => {
                // In case of global variable, look ahead and store the global value
                if self.is_global(&identifier.node_id) {
                    match &value.kind {
                        ExpressionKind::Integer(i) => {
                            self.global_map.insert(identifier.node_id.clone(), *i);
                        }
                        _ => {
                            panic!("CODEGEN:: Global value cannot be anything other than integer");
                        }
                    }
                    return Ok(());
                }

                let next_offset = self.get_current_frame().get_next_offset();

                self.get_current_frame()
                    .add_node_id_and_offset(identifier.node_id.clone(), next_offset);

                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                self.pop_to_x0();
                self.emit_line(&format!("str x0, [x29, #{next_offset}]"));
                self.emit_newline();
                Ok(())
            }
            StatementKind::Return(return_expression) => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(return_expression)?;
                self.pop_to_x0();
                self.emit_frame_release_assembly();
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
                self.emit_outer_level_line(
                    &format!("// dhanda {}", definition.identifier.node_id,),
                );
                self.emit_outer_level_line(".p2align 2");
                self.emit_outer_level_line(&format!("{}:", definition.identifier.node_id));

                // Save the stack pointer so later it can be used to retrieve arguments
                // Callee should use x9 to retrieve stack based arguments
                if definition.parameters.len() > 8 {
                    self.emit_line("mov x9, sp");
                }

                // TODO: Currently, we assume that each parameter occupies one byte
                // This assumption won't hold valid when we add structs
                self.push_frame_and_emit_assembly(Frame::new(
                    frame_size_for_block_in_bytes(&definition.body)
                        + translate_memory_units_to_bytes(definition.parameters.len()),
                ));

                for (idx, parameter) in definition.parameters.iter().enumerate() {
                    let next_offset = self.get_current_frame().get_next_offset();
                    self.get_current_frame()
                        .add_node_id_and_offset(parameter.node_id.clone(), next_offset);

                    // The first 8 arguments live on x0..x7, the rest live on the stack in increasing order
                    if idx < 8 {
                        let register_name = format!("x{}", idx);
                        self.emit_line(&format!("str {register_name}, [x29, #{next_offset}]"));
                    } else {
                        let offset_from_old_sp = (idx - 8) * 16;
                        self.emit_line(&format!("ldr x10, [x9, #{offset_from_old_sp}]"));
                        self.emit_line(&format!("str x10, [x29, #{next_offset}]"));
                    }
                }

                let mut has_return = false;
                for statement in definition.body.statements.iter() {
                    self.emit_statement_assembly(statement)?;
                    if matches!(statement.kind, StatementKind::Return(..)) {
                        has_return = true;
                    }
                }

                // Return void if a function doesn't contain a return statement
                if !has_return {
                    self.emit_line("mov x0, #0");
                    self.pop_frame_and_emit_release_assembly();
                    self.emit_line("ret");
                }

                self.pop_frame();
                self.end_and_register_current_text_section();
                Ok(())
            }
            StatementKind::If(conditional) => self.emit_conditional_assembly(conditional),
            StatementKind::While {
                condition,
                body,
                node_id,
            } => {
                // `L` marks a label the assembler resolves and then discards,
                // instead of exporting it into the object file's symbol table.
                let while_start_label = &format!("L{}_start", node_id);
                let while_end_label = &format!("L{}_end", node_id);

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
                node_id,
            } => {
                if let Some(initialiser) = initialiser {
                    self.emit_statement_assembly(initialiser)?;
                }
                let while_condition_label = format!("L{}_start", node_id);
                let while_end_label = format!("L{}_end", node_id);

                self.emit_outer_level_line(&format!("{while_condition_label}:"));

                self.emit_expression_evaluation_assembly_and_push_evaluated(condition)?;
                self.pop_to_x0();
                self.emit_line(&format!("cbz x0, {while_end_label}"));

                self.emit_block_assembly(body)?;

                self.emit_expression_evaluation_assembly_and_push_evaluated(update)?;
                self.pop_and_ignore_void();

                self.emit_line(&format!("b {while_condition_label}"));
                self.emit_outer_level_line(&format!("{while_end_label}:"));
                Ok(())
            }
        }
    }

    fn emit_conditional_assembly(&mut self, conditional: &Conditional) -> Result<(), Diagnostic> {
        let else_label = format!("L{}_else", conditional.node_id);
        let end_label = format!("L{}_end", conditional.node_id);

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

    fn emit_block_assembly(&mut self, block: &Block) -> Result<(), Diagnostic> {
        for statement in &block.statements {
            self.emit_statement_assembly(statement)?;
        }
        Ok(())
    }

    // Generate a sequence of statements to evaluate an expression and put the value on the top of the stack
    fn emit_expression_evaluation_assembly_and_push_evaluated(
        &mut self,
        expression: &Expression,
    ) -> Result<(), Diagnostic> {
        match &expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let resolved_node_id = &self.resolution.resolve(&identifier.node_id);

                if self.is_global(resolved_node_id) {
                    self.emit_line(&format!("adrp x0, L{resolved_node_id}@PAGE"));
                    self.emit_line(&format!("ldr x0, [x0, L{resolved_node_id}@PAGEOFF]"));
                    self.push_from_x0();
                    return Ok(());
                }

                let offset = self.get_frame_pointer_offset_for_node_id(resolved_node_id);
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

                match operator {
                    BinaryOperator::Add => self.emit_line("add x0, x1, x0"),
                    BinaryOperator::Subtract => self.emit_line("sub x0, x1, x0"),
                    BinaryOperator::Multiply => self.emit_line("mul x0, x1, x0"),
                    BinaryOperator::Divide => self.emit_line("sdiv x0, x1, x0"),
                    BinaryOperator::Remainder => {
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
                        self.emit_line("cmp x0, #0");
                        self.emit_line("cset x0, ne");
                        self.emit_line("cmp x1, #0");
                        self.emit_line("cset x1, ne");
                        self.emit_line("and x0, x0, x1");
                    }
                    BinaryOperator::BooleanOr => {
                        self.emit_line("cmp x0, #0");
                        self.emit_line("cset x0, ne");
                        self.emit_line("cmp x1, #0");
                        self.emit_line("cset x1, ne");
                        self.emit_line("orr x0, x0, x1");
                    }
                }

                // Push back onto the stack
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::Assignment { target, value } => {
                self.emit_expression_evaluation_assembly_and_push_evaluated(value)?;
                self.pop_to_xn(1);

                let AssignmentTarget::Variable(identifier) = target;
                let resolved_node_id = self.resolution.resolve(&identifier.node_id);

                // Resolve from global data block
                if self.is_global(&resolved_node_id) {
                    self.emit_line(&format!("adrp x0, L{resolved_node_id}@PAGE"));
                    self.emit_line(&format!("str x1, [x0, L{resolved_node_id}@PAGEOFF]"));
                    self.emit_line("mov x0, x1");
                    self.push_from_x0();
                    return Ok(());
                }

                let offset = self.get_frame_pointer_offset_for_node_id(&resolved_node_id);
                self.emit_line(&format!("str x0, [x29, #{offset}]"));
                self.push_from_x0();
                Ok(())
            }
            ExpressionKind::Call { callee, arguments } => {
                // Stack arguments
                for argument in arguments.iter().skip(8).rev() {
                    self.emit_expression_evaluation_assembly_and_push_evaluated(argument)?;
                    self.pop_to_x0();
                    self.push_from_x0();
                }

                let register_arguments: Vec<&Expression> = arguments.iter().take(8).collect();

                for argument in register_arguments.iter().rev() {
                    self.emit_expression_evaluation_assembly_and_push_evaluated(argument)?;
                }

                for idx in 0..register_arguments.len() {
                    let register = format!("x{idx}");
                    self.emit_line(&format!("ldr {register}, [sp], #16"));
                }

                let node_id = self.resolution.resolve(&callee.node_id);
                self.emit_line(&format!("bl {node_id}",));

                // Pop stack arguments
                if arguments.len() > 8 {
                    for _ in 0..(arguments.len() - 8) {
                        self.pop_and_ignore_void();
                    }
                }

                // Push the value onto the stack
                self.push_from_x0();
                return Ok(());
            }
        }
    }

    fn emit_preamble(&mut self) {
        self.emit_outer_level_line("// PREAMBLE");
        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_outer_level_line(".globl _main");
        self.emit_newline();
    }

    fn emit_builtin_debug_function(&mut self) {
        self.emit_outer_level_line("// BUILTIN DEBUGGER");

        self.emit_outer_level_line(".p2align 2");
        self.emit_outer_level_line("_debug_bol:");

        // Could use a frame here, but not necessary for a builtin
        self.push_frame_and_emit_assembly(Frame::new(16));

        // because printf expects arguments on the stack
        self.emit_line("str x0, [sp]");
        self.emit_line("adrp x0, Lfmt_integer@PAGE");
        self.emit_line("add x0, x0, Lfmt_integer@PAGEOFF");
        self.emit_line("bl _printf");

        self.pop_frame_and_emit_release_assembly();

        self.emit_line("ret");
        self.emit_newline();

        self.emit_outer_level_line(".section __TEXT,__cstring,cstring_literals");
        self.emit_outer_level_line("Lfmt_integer:");
        self.emit_line(".asciz \"%ld\\n\"");
        self.emit_newline();

        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_newline();
    }

    fn emit_globals(&mut self) {
        if self.global_map.is_empty() {
            return;
        }

        let globals: Vec<(String, i64)> = self
            .resolution
            .globals
            .iter()
            .map(|node_id| {
                let value = *self
                    .global_map
                    .get(node_id)
                    .expect("Every global must have been given a value during codegen");
                (node_id.clone(), value)
            })
            .collect();

        self.emit_outer_level_line("// GLOBALS");
        self.emit_outer_level_line(".section __DATA,__data");

        for (node_id, value) in globals {
            self.emit_outer_level_line(".p2align 3");
            self.emit_outer_level_line(&format!("L{node_id}:"));
            self.emit_line(&format!(".quad {value}"));
        }

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

fn frame_size_for_block_in_bytes(block: &Block) -> usize {
    return translate_memory_units_to_bytes(memory_units_occupied_by_block(block));
}

fn translate_memory_units_to_bytes(m: MemoryUnit) -> usize {
    return (m * MEMORY_UNIT_SCALE).div_ceil(STACK_ALIGNMENT) * STACK_ALIGNMENT;
}

// memory_units_occupied_by will give the total size of the given block
// in memory units.
// It'll walk the tree recursively, ignoring function definitions, and
fn memory_units_occupied_by_block(block: &Block) -> MemoryUnit {
    let mut total = 0;
    for stmt in &block.statements {
        total += memory_units_occupied_by_statement(stmt);
    }
    total
}

fn memory_units_occupied_by_statement(statement: &Statement) -> MemoryUnit {
    let mut total: usize = 0;
    match &statement.kind {
        StatementKind::Declaration { .. } => {
            total += 1;
        }
        StatementKind::If(conditional) => {
            total += memory_units_occupied_by_conditional(conditional);
        }
        StatementKind::While { body, .. } => total += memory_units_occupied_by_block(body),
        StatementKind::For {
            initialiser, body, ..
        } => {
            if let Some(initialiser) = initialiser {
                total += memory_units_occupied_by_statement(initialiser.deref());
            }
            total += memory_units_occupied_by_block(body);
        }
        _ => {}
    }
    total
}

fn memory_units_occupied_by_conditional(conditional: &Conditional) -> MemoryUnit {
    let mut total: MemoryUnit = 0;
    total += memory_units_occupied_by_block(&conditional.then);
    if let Some(otherwise) = &conditional.otherwise {
        match otherwise.deref() {
            Else::Block(block) => total += memory_units_occupied_by_block(block),
            Else::If(conditional) => total += memory_units_occupied_by_conditional(conditional),
        }
    }
    total
}
