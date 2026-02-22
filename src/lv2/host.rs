use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lilv::World;

use super::types::*;
use super::urid::UridMapper;

/// Global instance ID counter
static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Size of each atom port buffer (bytes).  Must be large enough for the
/// plugin to write its visualization / MIDI data.  Most plugins need 8-64 KB.
const ATOM_BUF_SIZE: usize = 65536;

/// Initialise a byte buffer as an empty LV2_Atom_Sequence.
///
/// Memory layout:
/// ```text
/// Offset 0: LV2_Atom       { u32 size, u32 type }               — 8 bytes
/// Offset 8: LV2_Atom_Sequence_Body { u32 unit, u32 pad }        — 8 bytes
/// Offset 16+: (events — empty for a freshly initialised sequence)
/// ```
///
/// For **input** buffers `is_output` should be `false`: `size` is set to 8
/// (just the sequence body header, no events).
///
/// For **output** buffers `is_output` should be `true`: `size` is set to
/// `capacity - 8` which tells the plugin how much space it may write into.
fn init_atom_sequence(buf: &mut [u8], capacity: usize, is_output: bool, sequence_type_urid: u32) {
    assert!(capacity >= 16, "atom buffer too small");
    // Zero the whole buffer first
    buf[..capacity].fill(0);

    let size: u32 = if is_output {
        // Tell the plugin the total writable space (capacity minus the
        // 8-byte LV2_Atom header that holds size+type).
        (capacity - 8) as u32
    } else {
        // Empty sequence: body header only (unit + pad = 8 bytes)
        8
    };

    // LV2_Atom.size  (offset 0, native endian)
    buf[0..4].copy_from_slice(&size.to_ne_bytes());
    // LV2_Atom.type  (offset 4) — URID of atom:Sequence so the plugin
    // recognises this buffer as a valid atom sequence.
    buf[4..8].copy_from_slice(&sequence_type_urid.to_ne_bytes());
    // LV2_Atom_Sequence_Body.unit (offset 8) — 0 = frames
    buf[8..12].copy_from_slice(&0u32.to_ne_bytes());
    // LV2_Atom_Sequence_Body.pad  (offset 12)
    buf[12..16].copy_from_slice(&0u32.to_ne_bytes());
}

/// Holds the state needed to process audio for one LV2 plugin instance.
/// This is owned by the PipeWire filter thread and is NOT Send/Sync
/// (lilv::Instance is !Sync).
pub struct Lv2PluginInstance {
    /// Our internal ID
    pub id: PluginInstanceId,
    /// The activated lilv instance.
    ///
    /// IMPORTANT: `instance` must be declared BEFORE `_world` and
    /// `_urid_map` so that Rust's drop order destroys the instance first
    /// (deactivate + cleanup), then the World / URID map.
    instance: lilv::instance::ActiveInstance,
    /// The lilv World that owns the plugin data.  Must be kept alive for
    /// the entire lifetime of `instance` because the plugin's descriptor,
    /// loaded `.ttl` data, and other internal pointers reference memory
    /// owned by the World.  Dropping the World while the instance is alive
    /// causes use-after-free in the plugin's `run()`/`deactivate()`.
    _world: lilv::World,
    /// Heap-pinned LV2_URID_Map struct.  Plugins store a pointer to this
    /// (via the LV2_Feature passed at instantiation) and call the `map`
    /// callback during `run()`.  Must outlive the instance.
    _urid_map: Box<lv2_raw::urid::LV2UridMap>,
    /// Plugin URI
    pub plugin_uri: String,
    /// Display name
    pub display_name: String,
    /// Audio input port indices (into the LV2 plugin)
    pub audio_input_indices: Vec<usize>,
    /// Audio output port indices (into the LV2 plugin)
    pub audio_output_indices: Vec<usize>,
    /// Control input port indices and their current values
    pub control_inputs: Vec<ControlPort>,
    /// Control output port indices and their storage
    pub control_outputs: Vec<ControlPort>,
    /// Atom input port buffers (reset to empty sequence before each run())
    pub atom_in_bufs: Vec<AtomBuf>,
    /// Atom output port buffers (read after each run() to extract plugin data)
    pub atom_out_bufs: Vec<AtomBuf>,
    /// Lock-free shared port values for UI synchronisation
    pub port_updates: SharedPortUpdates,
    /// URID of `http://lv2plug.in/ns/ext/atom#Sequence` — used to stamp
    /// atom buffer headers so the plugin recognises them.
    atom_sequence_urid: u32,
    /// Whether processing is bypassed
    pub bypassed: bool,
    /// Sample rate
    pub sample_rate: f64,
}

/// Per-port atom buffer that stays at a fixed memory address.
/// Connected to the plugin via `connect_port_mut` once; the plugin
/// reads/writes directly into `data`.
pub struct AtomBuf {
    pub port_index: usize,
    pub data: Vec<u8>,
}

/// A control port with its current value
#[derive(Debug, Clone)]
pub struct ControlPort {
    pub index: usize,
    pub symbol: String,
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl Lv2PluginInstance {
    /// Create a new plugin instance from a lilv plugin descriptor.
    ///
    /// `features` should include at minimum the LV2_URID__map feature, which
    /// is required by most plugins.
    ///
    /// # Safety
    /// This calls into the LV2 plugin's code which may be unsafe.
    pub unsafe fn new(
        world: lilv::World,
        plugin: &lilv::plugin::Plugin,
        plugin_info: &Lv2PluginInfo,
        sample_rate: f64,
        urid_mapper: &UridMapper,
    ) -> Option<Self> {
        let id = next_instance_id();
        let atom_sequence_urid = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Sequence");

        // Heap-allocate the LV2_URID_Map struct so the plugin can safely
        // store the feature pointer and call the map callback later (during
        // run(), activate(), etc.).  Stack-local would be use-after-free.
        let mut urid_map = Box::new(urid_mapper.as_lv2_urid_map());
        let urid_feature = unsafe { UridMapper::make_feature(&mut *urid_map as *mut _) };
        let features = vec![&urid_feature];

        let mut instance = unsafe { plugin.instantiate(sample_rate, features) }?;

        let mut audio_input_indices = Vec::new();
        let mut audio_output_indices = Vec::new();
        let mut control_inputs = Vec::new();
        let mut control_outputs = Vec::new();
        let mut atom_in_bufs = Vec::new();
        let mut atom_out_bufs = Vec::new();

        // Categorize ports — do NOT connect control ports yet because
        // pushing to the Vec may reallocate, invalidating any pointers
        // we gave to the plugin via connect_port_mut.
        for port_info in &plugin_info.ports {
            match port_info.port_type {
                Lv2PortType::AudioInput => {
                    audio_input_indices.push(port_info.index);
                }
                Lv2PortType::AudioOutput => {
                    audio_output_indices.push(port_info.index);
                }
                Lv2PortType::ControlInput => {
                    control_inputs.push(ControlPort {
                        index: port_info.index,
                        symbol: port_info.symbol.clone(),
                        name: port_info.name.clone(),
                        value: port_info.default_value,
                        min: port_info.min_value,
                        max: port_info.max_value,
                        default: port_info.default_value,
                    });
                }
                Lv2PortType::ControlOutput => {
                    control_outputs.push(ControlPort {
                        index: port_info.index,
                        symbol: port_info.symbol.clone(),
                        name: port_info.name.clone(),
                        value: 0.0,
                        min: port_info.min_value,
                        max: port_info.max_value,
                        default: port_info.default_value,
                    });
                }
                Lv2PortType::AtomInput => {
                    atom_in_bufs.push(AtomBuf {
                        port_index: port_info.index,
                        data: vec![0u8; ATOM_BUF_SIZE],
                    });
                }
                Lv2PortType::AtomOutput => {
                    atom_out_bufs.push(AtomBuf {
                        port_index: port_info.index,
                        data: vec![0u8; ATOM_BUF_SIZE],
                    });
                }
            }
        }

        // Now connect control ports — the Vecs are fully built and won't
        // reallocate, so the pointers to each cp.value will remain valid.
        for cp in &mut control_inputs {
            unsafe {
                instance.connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
        for cp in &mut control_outputs {
            unsafe {
                instance.connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }

        // Connect atom ports — each gets its own buffer.
        // Initialize with an empty LV2_Atom_Sequence header.
        for ab in atom_in_bufs.iter_mut() {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, false, atom_sequence_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.data.as_mut_ptr());
            }
        }
        for ab in atom_out_bufs.iter_mut() {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, true, atom_sequence_urid);
            unsafe {
                instance.connect_port_mut(ab.port_index, ab.data.as_mut_ptr());
            }
        }

        // Build the shared port updates buffer
        let port_updates = Arc::new(PortUpdates {
            control_inputs: control_inputs
                .iter()
                .map(|cp| PortSlot {
                    port_index: cp.index,
                    value: AtomicF32::new(cp.value),
                })
                .collect(),
            control_outputs: control_outputs
                .iter()
                .map(|cp| PortSlot {
                    port_index: cp.index,
                    value: AtomicF32::new(cp.value),
                })
                .collect(),
            atom_outputs: atom_out_bufs
                .iter()
                .map(|ab| AtomPortBuffer::new(ab.port_index))
                .collect(),
            atom_inputs: atom_in_bufs
                .iter()
                .map(|ab| AtomPortBuffer::new(ab.port_index))
                .collect(),
        });

        // Activate the instance
        let active_instance = unsafe { instance.activate() };

        Some(Self {
            id,
            instance: active_instance,
            _world: world,
            _urid_map: urid_map,
            plugin_uri: plugin_info.uri.clone(),
            display_name: plugin_info.name.clone(),
            audio_input_indices,
            audio_output_indices,
            control_inputs,
            control_outputs,
            atom_in_bufs,
            atom_out_bufs,
            port_updates,
            atom_sequence_urid,
            bypassed: false,
            sample_rate,
        })
    }

    /// Reconnect control port pointers after the Vec may have been reallocated.
    /// Must be called if control_inputs or control_outputs vectors change.
    ///
    /// # Safety
    /// Calls plugin code.
    pub unsafe fn reconnect_control_ports(&mut self) {
        for cp in &mut self.control_inputs {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
        for cp in &mut self.control_outputs {
            unsafe {
                self.instance
                    .instance_mut()
                    .connect_port_mut(cp.index, &mut cp.value as *mut f32);
            }
        }
    }

    /// Process a block of audio.
    ///
    /// `inputs` and `outputs` are slices of f32 buffers, one per audio port.
    /// The number of buffers must match audio_input_indices.len() and audio_output_indices.len().
    ///
    /// # Safety
    /// Calls plugin's run() method.
    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
    ) {
        if self.bypassed {
            // Pass-through: copy inputs to outputs directly
            for (i, output) in outputs.iter_mut().enumerate() {
                if i < inputs.len() {
                    let copy_len = output.len().min(inputs[i].len()).min(sample_count);
                    output[..copy_len].copy_from_slice(&inputs[i][..copy_len]);
                } else {
                    // No corresponding input - silence
                    for sample in output.iter_mut().take(sample_count) {
                        *sample = 0.0;
                    }
                }
            }
            return;
        }

        // Connect audio input ports
        for (i, &port_idx) in self.audio_input_indices.iter().enumerate() {
            if i < inputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port(port_idx, inputs[i].as_ptr());
                }
            }
        }

        // Connect audio output ports
        for (i, &port_idx) in self.audio_output_indices.iter().enumerate() {
            if i < outputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port_mut(port_idx, outputs[i].as_mut_ptr());
                }
            }
        }

        // Reset atom input buffers to empty sequence, then inject any atom
        // events the UI has sent (e.g. patch:Set messages).
        //
        // The UI sends individual LV2_Atom objects (via port_write_callback
        // with atom:eventTransfer protocol).  We need to wrap each one as an
        // event inside the LV2_Atom_Sequence that the plugin reads.
        //
        // Sequence layout:
        //   [0..4]   LV2_Atom.size  — total body size (Sequence_Body + events)
        //   [4..8]   LV2_Atom.type  — atom:Sequence URID
        //   [8..12]  unit (0 = frames)
        //   [12..16] pad
        //   [16..]   events, each: 8-byte timestamp + LV2_Atom (padded to 8)
        for (ab, shared) in self
            .atom_in_bufs
            .iter_mut()
            .zip(self.port_updates.atom_inputs.iter())
        {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, false, self.atom_sequence_urid);
            // If the UI wrote atom data, inject it as an event at frame 0
            let Some(ui_atom) = shared.read() else {
                continue;
            };
            if !ui_atom.is_empty() && ui_atom.len() >= 8 {
                // We need: 8-byte timestamp + the LV2_Atom data
                let event_size = 8 + ui_atom.len(); // timestamp + atom
                let padded_event_size = (event_size + 7) & !7;
                if 16 + padded_event_size <= ab.data.len() {
                    // Write timestamp = 0 at offset 16
                    ab.data[16..24].copy_from_slice(&0i64.to_ne_bytes());
                    // Write the LV2_Atom (header + body) at offset 24
                    ab.data[24..24 + ui_atom.len()].copy_from_slice(&ui_atom);
                    // Update sequence body size: 8 (Sequence_Body header) + event data
                    let body_size = 8u32 + padded_event_size as u32;
                    ab.data[0..4].copy_from_slice(&body_size.to_ne_bytes());
                }
            }
        }
        // Reset atom output buffers to full capacity so the plugin knows
        // how much space it can write into
        for ab in &mut self.atom_out_bufs {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, true, self.atom_sequence_urid);
        }

        // Run the plugin
        unsafe {
            self.instance.run(sample_count);
        }

        // Push control output values to the shared buffer so the UI can
        // read them (meters, gain reduction, etc.)
        for (cp, slot) in self
            .control_outputs
            .iter()
            .zip(self.port_updates.control_outputs.iter())
        {
            slot.value.store(cp.value);
        }
        // Also publish current control input values (in case host-side
        // automation changed them since the UI last read).
        for (cp, slot) in self
            .control_inputs
            .iter()
            .zip(self.port_updates.control_inputs.iter())
        {
            slot.value.store(cp.value);
        }

        // Copy atom output data to the shared buffers for UI consumption.
        // The plugin writes atom events into the output buffers during run().
        // We read the LV2_Atom.size field to know how much data was written.
        for (ab, shared) in self
            .atom_out_bufs
            .iter()
            .zip(self.port_updates.atom_outputs.iter())
        {
            if ab.data.len() >= 16 {
                // Read LV2_Atom.size (first 4 bytes, native endian)
                let atom_size =
                    u32::from_ne_bytes([ab.data[0], ab.data[1], ab.data[2], ab.data[3]]);
                // Total atom data = 8-byte LV2_Atom header + size bytes
                let total = 8 + atom_size as usize;
                // Only publish if the plugin actually wrote events
                // (size > 8 means there's more than just the empty
                // LV2_Atom_Sequence_Body header)
                if atom_size > 8 && total <= ab.data.len() {
                    shared.write(&ab.data[..total]);
                }
            }
        }
    }

    /// Set a control parameter value by port index
    pub fn set_parameter(&mut self, port_index: usize, value: f32) {
        if let Some(cp) = self
            .control_inputs
            .iter_mut()
            .find(|cp| cp.index == port_index)
        {
            let clamped = value.clamp(cp.min, cp.max);
            cp.value = clamped;

            // Also update the shared port_updates immediately so the
            // native UI timer reads the new value instead of a stale one.
            // Without this, the timer can echo the old value back to the
            // plugin UI which then fires port_write_callback, snapping
            // the parameter back to its previous value.
            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped);
            }
        }
    }

    /// Set a control parameter value by port symbol
    pub fn set_parameter_by_symbol(&mut self, symbol: &str, value: f32) {
        if let Some(idx) = self
            .control_inputs
            .iter()
            .position(|cp| cp.symbol == symbol)
        {
            let cp = &mut self.control_inputs[idx];
            let clamped = value.clamp(cp.min, cp.max);
            cp.value = clamped;
            let port_index = cp.index;

            if let Some(slot) = self
                .port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index)
            {
                slot.value.store(clamped);
            }
        }
    }

    /// Get current parameter values for UI display
    pub fn get_parameters(&self) -> Vec<Lv2ParameterValue> {
        self.control_inputs
            .iter()
            .map(|cp| Lv2ParameterValue {
                port_index: cp.index,
                symbol: cp.symbol.clone(),
                name: cp.name.clone(),
                value: cp.value,
                min: cp.min,
                max: cp.max,
                default: cp.default,
            })
            .collect()
    }

    /// Get instance info for UI display
    pub fn get_info(&self, pw_node_id: Option<u32>) -> Lv2InstanceInfo {
        Lv2InstanceInfo {
            id: self.id,
            stable_id: String::new(), // filled by the UI layer
            plugin_uri: self.plugin_uri.clone(),
            display_name: self.display_name.clone(),
            pw_node_id,
            parameters: self.get_parameters(),
            active: true,
            bypassed: self.bypassed,
        }
    }
}

/// Manager that coordinates plugin scanning and instance lifecycle.
/// Lives on the main/UI thread. Communicates with PipeWire thread
/// via commands.
pub struct Lv2Manager {
    /// The lilv world (for plugin discovery and instantiation)
    world: World,
    /// Cached list of available plugins
    available_plugins: Vec<Lv2PluginInfo>,
    /// Active plugin instances and their PipeWire node IDs
    active_instances: HashMap<PluginInstanceId, Lv2InstanceInfo>,
    /// Default sample rate for new instances
    pub sample_rate: f64,
}

impl Lv2Manager {
    /// Create a new LV2 manager, scanning all installed plugins
    pub fn new() -> Self {
        let world = World::with_load_all();
        let available_plugins = super::scanner::scan_plugins_with_world(&world);

        log::info!("LV2: Found {} plugins", available_plugins.len());

        Self {
            world,
            available_plugins,
            active_instances: HashMap::new(),
            sample_rate: 48000.0,
        }
    }

    /// Get list of all available plugins
    pub fn available_plugins(&self) -> &[Lv2PluginInfo] {
        &self.available_plugins
    }

    /// Rescan plugins
    pub fn rescan(&mut self) {
        self.available_plugins = super::scanner::scan_plugins_with_world(&self.world);
        log::info!(
            "LV2: Rescanned, found {} plugins",
            self.available_plugins.len()
        );
    }

    /// Find a plugin by URI
    pub fn find_plugin(&self, uri: &str) -> Option<&Lv2PluginInfo> {
        self.available_plugins.iter().find(|p| p.uri == uri)
    }

    /// Get the lilv world (needed for instantiation on the PW thread)
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Register that an instance has been created (called after PW thread creates it)
    pub fn register_instance(&mut self, info: Lv2InstanceInfo) {
        self.active_instances.insert(info.id, info);
    }

    /// Update the PipeWire node ID for an instance
    pub fn set_instance_pw_node_id(&mut self, instance_id: PluginInstanceId, pw_node_id: u32) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            info.pw_node_id = Some(pw_node_id);
        }
    }

    /// Remove an instance
    pub fn remove_instance(&mut self, instance_id: PluginInstanceId) {
        self.active_instances.remove(&instance_id);
    }

    /// Update parameter value in our cached state.
    /// If the parameter doesn't exist yet (e.g. initial population from
    /// ParameterChanged events), it is inserted.
    pub fn update_parameter(
        &mut self,
        instance_id: PluginInstanceId,
        port_index: usize,
        value: f32,
    ) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            if let Some(param) = info
                .parameters
                .iter_mut()
                .find(|p| p.port_index == port_index)
            {
                param.value = value;
            } else {
                // Parameter not in our list yet — insert it with minimal info.
                // The symbol/name will be empty but the port_index and value
                // are correct, which is enough for persistence and restore.
                info.parameters.push(super::types::Lv2ParameterValue {
                    port_index,
                    symbol: String::new(),
                    name: String::new(),
                    value,
                    min: 0.0,
                    max: 1.0,
                    default: 0.0,
                });
            }
        }
    }

    /// Get all active instances
    pub fn active_instances(&self) -> &HashMap<PluginInstanceId, Lv2InstanceInfo> {
        &self.active_instances
    }

    /// Get a specific instance
    pub fn get_instance(&self, id: PluginInstanceId) -> Option<&Lv2InstanceInfo> {
        self.active_instances.get(&id)
    }

    /// Get a mutable reference to a specific instance (for renaming, etc.)
    pub fn get_instance_mut(&mut self, id: PluginInstanceId) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances.get_mut(&id)
    }

    /// Find an instance by its stable_id (UUID that persists across sessions).
    pub fn find_by_stable_id(&self, stable_id: &str) -> Option<&Lv2InstanceInfo> {
        self.active_instances
            .values()
            .find(|info| info.stable_id == stable_id)
    }

    /// Find an instance by its stable_id (mutable).
    pub fn find_by_stable_id_mut(&mut self, stable_id: &str) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances
            .values_mut()
            .find(|info| info.stable_id == stable_id)
    }

    /// Find the instance ID for a given stable_id.
    pub fn instance_id_for_stable_id(&self, stable_id: &str) -> Option<PluginInstanceId> {
        self.active_instances
            .iter()
            .find(|(_, info)| info.stable_id == stable_id)
            .map(|(id, _)| *id)
    }
}
