pub mod filter;
pub mod cpu_stats;
pub mod manager;
pub mod paths;
pub mod sandbox;
pub mod scan;
pub mod types;

pub use manager::PluginManager;
pub use types::*;

use std::sync::atomic::AtomicBool;

pub static PASSIVE_EFFECTS: AtomicBool = AtomicBool::new(false);
