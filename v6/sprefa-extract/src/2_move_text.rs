//! `extract move ... --text-refs`: report-only scan for old-path spellings a
//! move leaves behind in non-TS text. Manifests and script strings are out of
//! scope for the move verb per
//! `plans/2026-08-25-extract-move-typescript.PLAN.md:642-644`; this pass
//! never writes a byte, only names the rows a human still has to fix.
//! @comment-ok: module header, the seam this pass and `1_move_manifest.rs` share

use std::path::Path;

use sprefa_extract::is_ts_family;

use crate::move_manifest::{
    build_paths, compiled_spellings, dirname, owning_package, package_manifests, parse_run,
    rel_string, walk_root, MoveRow,
};

pub fn run<I>(args: I) -> Result<(), String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString> + Clone,
{
    let plan = parse_run(args)?;
    if !plan.text_refs {
        return Ok(());
    }
    let manifests = package_manifests(&plan.root);
    let package_dirs: Vec<String> = manifests.iter().map(|rel| dirname(rel)).collect();
    let per_move: Vec<Vec<(String, String)>> = plan
        .moves
        .iter()
        .map(|mv| candidates(&plan.root, &package_dirs, mv))
        .collect();
    for hit in scan(&plan.root, &per_move) {
        println!(
            "text-ref {}:{} {} -> {}",
            hit.file, hit.line, hit.matched, hit.proposed
        );
    }
    Ok(())
}

/// Every spelling this move can leave behind, longest first (a scan line
/// matches the most specific one and stops).
fn candidates(root: &Path, package_dirs: &[String], mv: &MoveRow) -> Vec<(String, String)> {
    let mut out = segment_pairs(&mv.old_rel, &mv.new_rel);
    if let Some(dir) = owning_package(package_dirs, &mv.old_rel) {
        let build = build_paths(&root.join(dir));
        for (old_c, new_c) in compiled_spellings(dir, &build, &mv.old_rel, &mv.new_rel) {
            out.push((format!("../{old_c}"), format!("../{new_c}")));
            out.push((old_c, new_c));
        }
    }
    out.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    out
}

/// `old_rel` suffixes down to two segments, paired with `new_rel`'s suffix
/// after dropping the same leading segment count.
fn segment_pairs(old_rel: &str, new_rel: &str) -> Vec<(String, String)> {
    let old_segments: Vec<&str> = old_rel.split('/').collect();
    let new_segments: Vec<&str> = new_rel.split('/').collect();
    let total = old_segments.len();
    let start_k = if total >= 2 { 2 } else { 1 };
    let mut out = Vec::new();
    for k in start_k..=total {
        let dropped = total - k;
        if dropped > new_segments.len() {
            continue;
        }
        out.push((
            old_segments[dropped..].join("/"),
            new_segments[dropped..].join("/"),
        ));
    }
    out
}

struct Hit {
    file: String,
    line: usize,
    matched: String,
    proposed: String,
}

fn scan(root: &Path, per_move: &[Vec<(String, String)>]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for path in walk_root(root) {
        let Some(rel) = rel_string(root, &path) else {
            continue;
        };
        if rel == "package.json" || rel.ends_with("/package.json") || is_ts_family(&rel) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (line_number, line) in text.lines().enumerate() {
            for candidates in per_move {
                if let Some((matched, proposed)) = candidates
                    .iter()
                    .find(|(old, _)| line.contains(old.as_str()))
                {
                    hits.push(Hit {
                        file: rel.clone(),
                        line: line_number + 1,
                        matched: matched.clone(),
                        proposed: proposed.clone(),
                    });
                }
            }
        }
    }
    hits.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.matched.cmp(&right.matched))
    });
    hits
}
