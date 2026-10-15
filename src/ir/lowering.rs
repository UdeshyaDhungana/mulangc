use std::collections::HashMap;

use crate::{
    diagnostics::Span,
    ir::instructions::{
        BinOpSpec, BinaryOperation, CallSpec, FnDefnSpec, GoifSpec, IRFunctionBlock, IRProgram,
        Instruction::{self, BinaryOp, LoadFrameAddress, LoadImmediate, StoreFromRegister},
        LoadFrameAddrSpec, LoadImmSpec, LoadToRegSpec, MovSpec, RegisterNumber, ReturnSpec,
        StrFromRegSpec, UnaryOp, UnaryOpSpec,
    },
    syntax::{
        AST,
        BinaryOperator::{self},
        Block, Conditional, Else, Expression, ExpressionKind, FunctionDefinition, Identifier,
        Statement, StatementKind, UnaryOperator,
        ast::{DataType::IntegerType, SpannedDataType},
        sema::{Analysis, FunctionLayout, IdentifierLayout, MAIN_FUNCTION_NODE_ID, MemoryUnits},
        utils::Counter,
    },
};

// IR frame layout, in logical slots (see _ir_specs.md):
//   slot 0                      static link
//   slots 1 ..= variable_slot_count params, then locals
const RESERVED_LOGICAL_SLOTS_BEFORE_LOCALS: MemoryUnits = 1;
pub const STATIC_LINK_LOGICAL_OFFSET: MemoryUnits = 0;

pub struct Lowerer {
    analysis: Analysis,
    registered_functions: HashMap<String, IRFunctionBlock>,
    current_function_stack: Vec<IRFunctionBlock>,
    counter: Counter,
}

impl Lowerer {
    pub fn new(analysis: Analysis) -> Self {
        Self {
            analysis,
            registered_functions: HashMap::new(),
            current_function_stack: Vec::new(),
            counter: Counter::default(),
        }
    }

    fn get_current_frame_depth(&self) -> usize {
        self.current_function_stack.len() - 1
    }

    fn get_next_register(&mut self) -> RegisterNumber {
        self.current_function_stack
            .last_mut()
            .unwrap()
            .get_next_register()
    }

    fn append_instruction(&mut self, instruction: Instruction) {
        let current_function = self
            .current_function_stack
            .last_mut()
            .expect("At least one function must be present");
        current_function.push_instruction(instruction);
    }

    fn push_fn_to_frame_stack(&mut self, fn_defn: &FunctionDefinition) {
        let parameters = &fn_defn.parameters;
        let node_id = &fn_defn.identifier.node_id;
        let frame_size = self.frame_size_for_function(node_id);
        let params_sizes = &parameters
            .iter()
            .map(|p| p.spanned_data_type.size())
            .collect();

        let mut new_fn_block =
            IRFunctionBlock::new(&fn_defn.identifier.node_id, frame_size, params_sizes);
        new_fn_block.push_instruction(Instruction::FunctionDefinition(FnDefnSpec {
            node_id: node_id.clone(),
            frame_size,
            argc_sizes: params_sizes.clone(),
        }));

        self.current_function_stack.push(new_fn_block);
    }

    fn pop_fn_from_frame_stack(&mut self) {
        self.current_function_stack
            .last_mut()
            .expect("At least one function must be present")
            .end();
        let ended_function = self
            .current_function_stack
            .pop()
            .expect("At least one function must be present");
        self.registered_functions
            .insert(ended_function.id().clone(), ended_function);
    }

    pub fn lower(mut self, ast: AST) -> IRProgram {
        // The program cannot take arguments from OS as of now
        // We'll remove this shenanigan once we make main function compulsory
        // The same corollary goes for parser's parse wher we push the main
        // function to the stack at the start of the program
        self.push_fn_to_frame_stack(&FunctionDefinition {
            identifier: Identifier {
                name: String::from("main"),
                node_id: String::from(MAIN_FUNCTION_NODE_ID),
                span: Span::new(0, 0),
            },
            parameters: Vec::new(),
            return_type: SpannedDataType {
                data_type: IntegerType,
                span: Span::new(0, 0),
            },
            body: Block {
                statements: Vec::new(),
                span: Span::new(0, 0),
            },
        });

        self.lower_block(ast.block);

        // return 0;
        self.append_instruction(LoadImmediate(LoadImmSpec { dst_reg: 0, imm: 0 }));
        self.append_instruction(Instruction::Return(ReturnSpec { src_reg: 0 }));
        self.pop_fn_from_frame_stack();

        std::mem::take(&mut self.registered_functions)
    }

    fn lower_block(&mut self, block: Block) {
        for statement in block.statements {
            self.lower_statement(statement);
        }
    }

    fn lower_statement(&mut self, statement: Statement) {
        match statement.kind {
            StatementKind::Declaration {
                identifier, value, ..
            } => {
                let expr_reg = self.lower_expression(value);

                let frame_address_reg = self.get_next_register();
                self.append_instruction(LoadFrameAddress(LoadFrameAddrSpec {
                    dst_reg: frame_address_reg,
                }));

                let variable_location = self.analysis.must_get_variable_layout(&identifier.node_id);

                self.append_instruction(StoreFromRegister(StrFromRegSpec {
                    src_reg: expr_reg,
                    dst_base_reg: frame_address_reg,
                    offset: variable_location.offset_from_parent_frame_base
                        + RESERVED_LOGICAL_SLOTS_BEFORE_LOCALS,
                }));
            }
            StatementKind::Return(expression) => {
                let reg_num = self.lower_expression(expression);
                self.append_instruction(Instruction::Return(ReturnSpec { src_reg: reg_num }));
            }
            StatementKind::Expression(expression) => {
                self.lower_expression(expression);
            }
            StatementKind::Function(function_definition) => {
                self.push_fn_to_frame_stack(&function_definition);

                let frame_address_reg = self.get_next_register();
                self.append_instruction(LoadFrameAddress(LoadFrameAddrSpec {
                    dst_reg: frame_address_reg,
                }));
                self.append_instruction(StoreFromRegister(StrFromRegSpec {
                    src_reg: 0,
                    dst_base_reg: frame_address_reg,
                    offset: STATIC_LINK_LOGICAL_OFFSET,
                }));

                for (idx, parameter) in function_definition.parameters.iter().enumerate() {
                    let offset = self
                        .analysis
                        .must_get_variable_layout(&parameter.identifier.node_id)
                        .offset_from_parent_frame_base;
                    self.append_instruction(StoreFromRegister(StrFromRegSpec {
                        src_reg: idx + 1,
                        dst_base_reg: frame_address_reg,
                        offset: offset + RESERVED_LOGICAL_SLOTS_BEFORE_LOCALS,
                    }));
                }

                let mut has_return = false;
                for statement in function_definition.body.statements {
                    if matches!(statement.kind, StatementKind::Return(..)) {
                        has_return = true;
                    }
                    self.lower_statement(statement);
                }

                // Implicit return 0
                if !has_return {
                    let next_register = self.get_next_register();
                    self.append_instruction(Instruction::LoadImmediate(LoadImmSpec {
                        dst_reg: next_register,
                        imm: 0,
                    }));
                    self.append_instruction(Instruction::Return(ReturnSpec {
                        src_reg: next_register,
                    }));
                }

                self.pop_fn_from_frame_stack();
            }
            StatementKind::If(conditional) => {
                self.lower_conditional(conditional);
            }
            StatementKind::While {
                node_id,
                condition,
                body,
            } => {
                let condition_label = format!("{node_id}_condition");
                let body_label = format!("{node_id}_body");
                let end_label = format!("{node_id}_end");

                self.append_instruction(Instruction::Goto(condition_label.clone()));
                self.append_instruction(Instruction::Label(condition_label.clone()));
                let condition_reg = self.lower_expression(*condition);
                self.append_instruction(Instruction::Goif(GoifSpec {
                    condition_reg,
                    true_label: body_label.clone(),
                    false_label: end_label.clone(),
                }));
                self.append_instruction(Instruction::Label(body_label));
                self.lower_block(body);
                self.append_instruction(Instruction::Goto(condition_label));
                self.append_instruction(Instruction::Label(end_label));
            }
            StatementKind::For {
                node_id,
                initialiser,
                condition,
                update,
                body,
            } => {
                let condition_label = format!("{node_id}_condition");
                let body_label = format!("{node_id}_body");
                let end_label = format!("{node_id}_end");

                if let Some(initialiser) = initialiser {
                    self.lower_statement(*initialiser);
                }
                // To make CFG parsing easier later, we write an explicit goto
                // to mark the end of previous node
                self.append_instruction(Instruction::Goto(condition_label.clone()));
                self.append_instruction(Instruction::Label(condition_label.clone()));
                let condition_reg = self.lower_expression(*condition);
                self.append_instruction(Instruction::Goif(GoifSpec {
                    condition_reg,
                    true_label: body_label.clone(),
                    false_label: end_label.clone(),
                }));
                self.append_instruction(Instruction::Label(body_label));
                self.lower_block(body);
                self.lower_expression(*update);
                self.append_instruction(Instruction::Goto(condition_label));
                self.append_instruction(Instruction::Label(end_label));
            }
        }
    }

    // Returns the register number in which the result is calculated onto
    fn lower_expression(&mut self, expression: Expression) -> RegisterNumber {
        match expression.kind {
            ExpressionKind::Identifier(identifier) => {
                let resolved_node_id = self.analysis.must_resolve(&identifier.node_id).clone();
                let (frame_pointer_reg, offset) = self
                    .get_frame_pointer_reg_and_logical_local_offset_for_node_id(&resolved_node_id);
                let dst_reg = self.get_next_register();
                self.append_instruction(Instruction::LoadToRegister(LoadToRegSpec {
                    dst_reg,
                    src_base_reg: frame_pointer_reg,
                    offset,
                }));
                dst_reg
            }
            // Integer lives in one register
            ExpressionKind::Integer(value) => {
                let reg_num: RegisterNumber = self.get_next_register();
                self.append_instruction(Instruction::LoadImmediate(LoadImmSpec {
                    dst_reg: reg_num,
                    imm: value,
                }));
                reg_num
            }
            ExpressionKind::UnaryOperation { operator, operand } => {
                let operand_reg: RegisterNumber = self.lower_expression(*operand);
                let dst_reg: RegisterNumber = self.get_next_register();
                self.append_instruction(Instruction::UnaryOp(UnaryOpSpec {
                    dst_reg,
                    operation: operator.into(),
                    operand_reg,
                }));
                dst_reg
            }
            ExpressionKind::BinaryOperation {
                operator,
                left,
                right,
            } => {
                let left_reg: RegisterNumber = self.lower_expression(*left);
                let dst_reg: RegisterNumber = self.get_next_register();

                // For boolean operations, we evaluate things differently
                let Ok(operation) = BinaryOperation::try_from(operator) else {
                    let is_boolean_or = matches!(operator, BinaryOperator::BooleanOr);

                    let early_exit_label = format!("EARLY_EXIT_{}", self.counter.advance());
                    let late_exit_label = format!("LATE_EXIT_{}", self.counter.advance());
                    let end_label = format!("END_{}", self.counter.advance());

                    self.append_instruction(Instruction::Goif(GoifSpec {
                        condition_reg: left_reg,
                        true_label: if is_boolean_or {
                            early_exit_label.clone()
                        } else {
                            late_exit_label.clone()
                        },
                        false_label: if is_boolean_or {
                            late_exit_label.clone()
                        } else {
                            early_exit_label.clone()
                        },
                    }));

                    // Early exit
                    self.append_instruction(Instruction::Label(early_exit_label.clone()));
                    self.append_instruction(Instruction::Move(MovSpec {
                        src_reg: left_reg,
                        dst_reg: dst_reg,
                    }));
                    self.append_instruction(Instruction::Goto(end_label.clone()));

                    // Late exit
                    self.append_instruction(Instruction::Label(late_exit_label.clone()));
                    let right_reg = self.lower_expression(*right);
                    self.append_instruction(Instruction::Move(MovSpec {
                        dst_reg: dst_reg,
                        src_reg: right_reg,
                    }));
                    self.append_instruction(Instruction::Label(end_label.clone()));
                    return dst_reg;
                };

                let right_reg: RegisterNumber = self.lower_expression(*right);

                self.append_instruction(BinaryOp(BinOpSpec {
                    dst_reg,
                    left_reg,
                    right_reg,
                    operation,
                }));
                dst_reg
            }
            ExpressionKind::Assignment { identifier, value } => {
                let expression_reg: RegisterNumber = self.lower_expression(*value);
                let resolved_node_id = self.analysis.must_resolve(&identifier.node_id).clone();
                let (frame_pointer_reg, offset) = self
                    .get_frame_pointer_reg_and_logical_local_offset_for_node_id(&resolved_node_id);
                self.append_instruction(Instruction::StoreFromRegister(StrFromRegSpec {
                    src_reg: expression_reg,
                    dst_base_reg: frame_pointer_reg,
                    offset,
                }));
                expression_reg
            }
            ExpressionKind::Call { callee, arguments } => {
                let frame_reg: RegisterNumber = self.get_next_register();
                self.append_instruction(Instruction::LoadFrameAddress(LoadFrameAddrSpec {
                    dst_reg: frame_reg,
                }));
                let mut arg_regs = Vec::new();
                for argument in arguments {
                    let argument_reg = self.lower_expression(argument);
                    arg_regs.push(argument_reg)
                }

                let dst_reg = self.get_next_register();

                // If I'm the parent, pass my frame address, else
                // pass on whatever static link I was handed off
                let resolved_node_id = self.analysis.must_resolve(&callee.node_id).clone();
                let is_caller_parent = self.get_current_frame_depth()
                    == self
                        .analysis
                        .must_get_function_layout(&resolved_node_id)
                        .parent_frame_depth;
                self.append_instruction(Instruction::Call(CallSpec {
                    function_node_id: self.analysis.must_resolve(&callee.node_id).clone(),
                    static_link_reg: if is_caller_parent { frame_reg } else { 0 },
                    arg_regs,
                    dst_reg,
                }));
                dst_reg
            }
        }
    }

    fn lower_conditional(&mut self, conditional: Conditional) {
        let then_label = format!("{}_then", conditional.node_id);
        let else_label = format!("{}_else", conditional.node_id);
        let end_label = format!("{}_end", conditional.node_id);
        let expression_reg = self.lower_expression(conditional.condition);
        self.append_instruction(Instruction::Goif(GoifSpec {
            condition_reg: expression_reg,
            true_label: then_label.clone(),
            false_label: else_label.clone(),
        }));
        self.append_instruction(Instruction::Label(then_label));
        self.lower_block(conditional.then);
        self.append_instruction(Instruction::Goto(end_label.clone()));
        self.append_instruction(Instruction::Label(else_label));

        if let Some(otherwise) = conditional.otherwise {
            match *otherwise {
                Else::Block(block) => {
                    self.lower_block(block);
                    self.append_instruction(Instruction::Goto(end_label.clone()));
                }
                Else::If(nested) => {
                    self.lower_conditional(nested);
                    self.append_instruction(Instruction::Goto(end_label.clone()));
                }
            }
        }

        self.append_instruction(Instruction::Label(end_label));
    }

    // Returns the register number holding the frame pointer
    // and local offset within the frame for that node id
    fn get_frame_pointer_reg_and_logical_local_offset_for_node_id(
        &mut self,
        node_id: &String,
    ) -> (RegisterNumber, MemoryUnits) {
        let (variable_frame_depth, offset_from_frame_base) = {
            let location = self.analysis.must_get_variable_layout(&node_id);
            (
                location.parent_frame_depth,
                location.offset_from_parent_frame_base,
            )
        };

        let hops = self.get_current_frame_depth() - variable_frame_depth;
        let frame_address_reg = self.get_next_register();
        self.append_instruction(Instruction::LoadFrameAddress(LoadFrameAddrSpec {
            dst_reg: frame_address_reg,
        }));

        for _ in 0..hops {
            self.append_instruction(Instruction::LoadToRegister(LoadToRegSpec {
                dst_reg: frame_address_reg,
                src_base_reg: frame_address_reg,
                offset: 0,
            }));
        }

        (
            frame_address_reg,
            offset_from_frame_base + RESERVED_LOGICAL_SLOTS_BEFORE_LOCALS,
        )
    }

    // How many memory units does a function take
    fn frame_size_for_function(&self, node_id: &str) -> MemoryUnits {
        if let Some(IdentifierLayout::Function(FunctionLayout {
            locals_occupied_stack_size,
            ..
        })) = self.analysis.node_id_layout.get(node_id)
        {
            return *locals_occupied_stack_size + RESERVED_LOGICAL_SLOTS_BEFORE_LOCALS;
        };

        panic!("LOWERING:: No function location recorded for {node_id}");
    }
}

impl From<UnaryOperator> for UnaryOp {
    fn from(operator: UnaryOperator) -> Self {
        match operator {
            UnaryOperator::Negate => UnaryOp::Negate,
            UnaryOperator::Not => UnaryOp::Not,
        }
    }
}

// `&&` and `||` lower to branches, so they have no `BinaryOperation`.
// The unconverted operator is handed back as the error.
impl TryFrom<BinaryOperator> for BinaryOperation {
    type Error = BinaryOperator;

    fn try_from(operator: BinaryOperator) -> Result<Self, Self::Error> {
        Ok(match operator {
            BinaryOperator::Add => BinaryOperation::Add,
            BinaryOperator::Subtract => BinaryOperation::Subtract,
            BinaryOperator::Multiply => BinaryOperation::Multiply,
            BinaryOperator::Divide => BinaryOperation::Divide,
            BinaryOperator::Remainder => BinaryOperation::Remainder,
            BinaryOperator::Equal => BinaryOperation::Equal,
            BinaryOperator::NotEqual => BinaryOperation::NotEqual,
            BinaryOperator::Less => BinaryOperation::Less,
            BinaryOperator::LessOrEqual => BinaryOperation::LessOrEqual,
            BinaryOperator::Greater => BinaryOperation::Greater,
            BinaryOperator::GreaterOrEqual => BinaryOperation::GreaterOrEqual,
            BinaryOperator::BooleanAnd | BinaryOperator::BooleanOr => return Err(operator),
        })
    }
}
