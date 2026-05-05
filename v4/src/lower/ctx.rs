use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::{FactStore, Pipe};

use crate::{Cursor, Interner};

/// Lower-time context. Carries the fact store, optional Interner, root
/// path, and a binding map used to resolve `${X}` interpolations in
/// DSL bodies. Builder-style mutation via `with_*`.
pub struct LowerCtx {
    pub store:    Arc<dyn FactStore<Cursor>>,
    pub interner: Option<Arc<Interner>>,
    pub root:     PathBuf,
    /// Capture name → Pipe that grounds to a constant string. Used by
    /// `str` to substitute `${X}` at lower-time.
    pub bindings: HashMap<Arc<str>, Pipe<Cursor>>,
}

impl LowerCtx {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, root: PathBuf) -> Self {
        Self {
            store,
            interner: None,
            root,
            bindings: HashMap::new(),
        }
    }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i); self
    }
    pub fn with_binding(mut self, name: impl Into<Arc<str>>, pipe: Pipe<Cursor>) -> Self {
        self.bindings.insert(name.into(), pipe); self
    }
}

#[derive(Debug, Clone)]
pub enum LowerError {
    /// Validate emitted diags; lower refuses to run.
    Validate(Vec<effect_runtime::v2::Diag>),
    /// Lower-time only: a `${X}` reference has no binding.
    UnboundCapture(Arc<str>),
    /// Anything else.
    Unknown(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::Validate(ds) => write!(f, "validate: {} diag(s)", ds.len()),
            LowerError::UnboundCapture(n) => write!(f, "unbound capture: ${{{n}}}"),
            LowerError::Unknown(s) => write!(f, "{s}"),
        }
    }
}
impl std::error::Error for LowerError {}
