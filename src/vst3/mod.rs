//! VST3 plugin hosting backend.
//!
//! Provides scanning, instantiation, and real-time processing of VST3
//! audio plugins (.vst3 bundles).

pub mod com_host;
pub mod filter;
pub mod host;
pub mod scanner;
pub mod ui;
