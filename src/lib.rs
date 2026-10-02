#![forbid(unsafe_code)]

pub mod compiler;
pub mod diagnostics;
pub mod ir;
pub mod syntax;

pub fn to_usize(n: i64) -> usize {
    usize::try_from(n).expect(&format!("{n} doesn't fit in usize"))
}

pub fn to_i64(n: usize) -> i64 {
    i64::try_from(n).expect(&format!("{n} doesn't fit in i64"))
}
