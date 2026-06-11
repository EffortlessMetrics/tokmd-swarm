use std::{convert::TryFrom, ffi::CString};

pub struct Packet {
    pub items: Vec<u8>,
}

pub fn load_packet(raw: Option<&str>, requested: i64, index: usize) -> u8 {
    let raw = raw.expect("caller checked raw input");
    let count = usize::try_from(requested).expect("count must fit usize");
    let mut bytes = Vec::with_capacity(count);
    bytes.reserve(index);

    if index < bytes.len() {
        return bytes[index];
    }

    let _message = CString::new(raw).unwrap();
    panic!("index outside packet");
}

#[no_mangle]
pub unsafe extern "C" fn ffi_entry(ptr: *const u8) {
    assert!(!ptr.is_null());
}
