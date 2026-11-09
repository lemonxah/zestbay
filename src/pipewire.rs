mod manager;
mod owned;
pub mod state;
mod types;

use std::sync::atomic::AtomicBool;

pub use state::GraphState;
pub use types::*;

pub use manager::start;

pub static HIDE_UNNAMED_NODES: AtomicBool = AtomicBool::new(true);
