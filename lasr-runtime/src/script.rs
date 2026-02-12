use std::{ptr, slice, str};

#[repr(C)]
pub struct ScriptSlice {
    ptr: *const u8,
    len: usize,
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn lasr_script() -> ScriptSlice {
    ScriptSlice {
        ptr: ptr::null(),
        len: 0,
    }
}

pub fn script_str() -> &'static str {
    let slice = unsafe {
        let ScriptSlice { ptr, len } = lasr_script();
        slice::from_raw_parts(ptr, len)
    };
    str::from_utf8(slice).expect("lasr_script returned non-UTF-8")
}
