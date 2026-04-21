use super::*;
use crate::cursor::{PathSeg, SprfPath};
use std::fs;
use globset::Glob;
use walkdir::WalkDir;

pub fn run(_ctx: &mut OpCtx, modes: &[TermMode], _input: Vec<Cursor>) -> Result<OpAction, Diags> {
    let pattern = modes.first().and_then(term_to_string).unwrap_or_else(|| "*".into());
    let glob = Glob::new(&pattern)
        .map_err(|e| vec![super::super::diag::Diag::new(format!("fs: bad glob: {e}"))])?;
    let matcher = glob.compile_matcher();
    let base = std::env::current_dir().unwrap_or_else(|_| ".".into());

    let mut out = Vec::new();
    for entry in WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let rel = path.strip_prefix(&base).unwrap_or(path);
        if matcher.is_match(rel) {
            let content = fs::read(path).unwrap_or_default();
            let mut c = Cursor::new(
                SprfPath(vec![PathSeg::Step(format!("fs:{}", rel.display()).into())]),
                Arc::from(content),
            );
            c.captures.insert(
                super::super::cursor::CaptureSlot(0),
                super::super::lower::Scalar::String(rel.to_string_lossy().into()),
            );
            out.push(c);
        }
    }
    Ok(OpAction::Emit(out))
}
