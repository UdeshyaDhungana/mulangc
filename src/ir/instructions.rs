use std::{collections::HashMap, fmt};

use crate::syntax::sema::MemoryUnits;

pub type RegisterNumber = usize;

#[derive(Debug, Clone)]
pub struct FunctionDefinitionSpec {
    pub node_id: String,
    pub frame_size: MemoryUnits,
    pub argc: usize,
}

#[derive(Debug, Clone)]
pub struct LoadImmediateSpec {
    pub dst_reg: RegisterNumber,
    pub immediate: i64,
}

#[derive(Debug, Clone)]
pub struct MoveSpec {
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
pub struct BinaryOpSpec {
    pub dst_reg: RegisterNumber,
    pub operation: BinaryOperation,
    pub left_reg: RegisterNumber,
    pub right_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub enum UnaryOperation {
    Negate,
    Not,
}

#[derive(Debug, Clone)]
pub struct UnaryOpSpec {
    pub dst_reg: RegisterNumber,
    pub operation: UnaryOperation,
    pub operand_reg: RegisterNumber,
}

#[derive(Debug, Clone)]
pub struct LoadToRegisterSpec {
    pub dst_reg: RegisterNumber,
    pub src_base_reg: RegisterNumber,
    // Number of 'memory unit' offsets
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct StoreFromRegisterSpec {
    pub src_reg: RegisterNumber,
    pub dst_base_reg: RegisterNumber,
    pub offset: i64,
}

#[derive(Debug, Clone)]
pub struct LoadFrameAddressSpec {
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
    pub function_node_id: String,
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
    FunctionDefinition(FunctionDefinitionSpec),
    LoadImmediate(LoadImmediateSpec),
    Move(MoveSpec),
    BinaryOp(BinaryOpSpec),
    UnaryOp(UnaryOpSpec),
    LoadToRegister(LoadToRegisterSpec),
    StoreFromRegister(StoreFromRegisterSpec),
    LoadFrameAddress(LoadFrameAddressSpec),
    Label(String),
    Goto(String),
    Goif(GoifSpec),
    Call(CallSpec),
    Return(ReturnSpec),
}

pub struct IRFunction {
    instructions: Vec<Instruction>,
    ended: bool,
}

impl IRFunction {
    pub fn new(node_id: &str, frame_size: MemoryUnits, argc: usize) -> Self {
        IRFunction {
            instructions: vec![Instruction::FunctionDefinition(FunctionDefinitionSpec {
                node_id: String::from(node_id),
                frame_size: frame_size,
                argc: argc,
            })],
            ended: false,
        }
    }

    pub fn node_id(&self) -> String {
        if let Instruction::FunctionDefinition(definition) = &self.instructions[0] {
            return definition.node_id.clone();
        }
        panic!("First instruction of a function must be its definition")
    }

    pub fn push_instruction(&mut self, i: Instruction) {
        if matches!(i, Instruction::FunctionDefinition(..)) {
            panic!("Instruction::FunctionDefinition(..) cannot be pushed")
        }

        if self.ended {
            panic!("Cannot push instruction to an ended function")
        }

        self.instructions.push(i);
    }

    pub fn end(&mut self) {
        if self.instructions.len() == 0 {
            panic!("Cannot end function before it's begun")
        }
        self.ended = true;
    }

    pub fn get_instructions(&self) -> &Vec<Instruction> {
        if !self.ended {
            panic!("Cannot get instruction from a function which has not ended")
        }
        &self.instructions
    }
}

pub type IRProgram = HashMap<String, IRFunction>;

// Anything below this is slop generated
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

impl fmt::Display for UnaryOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            UnaryOperation::Negate => "-",
            UnaryOperation::Not => "!",
        })
    }
}

// Offsets of 0 are the default, so they are left out
fn format_offset(offset: i64) -> String {
    if offset == 0 {
        String::new()
    } else {
        format!(", {offset}")
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::FunctionDefinition(FunctionDefinitionSpec {
                node_id,
                frame_size,
                argc,
            }) => write!(formatter, "fn {node_id}, frame_size={frame_size}, {argc}"),
            Instruction::LoadImmediate(LoadImmediateSpec {
                dst_reg: register_num,
                immediate,
            }) => write!(formatter, "ldi r{register_num}, {immediate}"),
            Instruction::Move(MoveSpec {
                dst_reg: dest_reg,
                src_reg,
            }) => {
                write!(formatter, "mov r{dest_reg}, r{src_reg}")
            }
            Instruction::BinaryOp(BinaryOpSpec {
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
            Instruction::LoadToRegister(LoadToRegisterSpec {
                dst_reg,
                src_base_reg: base_reg,
                offset,
            }) => write!(
                formatter,
                "ldr r{dst_reg}, r{base_reg}{}",
                format_offset(*offset)
            ),
            Instruction::StoreFromRegister(StoreFromRegisterSpec {
                src_reg,
                dst_base_reg: base_reg,
                offset,
            }) => write!(
                formatter,
                "str r{src_reg}, r{base_reg}{}",
                format_offset(*offset)
            ),
            Instruction::LoadFrameAddress(LoadFrameAddressSpec { dst_reg }) => {
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

    for (_, ir_function) in program {
        for instruction in &ir_function.instructions {
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
