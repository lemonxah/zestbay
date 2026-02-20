//! User interface module
//!
//! Contains all UI components using egui.
//! Independent of PipeWire internals - only uses types for communication.

mod app;
mod graph;

pub use app::ZestBayApp;
#[allow(unused_imports)]
pub use graph::GraphView;
