//! `extract move ... --text-refs`: report-only scan for old-path spellings a
//! move leaves behind in plain text. Rewriting text carriers is out of scope for
//! the move verb (`plans/2026-08-25-extract-move-typescript.PLAN.md:642-644`);
//! this pass never writes a byte, only names the rows a human still has to fix.
//! @comment-ok: module header, the seam list every move file opens with

use std::collections::BTreeSet;

use sprefa_extract::{rehome_for, rehomes, MoveCx};

/// Every leftover spelling this run's batch can leave in a file no `Rehome` arm
/// owns, sorted by (file, line, matched).
pub fn report(cx: &MoveCx) {
    // A manifest is a carrier its own arm already rewrote through a Replace.
    let carriers: BTreeSet<String> = rehomes().iter().flat_map(|arm| arm.manifests(cx)).collect();
    let per_move: Vec<Vec<(String, String)>> = cx
        .moved()
        .iter()
        .map(|(old, new)| candidates(cx, old, new))
        .collect();
    for hit in scan(cx, &carriers, &per_move) {
        println!(
            "text-ref {}:{} {} -> {}",
            hit.file, hit.line, hit.matched, hit.proposed
        );
    }
}

/// Every spelling one move can leave behind, longest first (a scan line matches
/// the most specific one and stops).
fn candidates(cx: &MoveCx, old: &str, new: &str) -> Vec<(String, String)> {
    let mut out = segment_pairs(old, new);
    if let Some(arm) = rehome_for(old) {
        out.extend(arm.text_spellings(cx, old, new));
    }
    out.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    out
}

/// `old` suffixes down to two segments, paired with `new`'s suffix after
/// dropping the same leading segment count.
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

fn scan(cx: &MoveCx, carriers: &BTreeSet<String>, per_move: &[Vec<(String, String)>]) -> Vec<Hit> {
    let mut hits = Vec::new();
    for rel in cx.files() {
        if carriers.contains(rel) || rehome_for(rel).is_some() {
            continue;
        }
        let Some(text) = cx.text(rel) else {
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
