//! Front end and tree-walking back end for the mulang programming language.
//!
//! The crate is layered so that a compiler back end can be dropped in beside the
//! interpreter without touching anything above it:
//!
//! - [`diagnostics`] — source spans and human-readable error reporting.
//! - [`syntax`] — text in, [`syntax::Program`] out. Everything about tokens,
//!   precedence and grammar stays behind [`syntax::parse`].
//! - [`runtime`] — an [`runtime::Interpreter`] that executes a `Program`.
//! - [`repl`] — an interactive session built from the two layers above.

#![forbid(unsafe_code)]

pub mod diagnostics;
pub mod repl;
pub mod runtime;
pub mod syntax;
