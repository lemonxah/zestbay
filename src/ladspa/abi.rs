//! LADSPA 1.1 ABI (`ladspa.h`) and port metadata helpers shared by the
//! scanner and the host.

use std::ffi::{CStr, c_char, c_int, c_ulong, c_void};

pub const PORT_INPUT: c_int = 0x1;
pub const PORT_OUTPUT: c_int = 0x2;
pub const PORT_CONTROL: c_int = 0x4;
pub const PORT_AUDIO: c_int = 0x8;

pub const HINT_BOUNDED_BELOW: c_int = 0x1;
pub const HINT_BOUNDED_ABOVE: c_int = 0x2;
pub const HINT_TOGGLED: c_int = 0x4;
pub const HINT_SAMPLE_RATE: c_int = 0x8;
pub const HINT_LOGARITHMIC: c_int = 0x10;
pub const HINT_INTEGER: c_int = 0x20;
const HINT_DEFAULT_MASK: c_int = 0x3C0;
const HINT_DEFAULT_MINIMUM: c_int = 0x40;
const HINT_DEFAULT_LOW: c_int = 0x80;
const HINT_DEFAULT_MIDDLE: c_int = 0xC0;
const HINT_DEFAULT_HIGH: c_int = 0x100;
const HINT_DEFAULT_MAXIMUM: c_int = 0x140;
const HINT_DEFAULT_0: c_int = 0x200;
const HINT_DEFAULT_1: c_int = 0x240;
const HINT_DEFAULT_100: c_int = 0x280;
const HINT_DEFAULT_440: c_int = 0x2C0;

pub type Handle = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PortRangeHint {
    pub hint: c_int,
    pub lower: f32,
    pub upper: f32,
}

#[repr(C)]
pub struct Descriptor {
    pub unique_id: c_ulong,
    pub label: *const c_char,
    pub properties: c_int,
    pub name: *const c_char,
    pub maker: *const c_char,
    pub copyright: *const c_char,
    pub port_count: c_ulong,
    pub port_descriptors: *const c_int,
    pub port_names: *const *const c_char,
    pub port_range_hints: *const PortRangeHint,
    pub implementation_data: *mut c_void,
    pub instantiate: Option<unsafe extern "C" fn(*const Descriptor, c_ulong) -> Handle>,
    pub connect_port: Option<unsafe extern "C" fn(Handle, c_ulong, *mut f32)>,
    pub activate: Option<unsafe extern "C" fn(Handle)>,
    pub run: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub run_adding: Option<unsafe extern "C" fn(Handle, c_ulong)>,
    pub set_run_adding_gain: Option<unsafe extern "C" fn(Handle, f32)>,
    pub deactivate: Option<unsafe extern "C" fn(Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(Handle)>,
}

pub type DescriptorFn = unsafe extern "C" fn(c_ulong) -> *const Descriptor;

/// A dlopen'ed LADSPA library. Closed on drop.
pub struct Library {
    handle: *mut c_void,
    entry: DescriptorFn,
}

impl Library {
    /// # Safety
    /// Runs the library's initialisers; the module must match this process's
    /// architecture.
    pub unsafe fn open(path: &std::path::Path) -> Option<Self> {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            log::debug!("LADSPA: cannot load {}", path.display());
            return None;
        }
        let sym = unsafe { libc::dlsym(handle, c"ladspa_descriptor".as_ptr()) };
        if sym.is_null() {
            unsafe { libc::dlclose(handle) };
            return None;
        }
        let entry: DescriptorFn = unsafe { std::mem::transmute(sym) };
        Some(Self { handle, entry })
    }

    /// Descriptor `index`, or null past the end of the library.
    ///
    /// # Safety
    /// Calls plugin code. The pointer is valid while `self` lives.
    pub unsafe fn descriptor(&self, index: usize) -> *const Descriptor {
        unsafe { (self.entry)(index as c_ulong) }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.handle) };
    }
}

/// Upper bound on descriptors per library, against buggy plugins that never
/// return null.
pub const MAX_DESCRIPTORS: usize = 4096;
/// Upper bound on ports per plugin.
pub const MAX_PORTS: usize = 1024;

pub unsafe fn cstr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }
}

/// Range, default and flags of a control port, resolved for `sample_rate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlMeta {
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub toggled: bool,
    pub logarithmic: bool,
    pub steps: u32,
}

/// Resolve the range hints of a control port per `ladspa.h`.
pub fn control_meta(hint: PortRangeHint, sample_rate: f64) -> ControlMeta {
    let h = hint.hint;
    let scale = if h & HINT_SAMPLE_RATE != 0 { sample_rate as f32 } else { 1.0 };
    let below = h & HINT_BOUNDED_BELOW != 0;
    let above = h & HINT_BOUNDED_ABOVE != 0;
    let lo = if below && hint.lower.is_finite() { Some(hint.lower * scale) } else { None };
    let hi = if above && hint.upper.is_finite() { Some(hint.upper * scale) } else { None };
    let toggled = h & HINT_TOGGLED != 0;
    let logarithmic = h & HINT_LOGARITHMIC != 0;

    let blend = |wl: f32, wh: f32, lo: f32, hi: f32| -> f32 {
        if logarithmic && lo > 0.0 && hi > 0.0 {
            (lo.ln() * wl + hi.ln() * wh).exp()
        } else {
            lo * wl + hi * wh
        }
    };
    let default = match (h & HINT_DEFAULT_MASK, lo, hi) {
        (HINT_DEFAULT_MINIMUM, Some(lo), _) => Some(lo),
        (HINT_DEFAULT_MAXIMUM, _, Some(hi)) => Some(hi),
        (HINT_DEFAULT_LOW, Some(lo), Some(hi)) => Some(blend(0.75, 0.25, lo, hi)),
        (HINT_DEFAULT_MIDDLE, Some(lo), Some(hi)) => Some(blend(0.5, 0.5, lo, hi)),
        (HINT_DEFAULT_HIGH, Some(lo), Some(hi)) => Some(blend(0.25, 0.75, lo, hi)),
        (HINT_DEFAULT_0, _, _) => Some(0.0),
        (HINT_DEFAULT_1, _, _) => Some(1.0),
        (HINT_DEFAULT_100, _, _) => Some(100.0),
        (HINT_DEFAULT_440, _, _) => Some(440.0),
        _ => None,
    };

    let (mut min, mut max) = if toggled {
        (lo.unwrap_or(0.0), hi.unwrap_or(1.0))
    } else {
        (lo.unwrap_or(f32::NAN), hi.unwrap_or(f32::NAN))
    };
    let mut default = default.unwrap_or(f32::NAN);
    // Unbounded ends and missing defaults get usable values.
    if default.is_nan() {
        default = if min.is_finite() {
            min.max(0.0).min(if max.is_finite() { max } else { f32::MAX })
        } else if max.is_finite() {
            max.min(0.0)
        } else {
            0.0
        };
    }
    if !min.is_finite() {
        min = default.min(0.0) - 1000.0;
    }
    if !max.is_finite() {
        max = default.max(0.0) + 1000.0;
    }
    if max < min {
        std::mem::swap(&mut min, &mut max);
    }
    let default = default.clamp(min, max);
    let steps = if toggled {
        1
    } else if h & HINT_INTEGER != 0 {
        (max - min).round().clamp(0.0, 1000.0) as u32
    } else {
        0
    };
    ControlMeta { min, max, default, toggled, logarithmic, steps }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(h: c_int, lower: f32, upper: f32) -> PortRangeHint {
        PortRangeHint { hint: h, lower, upper }
    }

    #[test]
    fn bounded_default_low_and_middle() {
        let m = control_meta(
            hint(HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | HINT_DEFAULT_LOW, 0.0, 100.0),
            48000.0,
        );
        assert_eq!((m.min, m.max, m.default), (0.0, 100.0, 25.0));
        let m = control_meta(
            hint(HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | HINT_DEFAULT_MIDDLE, -10.0, 10.0),
            48000.0,
        );
        assert_eq!(m.default, 0.0);
    }

    #[test]
    fn logarithmic_middle_is_geometric_mean() {
        let m = control_meta(
            hint(
                HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | HINT_LOGARITHMIC | HINT_DEFAULT_MIDDLE,
                10.0,
                1000.0,
            ),
            48000.0,
        );
        assert!((m.default - 100.0).abs() < 0.01, "{}", m.default);
        assert!(m.logarithmic);
    }

    #[test]
    fn sample_rate_scales_bounds() {
        let m = control_meta(
            hint(
                HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | HINT_SAMPLE_RATE | HINT_DEFAULT_MAXIMUM,
                0.0,
                0.5,
            ),
            44100.0,
        );
        assert_eq!((m.min, m.max, m.default), (0.0, 22050.0, 22050.0));
    }

    #[test]
    fn toggled_and_integer_steps() {
        let m = control_meta(hint(HINT_TOGGLED | HINT_DEFAULT_1, 0.0, 0.0), 48000.0);
        assert_eq!((m.min, m.max, m.default, m.steps, m.toggled), (0.0, 1.0, 1.0, 1, true));
        let m = control_meta(
            hint(HINT_BOUNDED_BELOW | HINT_BOUNDED_ABOVE | HINT_INTEGER, 2.0, 10.0),
            48000.0,
        );
        assert_eq!((m.min, m.max, m.steps), (2.0, 10.0, 8));
        assert_eq!(m.default, 2.0);
    }

    #[test]
    fn unbounded_gets_usable_range() {
        let m = control_meta(hint(HINT_DEFAULT_0, 0.0, 0.0), 48000.0);
        assert!(m.min < 0.0 && m.max > 0.0 && m.default == 0.0);
        let m = control_meta(hint(0, f32::NAN, f32::INFINITY), 48000.0);
        assert!(m.min.is_finite() && m.max.is_finite() && m.min <= m.default && m.default <= m.max);
    }
}
