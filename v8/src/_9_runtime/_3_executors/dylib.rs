//! `(dylib_echo ?In ?Out)`: a plugin loaded by path from `DL8_DYLIB_PATH`,
//! reloaded when its mtime moves. C ABI in `labs/dylib_echo/src/lib.rs`.

use super::text_at;
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IExecutor};
use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const RELATION: &str = "dylib_echo";
pub const PATH_ENV: &str = "DL8_DYLIB_PATH";

type RelationFn = unsafe extern "C" fn() -> *const c_char;
type AnswerFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

pub struct Dylib {
    relation: TermId,
    path: PathBuf,
    library: Library,
    loaded_at: SystemTime,
}

impl Dylib {
    pub fn from_env(relation: TermId) -> Result<Dylib, String> {
        let path = std::env::var_os(PATH_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{PATH_ENV} is unset"))?;
        let loaded_at = mtime(&path)?;
        let library = load(&path)?;
        Ok(Dylib {
            relation,
            path,
            library,
            loaded_at,
        })
    }

    fn reload_if_changed(&mut self) {
        let Ok(current) = mtime(&self.path) else {
            return;
        };
        if current <= self.loaded_at {
            return;
        }
        match load(&self.path) {
            Ok(library) => {
                self.library = library;
                self.loaded_at = current;
                tracing::info!(target: "dl8::dylib", path = %self.path.display(), "reloaded");
            }
            Err(reason) => {
                tracing::warn!(target: "dl8::dylib", path = %self.path.display(), reason, "reload failed, keeping the loaded library");
            }
        }
    }

    /// One request/response round trip; `None` on a call or symbol failure.
    fn answer_json(&self, pending_json: &str) -> Option<String> {
        // SAFETY: name matches the plugin's `dl8_executor_answer` export.
        let answer: Symbol<AnswerFn> = unsafe { self.library.get(b"dl8_executor_answer\0") }.ok()?;
        // SAFETY: name matches the plugin's `dl8_executor_free` export.
        let free: Symbol<FreeFn> = unsafe { self.library.get(b"dl8_executor_free\0") }.ok()?;
        let input = CString::new(pending_json).ok()?;
        // SAFETY: `answer` is the plugin's exported fn; `input` outlives the call.
        let out = unsafe { answer(input.as_ptr()) };
        if out.is_null() {
            return None;
        }
        // SAFETY: `out` is this call's plugin allocation; read once before free.
        let text = unsafe { CStr::from_ptr(out) }.to_string_lossy().into_owned();
        // SAFETY: `out` came from this plugin's `dl8_executor_answer`, freed once.
        unsafe { free(out) };
        Some(text)
    }
}

fn mtime(path: &Path) -> Result<SystemTime, String> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Opens the library and checks all three exports resolve, so a file that is
/// not this plugin's shape fails here rather than at the first `answer`.
fn load(path: &Path) -> Result<Library, String> {
    // SAFETY: the plugin's only load-time side effect is registering the three
    // named exports below; no other initializer runs.
    let library =
        unsafe { Library::new(path) }.map_err(|e| format!("{}: {e}", path.display()))?;
    // SAFETY: verifies the `dl8_executor_relation` export exists and matches
    // the C-ABI contract; the symbol is dropped immediately, never called.
    let _relation: Symbol<RelationFn> = unsafe { library.get(b"dl8_executor_relation\0") }
        .map_err(|e| format!("{}: missing dl8_executor_relation: {e}", path.display()))?;
    // SAFETY: verifies the `dl8_executor_answer` export exists.
    let _answer: Symbol<AnswerFn> = unsafe { library.get(b"dl8_executor_answer\0") }
        .map_err(|e| format!("{}: missing dl8_executor_answer: {e}", path.display()))?;
    // SAFETY: verifies the `dl8_executor_free` export exists.
    let _free: Symbol<FreeFn> = unsafe { library.get(b"dl8_executor_free\0") }
        .map_err(|e| format!("{}: missing dl8_executor_free: {e}", path.display()))?;
    Ok(library)
}

impl IExecutor for Dylib {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn answer(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) -> Vec<Row> {
        self.reload_if_changed();
        let inputs: Vec<(TermId, String)> = pending
            .iter()
            .filter_map(|&application| text_at(u, application, 0))
            .collect();
        if inputs.is_empty() {
            return Vec::new();
        }
        let texts: Vec<&str> = inputs.iter().map(|(_, s)| s.as_str()).collect();
        let request = serde_json::to_string(&texts).unwrap_or_else(|_| "[]".to_string());
        let Some(response) = self.answer_json(&request) else {
            tracing::warn!(target: "dl8::dylib", path = %self.path.display(), "answer call failed");
            return Vec::new();
        };
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&response).unwrap_or_default();
        inputs
            .into_iter()
            .zip(parsed)
            .filter_map(|((input, _), entry)| {
                let output = entry.get("output")?.as_str()?;
                let output = u.string(output);
                Some(Row {
                    rel: self.relation,
                    args: vec![
                        u.compound("const", vec![input]),
                        u.compound("const", vec![output]),
                    ],
                })
            })
            .collect()
    }

    fn poll(&mut self, _u: &mut Universe, _timeout: Duration) -> Vec<Row> {
        Vec::new()
    }

    fn armed(&self) -> bool {
        false
    }
}
