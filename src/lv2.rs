//! LV2 Plugin Hosting
//!
//! This module provides LV2 plugin discovery, instantiation, and hosting.
//! Plugins are instantiated as PipeWire filter nodes that appear in the
//! audio graph and can be wired to other nodes.
//!
//! ## Architecture
//!
//! - `types` - Data types for plugin metadata and instance state
//! - `scanner` - Plugin discovery using lilv
//! - `host` - Plugin instance lifecycle and DSP processing
//! - `filter` - PipeWire filter node wrapping for LV2 instances

pub mod filter;
pub mod host;
pub mod scanner;
pub mod types;
pub mod ui;
pub mod urid;

pub use host::Lv2Manager;
pub use types::*;
