//! C-ABI echo plugin for `dl8`'s dylib executor lab, strings only:
//! `dl8_executor_answer` maps a JSON array of inputs to `{input,output}` rows.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

const PREFIX: &str = env!("DYLIB_ECHO_PREFIX");

/// SAFETY: 'static storage, never freed by the caller.
#[no_mangle]
pub extern "C" fn dl8_executor_relation() -> *const c_char {
    static RELATION: &[u8] = b"dylib_echo\0";
    RELATION.as_ptr() as *const c_char
}

/// SAFETY: `pending_json` is a NUL-terminated string, read but never freed
/// here; the returned pointer must return through `dl8_executor_free`.
#[no_mangle]
pub extern "C" fn dl8_executor_answer(pending_json: *const c_char) -> *mut c_char {
    let text = unsafe { CStr::from_ptr(pending_json) }
        .to_string_lossy()
        .into_owned();
    let pending: Vec<String> = serde_json::from_str(&text).unwrap_or_default();
    let rows: Vec<serde_json::Value> = pending
        .into_iter()
        .map(|input| {
            let output = format!("{PREFIX}:{input}");
            serde_json::json!({ "input": input, "output": output })
        })
        .collect();
    let encoded = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
    CString::new(encoded).unwrap_or_default().into_raw()
}

/// SAFETY: `s` must come from `dl8_executor_answer` and not already be freed.
#[no_mangle]
pub extern "C" fn dl8_executor_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}
