//! Async facade over the sync `Extract` core — the seam the REACTIVE engine holds.
//!
//! Why a shell and not "make it all async": the CPU core (parse / project / merge)
//! has NOTHING to await. Making it async is colored-function tax for zero gain
//! (crate-map sync/async ruling). It stays sync + rayon. This facade exists
//! because extraction is driven ON-DEMAND by the demand layer (a cold blob parses
//! when a subscription activates its cone), and that driving is async — so the
//! engine awaits this facade instead of `spawn_blocking`-ing at every cold blob.
//!
//! This is the `Store` (sync) / `StoreHandle` (async) split, applied to extract:
//! the sync core is reusable from sync callers (tests, the batch importer) and
//! the async shell from the reactive engine. One bridge, one `spawn_blocking` per
//! dispatch batch — the legitimate batch case (like the store's writer thread),
//! never per request and never per blob.
//!
//! OWNERSHIP (why two traits, not async methods on `Extract`): the sync core
//! borrows `&ProjectCx<'a>` — one refresh, lifetime-tied, lives in a rayon scope.
//! An async future must be `'static + Send` to run on tokio, so it cannot hold a
//! borrowed `&'a ProjectCx` across an await. The facade therefore takes OWNED
//! inputs: a `ProjectView` (an owned snapshot of the file set + manifests +
//! digest) and `ExtractJob`s that already carry their bytes (`Vec<u8>`,
//! pre-staged by the engine's async IO). The borrowed sync core and the owned
//! async shell are genuinely different shapes — folding them into one trait would
//! lie about one of them.

#![allow(async_fn_in_trait)]

use crate::_3_extract::_2_traits::{ExtractBudget, ExtractJob, Source};
use crate::_3_extract::_4_scip::ScipError;
use crate::_3_extract::_7_tasks::ExtractOutput;
use crate::_3_extract::_1_mask::ProjectDigest;
use crate::_3_extract::_0_shape::NameId;
use std::path::PathBuf;

/// Owned snapshot of the project context — the async twin of the sync core's
/// borrowed `ProjectCx<'a>`. Built by the engine (async IO) and handed to the
/// facade; lives for the dispatch, dropped after. `'static + Send` so it can
/// cross the runtime and move into a `spawn_blocking` closure.
pub struct ProjectView {
    pub digest: ProjectDigest,
    /// Project-relative tracked paths (owned). The resolution universe.
    pub files: Vec<NameId>,
    /// Manifest path -> contents (Cargo.toml / package.json / go.mod).
    pub manifests: Vec<(NameId, Vec<u8>)>,
    /// Owned byte reader: given a project-relative path, return its bytes. None
    /// for paths the engine did not pre-stage (the facade only sees staged jobs).
    pub reader: Option<Box<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>>,
}

/// The async shell. The reactive engine holds an impl of this; it never reaches
/// past it into the sync core. Each method is the on-demand entry point for one
/// thing the demand cone can activate.
pub trait ReactiveExtract {
    /// 🧪 OPEN · on-demand cone parse: fan the masked `jobs` out on rayon inside
    /// one `spawn_blocking`, return the merged outputs. The engine `.await`s this
    /// when a subscription activates a cone that includes cold blobs. Cache hits
    /// (blob+lang+mask already extracted) short-circuit before the rayon hop.
    async fn dispatch_cone(
        &self,
        jobs: Vec<ExtractJob>,
        view: ProjectView,
        sources: &[Source],
        budget: ExtractBudget,
    ) -> Vec<ExtractOutput>;

    /// 🧪 OPEN · run the foreign SCIP indexer over `root` as a subprocess via
    /// `tokio::process::Command` (genuinely IO-shaped — waiting on a child). The
    /// sync `ScipSource::build` stays for the batch CLI path; this is the
    /// reactive one (mtime says the index is stale -> rebuild without blocking).
    async fn build_scip(
        &self,
        root: PathBuf,
        indexer: &'static str,
    ) -> Result<(), ScipError>;

    // NOTE: `load_scip` (parse index.scip) stays SYNC on the core — it is a fast
    // file read + protobuf decode (CPU), and the engine `spawn_blocking`s it only
    // if the index is huge. Not duplicated here on purpose.
}

/// The stub. The real impl owns a sync `Extract` core + a rayon pool + a tokio
/// handle; `dispatch_cone` is `tokio::task::spawn_blocking(move || core.dispatch(...))`.
pub struct Tasks;

impl ReactiveExtract for Tasks {
    async fn dispatch_cone(
        &self,
        _jobs: Vec<ExtractJob>,
        _view: ProjectView,
        _sources: &[Source],
        _budget: ExtractBudget,
    ) -> Vec<ExtractOutput> {
        todo!("spawn_blocking -> Extract::dispatch(jobs, &view.into(), sources, &budget)")
    }
    async fn build_scip(&self, _root: PathBuf, _indexer: &'static str) -> Result<(), ScipError> {
        todo!("tokio::process::Command::new(indexer).args(...).output().await")
    }
}
