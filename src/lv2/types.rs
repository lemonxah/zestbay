use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

/// Unique identifier for an active plugin instance within ZestBay
pub type PluginInstanceId = u64;

/// Classification of an LV2 port
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lv2PortType {
    /// Audio sample data (f32 buffer)
    AudioInput,
    AudioOutput,
    /// Control value (single f32)
    ControlInput,
    ControlOutput,
    /// Atom / event port (MIDI, etc.) - not yet supported
    AtomInput,
    AtomOutput,
}

impl Lv2PortType {
    pub fn is_audio(&self) -> bool {
        matches!(self, Lv2PortType::AudioInput | Lv2PortType::AudioOutput)
    }

    pub fn is_control(&self) -> bool {
        matches!(self, Lv2PortType::ControlInput | Lv2PortType::ControlOutput)
    }

    pub fn is_input(&self) -> bool {
        matches!(
            self,
            Lv2PortType::AudioInput | Lv2PortType::ControlInput | Lv2PortType::AtomInput
        )
    }

    pub fn is_output(&self) -> bool {
        matches!(
            self,
            Lv2PortType::AudioOutput | Lv2PortType::ControlOutput | Lv2PortType::AtomOutput
        )
    }
}

/// Metadata for a single LV2 port
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lv2PortInfo {
    /// Port index within the plugin
    pub index: usize,
    /// Port symbol (short identifier)
    pub symbol: String,
    /// Port human-readable name
    pub name: String,
    /// Port type
    pub port_type: Lv2PortType,
    /// Default value (for control ports)
    pub default_value: f32,
    /// Minimum value (for control ports)
    pub min_value: f32,
    /// Maximum value (for control ports)
    pub max_value: f32,
}

/// Classification of an LV2 plugin
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lv2PluginCategory {
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

impl Lv2PluginCategory {
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

/// Metadata describing an available (but not instantiated) LV2 plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lv2PluginInfo {
    /// Plugin URI (globally unique identifier)
    pub uri: String,
    /// Human-readable name
    pub name: String,
    /// Plugin category/class
    pub category: Lv2PluginCategory,
    /// Author name
    pub author: Option<String>,
    /// Port descriptions
    pub ports: Vec<Lv2PortInfo>,
    /// Number of audio input ports
    pub audio_inputs: usize,
    /// Number of audio output ports
    pub audio_outputs: usize,
    /// Number of control input ports
    pub control_inputs: usize,
    /// Number of control output ports
    pub control_outputs: usize,
}

impl Lv2PluginInfo {
    /// Check if this is an effect (has both audio in and out)
    pub fn is_effect(&self) -> bool {
        self.audio_inputs > 0 && self.audio_outputs > 0
    }

    /// Check if this is an instrument/generator (has audio out but no audio in)
    pub fn is_instrument(&self) -> bool {
        self.audio_inputs == 0 && self.audio_outputs > 0
    }

    /// Check if this is an analyser (has audio in but no audio out)
    pub fn is_analyser(&self) -> bool {
        self.audio_inputs > 0 && self.audio_outputs == 0
    }
}

/// Saved state of a single plugin instance (for session persistence)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginInstance {
    /// Plugin URI to re-instantiate
    pub plugin_uri: String,
    /// Display name
    pub display_name: String,
    /// Whether the plugin was bypassed
    pub bypassed: bool,
    /// Saved parameter values (port_index -> value)
    pub parameters: Vec<SavedParameter>,
}

/// A single saved parameter value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedParameter {
    pub port_index: usize,
    pub symbol: String,
    pub value: f32,
}

/// Saved link between two nodes (by node name + port name)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginLink {
    pub output_node_name: String,
    pub output_port_name: String,
    pub input_node_name: String,
    pub input_port_name: String,
}

/// Full session state that gets persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    /// Active plugin instances to restore
    pub plugins: Vec<SavedPluginInstance>,
    /// Links involving LV2 plugin nodes to restore
    pub links: Vec<SavedPluginLink>,
}

/// Current parameter values for an active plugin instance
#[derive(Debug, Clone)]
pub struct Lv2ParameterValue {
    pub port_index: usize,
    pub symbol: String,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

/// State of an active LV2 plugin instance
#[derive(Debug, Clone)]
pub struct Lv2InstanceInfo {
    /// Our internal instance ID (changes each session)
    pub id: PluginInstanceId,
    /// Stable UUID that persists across sessions for matching saved state
    pub stable_id: String,
    /// Plugin URI
    pub plugin_uri: String,
    /// Human-readable name (user can rename)
    pub display_name: String,
    /// PipeWire node ID once registered (None until the filter is created)
    pub pw_node_id: Option<u32>,
    /// Current parameter values
    pub parameters: Vec<Lv2ParameterValue>,
    /// Whether the plugin is currently active/processing
    pub active: bool,
    /// Whether the plugin is bypassed
    pub bypassed: bool,
}

// ─── Lock-free DSP ↔ UI port value sharing ───────────────────────────────────

/// An f32 stored as atomic u32 bits — safe for lock-free sharing between
/// the PipeWire RT thread and the UI thread.
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

/// A single port slot in the shared buffer.
pub struct PortSlot {
    /// LV2 port index
    pub port_index: usize,
    /// Current value (written by DSP, read by UI)
    pub value: AtomicF32,
}

/// Shared, lock-free buffer of control port values for communication
/// between the DSP thread and the plugin UI window.
///
/// Created when a plugin is instantiated, stored as `Arc` so both the
/// `FilterData` (RT thread) and the UI thread can hold a reference.
pub struct PortUpdates {
    /// Control input ports (UI → DSP direction is handled via PwCommand,
    /// but DSP → UI needs these so the UI can show the current values
    /// even after host-side automation changes them).
    pub control_inputs: Vec<PortSlot>,
    /// Control output ports (meters, gain reduction, etc.)
    pub control_outputs: Vec<PortSlot>,
    /// Atom output ports — double-buffered for lock-free DSP → UI transfer.
    pub atom_outputs: Vec<AtomPortBuffer>,
    /// Atom input ports — double-buffered for lock-free UI → DSP transfer.
    /// The plugin UI writes atom data here (e.g. patch:Set messages);
    /// the DSP thread reads and copies into the plugin's atom input buffers.
    pub atom_inputs: Vec<AtomPortBuffer>,
}

impl PortUpdates {
    /// Snapshot all control port values as (port_index, value) pairs.
    /// Includes both inputs and outputs.
    pub fn snapshot_all(&self) -> Vec<(usize, f32)> {
        self.control_inputs
            .iter()
            .chain(self.control_outputs.iter())
            .map(|s| (s.port_index, s.value.load()))
            .collect()
    }
}

/// Double-buffered atom port data for lock-free DSP → UI transfer.
///
/// The DSP thread writes into the "write" buffer and flips the index.
/// The UI thread reads from the other buffer. Since the UI runs at
/// ~30 Hz and the DSP at ~1000 Hz, occasional dropped frames are fine.
pub struct AtomPortBuffer {
    pub port_index: usize,
    bufs: [parking_lot::Mutex<Vec<u8>>; 2],
    /// Which buffer the UI should read from (0 or 1).
    /// The DSP writes to the *other* buffer, then flips this.
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

    /// Called by the DSP thread after `run()` to publish atom output data.
    /// Copies `data` into the write buffer and flips the read index.
    pub fn write(&self, data: &[u8]) {
        let write_idx = 1 - self.read_idx.load(Ordering::Acquire);
        if let Some(mut buf) = self.bufs[write_idx as usize].try_lock() {
            buf.clear();
            buf.extend_from_slice(data);
            // Flip: make the UI read from this buffer now
            self.read_idx.store(write_idx, Ordering::Release);
        }
        // If try_lock fails, the UI is reading — just skip this frame
    }

    /// Called by the UI thread to read the latest atom data.
    /// Returns None if the buffer is empty.
    pub fn read(&self) -> Option<Vec<u8>> {
        let idx = self.read_idx.load(Ordering::Acquire);
        if let Some(buf) = self.bufs[idx as usize].try_lock() {
            if buf.is_empty() {
                None
            } else {
                Some(buf.clone())
            }
        } else {
            None // DSP is writing, skip
        }
    }
}

/// Convenience alias
pub type SharedPortUpdates = Arc<PortUpdates>;
