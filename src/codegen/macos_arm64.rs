use crate::{
    ir::instructions::{
        BinOpSpec, BinaryOperation, CallSpec, FnDefnSpec, GoifSpec, IRFunctionBlock, IRProgram,
        Instruction, LoadFrameAddrSpec, LoadImmSpec, LoadToRegSpec, MovSpec, ReturnSpec,
        StrFromRegSpec, UnaryOp, UnaryOpSpec,
    },
    syntax::{
        parser::{__DEBUG_BOL_BOOL_FUNCTION_NODE_ID, __DEBUG_BOL_FUNCTION_NODE_ID},
        sema::MemoryUnits,
    },
    to_i64,
};

const MEMORY_UNIT_SCALE: usize = 8;
const STACK_ALIGNMENT: usize = 16;
// Saved x29 and x30, pushed by every prologue
const FRAME_RECORD_BYTES: usize = 16;
const ARGUMENT_REGISTER_COUNT: usize = 8;
// Each stack argument takes a full 16 bytes so sp stays aligned
// I did it so that we wouldn't have to think too much about alignment here
// Might incure a small memory overhead but acceptable for now
const STACK_ARGUMENT_BYTES: usize = 16;

#[derive(Default)]
struct Section {
    lines: Vec<String>,
}

pub struct CodeGenerator {
    output_file: String,

    // For generating different text sections during codegen
    text_section_stack: Vec<Section>,
    registered_text_sections: Vec<Section>,
}

impl CodeGenerator {
    pub fn new(output_file: &str) -> Self {
        Self {
            output_file: output_file.to_string(),
            text_section_stack: vec![Default::default()],
            registered_text_sections: Vec::new(),
        }
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

    fn emit_assembly_for_reserving_frame(&mut self, logical_frame_size: MemoryUnits) {
        self.emit_line("stp x29, x30, [sp, #-16]!");
        self.emit_line("mov x29, sp");

        if logical_frame_size > 0 {
            let physical_frame_size =
                calculate_physical_aligned_frame_size_in_bytes(logical_frame_size);
            self.emit_load_immediate_into_xn(to_i64(physical_frame_size), 10);
            self.emit_line("sub sp, sp, x10");
        }
        self.emit_newline();
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

    fn emit_load_immediate_into_xn(&mut self, value: i64, n: usize) {
        self.emit_line(&format!("// Loading {} onto x{}", value, n));
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
            self.emit_line(&format!("{instruction} x{n}, #{halfword}, lsl #{shift}"));
            placed_first_halfword = true;
        }

        if !placed_first_halfword {
            self.emit_line(&format!("movz x{n}, #0"));
        }
    }

    fn emit_preamble(&mut self) {
        self.emit_outer_level_line("// PREAMBLE");
        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
        self.emit_outer_level_line(".globl _main");
        self.emit_newline();
    }

    #[allow(non_snake_case)]
    fn emit___debug(&mut self, node_id: &str) {
        self.emit_outer_level_line("// BUILTIN DEBUGGER");

        self.emit_outer_level_line(".p2align 2");
        self.emit_outer_level_line(&format!("{node_id}:"));

        // Could use a frame here, but not necessary for a builtin
        self.emit_assembly_for_reserving_frame(1);

        // because printf expects arguments on the stack
        self.emit_line("str x0, [sp]");
        self.emit_line(&format!("adrp x0, Lfmt_integer_{node_id}@PAGE"));
        self.emit_line(&format!("add x0, x0, Lfmt_integer_{node_id}@PAGEOFF"));
        self.emit_line("bl _printf");

        self.emit_frame_release_assembly();

        self.emit_line("ret");
        self.emit_newline();

        self.emit_outer_level_line(".section __TEXT,__cstring,cstring_literals");
        self.emit_outer_level_line(&format!("Lfmt_integer_{node_id}:"));
        self.emit_line(".asciz \"%ld\\n\"");
        self.emit_newline();

        self.emit_outer_level_line(".section __TEXT,__text,regular,pure_instructions");
    }

    pub fn generate(&mut self, ir_program: IRProgram) -> Result<(), std::io::Error> {
        self.begin_new_text_section();
        self.emit_preamble();

        for (fn_node_id, fn_instructions) in ir_program {
            if fn_node_id == __DEBUG_BOL_FUNCTION_NODE_ID
                || fn_node_id == __DEBUG_BOL_BOOL_FUNCTION_NODE_ID
            {
                self.emit___debug(&fn_node_id);
                continue;
            }
            self.generate_fn(fn_node_id, fn_instructions);
        }

        self.end_and_register_current_text_section();
        self.write_assembly_file()
    }

    fn generate_fn(&mut self, fn_node_id: String, ir_fn: IRFunctionBlock) {
        self.begin_new_text_section();
        let max_register_no = ir_fn.max_register_no();
        let argc = ir_fn.argc_sizes().iter().sum::<MemoryUnits>();
        let frame_size = ir_fn.frame_size();
        self.emit_outer_level_line(&format!(
            "// {fn_node_id}: max_register_no={max_register_no}, args={argc}, frame_size={frame_size}",
        ));
        self.emit_outer_level_line(".p2align 2");

        for instruction in ir_fn.get_instructions() {
            match instruction {
                Instruction::FunctionDefinition(FnDefnSpec { node_id, .. }) => {
                    self.emit_outer_level_line(&format!("{}:", node_id));
                    self.emit_assembly_for_reserving_frame(max_register_no);

                    // Copy x9 (static link) onto its slot
                    let static_link_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(0);
                    self.emit_load_immediate_into_xn(static_link_offset, 10);
                    self.emit_line("str x9, [x29, x10]");

                    // Copy x0..x7 onto their slots
                    for i in 0..argc {
                        let offset_for_arg_reg =
                            translate_logical_offset_to_physical_offset_for_ir_register(1 + i);
                        if i < ARGUMENT_REGISTER_COUNT {
                            self.emit_load_immediate_into_xn(offset_for_arg_reg, 10);
                            self.emit_line(&format!("str x{i}, [x29, x10]"));
                            continue;
                        }

                        // The remaining arguments were pushed by the caller, just above our frame record
                        let argument_offset_from_old_sp = to_i64(
                            FRAME_RECORD_BYTES
                                + (i - ARGUMENT_REGISTER_COUNT) * STACK_ARGUMENT_BYTES,
                        );
                        self.emit_load_immediate_into_xn(argument_offset_from_old_sp, 11);
                        self.emit_line("ldr x11, [x29, x11]");
                        self.emit_load_immediate_into_xn(offset_for_arg_reg, 10);
                        self.emit_line("str x11, [x29, x10]");
                    }
                }
                Instruction::LoadImmediate(LoadImmSpec { dst_reg, imm }) => {
                    self.emit_load_immediate_into_xn(*imm, 0);
                    let reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::Move(MovSpec { dst_reg, src_reg }) => {
                    let src_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*src_reg);
                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(src_reg_offset, 10);
                    self.emit_line("ldr x0, [x29, x10]");
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::BinaryOp(BinOpSpec {
                    operation,
                    left_reg,
                    right_reg,
                    dst_reg,
                }) => {
                    let left_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*left_reg);
                    let right_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*right_reg);
                    self.emit_load_immediate_into_xn(left_reg_offset, 10);
                    self.emit_line("ldr x1, [x29, x10]");
                    self.emit_load_immediate_into_xn(right_reg_offset, 10);
                    self.emit_line("ldr x2, [x29, x10]");
                    match operation {
                        BinaryOperation::Add => {
                            self.emit_line("add x0, x1, x2");
                        }
                        BinaryOperation::Subtract => {
                            self.emit_line("sub x0, x1, x2");
                        }
                        BinaryOperation::Multiply => {
                            self.emit_line("mul x0, x1, x2");
                        }
                        BinaryOperation::Divide => {
                            self.emit_line("sdiv x0, x1, x2");
                        }
                        BinaryOperation::Remainder => {
                            self.emit_line("udiv x0, x1, x2");
                            self.emit_line("msub x0, x0, x2, x1");
                        }
                        BinaryOperation::Equal => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, eq");
                        }
                        BinaryOperation::NotEqual => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, ne");
                        }
                        BinaryOperation::Less => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, lt");
                        }
                        BinaryOperation::LessOrEqual => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, le");
                        }
                        BinaryOperation::Greater => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, gt");
                        }
                        BinaryOperation::GreaterOrEqual => {
                            self.emit_line("cmp x1, x2");
                            self.emit_line("cset x0, ge");
                        }
                        BinaryOperation::BooleanOr | BinaryOperation::BooleanAnd => {
                            panic!(
                                "CODEGEN:: BooleanOr & BooleanAnd must have been handled by lowering"
                            )
                        }
                    }
                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::UnaryOp(UnaryOpSpec {
                    operand_reg,
                    operation,
                    dst_reg,
                }) => {
                    let operand_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*operand_reg);
                    self.emit_load_immediate_into_xn(operand_reg_offset, 10);
                    self.emit_line("ldr x1, [x29, x10]");
                    match operation {
                        UnaryOp::Negate => {
                            self.emit_line("neg x0, x1");
                        }
                        UnaryOp::Not => {
                            self.emit_line("cmp x1, #0");
                            self.emit_line("cset x0, eq");
                        }
                    }
                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::LoadToRegister(LoadToRegSpec {
                    dst_reg,
                    src_base_reg,
                    offset,
                }) => {
                    let src_base_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*src_base_reg);
                    self.emit_load_immediate_into_xn(src_base_reg_offset, 10);
                    self.emit_line("ldr x1, [x29, x10]");

                    let var_offset =
                        translate_logical_offset_to_physical_offset_for_local_variable(*offset);
                    self.emit_load_immediate_into_xn(var_offset, 10);
                    self.emit_line("ldr x0, [x1, x10]");

                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::StoreFromRegister(StrFromRegSpec {
                    src_reg,
                    dst_base_reg,
                    offset,
                }) => {
                    let src_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*src_reg);
                    self.emit_load_immediate_into_xn(src_reg_offset, 10);
                    self.emit_line("ldr x0, [x29, x10]");

                    let dst_base_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_base_reg);
                    self.emit_load_immediate_into_xn(dst_base_reg_offset, 10);
                    self.emit_line("ldr x1, [x29, x10]");

                    let var_offset =
                        translate_logical_offset_to_physical_offset_for_local_variable(*offset);

                    self.emit_load_immediate_into_xn(var_offset, 10);
                    self.emit_line("str x0, [x1, x10]");
                }
                Instruction::LoadFrameAddress(LoadFrameAddrSpec { dst_reg }) => {
                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x29, [x29, x10]");
                }
                Instruction::Label(label) => {
                    self.emit_outer_level_line(&format!("{label}:"));
                }
                Instruction::Goto(label) => {
                    self.emit_line(&format!("b {label}"));
                }
                Instruction::Goif(GoifSpec {
                    condition_reg,
                    true_label,
                    false_label,
                }) => {
                    let condition_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*condition_reg);
                    self.emit_load_immediate_into_xn(condition_reg_offset, 10);
                    self.emit_line("ldr x0, [x29, x10]");
                    self.emit_line(&format!("cbz x0, {}", false_label));
                    self.emit_line(&format!("b {}", true_label));
                }
                Instruction::Call(CallSpec {
                    function_node_id,
                    static_link_reg,
                    arg_regs,
                    dst_reg,
                }) => {
                    let static_link_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(
                            *static_link_reg,
                        );
                    self.emit_load_immediate_into_xn(static_link_reg_offset, 10);
                    self.emit_line("ldr x9, [x29, x10]");

                    // The first arguments go in x0..x7
                    for (i, arg_reg) in arg_regs.iter().take(ARGUMENT_REGISTER_COUNT).enumerate() {
                        let arg_reg_offset =
                            translate_logical_offset_to_physical_offset_for_ir_register(*arg_reg);
                        self.emit_load_immediate_into_xn(arg_reg_offset, 10);
                        self.emit_line(&format!("ldr x{i}, [x29, x10]"));
                    }

                    // The rest are pushed last first
                    for arg_reg in arg_regs.iter().skip(ARGUMENT_REGISTER_COUNT).rev() {
                        let arg_reg_offset =
                            translate_logical_offset_to_physical_offset_for_ir_register(*arg_reg);
                        self.emit_load_immediate_into_xn(arg_reg_offset, 10);
                        self.emit_line("ldr x10, [x29, x10]");
                        self.emit_line(&format!("str x10, [sp, #-{STACK_ARGUMENT_BYTES}]!"));
                    }
                    self.emit_line(&format!("bl {}", function_node_id));

                    let dst_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*dst_reg);
                    self.emit_load_immediate_into_xn(dst_reg_offset, 10);
                    self.emit_line("str x0, [x29, x10]");
                }
                Instruction::Return(ReturnSpec { src_reg }) => {
                    let src_reg_offset =
                        translate_logical_offset_to_physical_offset_for_ir_register(*src_reg);
                    self.emit_load_immediate_into_xn(src_reg_offset, 10);
                    self.emit_line("ldr x0, [x29, x10]");
                    self.emit_frame_release_assembly();
                    self.emit_line("ret");
                }
            }
        }
        self.end_and_register_current_text_section();
    }

    fn write_assembly_file(&self) -> Result<(), std::io::Error> {
        let path = format!("{}.s", self.output_file);

        let contents = self
            .registered_text_sections
            .iter()
            // sections that end early go later
            .rev()
            .map(|section| section.lines.concat())
            .collect::<Vec<String>>()
            .join("\n\n");

        std::fs::write(&path, contents)
    }
}

fn calculate_physical_aligned_frame_size_in_bytes(size: MemoryUnits) -> usize {
    (size * MEMORY_UNIT_SCALE).div_ceil(STACK_ALIGNMENT) * STACK_ALIGNMENT
}

fn translate_logical_offset_to_physical_offset_for_ir_register(reg_no: usize) -> i64 {
    // Register i is the ith local variable
    translate_logical_offset_to_physical_offset_for_local_variable(reg_no)
}

// Logical slot k sits at x29 - (k + 1) * 8, because x29 itself points at the
// saved x29/x30 pair, so slot 0 (the static link) is at x29 - 8
fn translate_logical_offset_to_physical_offset_for_local_variable(logical_offset: usize) -> i64 {
    -i64::try_from(MEMORY_UNIT_SCALE * (logical_offset + 1))
        .expect("Failed to convert slot to offset")
}
