use std::{collections::HashMap, fmt};

use crate::syntax::{ast::NodeId, sema::MemoryUnits};

pub type RegisterNumber = usize;

#[derive(Debug, Clone)]
pub struct FnDefnSpec {
    pub node_id: NodeId,
    pub frame_size: MemoryUnits,
    pub argc_sizes: Vec<MemoryUnits>,
}

#[derive(Debug, Clone)]
pub struct LoadImmSpec {
    pub dst_reg: RegisterNumber,
    pub imm: i64,
}

#[derive(Debug, Clone)]
pub struct MovSpec {
    pub dst_reg: RegisterNumber,
    pub src_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub enum BinaryOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    BooleanOr,
    BooleanAnd,
}

#[derive(Debug, Clone)]
pub struct BinOpSpec {
    pub dst_reg: RegisterNumber,
    pub operation: BinaryOperation,
    pub left_reg: RegisterNumber,
    pub right_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Debug, Clone)]
pub struct UnaryOpSpec {
    pub dst_reg: RegisterNumber,
    pub operation: UnaryOp,
    pub operand_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub struct LoadToRegSpec {
    pub dst_reg: RegisterNumber,
    pub src_base_reg: RegisterNumber,
    // Number of 'memory unit' offsets from src_base_reg
    pub offset: MemoryUnits,
}

#[derive(Debug, Clone)]
pub struct StrFromRegSpec {
    pub src_reg: RegisterNumber,
    pub dst_base_reg: RegisterNumber,
    pub offset: MemoryUnits,
}

#[derive(Debug, Clone)]
pub struct LoadFrameAddrSpec {
    pub dst_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub struct GoifSpec {
    pub condition_reg: RegisterNumber,
    pub true_label: String,
    pub false_label: String,
}

#[derive(Debug, Clone)]
pub struct CallSpec {
    pub function_node_id: NodeId,
    pub static_link_reg: RegisterNumber,
    pub arg_regs: Vec<RegisterNumber>,
    pub dst_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub struct ReturnSpec {
    pub src_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    FunctionDefinition(FnDefnSpec),
    LoadImmediate(LoadImmSpec),
    Move(MovSpec),
    BinaryOp(BinOpSpec),
    UnaryOp(UnaryOpSpec),
    LoadToRegister(LoadToRegSpec),
    StoreFromRegister(StrFromRegSpec),
    LoadFrameAddress(LoadFrameAddrSpec),
    Label(String),
    Goto(String),
    Goif(GoifSpec),
    Call(CallSpec),
    Return(ReturnSpec),
}

pub struct IRFunctionBlock {
    // Initialization
    id: String,
    frame_size: MemoryUnits,
    argc_sizes: Vec<MemoryUnits>,

    // Population
    next_reg_num: RegisterNumber,
    instructions: Vec<Instruction>,

    // State
    has_lowering_ended: bool,
}

impl IRFunctionBlock {
    pub fn new(
        defn_node_id: &NodeId,
        frame_size: MemoryUnits,
        argc_sizes: &[MemoryUnits],
    ) -> Self {
        // 1 memory slot is reserved for the static link
        IRFunctionBlock {
            id: defn_node_id.clone(),
            frame_size,
            argc_sizes: argc_sizes.to_vec(),

            // Reserve first register for static link
            next_reg_num: 1 + argc_sizes.iter().sum::<MemoryUnits>(),
            instructions: Vec::new(),

            has_lowering_ended: false,
        }
    }

    pub fn get_next_register(&mut self) -> RegisterNumber {
        let next = self.next_reg_num;
        self.next_reg_num += 1;
        next
    }

    pub fn max_register_no(&self) -> RegisterNumber {
        self.next_reg_num - 1
    }

    pub fn id(&self) -> &String {
        &self.id
    }

    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    pub fn argc_sizes(&self) -> &Vec<MemoryUnits> {
        &self.argc_sizes
    }

    pub fn push_instruction(&mut self, i: Instruction) {
        if self.has_lowering_ended {
            panic!("Cannot push instruction to an ended function")
        }

        self.instructions.push(i);
    }

    pub fn end(&mut self) {
        self.has_lowering_ended = true;
    }

    pub fn get_instructions(&self) -> &Vec<Instruction> {
        if !self.has_lowering_ended {
            panic!("Cannot get instruction from a function which has not ended")
        }
        &self.instructions
    }
}

pub type IRProgram = HashMap<String, IRFunctionBlock>;

// Anything below this line is slop
impl fmt::Display for BinaryOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            BinaryOperation::Add => "+",
            BinaryOperation::Subtract => "-",
            BinaryOperation::Multiply => "*",
            BinaryOperation::Divide => "/",
            BinaryOperation::Remainder => "%",
            BinaryOperation::Equal => "==",
            BinaryOperation::NotEqual => "!=",
            BinaryOperation::Less => "<",
            BinaryOperation::LessOrEqual => "<=",
            BinaryOperation::Greater => ">",
            BinaryOperation::GreaterOrEqual => ">=",
            BinaryOperation::BooleanAnd => "&&",
            BinaryOperation::BooleanOr => "||",
        })
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            UnaryOp::Negate => "-",
            UnaryOp::Not => "!",
        })
    }
}

// Offsets of 0 are the default, so they are left out
fn format_offset(offset: usize) -> String {
    format!(", {offset}")
}

impl fmt::Display for Instruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::FunctionDefinition(FnDefnSpec {
                node_id,
                frame_size,
                argc_sizes: argc,
            }) => {
                let argc_strs: Vec<String> = argc.iter().map(|s| s.to_string()).collect();
                write!(
                    formatter,
                    "fn {node_id}, frame_size={frame_size}, {}",
                    argc_strs.join(", ")
                )
            }
            Instruction::LoadImmediate(LoadImmSpec {
                dst_reg: register_num,
                imm: immediate,
            }) => write!(formatter, "ldi r{register_num}, {immediate}"),
            Instruction::Move(MovSpec {
                dst_reg: dest_reg,
                src_reg,
            }) => {
                write!(formatter, "mov r{dest_reg}, r{src_reg}")
            }
            Instruction::BinaryOp(BinOpSpec {
                dst_reg: dest_reg,
                left_reg,
                operation,
                right_reg,
            }) => write!(
                formatter,
                "r{dest_reg} <- r{left_reg} {operation} r{right_reg}"
            ),
            Instruction::UnaryOp(UnaryOpSpec {
                dst_reg: dest_reg,
                operation,
                operand_reg,
            }) => write!(formatter, "r{dest_reg} <- {operation}r{operand_reg}"),
            Instruction::LoadToRegister(LoadToRegSpec {
                dst_reg,
                src_base_reg: base_reg,
                offset,
            }) => write!(
                formatter,
                "ldr r{dst_reg}, r{base_reg}{}",
                format_offset(*offset)
            ),
            Instruction::StoreFromRegister(StrFromRegSpec {
                src_reg,
                dst_base_reg: base_reg,
                offset,
            }) => write!(
                formatter,
                "str r{src_reg}, r{base_reg}{}",
                format_offset(*offset)
            ),
            Instruction::LoadFrameAddress(LoadFrameAddrSpec { dst_reg }) => {
                write!(formatter, "ldaf r{dst_reg}")
            }
            Instruction::Label(label) => write!(formatter, "{label}:"),
            Instruction::Goto(label) => write!(formatter, "goto {label}"),
            Instruction::Goif(GoifSpec {
                condition_reg,
                true_label,
                false_label,
            }) => write!(
                formatter,
                "goif r{condition_reg}, {true_label}, {false_label}"
            ),
            Instruction::Call(CallSpec {
                function_node_id: function_name,
                static_link_reg: link_reg,
                arg_regs,
                dst_reg,
            }) => {
                write!(formatter, "r{dst_reg} <- call {function_name}, r{link_reg}")?;
                for argument_reg in arg_regs {
                    write!(formatter, ", r{argument_reg}")?;
                }
                Ok(())
            }
            Instruction::Return(ReturnSpec { src_reg }) => write!(formatter, "ret r{src_reg}"),
        }
    }
}

// Orphan rule: `IRProgram` is a `Vec`, so it cannot implement `Display` itself
pub fn format_program(program: &IRProgram) -> String {
    let mut output = String::new();

    for ir_function_block in program.values() {
        for instruction in &ir_function_block.instructions {
            let indentation = match instruction {
                Instruction::FunctionDefinition(_) | Instruction::Label(_) => "",
                _ => "  ",
            };
            output.push_str(&format!("{indentation}{instruction}\n"));
        }
        output.push('\n');
    }

    output
}
