//! Format-agnostic plugin types used across LV2, CLAP, and VST3 backends.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Unique identifier for a running plugin instance (format-agnostic).
pub type PluginInstanceId = u64;

// ---------------------------------------------------------------------------
// Plugin format
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginFormat {
    Lv2,
    Clap,
    Vst3,
}

impl PluginFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lv2 => "LV2",
            Self::Clap => "CLAP",
            Self::Vst3 => "VST3",
        }
    }
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Port types (unified across formats)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginPortType {
    AudioInput,
    AudioOutput,
    ControlInput,
    ControlOutput,
    /// LV2 atom ports (MIDI / atom events).  CLAP and VST3 handle MIDI
    /// through their own event mechanisms, so these are LV2-specific in
    /// practice but still kept in the unified enum for completeness.
    AtomInput,
    AtomOutput,
}

impl PluginPortType {
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::AudioInput | Self::AudioOutput)
    }

    pub fn is_control(&self) -> bool {
        matches!(self, Self::ControlInput | Self::ControlOutput)
    }

    pub fn is_input(&self) -> bool {
        matches!(
            self,
            Self::AudioInput | Self::ControlInput | Self::AtomInput
        )
    }

    pub fn is_output(&self) -> bool {
        matches!(
            self,
            Self::AudioOutput | Self::ControlOutput | Self::AtomOutput
        )
    }
}

// ---------------------------------------------------------------------------
// Port info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPortInfo {
    pub index: usize,
    pub symbol: String,
    pub name: String,
    pub port_type: PluginPortType,
    pub default_value: f32,
    pub min_value: f32,
    pub max_value: f32,
}

// ---------------------------------------------------------------------------
// Plugin category
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCategory {
    Amplifier,
    Analyser,
    Chorus,
    Compressor,
    Delay,
    Distortion,
    Dynamics,
    Envelope,
    Equaliser,
    Expander,
    Filter,
    Flanger,
    Gate,
    Generator,
    Instrument,
    Limiter,
    Mixer,
    Modulator,
    Oscillator,
    Phaser,
    Reverb,
    Simulator,
    Spatial,
    Utility,
    Waveshaper,
    Other(String),
}

impl PluginCategory {
    pub fn from_class_label(label: &str) -> Self {
        match label.to_lowercase().as_str() {
            s if s.contains("amplifier") => Self::Amplifier,
            s if s.contains("analyser") || s.contains("analyzer") => Self::Analyser,
            s if s.contains("chorus") => Self::Chorus,
            s if s.contains("compressor") => Self::Compressor,
            s if s.contains("delay") => Self::Delay,
            s if s.contains("distortion") => Self::Distortion,
            s if s.contains("dynamics") => Self::Dynamics,
            s if s.contains("envelope") => Self::Envelope,
            s if s.contains("equaliser") || s.contains("equalizer") || s.contains("eq") => {
                Self::Equaliser
            }
            s if s.contains("expander") => Self::Expander,
            s if s.contains("filter") => Self::Filter,
            s if s.contains("flanger") => Self::Flanger,
            s if s.contains("gate") => Self::Gate,
            s if s.contains("generator") => Self::Generator,
            s if s.contains("instrument") => Self::Instrument,
            s if s.contains("limiter") => Self::Limiter,
            s if s.contains("mixer") => Self::Mixer,
            s if s.contains("modulator") => Self::Modulator,
            s if s.contains("oscillator") => Self::Oscillator,
            s if s.contains("phaser") => Self::Phaser,
            s if s.contains("reverb") => Self::Reverb,
            s if s.contains("simulator") => Self::Simulator,
            s if s.contains("spatial") => Self::Spatial,
            s if s.contains("utility") => Self::Utility,
            s if s.contains("waveshaper") => Self::Waveshaper,
            _ => Self::Other(label.to_string()),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Amplifier => "Amplifier",
            Self::Analyser => "Analyser",
            Self::Chorus => "Chorus",
            Self::Compressor => "Compressor",
            Self::Delay => "Delay",
            Self::Distortion => "Distortion",
            Self::Dynamics => "Dynamics",
            Self::Envelope => "Envelope",
            Self::Equaliser => "Equaliser",
            Self::Expander => "Expander",
            Self::Filter => "Filter",
            Self::Flanger => "Flanger",
            Self::Gate => "Gate",
            Self::Generator => "Generator",
            Self::Instrument => "Instrument",
            Self::Limiter => "Limiter",
            Self::Mixer => "Mixer",
            Self::Modulator => "Modulator",
            Self::Oscillator => "Oscillator",
            Self::Phaser => "Phaser",
            Self::Reverb => "Reverb",
            Self::Simulator => "Simulator",
            Self::Spatial => "Spatial",
            Self::Utility => "Utility",
            Self::Waveshaper => "Waveshaper",
            Self::Other(s) => s.as_str(),
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin info (catalog entry — available but not necessarily instantiated)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Unique identifier for this plugin.  For LV2 this is the URI, for CLAP
    /// the plugin-id string, for VST3 the class-id hex string.
    pub uri: String,
    pub name: String,
    pub format: PluginFormat,
    pub category: PluginCategory,
    pub author: Option<String>,
    pub ports: Vec<PluginPortInfo>,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub control_inputs: usize,
    pub control_outputs: usize,
    pub required_features: Vec<String>,
    pub compatible: bool,
    /// Whether the plugin provides a native UI.
    pub has_ui: bool,
    /// Filesystem path to the plugin library (.clap file, .vst3 bundle).
    /// Empty for LV2 (which uses lilv for discovery).
    #[serde(default)]
    pub library_path: String,
}

impl PluginInfo {
    pub fn is_effect(&self) -> bool {
        self.audio_inputs > 0 && self.audio_outputs > 0
    }

    pub fn is_instrument(&self) -> bool {
        self.audio_inputs == 0 && self.audio_outputs > 0
    }

    pub fn is_analyser(&self) -> bool {
        self.audio_inputs > 0 && self.audio_outputs == 0
    }
}

// ---------------------------------------------------------------------------
// Parameter value (runtime state of one control parameter)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParameterValue {
    pub port_index: usize,
    pub symbol: String,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

// ---------------------------------------------------------------------------
// Instance info (metadata about a running plugin — kept in the manager)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PluginInstanceInfo {
    pub id: PluginInstanceId,
    pub stable_id: String,
    pub plugin_uri: String,
    pub format: PluginFormat,
    pub display_name: String,
    pub pw_node_id: Option<u32>,
    pub parameters: Vec<ParameterValue>,
    pub active: bool,
    pub bypassed: bool,
}

// ---------------------------------------------------------------------------
// Persistence types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginInstance {
    pub plugin_uri: String,
    pub display_name: String,
    pub bypassed: bool,
    pub parameters: Vec<SavedParameter>,
    /// Optional: "LV2", "CLAP", "VST3".  Defaults to LV2 for backwards compat.
    #[serde(default = "default_lv2_format")]
    pub format: String,
}

fn default_lv2_format() -> String {
    "LV2".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedParameter {
    pub port_index: usize,
    pub symbol: String,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginLink {
    pub output_node_name: String,
    pub output_port_name: String,
    pub input_node_name: String,
    pub input_port_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub plugins: Vec<SavedPluginInstance>,
    pub links: Vec<SavedPluginLink>,
}

// ---------------------------------------------------------------------------
// Lock-free port synchronisation primitives (shared between RT and UI threads)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        Self(AtomicU32::new(val.to_bits()))
    }
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn store(&self, val: f32) {
        self.0.store(val.to_bits(), Ordering::Relaxed);
    }
}

pub struct PortSlot {
    pub port_index: usize,
    pub value: AtomicF32,
}

pub struct AtomPortBuffer {
    pub port_index: usize,
    bufs: [parking_lot::Mutex<Vec<u8>>; 2],
    read_idx: AtomicU32,
}

impl AtomPortBuffer {
    pub fn new(port_index: usize) -> Self {
        Self {
            port_index,
            bufs: [
                parking_lot::Mutex::new(Vec::new()),
                parking_lot::Mutex::new(Vec::new()),
            ],
            read_idx: AtomicU32::new(0),
        }
    }

    pub fn write(&self, data: &[u8]) {
        let write_idx = 1 - self.read_idx.load(Ordering::Acquire);
        if let Some(mut buf) = self.bufs[write_idx as usize].try_lock() {
            buf.clear();
            buf.extend_from_slice(data);
            self.read_idx.store(write_idx, Ordering::Release);
        }
    }

    pub fn read(&self) -> Option<Vec<u8>> {
        let idx = self.read_idx.load(Ordering::Acquire);
        if let Some(mut buf) = self.bufs[idx as usize].try_lock() {
            if buf.is_empty() {
                None
            } else {
                let data = std::mem::take(&mut *buf);
                Some(data)
            }
        } else {
            None
        }
    }
}

pub struct PortUpdates {
    pub control_inputs: Vec<PortSlot>,
    pub control_outputs: Vec<PortSlot>,
    pub atom_outputs: Vec<AtomPortBuffer>,
    pub atom_inputs: Vec<AtomPortBuffer>,
}

impl PortUpdates {
    pub fn snapshot_all(&self) -> Vec<(usize, f32)> {
        self.control_inputs
            .iter()
            .chain(self.control_outputs.iter())
            .map(|s| (s.port_index, s.value.load()))
            .collect()
    }
}

pub type SharedPortUpdates = Arc<PortUpdates>;
