//! `extract move ... --text-refs` and `extract rename ... --text-refs`:
//! report-only scans for old spellings a verb leaves behind in plain text.
//! Rewriting text carriers is out of scope for both verbs
//! (`plans/2026-08-25-extract-move-typescript.PLAN.md:642-644`,
//! `plans/2026-08-27-extract-rename.PLAN.md` scope fence); these passes never
//! write a byte, only name the rows a human still has to fix.
//! @comment-ok: module header, the seam list every move file opens with

use std::collections::BTreeSet;

use sprefa_extract::{rehome_for, rehomes, rename_for, MoveCx, RenameCx, RenameRequest};

/// Every leftover spelling this run's batch can leave in a file no `Rehome` arm
/// owns, sorted by (file, line, matched).
pub fn report(cx: &MoveCx) {
    // A manifest is a carrier its own arm already rewrote through a Replace.
    let carriers: BTreeSet<String> = rehomes()
        .iter()
        .filter_map(|arm| arm.manifests)
        .flat_map(|leg| leg.manifests(cx))
        .collect();
    let per_move: Vec<Vec<(String, String)>> = cx
        .moved()
        .iter()
        .map(|(old, new)| candidates(cx, old, new))
        .collect();
    let skip_file = |rel: &str| carriers.contains(rel) || rehome_for(rel).is_some();
    for hit in scan(cx, &skip_file, &|_, _| false, &per_move) {
        println!(
            "text-ref {}:{} {} -> {}",
            hit.file, hit.line, hit.matched, hit.proposed
        );
    }
}

/// Every leftover spelling one rename can leave behind in a line the plan did
/// not rewrite, sorted by (file, line, matched). `rewritten` carries every
/// (file, line) a staged edit covers; those lines hold no old spelling after
/// the write, so the scan never names one.
pub fn report_rename(
    cx: &RenameCx,
    request: &RenameRequest,
    rewritten: &BTreeSet<(String, usize)>,
) {
    let Some(arm) = rename_for(&request.anchor) else {
        return;
    };
    let bare = arm.text_spellings(cx, request);
    let mut spellings: Vec<(String, String)> = Vec::with_capacity(bare.len() * (QUOTES.len() + 1));
    for (old, new) in &bare {
        spellings.push((old.clone(), new.clone()));
        for (open, close) in QUOTES {
            spellings.push((format!("{open}{old}{close}"), format!("{open}{new}{close}")));
        }
    }
    // Longest first: a quoted literal wins the line over the bare name.
    spellings.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
    let skip_line = |rel: &str, line: usize| rewritten.contains(&(rel.to_string(), line));
    for hit in scan(cx, &|_| false, &skip_line, &[spellings]) {
        println!(
            "text-ref {}:{} {} -> {}",
            hit.file, hit.line, hit.matched, hit.proposed
        );
    }
}

/// The quote styles a string-literal carrier wears a name inside.
const QUOTES: [(&str, &str); 3] = [("\"", "\""), ("'", "'"), ("`", "`")];

/// Every spelling one move can leave behind, longest first (a scan line matches
/// the most specific one and stops).
fn candidates(cx: &MoveCx, old: &str, new: &str) -> Vec<(String, String)> {
    let mut out = segment_pairs(old, new);
    if let Some(leg) = rehome_for(old).and_then(|arm| arm.text_spellings) {
        out.extend(leg.text_spellings(cx, old, new));
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

/// The corpus view a text scan reads. `MoveCx` and `RenameCx` are the two
/// verbs' views over one walk; the scan is shared, never duplicated.
trait ScanCx {
    fn files(&self) -> &[String];
    fn text(&self, rel: &str) -> Option<String>;
}

impl ScanCx for MoveCx {
    fn files(&self) -> &[String] {
        self.files()
    }

    fn text(&self, rel: &str) -> Option<String> {
        self.text(rel)
    }
}

impl ScanCx for RenameCx {
    fn files(&self) -> &[String] {
        self.files()
    }

    fn text(&self, rel: &str) -> Option<String> {
        self.text(rel)
    }
}

/// One row per unmatched line, sorted by (file, line, matched). A file the
/// `skip_file` predicate names is never read; a line `skip_line` names is
/// never matched.
fn scan(
    cx: &impl ScanCx,
    skip_file: &dyn Fn(&str) -> bool,
    skip_line: &dyn Fn(&str, usize) -> bool,
    per_scan: &[Vec<(String, String)>],
) -> Vec<Hit> {
    let mut hits = Vec::new();
    for rel in cx.files() {
        if skip_file(rel) {
            continue;
        }
        let Some(text) = cx.text(rel) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            if skip_line(rel, line_number) {
                continue;
            }
            for candidates in per_scan {
                if let Some((matched, proposed)) = candidates
                    .iter()
                    .find(|(old, _)| line.contains(old.as_str()))
                {
                    hits.push(Hit {
                        file: rel.clone(),
                        line: line_number,
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
