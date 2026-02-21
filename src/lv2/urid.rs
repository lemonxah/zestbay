use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;

use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle};

/// Thread-safe URI-to-URID mapper.
///
/// This is shared between all plugin instances. The Mutex is only
/// contended during the first call with a new URI (which happens at
/// instantiation time, not during RT processing).
pub struct UridMapper {
    inner: Mutex<UridMapperInner>,
}

struct UridMapperInner {
    uri_to_id: HashMap<String, LV2Urid>,
    id_to_uri: Vec<String>,
}

impl UridMapper {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(UridMapperInner {
                uri_to_id: HashMap::new(),
                // ID 0 is reserved (invalid), so start with a dummy entry
                id_to_uri: vec![String::new()],
            }),
        }
    }

    /// Map a URI string to a URID. If the URI hasn't been seen before,
    /// assign a new ID. Returns 0 on error.
    pub fn map(&self, uri: &str) -> LV2Urid {
        let mut inner = self.inner.lock().unwrap();
        if let Some(&id) = inner.uri_to_id.get(uri) {
            return id;
        }
        let id = inner.id_to_uri.len() as LV2Urid;
        inner.uri_to_id.insert(uri.to_string(), id);
        inner.id_to_uri.push(uri.to_string());
        id
    }

    /// Reverse-map a URID back to its URI string. Returns None if invalid.
    #[allow(dead_code)]
    pub fn unmap(&self, urid: LV2Urid) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner.id_to_uri.get(urid as usize).cloned()
    }

    /// Create an `LV2UridMap` struct pointing to this mapper.
    ///
    /// The returned struct holds a raw pointer to `self`, so the caller
    /// must ensure this `UridMapper` outlives all plugin instances.
    pub fn as_lv2_urid_map(&self) -> LV2UridMap {
        LV2UridMap {
            handle: self as *const UridMapper as LV2UridMapHandle,
            map: urid_map_callback,
        }
    }

    /// Create an `LV2Feature` for the URID map.
    ///
    /// The returned feature holds pointers into `map_struct`, so the caller
    /// must ensure both `map_struct` and `self` outlive all plugin instances.
    ///
    /// # Safety
    /// `map_struct` must be a valid pointer to an `LV2UridMap` returned by
    /// `as_lv2_urid_map()` and must remain valid for the lifetime of any
    /// plugins using this feature.
    pub unsafe fn make_feature(map_struct: *mut LV2UridMap) -> lv2_raw::core::LV2Feature {
        // LV2_URID__MAP from lv2_raw is a &str (not null-terminated).
        // LV2Feature.uri must be a C string, so use a null-terminated literal.
        const URID_MAP_URI: &CStr = c"http://lv2plug.in/ns/ext/urid#map";
        lv2_raw::core::LV2Feature {
            uri: URID_MAP_URI.as_ptr(),
            data: map_struct as *mut c_void,
        }
    }
}

/// C-callable callback for LV2_URID_Map::map.
///
/// `handle` is a `*const UridMapper` disguised as `LV2UridMapHandle`.
extern "C" fn urid_map_callback(handle: LV2UridMapHandle, uri: *const c_char) -> LV2Urid {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    let mapper = unsafe { &*(handle as *const UridMapper) };
    let c_str = unsafe { CStr::from_ptr(uri) };
    match c_str.to_str() {
        Ok(s) => mapper.map(s),
        Err(_) => 0,
    }
}
