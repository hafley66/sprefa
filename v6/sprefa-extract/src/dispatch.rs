//! Commit-1 dispatch: single-threaded. The rayon orchestrator + the
//! arena-per-worker budget land in the parallelism lab (epic 4). This drives one
//! file through parse -> project(CstF) -> (bundle, strings); the bin and the
//! snapshot test both go through here.

use crate::family::CstF;
use crate::lang::{AstGrepParser, CstProjector};
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
    let root = parser.parse(path, content)?;
    let mut bundle = FamilyBundle::<CstF>::default();
    let mut strings = Strings::new();
    projector.project(&root, &mut strings, &mut bundle);
    Ok((bundle, strings))
}
