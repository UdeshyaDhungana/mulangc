//! Executing a parsed program.
//!
//! [`Interpreter`] is the whole interface: hand it a [`Program`](crate::syntax::Program)
//! and it either produces a [`Value`] or the [`Diagnostic`](crate::diagnostics::Diagnostic)
//! explaining why it could not. Scopes, closures, operator semantics and the
//! builtin table are all private to this module.

mod builtins;
pub mod compiler;
mod interpreter;
mod operations;
mod scope;
mod value;

pub use interpreter::{Interpreter, REQUIRED_STACK_BYTES};
pub use value::Value;
