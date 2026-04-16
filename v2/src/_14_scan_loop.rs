//! Scan-check demand loop.
//!
//! # Role
//!
//! Walker ops stamp content-side captures as `Tri::Claimed`. The checker
//! (`_13_scan_check`) flips them to `Verified` or `Missing` against the
//! static `Config`. This loop chases the `Missing` set: it extends
//! `Config.repos`/`Config.revs` with the claimed values, re-runs the
//! pipelines under the new config, re-checks, and repeats until the config
//! stops growing or `depth` passes have run.
//!
//! # Algorithm
//!
//! ```text
//! for pass in 0..depth:
//!     store.clear()
//!     run_pass(config, store)      # re-run all pipelines → Claimed stamps
//!     check_scan_pointers(&store)  # Claimed → Verified / Missing + Warn
//!     unscanned = store.unscanned()
//!     extend config with any new (repo|rev, value) in unscanned
//!     if config did not grow → fixed point, keep this pass's diags, break
//!     if pass == depth - 1   → emit depth-exhausted Warn, keep diags
//! ```
//!
//! # Why clear between passes
//!
//! Re-running pipelines against the grown config re-stamps every capture
//! fresh. Without clearing, the store would accumulate duplicate rows
//! carrying stale `Missing` stamps from earlier config generations.
//!
//! # Diag semantics
//!
//! Only the final pass's diags are returned. Intermediate `Missing`
//! captures are superseded by the next pass once their values enter Config.

use std::sync::Arc;

use crate::_1_diagnostic::{Diagnostic, ScanPointerDepthExhausted};
use crate::_2_config::Config;
use crate::_12_result_store::ResultStore;
use crate::_13_scan_check::check_scan_pointers;
use crate::_0_types::{ParseSeg, ParseSite};
use std::path::Path;

/// Default demand-loop depth. v1 used implicit single-pass + external
/// driver; v2 bundles discovery into the run and caps at 8 to catch
/// cyclic claim explosion without heuristics.
pub const DEFAULT_DEPTH: usize = 8;

pub struct ScanLoopResult {
    pub config:          Arc<Config>,
    pub diags:           Vec<Box<dyn Diagnostic>>,
    pub passes:          usize,
    pub depth_exhausted: bool,
}

/// Drive the demand loop. `run_pass` is the caller's "run all pipelines into
/// `store` under `config`" function; it returns pipeline-side diags (parse,
/// xref, op-local warns) from that pass. The checker + extension logic
/// lives here.
/// Per-pass outcome emitted by the scan-loop stream.
struct Pass {
    config:          Arc<Config>,
    diags:           Vec<Box<dyn Diagnostic>>,
    passes:          usize,
    depth_exhausted: bool,
    fixed_point:     bool,
}

/// Drive the demand loop. `run_pass` is the caller's "run all pipelines into
/// `store` under `config`" function; it returns pipeline-side diags (parse,
/// xref, op-local warns) from that pass. The checker + extension logic
/// lives here.
///
/// Shape: `std::iter::from_fn` — the sync-Rust analog of `Rx.expand`. Each
/// call to `next()` yields one `Pass`; state (cfg, pass index, stop flag) is
/// threaded via captured locals. When `run_pass` turns async end-to-end, swap
/// this for `futures::stream::unfold` with the same body.
pub fn run_scan_loop<F>(
    store:          Arc<ResultStore>,
    initial_config: Arc<Config>,
    depth:          usize,
    mut run_pass:   F,
) -> ScanLoopResult
where
    F: FnMut(Arc<Config>, Arc<ResultStore>) -> Vec<Box<dyn Diagnostic>>,
{
    let mut i        = 0usize;
    let mut cfg      = initial_config.clone();
    let mut stop     = false;
    let store_ref    = store.clone();

    let passes_iter = std::iter::from_fn(move || {
        if stop || i >= depth { return None; }

        store_ref.clear();
        let pipe_diags  = run_pass(cfg.clone(), store_ref.clone());
        let check_diags = check_scan_pointers(&store_ref, &cfg);

        let (next_cfg, grew) = grow_from_unscanned(&cfg, &store_ref.unscanned());

        let mut diags = pipe_diags;
        diags.extend(check_diags);

        let reached_depth   = i + 1 == depth;
        let depth_exhausted = grew && reached_depth;
        if depth_exhausted {
            diags.push(Box::new(ScanPointerDepthExhausted {
                site:  synthetic_site(),
                depth,
            }));
        }

        let fixed_point = !grew;
        let pass = Pass {
            // Emit post-growth cfg so the final outcome reflects every
            // discovered name. For fixed_point, next_cfg == cfg anyway.
            config:          next_cfg.clone(),
            diags,
            passes:          i + 1,
            depth_exhausted,
            fixed_point,
        };

        i    += 1;
        cfg   = next_cfg;
        stop  = fixed_point || depth_exhausted;
        Some(pass)
    });

    let final_pass = passes_iter.last().expect("run_scan_loop: depth must be >= 1");

    ScanLoopResult {
        config:          final_pass.config,
        diags:           final_pass.diags,
        passes:          final_pass.passes,
        depth_exhausted: final_pass.depth_exhausted,
    }
}

/// Pure: (cfg, unscanned claims) -> (next_cfg, grew?).
fn grow_from_unscanned(
    cfg:       &Arc<Config>,
    unscanned: &[(Arc<str>, Arc<str>)],
) -> (Arc<Config>, bool) {
    let mut next = (**cfg).clone();
    let mut grew = false;
    for (sigil, value) in unscanned {
        let bucket = match sigil.as_ref() {
            "repo" => &mut next.repos,
            "rev"  => &mut next.revs,
            _      => continue, // fs + custom: Missing stays Missing.
        };
        if !bucket.iter().any(|r| r.as_ref() == value.as_ref()) {
            bucket.push(value.clone());
            grew = true;
        }
    }
    if grew { (Arc::new(next), true) } else { (cfg.clone(), false) }
}

fn synthetic_site() -> ParseSite {
    ParseSite {
        file:       Arc::from(Path::new("<scan-loop>")),
        path:       Arc::from(&[ParseSeg::Top { index: 0 }][..]),
        byte_range: 0..0,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_types::{Capture, Tri};
    use crate::_2_config::RuntimeConfig;
    use crate::_12_result_store::CaptureMap;
    use std::collections::HashMap;

    fn base_config() -> Arc<Config> {
        Arc::new(Config {
            repos:        vec![Arc::from("known")],
            revs:         vec![Arc::from("main")],
            fs_exclude:   vec![],
            sprf_files:   vec![],
            shell_allow:  vec![],
            runtime: RuntimeConfig {
                worker_threads:       1,
                buffer_size:          64,
                flush_interval_ms:    100,
                collect_witnesses:    false,
                xref_cartesian_limit: 1_000,
                max_passes:           8,
                max_claims_per_pass:  10_000,
                max_cursors_per_root: 1_000_000,
            },
            content_hash: 0,
        })
    }

    fn claimed_row(pairs: &[(&str, &str, &str)]) -> CaptureMap {
        // (var, value, sigil)
        let mut m: HashMap<Arc<str>, Capture> = HashMap::new();
        for (k, v, s) in pairs {
            let c = Capture::new(Arc::from(*v))
                .with_scan(Arc::from(*s), Tri::Claimed);
            m.insert(Arc::from(*k), c);
        }
        m
    }

    fn stamp_rows(store: &Arc<ResultStore>, rule: &str, rows: Vec<CaptureMap>) {
        let rn: Arc<str> = Arc::from(rule);
        for r in rows { store.append(&rn, r); }
        store.mark_complete(&rn);
    }

    // ------------- Test: no new claims = single pass -------------
    #[test]
    fn single_pass_when_all_verified() {
        let store = Arc::new(ResultStore::new());
        let cfg   = base_config();

        let res = run_scan_loop(store.clone(), cfg, DEFAULT_DEPTH, |_cfg, s| {
            stamp_rows(&s, "r", vec![claimed_row(&[("R", "known", "repo")])]);
            vec![]
        });

        assert_eq!(res.passes, 1);
        assert!(!res.depth_exhausted);
        assert!(res.diags.iter().all(|d| d.code() != "scan-pointer/unverified"));
    }

    // ------------- Test: demand loop grows config -------------
    #[test]
    fn demand_loop_grows_config() {
        let store = Arc::new(ResultStore::new());
        let cfg   = base_config();

        let res = run_scan_loop(store.clone(), cfg, DEFAULT_DEPTH, |cfg, s| {
            // Pass 0 claims "discovered"; pass 1 claims "known" + "discovered"
            // (both in config by then → fixed point).
            let has_disc = cfg.repos.iter().any(|r| r.as_ref() == "discovered");
            if has_disc {
                stamp_rows(&s, "r", vec![
                    claimed_row(&[("R", "known", "repo")]),
                    claimed_row(&[("R", "discovered", "repo")]),
                ]);
            } else {
                stamp_rows(&s, "r", vec![
                    claimed_row(&[("R", "known", "repo")]),
                    claimed_row(&[("R", "discovered", "repo")]),
                ]);
            }
            vec![]
        });

        assert_eq!(res.passes, 2);
        assert!(!res.depth_exhausted);
        assert!(res.config.repos.iter().any(|r| r.as_ref() == "discovered"));
        assert!(res.diags.iter().all(|d| d.code() != "scan-pointer/unverified"));
    }

    // ------------- Test: diamond chain via tag sigils -------------
    #[test]
    fn diamond_tag_chain_converges() {
        // "deploy-a" rev claims "deploy-b" + "deploy-c" revs; each of those
        // claims back "deploy-a" (diamond). All rev sigils. Start with only
        // "main" rev. Loop should add all four, then terminate.
        let store = Arc::new(ResultStore::new());
        let cfg   = base_config();

        let res = run_scan_loop(store.clone(), cfg, DEFAULT_DEPTH, |_cfg, s| {
            stamp_rows(&s, "r", vec![
                claimed_row(&[("T", "deploy-a", "rev")]),
                claimed_row(&[("T", "deploy-b", "rev")]),
                claimed_row(&[("T", "deploy-c", "rev")]),
                claimed_row(&[("T", "main",     "rev")]),
            ]);
            vec![]
        });

        // Pass 0: all four Claimed, 3 are Missing → grow to include a,b,c.
        // Pass 1: all four Claimed, all in config → fixed point.
        assert_eq!(res.passes, 2);
        assert!(!res.depth_exhausted);
        let revs: Vec<&str> = res.config.revs.iter().map(|r| r.as_ref()).collect();
        for r in &["main", "deploy-a", "deploy-b", "deploy-c"] {
            assert!(revs.contains(r), "expected rev {r:?} in {revs:?}");
        }
    }

    // ------------- Test: depth cap fires on runaway -------------
    #[test]
    fn depth_cap_fires_on_cyclic_explosion() {
        let store = Arc::new(ResultStore::new());
        let cfg   = base_config();

        let res = run_scan_loop(store.clone(), cfg, 4, |cfg, s| {
            // Each pass invents a fresh repo name keyed by current config size.
            let fresh = format!("r{}", cfg.repos.len());
            stamp_rows(&s, "r", vec![
                claimed_row(&[("R", "known", "repo")]),
                claimed_row(&[("R", Box::leak(fresh.into_boxed_str()), "repo")]),
            ]);
            vec![]
        });

        assert_eq!(res.passes, 4);
        assert!(res.depth_exhausted);
        let depth_warn = res.diags.iter().any(|d| d.code() == "scan-pointer/depth-exhausted");
        assert!(depth_warn, "expected depth-exhausted diag; got {:?}",
            res.diags.iter().map(|d| d.code().to_string()).collect::<Vec<_>>());
    }

    // ------------- Test: unknown sigil (fs) stays Missing, no config grow -------------
    #[test]
    fn fs_missing_does_not_grow_config() {
        let store = Arc::new(ResultStore::new());
        let cfg   = base_config();
        let initial_repos = cfg.repos.len();
        let initial_revs  = cfg.revs.len();

        let res = run_scan_loop(store.clone(), cfg, DEFAULT_DEPTH, |_cfg, s| {
            // fs claim with no verified sibling repo/rev → checker stamps Missing.
            stamp_rows(&s, "r", vec![
                claimed_row(&[("P", "src/ghost.rs", "fs")]),
            ]);
            vec![]
        });

        assert_eq!(res.config.repos.len(), initial_repos);
        assert_eq!(res.config.revs.len(),  initial_revs);
        // Single pass: no growth → fixed point immediately.
        assert_eq!(res.passes, 1);
        let has_warn = res.diags.iter().any(|d| d.code() == "scan-pointer/unverified");
        assert!(has_warn, "expected scan-pointer/unverified diag for fs claim");
    }
}
