//! The diet_scip ratchet over the committed oracles (user 2026-08-29:
//! "diet-scip ratcheted as high as possible so it is fast"). Ignored by
//! default: the corpora are machine-local checkouts (COMMON.md), so CI never
//! runs this leg. `just extract-ratchet` is the local recipe; it runs each
//! (lang, tier) leg as its own test process so `rss_mb` is that pair's peak
//! alone, not the previous leg's high-water mark.
//!
//! One leg = one extraction = every `bench::cases()` row naming that (lang,
//! tier). Accuracy floors live in RATCHET.tsv keyed on
//! (lang, family, tier, oracle); cost ceilings live in RATCHET.cost.tsv keyed
//! on (lang, tier).
//!
//! Mode per run: default asserts both files (recall/precision 0.10 pt,
//! wall +15%, rss +10%); `RATCHET_BUMP=1` moves floors up and ceilings down
//! only; `RATCHET_FORCE=1` beside a bump rewrites every measured row.

mod bench;

use bench::Tier;

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_ts5_syntax() {
    bench::ratchet("ts5", Tier::Syntax);
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_ts5_checker() {
    bench::ratchet("ts5", Tier::Checker);
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_go_syntax() {
    bench::ratchet("go", Tier::Syntax);
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_go_checker() {
    bench::ratchet("go", Tier::Checker);
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_rust_syntax() {
    bench::ratchet("rust", Tier::Syntax);
}

#[test]
#[ignore = "local corpora only; run via `just extract-ratchet`"]
fn ratchet_rust_checker() {
    bench::ratchet("rust", Tier::Checker);
}
