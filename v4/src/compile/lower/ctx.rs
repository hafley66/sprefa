use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::{FactStore, Pipe, ProbeSink};

use crate::store::SprfStore;
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
    /// Optional per-emit probe sink. When set, the walker wraps every
    /// lowered Component in a `SpannedComponent` that fires a probe on
    /// each emitted cursor, tagged with the source op's byte range.
    /// Default = `None` = zero overhead beyond a single Option check.
    pub probe:    Option<Arc<dyn ProbeSink<Cursor>>>,
    /// Optional content-derived intern store. Set via
    /// `with_sprf_store(...)` from the host (`SprfState::run` or
    /// equivalent). When present, source/pattern emitters stamp the
    /// coord-space side of Cursor (`value_id`, `at`, `terms`) alongside
    /// legacy `raw_terms` writes. When absent, only legacy writes fire.
    pub sprf_store: Option<Arc<SprfStore>>,
}

impl LowerCtx {
    pub fn new(store: Arc<dyn FactStore<Cursor>>, root: PathBuf) -> Self {
        Self {
            store,
            interner: None,
            root,
            bindings: HashMap::new(),
            probe:    None,
            sprf_store: None,
        }
    }
    pub fn with_interner(mut self, i: Arc<Interner>) -> Self {
        self.interner = Some(i); self
    }
    pub fn with_probe(mut self, p: Arc<dyn ProbeSink<Cursor>>) -> Self {
        self.probe = Some(p); self
    }
    pub fn with_binding(mut self, name: impl Into<Arc<str>>, pipe: Pipe<Cursor>) -> Self {
        self.bindings.insert(name.into(), pipe); self
    }
    /// Attach the content-derived intern store. Emitters lowered after
    /// this call stamp coord-space terms alongside legacy raw_terms.
    pub fn with_sprf_store(mut self, s: Arc<SprfStore>) -> Self {
        self.sprf_store = Some(s); self
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
