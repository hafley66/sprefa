#!/usr/bin/env bash
# ONE entry point for "did every test pass". `just all-tests`.
#
# WHY THIS EXISTS (incident 2026-07-20): the test surface had drifted into
# three disconnected piles — the default `cargo test` run, 28 `#[ignore]`d
# tests reachable only by hand-typing a different filter each, and the perf
# harnesses behind their own just recipes. Nobody ran all three, so nobody
# knew the branch's real state. Two defects were living in the gap:
# `extract_cache::df_twins_*` (a design's own spec test, left asserting the
# pre-change behavior) and a `verify.sh` that skipped the entire 978-test `it`
# binary whenever one `--lib` test flaked, then stamped the tree verified.
#
# The rule this script enforces: a test is either RUN or NAMED. Nothing is
# silently skipped. Tier 3 prints every excluded test with its reason and the
# exact command to run it, so the excluded set stays visible and auditable
# instead of decaying into an `#[ignore]` graveyard.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
note() { printf '\n=== %s ===\n' "$1"; }

# ── Tier 1: build, lint, the default suite, and the rails ────────────────────
# Delegated to verify.sh rather than re-running `cargo test` here: verify.sh
# already owns build + clippy + fmt + the full suite (with the FSEvents flake
# re-run policy) + the rails, and it writes the verified-tree stamp. Calling
# both meant running the entire suite twice per invocation for no new
# information.
note "tier 1: build, lint, full suite, rails (scripts/verify.sh)"
./scripts/verify.sh || fail=1

# ── Tier 2: real tests excluded from tier 1 only for thread contention ───────
# These assert real behavior and pass in isolation; they flake when the `it`
# binary runs them alongside 900 other tests. Contention is a HARNESS problem,
# so the fix is a serial pass (--test-threads=1), not an `#[ignore]` that
# removes the assertion from the build's judgment entirely.
note "tier 2: load-sensitive tests, run serially"
tier2_it=(
  budget_cpu::governor_toggle_caps_cpu_concurrency
  perf_facts::small_tick_under_timing_ceiling
  node2vec::node2vec_w2_cache_hit_on_revisited_graph
  node2vec::node2vec_w2_cache_is_bounded
  retraction_props::empty_changed_set_is_a_fixpoint_high_volume
  retraction_props::equivalence_and_memo_hold_at_every_step_high_volume
)
for t in "${tier2_it[@]}"; do
  echo "[all-tests] serial: $t"
  cargo test --test it "$t" -- --ignored --exact --test-threads=1 || fail=1
done


# ── Tier 3: NOT run here, and why ────────────────────────────────────────────
# Each line is a standing claim that this test cannot run in a plain local
# gauntlet. If a reason stops being true, the test moves up to tier 2.
note "tier 3: excluded (named, not silently skipped)"
cat <<'EOF'
  needs network / live credentials:
    gh_cache::gh_cache_live_against_github
      cargo test --test it gh_cache_live_against_github -- --ignored
  needs rust-analyzer + minutes of wall time:
    oracle_rust::call_resolution_parity_vs_rust_analyzer
    oracle_rust::<recall snapshot>
      cargo test --test it oracle_rust -- --ignored --nocapture
  needs an external multi-language corpus:
    oracle_corpus::corpus_parity_{rust,go,ts,python,kotlin}
      cargo test --test it oracle_corpus -- --ignored --nocapture
  needs a specific OS / unshipped feature:
    daemon::git_checkout_rewrite_triggers_tick   (macOS FSEvents drops git-checkout events)
    daemon::linux_inotify_watch_count_tracks_unignored
        ^ NOT a flake: this test IS the spec for Linux per-directory watch
          pruning, which has not landed. It fails by design until it does.
  needs a pre-built SCIP index + minutes of wall time in debug mode:
    propose::tests::symbol_ranges_subset_of_ast_on_engine_rs
        ^ formerly GREEN-BY-SKIP (needed `<repo>/../index.scip`, a path that
          stopped existing at the v5 lift, and a missing index hit
          `Err(_) => { warn; return; }`, reporting PASS in 0.00s having
          asserted NOTHING). The path is fixed (`<repo>/index.scip`) and the
          missing-index arm now panics with the build command instead of
          silently returning, so a run without the index FAILS LOUDLY rather
          than reporting a false green. Build the index first:
            rust-analyzer scip . -o index.scip
            cargo test --release --lib symbol_ranges_subset_of_ast_on_engine_rs -- --ignored
  needs an env-pinned production corpus:
    analysis_bundle_ab::production_corpus_full_warm_incremental  (DL_AB_ROOT, DL_AB_DB)
  measurement harnesses, not assertions (run when you want the numbers):
    family_scaling_probe, cst_node_perf, cold_stage::measure_*
      just perf-reactivity-build && just perf-reactivity
  regeneration utility, not a test:
    call_golden::regen_call_goldens
EOF

note "result"
if [ "$fail" -ne 0 ]; then
  echo "[all-tests] FAILED — see the tier above that reported non-zero"
  exit 1
fi
echo "[all-tests] green: tiers 1 and 2 passed; tier 3 excluded by name above"
