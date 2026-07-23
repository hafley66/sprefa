//! Commit-1 dispatch: single-threaded. The rayon orchestrator + the
//! arena-per-worker budget land in the parallelism lab (epic 4). This drives one
//! file through parse -> project(CstF) -> (bundle, strings); the bin and the
//! snapshot test both go through here.

use crate::family::{CallF, CstF, TypeF};
use crate::lang::{AstGrepParser, CallProjector, CstProjector, OxcParser, TypeProjector};
use crate::rows::FamilyBundle;
use crate::seams::{ParseError, Parser, Project};
use crate::shape::Strings;

/// Parse + project one file's bytes to its CstF bundle (and the interner it
/// filled). The generic rayon `dispatch` over many `ExtractJob`s lands with the
/// dispatch lab; this is the single-file piping core both the bin and the
/// snapshot exercise.
pub fn dispatch_cst(
    path: &str,
    content: &[u8],
    parser: &AstGrepParser,
    projector: &CstProjector,
) -> Result<(FamilyBundle<CstF>, Strings), ParseError> {
    let arena = parser.make_arena();
    let parsed = parser.parse(&arena, path, content)?;
    let mut bundle = FamilyBundle::<CstF>::default();
    let mut strings = Strings::new();
    projector.project(&parsed, &mut strings, &mut bundle);
    Ok((bundle, strings))
}

/// Parse + project one file's bytes to its TypeF bundle via oxc. Same shape as
/// `dispatch_cst`; the arena is the oxc `Allocator` the `Program` borrows.
pub fn dispatch_type(
    path: &str,
    content: &[u8],
    parser: &OxcParser,
    projector: &TypeProjector,
) -> Result<(FamilyBundle<TypeF>, Strings), ParseError> {
    let arena = parser.make_arena();
    let parsed = parser.parse(&arena, path, content)?;
    let mut bundle = FamilyBundle::<TypeF>::default();
    let mut strings = Strings::new();
    projector.project(&parsed, &mut strings, &mut bundle);
    Ok((bundle, strings))
}

/// Parse + project one file's bytes to its CallF bundle via oxc. Same shape as
/// `dispatch_type` (shares the parse; a second projection over the same tree).
pub fn dispatch_call(
    path: &str,
    content: &[u8],
    parser: &OxcParser,
    projector: &CallProjector,
) -> Result<(FamilyBundle<CallF>, Strings), ParseError> {
    let arena = parser.make_arena();
    let parsed = parser.parse(&arena, path, content)?;
    let mut bundle = FamilyBundle::<CallF>::default();
    let mut strings = Strings::new();
    projector.project(&parsed, &mut strings, &mut bundle);
    Ok((bundle, strings))
}
