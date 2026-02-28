//! Format-agnostic plugin abstraction layer.
//!
//! This module defines the shared types and interfaces used by all plugin
//! format backends (LV2, CLAP, VST3).  Each backend lives in its own
//! top-level module (`src/lv2/`, `src/clap/`, `src/vst3/`) and feeds into
//! the unified [`PluginManager`].

pub mod cpu_stats;
pub mod manager;
pub mod types;

pub use manager::PluginManager;
pub use types::*;
