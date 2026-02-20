//! PipeWire integration module
//!
//! This module handles all PipeWire communication and state management.
//! It exposes a clean API through channels - no UI dependencies.
//!
//! # Architecture
//!
//! - `types`: Core data structures (Node, Port, Link, events, commands)
//! - `state`: Thread-safe graph state
//! - `manager`: PipeWire thread and registry handling
//!
//! # Usage
//!
//! ```ignore
//! use zestbay::pipewire::{self, GraphState, PwEvent, PwCommand};
//!
//! let graph = GraphState::new();
//! let (event_rx, cmd_tx) = pipewire::start(graph.clone());
//!
//! // Receive events
//! while let Ok(event) = event_rx.recv() {
//!     match event {
//!         PwEvent::NodeChanged(node) => { /* ... */ }
//!         // ...
//!     }
//! }
//!
//! // Send commands
//! cmd_tx.send(PwCommand::Connect { output_port_id: 1, input_port_id: 2 });
//! ```

mod manager;
mod state;
mod types;

// Re-export public types
pub use state::GraphState;
pub use types::*;

// Re-export the start function
pub use manager::start;
