//! Patchbay module
//!
//! Handles automatic connection management based on user-defined rules.
//! Independent of UI — works purely with PipeWire types.

pub mod manager;
pub mod rules;

pub use manager::PatchbayManager;
