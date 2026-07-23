//! S5: the CPU trait seams. One seam per orthogonal dimension, no fat trait.
//! Closed vocabularies (per-family kind enums, `FamilyTag`, `Producer`) stay
//! enums; open extension points are traits.
//!
//!   Parser      content -> parsed CST handle. One impl per backing engine
//!               (commit 1: `AstGrepParser`, ast-grep grammars for rust/ts/go).
//!   Project<F>  phase 1: parsed -> FamilyBundle<F> (one per masked family).
//!   Resolve<F>  phase 2: cross-file resolution (commit 1+: not exercised; CstF
//!               has no phase 2).
//!   BlobSource  file bytes in, content-hashed (commit 1: declared; the engine
//!               supplies bytes directly).
//!
//! Commit 1 is single-threaded (the rayon orchestrator lands in the parallelism
//! lab); the trait seams exist and are implemented so the piping proof abides.

use std::fmt;

use crate::family::Family;
use crate::rows::FamilyBundle;
use crate::shape::Strings;

/// Why a parse failed. `NoGrammar` = no ast-grep grammar for the path's
/// extension; `Utf8` = the bytes were not valid UTF-8 (ast-grep parses `&str`).
#[derive(Debug)]
pub enum ParseError {
    NoGrammar(String),
    Utf8(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoGrammar(path) => write!(f, "no ast-grep grammar for {path}"),
            ParseError::Utf8(msg) => write!(f, "source is not valid UTF-8: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// content -> parsed CST handle. One impl per backing engine. `parse` takes the
/// path so the grammar can be selected (`SupportLang::from_path`); the lifetime
/// of the returned handle is owned by the caller and dropped after projection.
pub trait Parser: Sync + Send {
    type Parsed;
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn parse(&self, path: &str, content: &[u8]) -> Result<Self::Parsed, ParseError>;
}

/// Phase 1: one parse, masked projections. `strings` is the per-file interner
/// (shared across families); `sink` receives the bundle. The projector interns
/// names/kinds into `strings` and pushes rows into `sink`.
pub trait Project<F: Family>: Sync + Send {
    type Parsed;
    fn project(&self, parsed: &Self::Parsed, strings: &mut Strings, sink: &mut FamilyBundle<F>);
}

/// File bytes in, content-hashed out. SOURCE-AGNOSTIC: a corpus may be a git
/// worktree (`GitShellout`) or a plain directory (`Filesystem`). The content
/// hash is the cache key; how bytes were found never is. Commit 1 declares it;
/// the engine supplies bytes directly via `dispatch_cst`.
pub trait BlobSource: Sync + Send {
    fn blob(&self, path: &str) -> Option<Vec<u8>>;
}
