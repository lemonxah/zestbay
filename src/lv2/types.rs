use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type PluginInstanceId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lv2PortType {
    AudioInput,
    AudioOutput,
    ControlInput,
    ControlOutput,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lv2PortInfo {
    pub index: usize,
    pub symbol: String,
    pub name: String,
    pub port_type: Lv2PortType,
    pub default_value: f32,
    pub min_value: f32,
    pub max_value: f32,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lv2PluginInfo {
    pub uri: String,
    pub name: String,
    pub category: Lv2PluginCategory,
    pub author: Option<String>,
    pub ports: Vec<Lv2PortInfo>,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub control_inputs: usize,
    pub control_outputs: usize,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default = "default_true")]
    pub compatible: bool,
}

fn default_true() -> bool {
    true
}

impl Lv2PluginInfo {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPluginInstance {
    pub plugin_uri: String,
    pub display_name: String,
    pub bypassed: bool,
    pub parameters: Vec<SavedParameter>,
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

#[derive(Debug, Clone)]
pub struct Lv2InstanceInfo {
    pub id: PluginInstanceId,
    pub stable_id: String,
    pub plugin_uri: String,
    pub display_name: String,
    pub pw_node_id: Option<u32>,
    pub parameters: Vec<Lv2ParameterValue>,
    pub active: bool,
    pub bypassed: bool,
}

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
        if let Some(buf) = self.bufs[idx as usize].try_lock() {
            if buf.is_empty() {
                None
            } else {
                Some(buf.clone())
            }
        } else {
            None
        }
    }
}

pub type SharedPortUpdates = Arc<PortUpdates>;
