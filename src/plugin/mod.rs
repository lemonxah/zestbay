//! Format-agnostic plugin abstraction layer.
//!
//! This module defines the shared types and interfaces used by all plugin
//! format backends (LV2, CLAP, VST3).  Each backend lives in its own
//! top-level module (`src/lv2/`, `src/clap/`, `src/vst3/`) and feeds into
//! the unified [`PluginManager`].

pub mod cpu_stats;
pub mod manager;
pub mod paths;
pub mod sandbox;
pub mod types;

pub use manager::PluginManager;
pub use types::*;

use std::sync::atomic::AtomicBool;

/// Create effect-plugin filter nodes as passive PipeWire nodes (preference
/// `passive_plugins`, off by default). A passive node never wakes the
/// devices it is linked to, so a chain like mic -> effect -> headset stays
/// silent until something else (e.g. a volume meter) wakes the mic; off keeps
/// such chains running. Read when a plugin's filter node is created.
pub static PASSIVE_EFFECTS: AtomicBool = AtomicBool::new(false);
