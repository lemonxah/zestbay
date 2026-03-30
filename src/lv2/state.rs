//! LV2 State extension implementation.
//!
//! Allows saving and restoring arbitrary plugin state (KVT entries,
//! channel labels, etc.) via the `LV2_State_Interface` extension.

use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::Arc;

use super::urid::UridMapper;

pub const LV2_STATE__INTERFACE: &str = "http://lv2plug.in/ns/ext/state#interface";

#[allow(non_camel_case_types)]
pub type LV2_State_Handle = *mut c_void;
#[allow(non_camel_case_types)]
pub type LV2_State_Status = u32;
pub const LV2_STATE_SUCCESS: LV2_State_Status = 0;

#[allow(non_camel_case_types)]
pub type LV2_State_Store_Function = unsafe extern "C" fn(
    handle: LV2_State_Handle,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> LV2_State_Status;

#[allow(non_camel_case_types)]
pub type LV2_State_Retrieve_Function = unsafe extern "C" fn(
    handle: LV2_State_Handle,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct LV2_State_Interface {
    pub save: unsafe extern "C" fn(
        instance: *mut c_void,
        store: LV2_State_Store_Function,
        handle: LV2_State_Handle,
        flags: u32,
        features: *const *const lv2_raw::core::LV2Feature,
    ) -> LV2_State_Status,
    pub restore: unsafe extern "C" fn(
        instance: *mut c_void,
        retrieve: LV2_State_Retrieve_Function,
        handle: LV2_State_Handle,
        flags: u32,
        features: *const *const lv2_raw::core::LV2Feature,
    ) -> LV2_State_Status,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct StateEntry {
    pub key_uri: String,
    pub type_uri: String,
    pub value: Vec<u8>,
    pub flags: u32,
}

impl StateEntry {
    pub fn new_string(key_uri: &str, value: &str) -> Self {
        let atom_string_uri = "http://lv2plug.in/ns/ext/atom#String";
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0); // null-terminated C string
        Self {
            key_uri: key_uri.to_string(),
            type_uri: atom_string_uri.to_string(),
            value: bytes,
            flags: 0,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        if !self.type_uri.contains("String") {
            return None;
        }
        let bytes = if self.value.last() == Some(&0) {
            &self.value[..self.value.len() - 1]
        } else {
            &self.value
        };
        std::str::from_utf8(bytes).ok()
    }
}

struct StoreContext {
    entries: Vec<StateEntry>,
    mapper: Arc<UridMapper>,
}

unsafe extern "C" fn store_callback(
    handle: LV2_State_Handle,
    key: u32,
    value: *const c_void,
    size: usize,
    type_: u32,
    flags: u32,
) -> LV2_State_Status {
    unsafe {
        let ctx = &mut *(handle as *mut StoreContext);
        let key_uri = match ctx.mapper.unmap(key) {
            Some(uri) => uri,
            None => return 1,
        };
        let type_uri = match ctx.mapper.unmap(type_) {
            Some(uri) => uri,
            None => return 1,
        };
        let data = std::slice::from_raw_parts(value as *const u8, size).to_vec();
        ctx.entries.push(StateEntry {
            key_uri,
            type_uri,
            value: data,
            flags,
        });
        LV2_STATE_SUCCESS
    }
}

struct RetrieveContext {
    entries: Vec<RetrieveEntry>,
}

struct RetrieveEntry {
    key_urid: u32,
    type_urid: u32,
    data: Vec<u8>,
    flags: u32,
}

unsafe extern "C" fn retrieve_callback(
    handle: LV2_State_Handle,
    key: u32,
    size: *mut usize,
    type_: *mut u32,
    flags: *mut u32,
) -> *const c_void {
    unsafe {
        let ctx = &*(handle as *const RetrieveContext);
        for entry in &ctx.entries {
            if entry.key_urid == key {
                *size = entry.data.len();
                *type_ = entry.type_urid;
                *flags = entry.flags;
                return entry.data.as_ptr() as *const c_void;
            }
        }
        std::ptr::null()
    }
}

pub unsafe fn save_plugin_state(
    handle: *mut c_void,
    iface: *const LV2_State_Interface,
    mapper: &Arc<UridMapper>,
) -> Option<Vec<StateEntry>> {
    let mut ctx = StoreContext {
        entries: Vec::new(),
        mapper: mapper.clone(),
    };
    let null_feature: *const lv2_raw::core::LV2Feature = std::ptr::null();
    let features = &null_feature as *const *const lv2_raw::core::LV2Feature;

    let status = unsafe {
        ((*iface).save)(
            handle,
            store_callback,
            &mut ctx as *mut StoreContext as LV2_State_Handle,
            0,
            features,
        )
    };
    if status == LV2_STATE_SUCCESS {
        Some(ctx.entries)
    } else {
        log::warn!("LV2 state save failed with status {}", status);
        None
    }
}

pub unsafe fn restore_plugin_state(
    handle: *mut c_void,
    iface: *const LV2_State_Interface,
    mapper: &Arc<UridMapper>,
    entries: &[StateEntry],
) {
    let retrieve_entries: Vec<RetrieveEntry> = entries
        .iter()
        .map(|e| RetrieveEntry {
            key_urid: mapper.map(&e.key_uri),
            type_urid: mapper.map(&e.type_uri),
            data: e.value.clone(),
            flags: e.flags,
        })
        .collect();
    let ctx = RetrieveContext {
        entries: retrieve_entries,
    };
    let null_feature: *const lv2_raw::core::LV2Feature = std::ptr::null();
    let features = &null_feature as *const *const lv2_raw::core::LV2Feature;

    let status = unsafe {
        ((*iface).restore)(
            handle,
            retrieve_callback,
            &ctx as *const RetrieveContext as LV2_State_Handle,
            0,
            features,
        )
    };
    if status != LV2_STATE_SUCCESS {
        log::warn!("LV2 state restore failed with status {}", status);
    }
}

// ── State path features (makePath, freePath, mapPath) ──

const LV2_STATE_MAKE_PATH_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/state#makePath";
const LV2_STATE_FREE_PATH_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/state#freePath";
const LV2_STATE_MAP_PATH_URI: &std::ffi::CStr = c"http://lv2plug.in/ns/ext/state#mapPath";

#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2_State_Make_Path {
    handle: *mut c_void,
    path: unsafe extern "C" fn(handle: *mut c_void, path: *const c_char) -> *mut c_char,
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2_State_Free_Path {
    handle: *mut c_void,
    free_path: unsafe extern "C" fn(handle: *mut c_void, path: *mut c_char),
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct LV2_State_Map_Path {
    handle: *mut c_void,
    abstract_path: unsafe extern "C" fn(handle: *mut c_void, absolute_path: *const c_char) -> *mut c_char,
    absolute_path: unsafe extern "C" fn(handle: *mut c_void, abstract_path: *const c_char) -> *mut c_char,
}

struct StatePathContext {
    state_dir: std::path::PathBuf,
}

fn sanitize_uri(uri: &str) -> String {
    uri.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn malloc_cstring(s: &str) -> *mut c_char {
    unsafe {
        let len = s.len() + 1;
        let ptr = libc::malloc(len) as *mut c_char;
        if ptr.is_null() {
            return std::ptr::null_mut();
        }
        std::ptr::copy_nonoverlapping(s.as_ptr(), ptr as *mut u8, s.len());
        *ptr.add(s.len()) = 0;
        ptr
    }
}

unsafe extern "C" fn make_path_callback(
    handle: *mut c_void,
    path: *const c_char,
) -> *mut c_char {
    if handle.is_null() || path.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let ctx = &*(handle as *const StatePathContext);
        let rel_path = match std::ffi::CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        if let Err(e) = std::fs::create_dir_all(&ctx.state_dir) {
            log::warn!("LV2 state makePath: failed to create dir {:?}: {}", ctx.state_dir, e);
            return std::ptr::null_mut();
        }
        let full = ctx.state_dir.join(rel_path);
        let full_str = full.to_string_lossy();
        malloc_cstring(&full_str)
    }
}

unsafe extern "C" fn free_path_callback(
    _handle: *mut c_void,
    path: *mut c_char,
) {
    if !path.is_null() {
        unsafe { libc::free(path as *mut c_void) };
    }
}

unsafe extern "C" fn abstract_path_callback(
    handle: *mut c_void,
    absolute_path: *const c_char,
) -> *mut c_char {
    if handle.is_null() || absolute_path.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let ctx = &*(handle as *const StatePathContext);
        let abs = match std::ffi::CStr::from_ptr(absolute_path).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let abs_path = std::path::Path::new(abs);
        let state_dir_str = ctx.state_dir.to_string_lossy();
        if let Ok(rel) = abs_path.strip_prefix(&*state_dir_str) {
            malloc_cstring(&rel.to_string_lossy())
        } else {
            malloc_cstring(abs)
        }
    }
}

unsafe extern "C" fn absolute_path_callback(
    handle: *mut c_void,
    abstract_path: *const c_char,
) -> *mut c_char {
    if handle.is_null() || abstract_path.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        let ctx = &*(handle as *const StatePathContext);
        let rel = match std::ffi::CStr::from_ptr(abstract_path).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        };
        let rel_path = std::path::Path::new(rel);
        let full = if rel_path.is_absolute() {
            rel_path.to_path_buf()
        } else {
            ctx.state_dir.join(rel_path)
        };
        malloc_cstring(&full.to_string_lossy())
    }
}

pub struct Lv2StatePathSetup {
    make_path_struct: Box<LV2_State_Make_Path>,
    free_path_struct: Box<LV2_State_Free_Path>,
    map_path_struct: Box<LV2_State_Map_Path>,
    ctx_ptr: *mut StatePathContext,
}

unsafe impl Send for Lv2StatePathSetup {}

impl Lv2StatePathSetup {
    pub fn new(plugin_uri: &str) -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~/.config"));
        let state_dir = config_dir
            .join("zestbay")
            .join("plugin-state")
            .join(sanitize_uri(plugin_uri));

        let ctx = Box::new(StatePathContext { state_dir });
        let ctx_ptr = Box::into_raw(ctx);
        let handle = ctx_ptr as *mut c_void;

        let make_path_struct = Box::new(LV2_State_Make_Path {
            handle,
            path: make_path_callback,
        });

        let free_path_struct = Box::new(LV2_State_Free_Path {
            handle,
            free_path: free_path_callback,
        });

        let map_path_struct = Box::new(LV2_State_Map_Path {
            handle,
            abstract_path: abstract_path_callback,
            absolute_path: absolute_path_callback,
        });

        Lv2StatePathSetup {
            make_path_struct,
            free_path_struct,
            map_path_struct,
            ctx_ptr,
        }
    }

    pub fn make_make_path_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_STATE_MAKE_PATH_URI.as_ptr(),
            data: &*self.make_path_struct as *const LV2_State_Make_Path as *mut c_void,
        }
    }

    pub fn make_free_path_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_STATE_FREE_PATH_URI.as_ptr(),
            data: &*self.free_path_struct as *const LV2_State_Free_Path as *mut c_void,
        }
    }

    pub fn make_map_path_feature(&self) -> lv2_raw::core::LV2Feature {
        lv2_raw::core::LV2Feature {
            uri: LV2_STATE_MAP_PATH_URI.as_ptr(),
            data: &*self.map_path_struct as *const LV2_State_Map_Path as *mut c_void,
        }
    }
}

impl Drop for Lv2StatePathSetup {
    fn drop(&mut self) {
        if !self.ctx_ptr.is_null() {
            unsafe { drop(Box::from_raw(self.ctx_ptr)) };
            self.ctx_ptr = std::ptr::null_mut();
        }
    }
}
