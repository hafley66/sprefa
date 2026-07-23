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
    Parse(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoGrammar(path) => write!(f, "no grammar for {path}"),
            ParseError::Utf8(msg) => write!(f, "source is not valid UTF-8: {msg}"),
            ParseError::Parse(msg) => write!(f, "parser failed: {msg}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// content -> parsed CST handle. One impl per backing engine. `parse` takes the
/// path so the grammar can be selected (`SupportLang::from_path`) and a
/// caller-owned `Arena` the returned handle borrows.
///
/// The arena is owned by the dispatch and lent to `parse`, because some engines
/// borrow their backing store: oxc's `Program<'a>` borrows its `Allocator`, so it
/// cannot be returned as an owned value the way ast-grep's `AstGrep` (which owns
/// its buffer) can. ast-grep sets `type Arena = ()`; oxc sets
/// `type Arena = oxc_allocator::Allocator`. The dispatch holds the arena across
/// `parse` + `project`, then drops it.
pub trait Parser: Sync + Send {
    type Arena;
    type Parsed<'a>
    where
        Self: 'a;

    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn make_arena(&self) -> Self::Arena;
    fn parse<'a>(
        &self,
        arena: &'a Self::Arena,
        path: &str,
        content: &'a [u8],
    ) -> Result<Self::Parsed<'a>, ParseError>;
}

/// Phase 1: one parse, masked projections. `strings` is the per-file interner
/// (shared across families); `sink` receives the bundle. The projector interns
/// names/kinds into `strings` and pushes rows into `sink`. `Parsed<'a>` matches
/// the paired `Parser`'s handle (the dispatch pins them together).
pub trait Project<F: Family>: Sync + Send {
    type Parsed<'a>;
    fn project<'a>(
        &self,
        parsed: &Self::Parsed<'a>,
        strings: &mut Strings,
        sink: &mut FamilyBundle<F>,
    );
}

/// File bytes in, content-hashed out. SOURCE-AGNOSTIC: a corpus may be a git
/// worktree (`GitShellout`) or a plain directory (`Filesystem`). The content
/// hash is the cache key; how bytes were found never is. Commit 1 declares it;
/// the engine supplies bytes directly via `dispatch_cst`.
pub trait BlobSource: Sync + Send {
    fn blob(&self, path: &str) -> Option<Vec<u8>>;
}
