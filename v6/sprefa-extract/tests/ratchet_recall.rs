//! The diet_scip ratchet over the committed oracles (user 2026-08-29:
//! "diet-scip ratcheted as high as possible so it is fast"). Ignored by
//! default: the corpora are machine-local checkouts (COMMON.md), so CI never
//! runs this leg. `just extract-ratchet` is the local recipe; it runs each
//! corpus as its own test process so `rss_mb` is that corpus's peak alone,
//! not the previous corpus's high-water mark.
//!
//! Mode per run: default asserts RATCHET.tsv (recall/precision 0.10 pt,
//! wall +15%, rss +10%); `RATCHET_BUMP=1` moves floors up and ceilings down
//! only; `RATCHET_FORCE=1` beside a bump rewrites every measured row.

mod bench;

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_ts5() {
    bench::ratchet("ts5");
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_go() {
    bench::ratchet("go");
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_rust() {
    bench::ratchet("rust");
}
