use std::collections::HashMap;

use crate::syntax::sema::{__DEBUG_BOL_FUNCTION_NODE_ID, MAIN_FUNCTION_NODE_ID};
use crate::{to_i64, to_usize};

use crate::ir::instructions::{
    BinaryOpSpec, BinaryOperation, CallSpec, GoifSpec, IRProgram, Instruction,
    LoadFrameAddressSpec, LoadImmediateSpec, LoadToRegisterSpec, MoveSpec, RegisterNumber,
    ReturnSpec, StoreFromRegisterSpec, UnaryOpSpec, UnaryOperation,
};

struct Frame {
    regs: HashMap<RegisterNumber, i64>,
}

impl Frame {
    fn new(regs: HashMap<RegisterNumber, i64>) -> Self {
        Self { regs }
    }

    fn insert(&mut self, reg: RegisterNumber, value: i64) {
        self.regs.insert(reg, value);
    }

    fn get(&self, reg: RegisterNumber) -> i64 {
        *(self
            .regs
            .get(&reg)
            .expect(&format!("Register r{reg} doesn't exist")))
    }
}

pub struct IRInterpreter {
    ir_program: IRProgram,
    frames: Vec<Frame>,
}

impl IRInterpreter {
    pub fn new(ir_program: IRProgram) -> Self {
        Self {
            frames: Vec::new(),
            ir_program,
        }
    }

    fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    fn pop_frame(&mut self) {
        if self.frames.len() == 0 {
            panic!("Cannot pop when there are no frames")
        }
        self.frames.pop();
    }

    fn get_frame_at_address(&mut self, addr: usize) -> &mut Frame {
        self.frames
            .get_mut(addr)
            .expect(&format!("Frame at address {addr} doesn't exist"))
    }

    fn current_frame_addr(&self) -> usize {
        self.frames.len() - to_usize(1)
    }

    fn current_frame(&mut self) -> &mut Frame {
        self.get_frame_at_address(self.current_frame_addr())
    }

    pub fn run(&mut self) {
        let empty_frame: HashMap<RegisterNumber, i64> = HashMap::new();
        self.run_fn(String::from(MAIN_FUNCTION_NODE_ID), Frame::new(empty_frame));
    }

    fn run_fn(&mut self, fn_node_id: String, args: Frame) -> i64 {
        self.push_frame(args);
        let ir_fn = self.ir_program.get(&fn_node_id).unwrap();
        let instructions = ir_fn.get_instructions().clone();
        let mut labels_map: HashMap<String, usize> = HashMap::new();

        // Record labels (not musical)
        for (i, instruction) in instructions.iter().enumerate() {
            if let Instruction::Label(name) = instruction {
                labels_map.insert(name.clone(), i);
            }
        }

        let mut return_value: i64 = 0;

        let mut i: usize = 0;
        while i < instructions.len() {
            let instruction = &instructions[i];
            match instruction {
                Instruction::FunctionDefinition(_) => {}
                Instruction::LoadImmediate(LoadImmediateSpec { dst_reg, immediate }) => {
                    self.current_frame().insert(*dst_reg, *immediate);
                }
                Instruction::Move(MoveSpec { src_reg, dst_reg }) => {
                    let value = self.current_frame().get(*src_reg);
                    self.current_frame().insert(*dst_reg, value);
                }
                Instruction::BinaryOp(BinaryOpSpec {
                    dst_reg,
                    operation,
                    left_reg,
                    right_reg,
                }) => {
                    let left = self.current_frame().get(*left_reg);
                    let right = self.current_frame().get(*right_reg);
                    let result = match operation {
                        BinaryOperation::Add => left + right,
                        BinaryOperation::Subtract => left - right,
                        BinaryOperation::Multiply => left * right,
                        BinaryOperation::Divide => left / right,
                        BinaryOperation::Remainder => left % right,
                        BinaryOperation::Equal => (left == right) as i64,
                        BinaryOperation::NotEqual => (left != right) as i64,
                        BinaryOperation::Less => (left < right) as i64,
                        BinaryOperation::LessOrEqual => (left <= right) as i64,
                        BinaryOperation::Greater => (left > right) as i64,
                        BinaryOperation::GreaterOrEqual => (left >= right) as i64,
                        _ => {
                            panic!("These should have been handled in lowering")
                        }
                    };
                    self.current_frame().insert(*dst_reg, result);
                }
                Instruction::UnaryOp(UnaryOpSpec {
                    dst_reg,
                    operation,
                    operand_reg,
                }) => {
                    let operand = self.current_frame().get(*operand_reg);
                    let result = match operation {
                        UnaryOperation::Negate => -operand,
                        UnaryOperation::Not => (operand == 0) as i64,
                    };
                    self.current_frame().insert(*dst_reg, result);
                }
                Instruction::LoadToRegister(LoadToRegisterSpec {
                    dst_reg,
                    src_base_reg,
                    offset,
                }) => {
                    let frame_addr = self.current_frame().get(*src_base_reg);
                    let frame = self.get_frame_at_address(to_usize(frame_addr));
                    let value = frame.get(*offset);
                    self.current_frame().insert(*dst_reg, value);
                }
                Instruction::StoreFromRegister(StoreFromRegisterSpec {
                    src_reg,
                    dst_base_reg,
                    offset,
                }) => {
                    let value = self.current_frame().get(*src_reg);
                    let frame_addr = self.current_frame().get(*dst_base_reg);
                    let frame = self.get_frame_at_address(to_usize(frame_addr));
                    frame.insert(*offset, value);
                }
                Instruction::LoadFrameAddress(LoadFrameAddressSpec { dst_reg }) => {
                    let current_frame_addr = to_i64(self.current_frame_addr());
                    self.current_frame().insert(*dst_reg, current_frame_addr);
                }
                // Ignore labels, they'd been already processed
                Instruction::Label(_) => {}
                Instruction::Goto(label) => {
                    i = *(labels_map.get(label).unwrap());
                }
                Instruction::Goif(GoifSpec {
                    condition_reg,
                    true_label,
                    false_label,
                }) => {
                    let value = self.current_frame().get(*condition_reg);
                    if value != 0 {
                        i = *(labels_map.get(true_label).unwrap());
                    } else {
                        i = *(labels_map.get(false_label).unwrap());
                    }
                }
                Instruction::Call(CallSpec {
                    function_node_id,
                    static_link_reg,
                    arg_regs,
                    dst_reg,
                }) => {
                    // DEBUG BEGIN
                    if function_node_id == __DEBUG_BOL_FUNCTION_NODE_ID {
                        if arg_regs.len() != 1 {
                            panic!("bol() can have only one argument")
                        }
                        let value = self.current_frame().get(arg_regs[0]);
                        println!("{value}");
                        self.current_frame().insert(*dst_reg, 0);
                        i += 1;
                        continue;
                    }
                    // DEBUG END

                    let mut new_frame = Frame::new(HashMap::new());
                    new_frame.insert(0, self.current_frame().get(*static_link_reg));
                    for (i, arg_reg) in arg_regs.iter().enumerate() {
                        new_frame.insert(i + 1, self.current_frame().get(*arg_reg));
                    }
                    let result = self.run_fn(function_node_id.clone(), new_frame);
                    self.current_frame().insert(*dst_reg, result);
                }
                Instruction::Return(ReturnSpec { src_reg }) => {
                    return_value = self.current_frame().get(*src_reg);
                    break;
                }
            }
            i += 1;
        }
        self.pop_frame();
        return_value
    }
}
