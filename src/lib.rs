//! Front end and AArch64 back end for the mulang programming language.
//!
//! - [`diagnostics`] — source spans and human-readable error reporting.
//! - [`syntax`] — text in, [`syntax::Program`] out. Everything about tokens,
//!   precedence and grammar stays behind [`syntax::parse`].
//! - [`compiler`] — a `Program` in, macOS ARM64 assembly out.

#![forbid(unsafe_code)]

pub mod compiler;
pub mod diagnostics;
pub mod syntax;
