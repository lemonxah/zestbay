//! LV2-specific type aliases that re-export from the unified plugin types.
//!
//! All format-agnostic types now live in `crate::plugin::types`.  This module
//! provides backward-compatible aliases so existing LV2 code continues to
//! compile without changes.

// Re-export everything from the unified plugin module
#[allow(unused_imports)]
pub use crate::plugin::types::*;

// ---------------------------------------------------------------------------
// Backward-compatible type aliases
// ---------------------------------------------------------------------------

/// Alias: the old name for `PluginPortType`.
pub type Lv2PortType = PluginPortType;

/// Alias: the old name for `PluginPortInfo`.
pub type Lv2PortInfo = PluginPortInfo;

/// Alias: the old name for `PluginCategory`.
pub type Lv2PluginCategory = PluginCategory;

/// Alias: the old name for `PluginInfo`.
pub type Lv2PluginInfo = PluginInfo;

/// Alias: the old name for `ParameterValue`.
pub type Lv2ParameterValue = ParameterValue;

/// Alias: the old name for `PluginInstanceInfo`.
pub type Lv2InstanceInfo = PluginInstanceInfo;
