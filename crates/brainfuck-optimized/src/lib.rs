#![forbid(unsafe_code)]

mod compile;
mod fast;
mod ir;
mod machine;
pub mod replay;
mod state;

pub use ir::{Op, OpKind, OptimizedProgram, Term};
pub use machine::{OptimizedMachine, OptimizedSnapshot};
