//! Host-side `IMessage` and `IAttributeList`, handed out through
//! `IHostApplication::createInstance`. Components and controllers of one
//! plugin use them to talk to each other over `IConnectionPoint`.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering, fence};

use vst3::Steinberg::Vst::IAttributeList_::AttrID;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;

enum Attr {
    Int(i64),
    Float(f64),
    /// UTF-16, without terminator.
    Str(Vec<u16>),
    Bin(Vec<u8>),
}

// ---------------------------------------------------------------------------
// IAttributeList
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct HostAttributeList {
    vtbl: *const IAttributeListVtbl,
    ref_count: AtomicU32,
    attrs: Mutex<HashMap<String, Attr>>,
}

static ATTR_LIST_VTBL: IAttributeListVtbl = IAttributeListVtbl {
    base: FUnknownVtbl {
        queryInterface: list_query_interface,
        addRef: list_add_ref,
        release: list_release,
    },
    setInt: list_set_int,
    getInt: list_get_int,
    setFloat: list_set_float,
    getFloat: list_get_float,
    setString: list_set_string,
    getString: list_get_string,
    setBinary: list_set_binary,
    getBinary: list_get_binary,
};

/// New attribute list with one reference.
pub fn new_attribute_list() -> *mut HostAttributeList {
    Box::into_raw(Box::new(HostAttributeList {
        vtbl: &ATTR_LIST_VTBL,
        ref_count: AtomicU32::new(1),
        attrs: Mutex::new(HashMap::new()),
    }))
}

unsafe fn key(id: AttrID) -> Option<String> {
    if id.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned())
    }
}

unsafe fn with_attrs<R>(this: *mut IAttributeList, f: impl FnOnce(&mut HashMap<String, Attr>) -> R) -> R {
    let list = unsafe { &*(this as *mut HostAttributeList) };
    f(&mut list.attrs.lock().unwrap_or_else(|e| e.into_inner()))
}

unsafe extern "system" fn list_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        if *iid == FUnknown_iid || *iid == IAttributeList_iid {
            list_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn list_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe { (*(this as *mut HostAttributeList)).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn list_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let list = this as *mut HostAttributeList;
        let old = (*list).ref_count.fetch_sub(1, Ordering::Release);
        if old == 1 {
            fence(Ordering::Acquire);
            drop(Box::from_raw(list));
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn list_set_int(this: *mut IAttributeList, id: AttrID, value: int64) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        with_attrs(this, |a| a.insert(k, Attr::Int(value)));
        kResultOk
    }
}

unsafe extern "system" fn list_get_int(this: *mut IAttributeList, id: AttrID, value: *mut int64) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if value.is_null() {
            return kInvalidArgument;
        }
        with_attrs(this, |a| match a.get(&k) {
            Some(Attr::Int(v)) => {
                *value = *v;
                kResultOk
            }
            _ => kResultFalse,
        })
    }
}

unsafe extern "system" fn list_set_float(this: *mut IAttributeList, id: AttrID, value: f64) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        with_attrs(this, |a| a.insert(k, Attr::Float(value)));
        kResultOk
    }
}

unsafe extern "system" fn list_get_float(this: *mut IAttributeList, id: AttrID, value: *mut f64) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if value.is_null() {
            return kInvalidArgument;
        }
        with_attrs(this, |a| match a.get(&k) {
            Some(Attr::Float(v)) => {
                *value = *v;
                kResultOk
            }
            _ => kResultFalse,
        })
    }
}

/// Longest string accepted, in UTF-16 units (against an unterminated input).
const MAX_STRING: usize = 1 << 20;

unsafe extern "system" fn list_set_string(this: *mut IAttributeList, id: AttrID, string: *const TChar) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut units = Vec::new();
        while units.len() < MAX_STRING && *string.add(units.len()) != 0 {
            units.push(*string.add(units.len()));
        }
        with_attrs(this, |a| a.insert(k, Attr::Str(units)));
        kResultOk
    }
}

unsafe extern "system" fn list_get_string(
    this: *mut IAttributeList,
    id: AttrID,
    string: *mut TChar,
    size_in_bytes: uint32,
) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if string.is_null() {
            return kInvalidArgument;
        }
        let capacity = size_in_bytes as usize / std::mem::size_of::<TChar>();
        with_attrs(this, |a| match a.get(&k) {
            Some(Attr::Str(units)) if capacity > 0 => {
                // Copy what fits and always terminate.
                let n = units.len().min(capacity - 1);
                for (i, u) in units[..n].iter().enumerate() {
                    *string.add(i) = *u;
                }
                *string.add(n) = 0;
                kResultOk
            }
            _ => kResultFalse,
        })
    }
}

unsafe extern "system" fn list_set_binary(
    this: *mut IAttributeList,
    id: AttrID,
    data: *const c_void,
    size_in_bytes: uint32,
) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if data.is_null() && size_in_bytes > 0 {
            return kInvalidArgument;
        }
        let bytes = if size_in_bytes == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(data as *const u8, size_in_bytes as usize).to_vec()
        };
        with_attrs(this, |a| a.insert(k, Attr::Bin(bytes)));
        kResultOk
    }
}

unsafe extern "system" fn list_get_binary(
    this: *mut IAttributeList,
    id: AttrID,
    data: *mut *const c_void,
    size_in_bytes: *mut uint32,
) -> tresult {
    unsafe {
        let Some(k) = key(id) else { return kInvalidArgument };
        if data.is_null() || size_in_bytes.is_null() {
            return kInvalidArgument;
        }
        // The pointer stays valid until this attribute is set again or the
        // list is freed, as the VST3 interface specifies.
        with_attrs(this, |a| match a.get(&k) {
            Some(Attr::Bin(bytes)) => {
                *data = bytes.as_ptr() as *const c_void;
                *size_in_bytes = bytes.len() as uint32;
                kResultOk
            }
            _ => kResultFalse,
        })
    }
}

// ---------------------------------------------------------------------------
// IMessage
// ---------------------------------------------------------------------------

#[repr(C)]
pub struct HostMessage {
    vtbl: *const IMessageVtbl,
    ref_count: AtomicU32,
    id: Mutex<CString>,
    /// Owned reference, released with the message.
    attributes: *mut HostAttributeList,
}

static MESSAGE_VTBL: IMessageVtbl = IMessageVtbl {
    base: FUnknownVtbl {
        queryInterface: msg_query_interface,
        addRef: msg_add_ref,
        release: msg_release,
    },
    getMessageID: msg_get_id,
    setMessageID: msg_set_id,
    getAttributes: msg_get_attributes,
};

/// New message with one reference and its own attribute list.
pub fn new_message() -> *mut HostMessage {
    Box::into_raw(Box::new(HostMessage {
        vtbl: &MESSAGE_VTBL,
        ref_count: AtomicU32::new(1),
        id: Mutex::new(CString::default()),
        attributes: new_attribute_list(),
    }))
}

unsafe extern "system" fn msg_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe {
        if iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        if *iid == FUnknown_iid || *iid == IMessage_iid {
            msg_add_ref(this);
            *obj = this as *mut c_void;
            return kResultOk;
        }
        *obj = std::ptr::null_mut();
        kNoInterface
    }
}

unsafe extern "system" fn msg_add_ref(this: *mut FUnknown) -> uint32 {
    unsafe { (*(this as *mut HostMessage)).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn msg_release(this: *mut FUnknown) -> uint32 {
    unsafe {
        let msg = this as *mut HostMessage;
        let old = (*msg).ref_count.fetch_sub(1, Ordering::Release);
        if old == 1 {
            fence(Ordering::Acquire);
            let msg = Box::from_raw(msg);
            list_release(msg.attributes as *mut FUnknown);
            return 0;
        }
        old - 1
    }
}

unsafe extern "system" fn msg_get_id(this: *mut IMessage) -> FIDString {
    // Valid until the id is set again, as the interface specifies.
    unsafe { (*(this as *mut HostMessage)).id.lock().unwrap_or_else(|e| e.into_inner()).as_ptr() }
}

unsafe extern "system" fn msg_set_id(this: *mut IMessage, id: FIDString) {
    unsafe {
        let value = if id.is_null() { CString::default() } else { CStr::from_ptr(id).to_owned() };
        *(*(this as *mut HostMessage)).id.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }
}

unsafe extern "system" fn msg_get_attributes(this: *mut IMessage) -> *mut IAttributeList {
    // Owned by the message; no extra reference is given out.
    unsafe { (*(this as *mut HostMessage)).attributes as *mut IAttributeList }
}

/// `IHostApplication::createInstance` for the interfaces the host provides.
/// Returns `None` for anything else. The object has one reference, owned by
/// the caller.
///
/// # Safety
/// `cid` and `iid` must be null or point to valid TUIDs.
pub unsafe fn create_instance(cid: *const TUID, iid: *const TUID) -> Option<*mut c_void> {
    if cid.is_null() || iid.is_null() {
        return None;
    }
    let (cid, iid) = unsafe { (*cid, *iid) };
    if cid == IMessage_iid && iid == IMessage_iid {
        Some(new_message() as *mut c_void)
    } else if cid == IAttributeList_iid && iid == IAttributeList_iid {
        Some(new_attribute_list() as *mut c_void)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn call_list(p: *mut c_void) -> &'static IAttributeListVtbl {
        unsafe { &*(*(p as *mut *const IAttributeListVtbl)) }
    }

    #[test]
    fn attribute_list_round_trips_every_type() {
        unsafe {
            let p = new_attribute_list() as *mut c_void;
            let list = p as *mut IAttributeList;
            let v = call_list(p);
            assert_eq!((v.setInt)(list, c"i".as_ptr(), -5), kResultOk);
            assert_eq!((v.setFloat)(list, c"f".as_ptr(), 1.5), kResultOk);
            let text: Vec<TChar> = "héllo".encode_utf16().chain([0]).map(|u| u as TChar).collect();
            assert_eq!((v.setString)(list, c"s".as_ptr(), text.as_ptr()), kResultOk);
            assert_eq!((v.setBinary)(list, c"b".as_ptr(), [1u8, 2, 3].as_ptr().cast(), 3), kResultOk);

            let (mut i, mut f) = (0i64, 0f64);
            assert_eq!((v.getInt)(list, c"i".as_ptr(), &mut i), kResultOk);
            assert_eq!((v.getFloat)(list, c"f".as_ptr(), &mut f), kResultOk);
            assert_eq!((i, f), (-5, 1.5));
            // Wrong type / missing key report failure.
            assert_eq!((v.getFloat)(list, c"i".as_ptr(), &mut f), kResultFalse);
            assert_eq!((v.getInt)(list, c"missing".as_ptr(), &mut i), kResultFalse);

            let mut out = [0 as TChar; 16];
            assert_eq!((v.getString)(list, c"s".as_ptr(), out.as_mut_ptr(), 32), kResultOk);
            assert_eq!(&out[..6], &text[..]);
            // A short buffer is truncated and terminated.
            let mut small = [7 as TChar; 3];
            assert_eq!((v.getString)(list, c"s".as_ptr(), small.as_mut_ptr(), 6), kResultOk);
            assert_eq!(small[2], 0);

            let (mut data, mut size) = (std::ptr::null(), 0u32);
            assert_eq!((v.getBinary)(list, c"b".as_ptr(), &mut data, &mut size), kResultOk);
            assert_eq!(std::slice::from_raw_parts(data as *const u8, size as usize), &[1, 2, 3]);

            assert_eq!((v.setInt)(list, std::ptr::null(), 1), kInvalidArgument);
            assert_eq!((v.base.release)(p as *mut FUnknown), 0);
        }
    }

    #[test]
    fn message_has_id_and_attributes_and_query_interface() {
        unsafe {
            let m = create_instance(&IMessage_iid, &IMessage_iid).expect("IMessage");
            let vtbl = &*(*(m as *mut *const IMessageVtbl));
            let msg = m as *mut IMessage;
            assert_eq!(CStr::from_ptr((vtbl.getMessageID)(msg)).to_bytes(), b"");
            (vtbl.setMessageID)(msg, c"ping".as_ptr());
            assert_eq!(CStr::from_ptr((vtbl.getMessageID)(msg)).to_bytes(), b"ping");
            let attrs = (vtbl.getAttributes)(msg);
            assert!(!attrs.is_null());
            assert_eq!((call_list(attrs.cast()).setInt)(attrs, c"n".as_ptr(), 9), kResultOk);

            let mut other = std::ptr::null_mut();
            assert_eq!((vtbl.base.queryInterface)(m as *mut FUnknown, &IMessage_iid, &mut other), kResultOk);
            assert_eq!(other, m);
            assert_eq!((vtbl.base.queryInterface)(m as *mut FUnknown, &IAttributeList_iid, &mut other), kNoInterface);
            assert_eq!((vtbl.base.release)(m as *mut FUnknown), 1);
            assert_eq!((vtbl.base.release)(m as *mut FUnknown), 0);
        }
    }

    #[test]
    fn create_instance_rejects_other_interfaces() {
        unsafe {
            assert!(create_instance(&IBStream_iid, &IBStream_iid).is_none());
            assert!(create_instance(&IMessage_iid, &IAttributeList_iid).is_none());
            assert!(create_instance(std::ptr::null(), &IMessage_iid).is_none());
            let list = create_instance(&IAttributeList_iid, &IAttributeList_iid).expect("list");
            assert_eq!((call_list(list).base.release)(list as *mut FUnknown), 0);
        }
    }
}
