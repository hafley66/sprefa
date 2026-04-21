//! Scan-pointer checker — Pass A of the runner's two-phase verification.
//!
//! # Role
//!
//! Walker ops (json/line/…) stamp captures as `Tri::Claimed` with a sigil
//! hint. Bind ops (repo/rev/fs) stamp their own outputs as `Tri::Verified`
//! immediately. What this pass does is take the Claimed residue and try to
//! flip each capture to Verified or Missing using the static config (for
//! repo/rev) and a git-tree probe (for fs).
//!
//! # Algorithm
//!
//! For every Capture in every RuleResult row in the `ResultStore`:
//!
//! - If `scan_pointer.is_none()` or `verified != Tri::Claimed`, skip.
//! - Match on the sigil:
//!     - `"repo"` → membership test in `config.repos`.
//!     - `"rev"`  → membership test in `config.revs`.
//!     - `"fs"`   → call `fs_path_in_tree(repo, rev, path)`. If the
//!       sibling captures for `$repo` and `$rev` are not yet Verified,
//!       the probe can't run — stamp Missing for safety and leave a TODO
//!       for the real git2 wiring. Sibling lookup is a best-effort scan
//!       of the surrounding `CaptureMap`.
//!     - Anything else → leave alone. Lowering-time diags (Phase 0,
//!       `xref/unknown-scan-pointer`) already cover unknown sigils.
//!
//! On Verified: mutate the Capture in place (via row mutex).
//! On Missing:  mutate + push a `ScanPointerUnverified` Warn.
//!
//! # Output
//!
//! A `Vec<Box<dyn Diagnostic>>` is returned rather than emitted on a sink
//! so the runner can batch them into a single `RunEvent::DiagBatch`.
//!
//! # What this pass is NOT
//!
//! - It does not perform the demand scan that populates the configured
//!   universe. The runner's Pass B (not yet wired) diffs the Missing set
//!   against the walker stamps and decides which Missing values are worth
//!   re-scanning.
//! - It does no parse-site anchoring beyond the ParseSite attached to the
//!   Capture's originating op. In the current model Captures don't carry
//!   their own site, so the diag uses a synthetic zero-span site on the
//!   capture's rule-declaration path. When cursors gain per-capture sites
//!   (scan-pointer runtime follow-up), upgrade `site_for` below.

use std::path::Path;
use std::sync::Arc;

use crate::types::{Capture, ParseSeg, ParseSite, Tri};
use crate::diagnostic::{Diagnostic, ScanPointerUnverified};
use crate::config::Config;
use crate::result_store::ResultStore;

/// Git-tree presence probe. Stubbed: returns `true` so the pass is a
/// no-op on fs for now. Real impl will open the repo on `repo`, resolve
/// `rev`, and check whether `path` exists in the resulting tree.
///
/// TODO: wire git2::Repository::find_tree + TreeWalkMode traversal.
pub fn fs_path_in_tree(_repo: &str, _rev: &str, _path: &str) -> bool {
    true
}

/// Synthetic ParseSite used when the Capture has no attached site. Anchors
/// the diag at byte 0 of the `.sprf` source so the renderer at least points
/// at the right file. Upgrade once captures carry sites.
fn site_for(_cap: &Capture) -> ParseSite {
    ParseSite {
        file:       Arc::from(Path::new("<scan-check>")),
        path:       Arc::from(&[ParseSeg::Top { index: 0 }][..]),
        byte_range: 0..0,
    }
}

fn sibling_verified(row_captures: &std::collections::HashMap<Arc<str>, Capture>,
                    sigil: &str) -> Option<Arc<str>> {
    for cap in row_captures.values() {
        if let Some(p) = &cap.scan_pointer {
            if p.as_ref() == sigil && cap.verified == Tri::Verified {
                return Some(cap.value.clone());
            }
        }
    }
    None
}

pub fn check_scan_pointers(store: &ResultStore, config: &Config)
    -> Vec<Box<dyn Diagnostic>>
{
    let _s = tracing::info_span!("scan_check.check_scan_pointers").entered();
    let mut diags: Vec<Box<dyn Diagnostic>> = Vec::new();

    let entries: Vec<_> = {
        let map = store.by_rule.lock().unwrap();
        map.values().cloned().collect()
    };

    for rr in entries {
        let mut rows = rr.rows.lock().unwrap();
        for row in rows.iter_mut() {
            // Snapshot for sibling lookup (avoid borrow conflicts with the
            // mutable walk below).
            let snapshot = row.clone();

            for cap in row.values_mut() {
                if cap.verified != Tri::Claimed { continue; }
                let Some(sigil) = cap.scan_pointer.clone() else { continue; };

                let verified = match sigil.as_ref() {
                    "repo" => config.repos.iter().any(|r| r.as_ref() == cap.value.as_ref()),
                    "rev"  => config.revs.iter().any(|r| r.as_ref() == cap.value.as_ref()),
                    "fs"   => {
                        let repo = sibling_verified(&snapshot, "repo");
                        let rev  = sibling_verified(&snapshot, "rev");
                        match (repo, rev) {
                            (Some(r), Some(v)) => fs_path_in_tree(&r, &v, &cap.value),
                            // Conservative: without verified siblings we
                            // can't probe the git tree, so stamp Missing.
                            _ => false,
                        }
                    }
                    _ => continue,
                };

                if verified {
                    cap.verified = Tri::Verified;
                } else {
                    cap.verified = Tri::Missing;
                    diags.push(Box::new(ScanPointerUnverified {
                        site:  site_for(cap),
                        sigil: sigil.clone(),
                        value: cap.value.clone(),
                    }));
                }
            }
        }
    }

    diags
}
