use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;

use lv2_raw::urid::{LV2Urid, LV2UridMap, LV2UridMapHandle};

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
                id_to_uri: vec![String::new()],
            }),
        }
    }

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

    #[allow(dead_code)]
    pub fn unmap(&self, urid: LV2Urid) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        inner.id_to_uri.get(urid as usize).cloned()
    }

    pub fn as_lv2_urid_map(&self) -> LV2UridMap {
        LV2UridMap {
            handle: self as *const UridMapper as LV2UridMapHandle,
            map: urid_map_callback,
        }
    }

    pub unsafe fn make_feature(map_struct: *mut LV2UridMap) -> lv2_raw::core::LV2Feature {
        const URID_MAP_URI: &CStr = c"http://lv2plug.in/ns/ext/urid#map";
        lv2_raw::core::LV2Feature {
            uri: URID_MAP_URI.as_ptr(),
            data: map_struct as *mut c_void,
        }
    }
}

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
