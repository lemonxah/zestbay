use std::cell::UnsafeCell;
use std::ffi::c_ulong;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::abi::{self, *};
use crate::clap::threading;
use crate::midi::processing::RawMidiEvent;
use crate::midi::rt::MAX_BLOCK;
use crate::plugin::types::*;

#[derive(Clone)]
pub struct LadspaParam {
    pub port_index: usize,
    pub name: String,
    pub meta: ControlMeta,
}

#[derive(Clone, Copy)]
enum PortRole {
    AudioIn(usize),
    AudioOut(usize),
    ControlIn(usize),
    ControlOut(usize),
    Unused,
}

struct AudioState {
    ctrl_in: Box<[f32]>,
    ctrl_out: Box<[f32]>,
    silence: Box<[f32]>,
    sink: Box<[f32]>,
}

pub struct LadspaPluginInstance {
    _library: Library,
    desc: *const Descriptor,
    handle: Handle,
    #[allow(dead_code)]
    pub display_name: String,
    pub audio_input_channels: usize,
    pub audio_output_channels: usize,
    pub has_midi_in: bool,
    pub params: Vec<LadspaParam>,
    pub port_updates: SharedPortUpdates,
    pub sample_rate: f64,
    roles: Vec<PortRole>,
    bypassed: AtomicBool,
    audio: UnsafeCell<AudioState>,
}

impl LadspaPluginInstance {
    pub unsafe fn new(
        library_path: &str,
        plugin_id: &str,
        info: &PluginInfo,
        sample_rate: f64,
        _state: Option<&[u8]>,
    ) -> Option<Self> {
        threading::set_main_thread();
        let label = super::scanner::parse_label(plugin_id)?;
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return None;
        }
        unsafe {
            let library = Library::open(Path::new(library_path))?;
            let mut desc = std::ptr::null();
            for index in 0..MAX_DESCRIPTORS {
                let d = library.descriptor(index);
                if d.is_null() {
                    break;
                }
                if abi::cstr((*d).label) == label {
                    desc = d;
                    break;
                }
            }
            if desc.is_null() {
                return None;
            }
            let d = &*desc;
            let port_count = d.port_count as usize;
            if port_count > MAX_PORTS
                || (port_count > 0
                    && (d.port_descriptors.is_null()
                        || d.port_names.is_null()
                        || d.port_range_hints.is_null()))
            {
                return None;
            }
            let (instantiate, connect_port) = (d.instantiate?, d.connect_port?);
            d.run?;

            let mut roles = Vec::with_capacity(port_count);
            let mut params = Vec::new();
            let (mut ai, mut ao, mut ci, mut co) = (0usize, 0usize, 0usize, 0usize);
            let mut out_slots = Vec::new();
            for i in 0..port_count {
                let flags = *d.port_descriptors.add(i);
                let input = flags & PORT_INPUT != 0;
                let output = flags & PORT_OUTPUT != 0;
                let audio = flags & PORT_AUDIO != 0;
                let control = flags & PORT_CONTROL != 0;
                if input == output || audio == control {
                    roles.push(PortRole::Unused);
                    continue;
                }
                let name = abi::cstr(*d.port_names.add(i));
                match (audio, input) {
                    (true, true) => {
                        roles.push(PortRole::AudioIn(ai));
                        ai += 1;
                    }
                    (true, false) => {
                        roles.push(PortRole::AudioOut(ao));
                        ao += 1;
                    }
                    (false, true) => {
                        roles.push(PortRole::ControlIn(ci));
                        ci += 1;
                        params.push(LadspaParam {
                            port_index: i,
                            name: if name.is_empty() { format!("Port {}", i + 1) } else { name },
                            meta: control_meta(*d.port_range_hints.add(i), sample_rate),
                        });
                    }
                    (false, false) => {
                        roles.push(PortRole::ControlOut(co));
                        co += 1;
                        out_slots.push(i);
                    }
                }
            }

            let handle = instantiate(desc, sample_rate as c_ulong);
            if handle.is_null() {
                return None;
            }

            let mut audio = AudioState {
                ctrl_in: params.iter().map(|p| p.meta.default).collect(),
                ctrl_out: vec![0.0; co].into_boxed_slice(),
                silence: vec![0.0; MAX_BLOCK].into_boxed_slice(),
                sink: vec![0.0; MAX_BLOCK].into_boxed_slice(),
            };
            for (i, role) in roles.iter().enumerate() {
                let ptr: *mut f32 = match *role {
                    PortRole::ControlIn(k) => &mut audio.ctrl_in[k],
                    PortRole::ControlOut(k) => &mut audio.ctrl_out[k],
                    PortRole::AudioIn(_) => audio.silence.as_mut_ptr(),
                    PortRole::AudioOut(_) | PortRole::Unused => audio.sink.as_mut_ptr(),
                };
                connect_port(handle, i as c_ulong, ptr);
            }
            if let Some(activate) = d.activate {
                activate(handle);
            }

            let port_updates = Arc::new(PortUpdates {
                control_inputs: params
                    .iter()
                    .map(|p| PortSlot {
                        port_index: p.port_index,
                        value: AtomicF32::new(p.meta.default),
                    })
                    .collect(),
                control_outputs: out_slots
                    .iter()
                    .map(|&i| PortSlot {
                        port_index: i,
                        value: AtomicF32::new(0.0),
                    })
                    .collect(),
                atom_inputs: Vec::new(),
                atom_outputs: Vec::new(),
            });

            Some(Self {
                _library: library,
                desc,
                handle,
                display_name: info.name.clone(),
                audio_input_channels: ai,
                audio_output_channels: ao,
                has_midi_in: false,
                params,
                port_updates,
                sample_rate,
                roles,
                bypassed: AtomicBool::new(false),
                audio: UnsafeCell::new(audio),
            })
        }
    }

    pub fn set_parameter(&self, port_index: usize, value: f32) {
        if !value.is_finite() {
            return;
        }
        if let (Some(p), Some(slot)) = (
            self.params.iter().find(|p| p.port_index == port_index),
            self.port_updates
                .control_inputs
                .iter()
                .find(|s| s.port_index == port_index),
        ) {
            slot.value.store(value.clamp(p.meta.min, p.meta.max));
        }
    }

    pub fn set_bypassed(&self, bypassed: bool) {
        self.bypassed.store(bypassed, Ordering::Relaxed);
    }

    pub fn is_bypassed(&self) -> bool {
        self.bypassed.load(Ordering::Relaxed)
    }

    pub fn get_parameters(&self) -> Vec<ParameterValue> {
        self.params
            .iter()
            .zip(&self.port_updates.control_inputs)
            .map(|(p, slot)| ParameterValue {
                port_index: p.port_index,
                symbol: format!("port_{}", p.port_index),
                name: p.name.clone(),
                value: slot.value.load(),
                min: p.meta.min,
                max: p.meta.max,
                default: p.meta.default,
                is_toggle: p.meta.toggled,
            })
            .collect()
    }

    pub unsafe fn process(
        &self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        n: usize,
        _midi: &[RawMidiEvent],
    ) {
        threading::mark_audio_thread();
        if n == 0 {
            return;
        }
        if self.is_bypassed() {
            threading::passthrough(inputs, outputs, n);
            return;
        }
        let audio = unsafe { &mut *self.audio.get() };
        let d = unsafe { &*self.desc };
        let (Some(connect_port), Some(run)) = (d.connect_port, d.run) else {
            threading::passthrough(inputs, outputs, n);
            return;
        };

        for (k, p) in self.params.iter().enumerate() {
            let v = self.port_updates.control_inputs[k].value.load();
            audio.ctrl_in[k] = if v.is_finite() { v.clamp(p.meta.min, p.meta.max) } else { p.meta.default };
        }

        let mut start = 0;
        while start < n {
            let len = (n - start).min(MAX_BLOCK);
            for (i, role) in self.roles.iter().enumerate() {
                let ptr: *mut f32 = match *role {
                    PortRole::AudioIn(ch) => match inputs.get(ch) {
                        Some(buf) if buf.len() >= start + len => {
                            buf[start..].as_ptr() as *mut f32
                        }
                        _ => audio.silence.as_mut_ptr(),
                    },
                    PortRole::AudioOut(ch) => match outputs.get_mut(ch) {
                        Some(buf) if buf.len() >= start + len => buf[start..].as_mut_ptr(),
                        _ => audio.sink.as_mut_ptr(),
                    },
                    _ => continue,
                };
                unsafe { connect_port(self.handle, i as c_ulong, ptr) };
            }
            unsafe { run(self.handle, len as c_ulong) };
            start += len;
        }

        for out in outputs.iter_mut().skip(self.audio_output_channels) {
            let m = out.len().min(n);
            out[..m].fill(0.0);
        }
        for (k, slot) in self.port_updates.control_outputs.iter().enumerate() {
            let v = audio.ctrl_out[k];
            if v.is_finite() {
                slot.value.store(v);
            }
        }
    }
}

impl Drop for LadspaPluginInstance {
    fn drop(&mut self) {
        unsafe {
            let d = &*self.desc;
            let audio = &mut *self.audio.get();
            if let Some(connect_port) = d.connect_port {
                for (i, role) in self.roles.iter().enumerate() {
                    match *role {
                        PortRole::AudioIn(_) => {
                            connect_port(self.handle, i as c_ulong, audio.silence.as_mut_ptr())
                        }
                        PortRole::AudioOut(_) => {
                            connect_port(self.handle, i as c_ulong, audio.sink.as_mut_ptr())
                        }
                        _ => {}
                    }
                }
            }
            if let Some(deactivate) = d.deactivate {
                deactivate(self.handle);
            }
            if let Some(cleanup) = d.cleanup {
                cleanup(self.handle);
            }
        }
    }
}
