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
pub const LV2_STATE_ERR_BAD_FLAGS: LV2_State_Status = 3;

pub const LV2_STATE_IS_POD: u32 = 1;
pub const LV2_STATE_IS_PORTABLE: u32 = 2;

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
        if flags & LV2_STATE_IS_POD == 0 {
            // Values are persisted as JSON bytes; non-POD data (pointers,
            // handles) cannot survive that.
            log::warn!(
                "LV2 state: skipping non-POD value for key {:?}",
                ctx.mapper.unmap(key)
            );
            return LV2_STATE_ERR_BAD_FLAGS;
        }
        if value.is_null() && size > 0 {
            return 1;
        }
        let key_uri = match ctx.mapper.unmap(key) {
            Some(uri) => uri,
            None => return 1,
        };
        let type_uri = match ctx.mapper.unmap(type_) {
            Some(uri) => uri,
            None => return 1,
        };
        let data = if size == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(value as *const u8, size).to_vec()
        };
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
    paths: &Lv2StatePathSetup,
) -> Option<Vec<StateEntry>> {
    let mut ctx = StoreContext {
        entries: Vec::new(),
        mapper: mapper.clone(),
    };
    let path_features = paths.make_features();
    let features = feature_ptrs(&path_features);

    let status = unsafe {
        ((*iface).save)(
            handle,
            store_callback,
            &mut ctx as *mut StoreContext as LV2_State_Handle,
            LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE,
            features.as_ptr(),
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
    paths: &Lv2StatePathSetup,
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
    let path_features = paths.make_features();
    let features = feature_ptrs(&path_features);

    let status = unsafe {
        ((*iface).restore)(
            handle,
            retrieve_callback,
            &ctx as *const RetrieveContext as LV2_State_Handle,
            LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE,
            features.as_ptr(),
        )
    };
    if status != LV2_STATE_SUCCESS {
        log::warn!("LV2 state restore failed with status {}", status);
    }
}

/// NULL-terminated pointer array over `features` (which must outlive it).
fn feature_ptrs(features: &[lv2_raw::core::LV2Feature]) -> Vec<*const lv2_raw::core::LV2Feature> {
    features
        .iter()
        .map(|f| f as *const _)
        .chain(std::iter::once(std::ptr::null()))
        .collect()
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
    /// Per-plugin-URI base directory.
    base_dir: std::path::PathBuf,
    /// Per-instance directory (base_dir/<scope>); changed by `set_scope`.
    state_dir: parking_lot::Mutex<std::path::PathBuf>,
}

impl StatePathContext {
    fn dir(&self) -> std::path::PathBuf {
        self.state_dir.lock().clone()
    }
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
        let state_dir = ctx.dir();
        let full = state_dir.join(rel_path);
        let parent = full.parent().unwrap_or(&state_dir);
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("LV2 state makePath: failed to create dir {:?}: {}", parent, e);
            return std::ptr::null_mut();
        }
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
        if let Ok(rel) = abs_path.strip_prefix(ctx.dir()) {
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
            ctx.dir().join(rel_path)
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
    /// `default_scope` names the per-instance directory used until
    /// `set_scope` is called (so unscoped instances never share files).
    pub fn new(plugin_uri: &str, default_scope: &str) -> Self {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~/.config"));
        let base_dir = config_dir
            .join("zestbay")
            .join("plugin-state")
            .join(sanitize_uri(plugin_uri));
        let state_dir = base_dir.join(sanitize_uri(default_scope));

        let ctx = Box::new(StatePathContext {
            base_dir,
            state_dir: parking_lot::Mutex::new(state_dir),
        });
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

    /// Scope makePath/mapPath files to a persistent instance identity.
    pub fn set_scope(&self, scope: &str) {
        let ctx = unsafe { &*self.ctx_ptr };
        *ctx.state_dir.lock() = ctx.base_dir.join(sanitize_uri(scope));
    }

    pub fn state_dir(&self) -> std::path::PathBuf {
        unsafe { &*self.ctx_ptr }.dir()
    }

    /// makePath, freePath and mapPath features (for instantiate/save/restore).
    pub fn make_features(&self) -> [lv2_raw::core::LV2Feature; 3] {
        [
            self.make_make_path_feature(),
            self.make_free_path_feature(),
            self.make_map_path_feature(),
        ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_changes_state_dir_per_instance() {
        let a = Lv2StatePathSetup::new("http://example.org/plug", "default-1");
        let b = Lv2StatePathSetup::new("http://example.org/plug", "default-2");
        assert_ne!(a.state_dir(), b.state_dir());
        a.set_scope("stable-a");
        b.set_scope("stable-b");
        assert!(a.state_dir().ends_with("stable-a"));
        assert!(b.state_dir().ends_with("stable-b"));
        assert_eq!(a.state_dir().parent(), b.state_dir().parent());
    }

    #[test]
    fn absolute_and_abstract_paths_round_trip() {
        let setup = Lv2StatePathSetup::new("http://example.org/plug", "x");
        setup.set_scope("round-trip");
        let handle = setup.ctx_ptr as *mut c_void;
        unsafe {
            let abs = absolute_path_callback(handle, c"sample.wav".as_ptr());
            let abs_str = std::ffi::CStr::from_ptr(abs).to_str().unwrap().to_string();
            assert!(abs_str.ends_with("round-trip/sample.wav"));
            let rel = abstract_path_callback(handle, abs);
            assert_eq!(std::ffi::CStr::from_ptr(rel).to_str().unwrap(), "sample.wav");
            free_path_callback(handle, abs);
            free_path_callback(handle, rel);
        }
    }

    unsafe extern "C" fn pod_save(
        _instance: *mut c_void,
        store: LV2_State_Store_Function,
        handle: LV2_State_Handle,
        flags: u32,
        features: *const *const lv2_raw::core::LV2Feature,
    ) -> LV2_State_Status {
        assert_eq!(flags, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE);
        unsafe {
            let mut n = 0;
            while !(*features.add(n)).is_null() {
                n += 1;
            }
            assert_eq!(n, 3);
            let mapper = (*(handle as *mut StoreContext)).mapper.clone();
            let key = mapper.map("urn:key");
            let ty = mapper.map("urn:type");
            let val = [1u8, 2, 3];
            store(handle, key, val.as_ptr() as *const c_void, 3, ty, LV2_STATE_IS_POD);
            let key2 = mapper.map("urn:ptr");
            store(handle, key2, val.as_ptr() as *const c_void, 3, ty, 0);
        }
        LV2_STATE_SUCCESS
    }

    unsafe extern "C" fn noop_restore(
        _instance: *mut c_void,
        _retrieve: LV2_State_Retrieve_Function,
        _handle: LV2_State_Handle,
        _flags: u32,
        _features: *const *const lv2_raw::core::LV2Feature,
    ) -> LV2_State_Status {
        LV2_STATE_SUCCESS
    }

    #[test]
    fn save_passes_features_and_skips_non_pod() {
        let mapper = Arc::new(UridMapper::new());
        let paths = Lv2StatePathSetup::new("http://example.org/plug", "save");
        let iface = LV2_State_Interface {
            save: pod_save,
            restore: noop_restore,
        };
        let entries =
            unsafe { save_plugin_state(std::ptr::null_mut(), &iface, &mapper, &paths) }.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_uri, "urn:key");
        assert_eq!(entries[0].value, vec![1, 2, 3]);
    }
}
