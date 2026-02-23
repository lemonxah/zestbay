use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lilv::World;

use super::types::*;
use super::urid::UridMapper;

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

fn next_instance_id() -> PluginInstanceId {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Clamp a value to [min, max], tolerating NaN bounds (some LV2 plugins
/// have unset min/max ranges).  If min or max is NaN, the value is returned
/// as-is.
fn safe_clamp(value: f32, min: f32, max: f32) -> f32 {
    if min.is_nan() || max.is_nan() {
        return value;
    }
    value.clamp(min, max)
}

const ATOM_BUF_SIZE: usize = 65536;

fn init_atom_sequence(buf: &mut [u8], capacity: usize, is_output: bool, sequence_type_urid: u32) {
    assert!(capacity >= 16, "atom buffer too small");
    buf[..capacity].fill(0);

    let size: u32 = if is_output { (capacity - 8) as u32 } else { 8 };

    buf[0..4].copy_from_slice(&size.to_ne_bytes());
    buf[4..8].copy_from_slice(&sequence_type_urid.to_ne_bytes());
    buf[8..12].copy_from_slice(&0u32.to_ne_bytes());
    buf[12..16].copy_from_slice(&0u32.to_ne_bytes());
}

pub struct Lv2PluginInstance {
    pub id: PluginInstanceId,
    instance: lilv::instance::ActiveInstance,
    _world: lilv::World,
    _urid_map: Box<lv2_raw::urid::LV2UridMap>,
    pub plugin_uri: String,
    pub display_name: String,
    pub audio_input_indices: Vec<usize>,
    pub audio_output_indices: Vec<usize>,
    pub control_inputs: Vec<ControlPort>,
    pub control_outputs: Vec<ControlPort>,
    pub atom_in_bufs: Vec<AtomBuf>,
    pub atom_out_bufs: Vec<AtomBuf>,
    pub port_updates: SharedPortUpdates,
    atom_sequence_urid: u32,
    pub bypassed: bool,
    pub sample_rate: f64,
}

pub struct AtomBuf {
    pub port_index: usize,
    pub data: Vec<u8>,
}

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
    pub unsafe fn new(
        world: lilv::World,
        plugin: &lilv::plugin::Plugin,
        plugin_info: &Lv2PluginInfo,
        sample_rate: f64,
        urid_mapper: &UridMapper,
    ) -> Option<Self> {
        let id = next_instance_id();
        let atom_sequence_urid = urid_mapper.map("http://lv2plug.in/ns/ext/atom#Sequence");

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

    pub unsafe fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        sample_count: usize,
    ) {
        if self.bypassed {
            for (i, output) in outputs.iter_mut().enumerate() {
                if i < inputs.len() {
                    let copy_len = output.len().min(inputs[i].len()).min(sample_count);
                    output[..copy_len].copy_from_slice(&inputs[i][..copy_len]);
                } else {
                    for sample in output.iter_mut().take(sample_count) {
                        *sample = 0.0;
                    }
                }
            }
            return;
        }

        for (i, &port_idx) in self.audio_input_indices.iter().enumerate() {
            if i < inputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port(port_idx, inputs[i].as_ptr());
                }
            }
        }

        for (i, &port_idx) in self.audio_output_indices.iter().enumerate() {
            if i < outputs.len() {
                unsafe {
                    self.instance
                        .instance_mut()
                        .connect_port_mut(port_idx, outputs[i].as_mut_ptr());
                }
            }
        }

        for (ab, shared) in self
            .atom_in_bufs
            .iter_mut()
            .zip(self.port_updates.atom_inputs.iter())
        {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, false, self.atom_sequence_urid);
            let Some(ui_atom) = shared.read() else {
                continue;
            };
            if !ui_atom.is_empty() && ui_atom.len() >= 8 {
                let event_size = 8 + ui_atom.len();
                let padded_event_size = (event_size + 7) & !7;
                if 16 + padded_event_size <= ab.data.len() {
                    ab.data[16..24].copy_from_slice(&0i64.to_ne_bytes());
                    ab.data[24..24 + ui_atom.len()].copy_from_slice(&ui_atom);
                    let body_size = 8u32 + padded_event_size as u32;
                    ab.data[0..4].copy_from_slice(&body_size.to_ne_bytes());
                }
            }
        }

        for ab in &mut self.atom_out_bufs {
            init_atom_sequence(&mut ab.data, ATOM_BUF_SIZE, true, self.atom_sequence_urid);
        }

        unsafe {
            self.instance.run(sample_count);
        }

        for (cp, slot) in self
            .control_outputs
            .iter()
            .zip(self.port_updates.control_outputs.iter())
        {
            slot.value.store(cp.value);
        }

        for (cp, slot) in self
            .control_inputs
            .iter()
            .zip(self.port_updates.control_inputs.iter())
        {
            slot.value.store(cp.value);
        }

        for (ab, shared) in self
            .atom_out_bufs
            .iter()
            .zip(self.port_updates.atom_outputs.iter())
        {
            if ab.data.len() >= 16 {
                let atom_size =
                    u32::from_ne_bytes([ab.data[0], ab.data[1], ab.data[2], ab.data[3]]);
                let total = 8 + atom_size as usize;
                if atom_size > 8 && total <= ab.data.len() {
                    shared.write(&ab.data[..total]);
                }
            }
        }
    }

    pub fn set_parameter(&mut self, port_index: usize, value: f32) {
        if let Some(cp) = self
            .control_inputs
            .iter_mut()
            .find(|cp| cp.index == port_index)
        {
            let clamped = safe_clamp(value, cp.min, cp.max);
            cp.value = clamped;

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

    pub fn set_parameter_by_symbol(&mut self, symbol: &str, value: f32) {
        if let Some(idx) = self
            .control_inputs
            .iter()
            .position(|cp| cp.symbol == symbol)
        {
            let cp = &mut self.control_inputs[idx];
            let clamped = safe_clamp(value, cp.min, cp.max);
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

    pub fn get_info(&self, pw_node_id: Option<u32>) -> Lv2InstanceInfo {
        Lv2InstanceInfo {
            id: self.id,
            stable_id: String::new(),
            plugin_uri: self.plugin_uri.clone(),
            format: PluginFormat::Lv2,
            display_name: self.display_name.clone(),
            pw_node_id,
            parameters: self.get_parameters(),
            active: true,
            bypassed: self.bypassed,
        }
    }
}

pub struct Lv2Manager {
    world: World,
    available_plugins: Vec<Lv2PluginInfo>,
    active_instances: HashMap<PluginInstanceId, Lv2InstanceInfo>,
    pub sample_rate: f64,
}

impl Lv2Manager {
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

    pub fn available_plugins(&self) -> &[Lv2PluginInfo] {
        &self.available_plugins
    }

    pub fn rescan(&mut self) {
        self.available_plugins = super::scanner::scan_plugins_with_world(&self.world);
        log::info!(
            "LV2: Rescanned, found {} plugins",
            self.available_plugins.len()
        );
    }

    pub fn find_plugin(&self, uri: &str) -> Option<&Lv2PluginInfo> {
        self.available_plugins.iter().find(|p| p.uri == uri)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn register_instance(&mut self, info: Lv2InstanceInfo) {
        self.active_instances.insert(info.id, info);
    }

    pub fn set_instance_pw_node_id(&mut self, instance_id: PluginInstanceId, pw_node_id: u32) {
        if let Some(info) = self.active_instances.get_mut(&instance_id) {
            info.pw_node_id = Some(pw_node_id);
        }
    }

    pub fn remove_instance(&mut self, instance_id: PluginInstanceId) {
        self.active_instances.remove(&instance_id);
    }

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

    pub fn active_instances(&self) -> &HashMap<PluginInstanceId, Lv2InstanceInfo> {
        &self.active_instances
    }

    pub fn get_instance(&self, id: PluginInstanceId) -> Option<&Lv2InstanceInfo> {
        self.active_instances.get(&id)
    }

    pub fn get_instance_mut(&mut self, id: PluginInstanceId) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances.get_mut(&id)
    }

    pub fn find_by_stable_id(&self, stable_id: &str) -> Option<&Lv2InstanceInfo> {
        self.active_instances
            .values()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn find_by_stable_id_mut(&mut self, stable_id: &str) -> Option<&mut Lv2InstanceInfo> {
        self.active_instances
            .values_mut()
            .find(|info| info.stable_id == stable_id)
    }

    pub fn instance_id_for_stable_id(&self, stable_id: &str) -> Option<PluginInstanceId> {
        self.active_instances
            .iter()
            .find(|(_, info)| info.stable_id == stable_id)
            .map(|(id, _)| *id)
    }
}
