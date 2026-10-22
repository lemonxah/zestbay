//! Format-agnostic plugin types used across LV2, CLAP, and VST3 backends.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
    /// Whether this port is a boolean toggle (e.g. LV2 `lv2:toggled`,
    /// CLAP stepped 0–1, VST3 `stepCount == 1`).
    #[serde(default)]
    pub is_toggle: bool,
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
    pub is_toggle: bool,
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
    /// Cached LV2 state entries (populated from PW thread on remove, used for persistence)
    pub lv2_state: Vec<crate::lv2::state::StateEntry>,
    /// Cached CLAP/VST3 opaque state blob, used for persistence
    pub plugin_state: Option<Vec<u8>>,
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

// ---------------------------------------------------------------------------
// Lock-free message ring (length-prefixed byte messages over an rtrb SPSC ring)
// ---------------------------------------------------------------------------

/// Result of popping a message into a caller-provided buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgPop {
    Empty,
    /// A message of this many bytes was copied into the buffer.
    Data(usize),
    /// The next message (this many bytes) does not fit; it was left queued.
    TooLarge(usize),
}

const MSG_HEADER: usize = 4;

/// Producer half of a length-prefixed message ring.  Never allocates.
pub struct MsgProducer(rtrb::Producer<u8>);

/// Consumer half of a length-prefixed message ring.  Never allocates
/// (except `pop_vec`, which is for non-RT callers).
pub struct MsgConsumer(rtrb::Consumer<u8>);

pub fn msg_ring(capacity: usize) -> (MsgProducer, MsgConsumer) {
    let (p, c) = rtrb::RingBuffer::new(capacity);
    (MsgProducer(p), MsgConsumer(c))
}

impl MsgProducer {
    /// Push one message.  Returns false (dropping it) if there is not
    /// enough free space.  Header and payload are committed together.
    pub fn push(&mut self, data: &[u8]) -> bool {
        self.push_parts(&[data])
    }

    /// Push one message made of several concatenated parts.
    pub fn push_parts(&mut self, parts: &[&[u8]]) -> bool {
        let len: usize = parts.iter().map(|p| p.len()).sum();
        let Ok(len32) = u32::try_from(len) else {
            return false;
        };
        let total = MSG_HEADER + len;
        let Ok(mut chunk) = self.0.write_chunk(total) else {
            return false;
        };
        let (a, b) = chunk.as_mut_slices();
        let header = len32.to_ne_bytes();
        let mut pos = 0usize;
        let mut put = |mut bytes: &[u8]| {
            if pos < a.len() {
                let n = (a.len() - pos).min(bytes.len());
                a[pos..pos + n].copy_from_slice(&bytes[..n]);
                pos += n;
                bytes = &bytes[n..];
            }
            if !bytes.is_empty() {
                let off = pos - a.len();
                b[off..off + bytes.len()].copy_from_slice(bytes);
                pos += bytes.len();
            }
        };
        put(&header);
        for part in parts {
            put(part);
        }
        chunk.commit_all();
        true
    }
}

impl MsgConsumer {
    /// Length of the next complete message, if any.
    pub fn peek_len(&mut self) -> Option<usize> {
        let chunk = self.0.read_chunk(MSG_HEADER).ok()?;
        let (a, b) = chunk.as_slices();
        let mut header = [0u8; MSG_HEADER];
        for (i, byte) in a.iter().chain(b.iter()).enumerate() {
            header[i] = *byte;
        }
        let len = u32::from_ne_bytes(header) as usize;
        drop(chunk);
        if self.0.slots() >= MSG_HEADER + len {
            Some(len)
        } else {
            None
        }
    }

    /// Copy the next message into `out` and consume it.
    pub fn pop_into(&mut self, out: &mut [u8]) -> MsgPop {
        let Some(len) = self.peek_len() else {
            return MsgPop::Empty;
        };
        if len > out.len() {
            return MsgPop::TooLarge(len);
        }
        let Ok(chunk) = self.0.read_chunk(MSG_HEADER + len) else {
            return MsgPop::Empty;
        };
        let (a, b) = chunk.as_slices();
        if a.len() >= MSG_HEADER {
            let first = &a[MSG_HEADER..];
            out[..first.len()].copy_from_slice(first);
            out[first.len()..len].copy_from_slice(b);
        } else {
            let skip = MSG_HEADER - a.len();
            out[..len].copy_from_slice(&b[skip..]);
        }
        chunk.commit_all();
        MsgPop::Data(len)
    }

    /// Drop the next message without reading it.
    pub fn discard(&mut self) -> bool {
        let Some(len) = self.peek_len() else {
            return false;
        };
        if let Ok(chunk) = self.0.read_chunk(MSG_HEADER + len) {
            chunk.commit_all();
        }
        true
    }

    /// Pop the next message into a freshly allocated Vec (non-RT only).
    pub fn pop_vec(&mut self) -> Option<Vec<u8>> {
        let len = self.peek_len()?;
        let mut out = vec![0u8; len];
        match self.pop_into(&mut out) {
            MsgPop::Data(_) => Some(out),
            _ => None,
        }
    }

    pub fn clear(&mut self) {
        while self.discard() {}
    }
}

/// Default capacity of an [`AtomPortBuffer`] ring, in bytes.
pub const ATOM_RING_CAPACITY: usize = 256 * 1024;

/// Lock-free queue of atom messages for one atom port.
///
/// * Atom inputs (UI → plugin): UI threads `write()`, the RT thread drains
///   with `pop_into_rt()` each cycle; messages are queued, not overwritten.
/// * Atom outputs (plugin → UI): the RT thread `write_rt()`s the output
///   sequence after `run()` (only while a UI is listening), the UI drains
///   with `read()`.
///
/// Each side sits behind a mutex only so the struct can be shared through an
/// `Arc`; the RT side uses `try_lock`, which never blocks (it is the sole
/// user of its half).
pub struct AtomPortBuffer {
    pub port_index: usize,
    producer: parking_lot::Mutex<MsgProducer>,
    consumer: parking_lot::Mutex<MsgConsumer>,
    listening: AtomicBool,
}

impl AtomPortBuffer {
    pub fn new(port_index: usize) -> Self {
        Self::with_capacity(port_index, ATOM_RING_CAPACITY)
    }

    pub fn with_capacity(port_index: usize, capacity: usize) -> Self {
        let (producer, consumer) = msg_ring(capacity);
        Self {
            port_index,
            producer: parking_lot::Mutex::new(producer),
            consumer: parking_lot::Mutex::new(consumer),
            listening: AtomicBool::new(false),
        }
    }

    /// Queue a message (non-RT producer).  Returns false if the queue is full.
    pub fn write(&self, data: &[u8]) -> bool {
        self.producer.lock().push(data)
    }

    /// Queue a message from the RT thread.  Never blocks or allocates;
    /// drops the message if the queue is full or contended.
    pub fn write_rt(&self, data: &[u8]) -> bool {
        match self.producer.try_lock() {
            Some(mut p) => p.push(data),
            None => false,
        }
    }

    /// Pop the next message (non-RT consumer).
    pub fn read(&self) -> Option<Vec<u8>> {
        self.consumer.lock().pop_vec()
    }

    /// Pop the next message into `out` from the RT thread.
    pub fn pop_into_rt(&self, out: &mut [u8]) -> MsgPop {
        match self.consumer.try_lock() {
            Some(mut c) => c.pop_into(out),
            None => MsgPop::Empty,
        }
    }

    /// Drop the next queued message from the RT thread (e.g. one that can
    /// never fit the plugin's port buffer).
    pub fn discard_rt(&self) -> bool {
        match self.consumer.try_lock() {
            Some(mut c) => c.discard(),
            None => false,
        }
    }

    pub fn clear(&self) {
        self.consumer.lock().clear();
    }

    /// Whether a consumer (UI) wants plugin→UI output.  The RT thread skips
    /// writing output messages while nobody listens.
    pub fn is_listening(&self) -> bool {
        self.listening.load(Ordering::Acquire)
    }

    pub fn set_listening(&self, listening: bool) {
        if listening {
            self.clear();
        }
        self.listening.store(listening, Ordering::Release);
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

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    // ---- AtomicF32 ----

    #[test]
    fn atomic_f32_store_load() {
        let a = AtomicF32::new(0.0);
        assert!((a.load() - 0.0).abs() < f32::EPSILON);
        a.store(PI);
        assert!((a.load() - PI).abs() < 1e-5);
    }

    #[test]
    fn atomic_f32_negative() {
        let a = AtomicF32::new(-1.5);
        assert!((a.load() - (-1.5)).abs() < 1e-5);
    }

    #[test]
    fn atomic_f32_special_values() {
        let a = AtomicF32::new(f32::INFINITY);
        assert!(a.load().is_infinite());

        a.store(f32::NEG_INFINITY);
        assert!(a.load().is_infinite() && a.load().is_sign_negative());

        a.store(f32::NAN);
        assert!(a.load().is_nan());
    }

    // ---- PortSlot ----

    #[test]
    fn port_slot_basic() {
        let slot = PortSlot {
            port_index: 5,
            value: AtomicF32::new(0.42),
        };
        assert_eq!(slot.port_index, 5);
        assert!((slot.value.load() - 0.42).abs() < 1e-5);
    }

    // ---- AtomPortBuffer ----

    #[test]
    fn atom_port_buffer_write_then_read() {
        let buf = AtomPortBuffer::new(0);
        assert!(buf.write(b"hello"));
        let data = buf.read();
        assert_eq!(data, Some(b"hello".to_vec()));

        // Second read should be empty (consumed)
        let data2 = buf.read();
        assert_eq!(data2, None);
    }

    #[test]
    fn atom_port_buffer_empty_read() {
        let buf = AtomPortBuffer::new(0);
        assert_eq!(buf.read(), None);
    }

    #[test]
    fn atom_port_buffer_queues_messages() {
        let buf = AtomPortBuffer::new(0);
        buf.write(b"first");
        buf.write(b"second");
        assert_eq!(buf.read(), Some(b"first".to_vec()));
        assert_eq!(buf.read(), Some(b"second".to_vec()));
        assert_eq!(buf.read(), None);
    }

    #[test]
    fn atom_port_buffer_rt_pop_into() {
        let buf = AtomPortBuffer::new(0);
        buf.write(b"abcdef");
        let mut small = [0u8; 4];
        assert_eq!(buf.pop_into_rt(&mut small), MsgPop::TooLarge(6));
        let mut out = [0u8; 16];
        assert_eq!(buf.pop_into_rt(&mut out), MsgPop::Data(6));
        assert_eq!(&out[..6], b"abcdef");
        assert_eq!(buf.pop_into_rt(&mut out), MsgPop::Empty);
    }

    #[test]
    fn atom_port_buffer_full_drops() {
        let buf = AtomPortBuffer::with_capacity(0, 16);
        assert!(buf.write_rt(&[1u8; 8]));
        assert!(!buf.write_rt(&[2u8; 8]));
        assert!(buf.discard_rt());
        assert!(buf.write_rt(&[3u8; 8]));
        assert_eq!(buf.read(), Some(vec![3u8; 8]));
    }

    #[test]
    fn atom_port_buffer_listening_clears() {
        let buf = AtomPortBuffer::new(0);
        buf.write(b"stale");
        assert!(!buf.is_listening());
        buf.set_listening(true);
        assert!(buf.is_listening());
        assert_eq!(buf.read(), None);
    }

    #[test]
    fn msg_ring_wraps_around() {
        let (mut p, mut c) = msg_ring(32);
        for i in 0..20u8 {
            assert!(p.push_parts(&[&[i; 3], &[i; 7]]));
            let mut out = [0u8; 10];
            assert_eq!(c.pop_into(&mut out), MsgPop::Data(10));
            assert_eq!(out, [[i; 3].as_slice(), [i; 7].as_slice()].concat().as_slice());
        }
        assert_eq!(c.peek_len(), None);
    }

    // ---- PortUpdates ----

    #[test]
    fn port_updates_snapshot_all() {
        let pu = PortUpdates {
            control_inputs: vec![
                PortSlot {
                    port_index: 0,
                    value: AtomicF32::new(0.1),
                },
                PortSlot {
                    port_index: 1,
                    value: AtomicF32::new(0.2),
                },
            ],
            control_outputs: vec![PortSlot {
                port_index: 2,
                value: AtomicF32::new(0.3),
            }],
            atom_outputs: Vec::new(),
            atom_inputs: Vec::new(),
        };
        let snapshot = pu.snapshot_all();
        assert_eq!(snapshot.len(), 3);
        assert_eq!(snapshot[0].0, 0);
        assert!((snapshot[0].1 - 0.1).abs() < 1e-5);
        assert_eq!(snapshot[1].0, 1);
        assert!((snapshot[1].1 - 0.2).abs() < 1e-5);
        assert_eq!(snapshot[2].0, 2);
        assert!((snapshot[2].1 - 0.3).abs() < 1e-5);
    }

    // ---- PluginFormat ----

    #[test]
    fn plugin_format_as_str() {
        assert_eq!(PluginFormat::Lv2.as_str(), "LV2");
        assert_eq!(PluginFormat::Clap.as_str(), "CLAP");
        assert_eq!(PluginFormat::Vst3.as_str(), "VST3");
    }

    #[test]
    fn plugin_format_display() {
        assert_eq!(format!("{}", PluginFormat::Lv2), "LV2");
        assert_eq!(format!("{}", PluginFormat::Clap), "CLAP");
        assert_eq!(format!("{}", PluginFormat::Vst3), "VST3");
    }

    // ---- PluginPortType ----

    #[test]
    fn port_type_predicates() {
        assert!(PluginPortType::AudioInput.is_audio());
        assert!(PluginPortType::AudioOutput.is_audio());
        assert!(!PluginPortType::ControlInput.is_audio());

        assert!(PluginPortType::ControlInput.is_control());
        assert!(PluginPortType::ControlOutput.is_control());
        assert!(!PluginPortType::AudioInput.is_control());

        assert!(PluginPortType::AudioInput.is_input());
        assert!(PluginPortType::ControlInput.is_input());
        assert!(PluginPortType::AtomInput.is_input());
        assert!(!PluginPortType::AudioOutput.is_input());

        assert!(PluginPortType::AudioOutput.is_output());
        assert!(PluginPortType::ControlOutput.is_output());
        assert!(PluginPortType::AtomOutput.is_output());
        assert!(!PluginPortType::AudioInput.is_output());
    }

    // ---- PluginCategory ----

    #[test]
    fn category_from_class_label() {
        assert_eq!(
            PluginCategory::from_class_label("Amplifier"),
            PluginCategory::Amplifier
        );
        assert_eq!(
            PluginCategory::from_class_label("Analyser"),
            PluginCategory::Analyser
        );
        assert_eq!(
            PluginCategory::from_class_label("Analyzer"),
            PluginCategory::Analyser
        );
        assert_eq!(
            PluginCategory::from_class_label("Chorus"),
            PluginCategory::Chorus
        );
        assert_eq!(
            PluginCategory::from_class_label("Compressor"),
            PluginCategory::Compressor
        );
        assert_eq!(
            PluginCategory::from_class_label("Delay"),
            PluginCategory::Delay
        );
        assert_eq!(
            PluginCategory::from_class_label("Distortion"),
            PluginCategory::Distortion
        );
        assert_eq!(
            PluginCategory::from_class_label("Dynamics"),
            PluginCategory::Dynamics
        );
        assert_eq!(
            PluginCategory::from_class_label("Equaliser"),
            PluginCategory::Equaliser
        );
        assert_eq!(
            PluginCategory::from_class_label("Equalizer"),
            PluginCategory::Equaliser
        );
        assert_eq!(
            PluginCategory::from_class_label("EQ Plugin"),
            PluginCategory::Equaliser
        );
        assert_eq!(
            PluginCategory::from_class_label("Filter"),
            PluginCategory::Filter
        );
        assert_eq!(
            PluginCategory::from_class_label("Instrument"),
            PluginCategory::Instrument
        );
        assert_eq!(
            PluginCategory::from_class_label("Reverb"),
            PluginCategory::Reverb
        );
        assert_eq!(
            PluginCategory::from_class_label("Spatial"),
            PluginCategory::Spatial
        );
        assert_eq!(
            PluginCategory::from_class_label("Utility"),
            PluginCategory::Utility
        );
    }

    #[test]
    fn category_from_class_label_unknown() {
        let cat = PluginCategory::from_class_label("SomethingWeird");
        assert!(matches!(cat, PluginCategory::Other(_)));
    }

    #[test]
    fn category_display_name() {
        assert_eq!(PluginCategory::Reverb.display_name(), "Reverb");
        assert_eq!(PluginCategory::Instrument.display_name(), "Instrument");
        assert_eq!(
            PluginCategory::Other("Custom".to_string()).display_name(),
            "Custom"
        );
    }

    // ---- PluginInfo helpers ----

    #[test]
    fn plugin_info_is_effect() {
        let info = PluginInfo {
            uri: String::new(),
            name: String::new(),
            format: PluginFormat::Vst3,
            category: PluginCategory::Reverb,
            author: None,
            ports: Vec::new(),
            audio_inputs: 2,
            audio_outputs: 2,
            control_inputs: 0,
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: true,
            has_ui: false,
            library_path: String::new(),
        };
        assert!(info.is_effect());
        assert!(!info.is_instrument());
        assert!(!info.is_analyser());
    }

    #[test]
    fn plugin_info_is_instrument() {
        let info = PluginInfo {
            uri: String::new(),
            name: String::new(),
            format: PluginFormat::Vst3,
            category: PluginCategory::Instrument,
            author: None,
            ports: Vec::new(),
            audio_inputs: 0,
            audio_outputs: 2,
            control_inputs: 0,
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: true,
            has_ui: false,
            library_path: String::new(),
        };
        assert!(info.is_instrument());
        assert!(!info.is_effect());
        assert!(!info.is_analyser());
    }

    #[test]
    fn plugin_info_is_analyser() {
        let info = PluginInfo {
            uri: String::new(),
            name: String::new(),
            format: PluginFormat::Vst3,
            category: PluginCategory::Analyser,
            author: None,
            ports: Vec::new(),
            audio_inputs: 2,
            audio_outputs: 0,
            control_inputs: 0,
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: true,
            has_ui: false,
            library_path: String::new(),
        };
        assert!(info.is_analyser());
        assert!(!info.is_effect());
        assert!(!info.is_instrument());
    }

    // ---- SavedPluginInstance default format ----

    #[test]
    fn saved_plugin_instance_default_format_is_lv2() {
        let json =
            r#"{"plugin_uri":"test","display_name":"Test","bypassed":false,"parameters":[]}"#;
        let saved: SavedPluginInstance = serde_json::from_str(json).unwrap();
        assert_eq!(saved.format, "LV2");
    }

    #[test]
    fn saved_plugin_instance_explicit_format() {
        let json = r#"{"plugin_uri":"test","display_name":"Test","bypassed":false,"parameters":[],"format":"VST3"}"#;
        let saved: SavedPluginInstance = serde_json::from_str(json).unwrap();
        assert_eq!(saved.format, "VST3");
    }
}
