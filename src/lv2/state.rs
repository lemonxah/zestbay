//! LV2 State extension implementation.
//!
//! Allows saving and restoring arbitrary plugin state (KVT entries,
//! channel labels, etc.) via the `LV2_State_Interface` extension.

use std::ffi::c_void;
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
