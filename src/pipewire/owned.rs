//! Nodes ZestBay creates in PipeWire itself (not plugins): subgraph boundary
//! pass-through filters and loopback pairs (subgraphs shown as devices, and
//! user virtual devices). Lives on the PipeWire thread.

use std::ffi::CString;

use pipewire::context::ContextRc;
use pipewire::core::CoreRc;

use super::types::{LoopbackSpec, OwnedNodeSpec, PassSpec};

/// A live owned node; dropping it removes the node from PipeWire.
#[allow(dead_code)] // held for its Drop
pub enum OwnedNode {
    Pass(PassNode),
    Loopback(LoopbackModule),
}

impl OwnedNode {
    pub fn create(
        context: &ContextRc,
        core: &CoreRc,
        spec: &OwnedNodeSpec,
    ) -> Result<Self, String> {
        match spec {
            OwnedNodeSpec::Pass(s) => PassNode::new(core, s).map(OwnedNode::Pass),
            OwnedNodeSpec::Loopback(s) => LoopbackModule::new(context, s).map(OwnedNode::Loopback),
        }
    }
}

// ---------------------------------------------------------------------------
// Pass-through filter
// ---------------------------------------------------------------------------

/// User data of the pass-through filter: port pointers, written only before
/// the filter connects and then read by the RT `process` callback.
struct PassData {
    inputs: Vec<*mut std::ffi::c_void>,
    outputs: Vec<*mut std::ffi::c_void>,
    midi_in: *mut std::ffi::c_void,
    midi_out: *mut std::ffi::c_void,
}

pub struct PassNode {
    filter: *mut pipewire::sys::pw_filter,
    _hook: Box<libspa::sys::spa_hook>,
    _events: Box<pipewire::sys::pw_filter_events>,
    data: *mut PassData,
    _core: CoreRc,
}

impl PassNode {
    fn new(core: &CoreRc, spec: &PassSpec) -> Result<Self, String> {
        let name = cstring(&spec.node_name);
        let props = unsafe {
            let p = pipewire::sys::pw_properties_new(std::ptr::null::<std::os::raw::c_char>());
            let set = |k: &str, v: &str| {
                let (k, v) = (cstring(k), cstring(v));
                pipewire::sys::pw_properties_set(p, k.as_ptr(), v.as_ptr());
            };
            set("media.type", "Audio");
            set("media.category", "Filter");
            set("media.role", "DSP");
            set("node.virtual", "true");
            set("node.name", &spec.node_name);
            set("node.description", &spec.description);
            for (k, v) in &spec.props {
                set(k, v);
            }
            p
        };

        let filter = unsafe { pipewire::sys::pw_filter_new(core.as_raw_ptr(), name.as_ptr(), props) };
        if filter.is_null() {
            return Err(format!("{}: pw_filter_new failed", spec.node_name));
        }

        let mut data = Box::new(PassData {
            inputs: Vec::with_capacity(spec.channels.len()),
            outputs: Vec::with_capacity(spec.channels.len()),
            midi_in: std::ptr::null_mut(),
            midi_out: std::ptr::null_mut(),
        });
        const AUDIO: &[u8] = b"32 bit float mono audio\0";
        const MIDI: &[u8] = b"8 bit raw midi\0";
        unsafe {
            for ch in &spec.channels {
                data.inputs.push(add_port(filter, libspa::sys::SPA_DIRECTION_INPUT, &format!("in_{ch}"), Some(ch), AUDIO));
                data.outputs.push(add_port(filter, libspa::sys::SPA_DIRECTION_OUTPUT, &format!("out_{ch}"), Some(ch), AUDIO));
            }
            if spec.midi {
                data.midi_in = add_port(filter, libspa::sys::SPA_DIRECTION_INPUT, "midi_in", None, MIDI);
                data.midi_out = add_port(filter, libspa::sys::SPA_DIRECTION_OUTPUT, "midi_out", None, MIDI);
            }
        }
        let data = Box::into_raw(data);

        let events = Box::new(pipewire::sys::pw_filter_events {
            version: pipewire::sys::PW_VERSION_FILTER_EVENTS,
            destroy: None,
            state_changed: None,
            io_changed: None,
            param_changed: None,
            add_buffer: None,
            remove_buffer: None,
            process: Some(on_pass_process),
            drained: None,
            command: None,
        });
        let mut hook = Box::new(unsafe { std::mem::zeroed::<libspa::sys::spa_hook>() });
        unsafe {
            pipewire::sys::pw_filter_add_listener(
                filter,
                hook.as_mut() as *mut libspa::sys::spa_hook,
                events.as_ref() as *const pipewire::sys::pw_filter_events,
                data as *mut std::ffi::c_void,
            );
        }

        let flags = pipewire::sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS;
        let ret = unsafe { pipewire::sys::pw_filter_connect(filter, flags, std::ptr::null_mut(), 0) };
        if ret < 0 {
            unsafe {
                pipewire::sys::pw_filter_destroy(filter);
                drop(Box::from_raw(data));
            }
            return Err(format!("{}: pw_filter_connect failed ({ret})", spec.node_name));
        }

        log::info!(
            "Pass-through node created: {} ({} channels{})",
            spec.description,
            spec.channels.len(),
            if spec.midi { " + MIDI" } else { "" }
        );
        Ok(Self {
            filter,
            _hook: hook,
            _events: events,
            data,
            _core: core.clone(),
        })
    }
}

impl Drop for PassNode {
    fn drop(&mut self) {
        unsafe {
            if !self.filter.is_null() {
                pipewire::sys::pw_filter_destroy(self.filter);
                self.filter = std::ptr::null_mut();
            }
            if !self.data.is_null() {
                drop(Box::from_raw(self.data));
                self.data = std::ptr::null_mut();
            }
        }
    }
}

/// # Safety
/// `filter` must be a valid, not yet connected pw_filter.
unsafe fn add_port(
    filter: *mut pipewire::sys::pw_filter,
    direction: libspa::sys::spa_direction,
    name: &str,
    channel: Option<&str>,
    format: &[u8],
) -> *mut std::ffi::c_void {
    unsafe {
        let props = pipewire::sys::pw_properties_new(std::ptr::null::<std::os::raw::c_char>());
        let set = |k: &str, v: &str| {
            let (k, v) = (cstring(k), cstring(v));
            pipewire::sys::pw_properties_set(props, k.as_ptr(), v.as_ptr());
        };
        set("port.name", name);
        if let Some(ch) = channel {
            set("audio.channel", ch);
        }
        let fmt = CString::from_vec_with_nul(format.to_vec()).unwrap_or_default();
        pipewire::sys::pw_properties_set(props, c"format.dsp".as_ptr(), fmt.as_ptr());
        let port = pipewire::sys::pw_filter_add_port(
            filter,
            direction,
            pipewire::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
            0,
            props,
            std::ptr::null_mut(),
            0,
        );
        if port.is_null() {
            log::error!("Pass-through node: failed to add port {name}");
        }
        port
    }
}

unsafe extern "C" fn on_pass_process(
    data: *mut std::ffi::c_void,
    position: *mut libspa::sys::spa_io_position,
) {
    unsafe {
        if data.is_null() || position.is_null() {
            return;
        }
        let d = &*(data as *const PassData);
        let n = (*position).clock.duration as u32;
        for (inp, out) in d.inputs.iter().zip(&d.outputs) {
            if out.is_null() {
                continue;
            }
            let dst = pipewire::sys::pw_filter_get_dsp_buffer(*out, n) as *mut f32;
            if dst.is_null() {
                continue;
            }
            let src = if inp.is_null() {
                std::ptr::null_mut()
            } else {
                pipewire::sys::pw_filter_get_dsp_buffer(*inp, n) as *const f32
            };
            if src.is_null() {
                std::ptr::write_bytes(dst, 0, n as usize);
            } else {
                std::ptr::copy_nonoverlapping(src, dst, n as usize);
            }
        }
        if !d.midi_out.is_null() {
            let midi = if d.midi_in.is_null() {
                std::ptr::null_mut()
            } else {
                pipewire::sys::pw_filter_get_dsp_buffer(d.midi_in, 0)
            };
            crate::midi::processing::write_midi_output(d.midi_out, midi);
        }
    }
}

// ---------------------------------------------------------------------------
// Loopback
// ---------------------------------------------------------------------------

pub struct LoopbackModule {
    module: *mut pipewire::sys::pw_impl_module,
}

impl LoopbackModule {
    fn new(context: &ContextRc, spec: &LoopbackSpec) -> Result<Self, String> {
        let args = cstring(&loopback_args(spec));
        let module = unsafe {
            pipewire::sys::pw_context_load_module(
                context.as_raw_ptr(),
                c"libpipewire-module-loopback".as_ptr(),
                args.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        if module.is_null() {
            return Err(format!(
                "{}: could not load libpipewire-module-loopback ({})",
                spec.description,
                std::io::Error::last_os_error()
            ));
        }
        log::info!("Loopback created: {} ({} channels)", spec.description, spec.channels.len());
        Ok(Self { module })
    }
}

impl Drop for LoopbackModule {
    fn drop(&mut self) {
        if !self.module.is_null() {
            unsafe { pipewire::sys::pw_impl_module_destroy(self.module) };
            self.module = std::ptr::null_mut();
        }
    }
}

/// SPA-JSON arguments for `libpipewire-module-loopback`.
pub fn loopback_args(spec: &LoopbackSpec) -> String {
    let props = |list: &[(String, String)]| -> String {
        list.iter()
            .map(|(k, v)| format!("{} = {}", spa_key(k), spa_string(v)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "{{ node.description = {} audio.channels = {} audio.position = [ {} ] \
         capture.props = {{ {} }} playback.props = {{ {} }} }}",
        spa_string(&spec.description),
        spec.channels.len(),
        spec.channels.join(" "),
        props(&spec.capture_props),
        props(&spec.playback_props),
    )
}

/// A quoted SPA-JSON string.
fn spa_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Property keys are plain identifiers; drop anything else.
fn spa_key(k: &str) -> String {
    k.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect()
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_args_quote_and_escape() {
        let spec = LoopbackSpec {
            description: "Mic \"Chain\"".into(),
            channels: vec!["FL".into(), "FR".into()],
            capture_props: vec![("media.class".into(), "Audio/Sink".into())],
            playback_props: vec![("node.autoconnect".into(), "false".into()), ("bad key!".into(), "x".into())],
        };
        let a = loopback_args(&spec);
        assert!(a.contains("node.description = \"Mic \\\"Chain\\\"\""));
        assert!(a.contains("audio.channels = 2"));
        assert!(a.contains("audio.position = [ FL FR ]"));
        assert!(a.contains("capture.props = { media.class = \"Audio/Sink\" }"));
        assert!(a.contains("node.autoconnect = \"false\""));
        assert!(a.contains("badkey = \"x\""));
    }
}
