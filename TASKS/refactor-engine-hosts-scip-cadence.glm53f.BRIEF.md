# Brief: engine hosts stop deciding SCIP mode and stop comparing cadence

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-leaky-types-review.PLAN.md` rows 15 and 19.

## First action
```bash
git merge --ff-only 946460d75   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-engine-rs/src/hosts.rs`
- `v6/sprefa-engine-rs/src/run.rs`
- `v6/sprefa-extract/src/project.rs` (one new constructor only, see below)
- `v6/sprefa-extract/src/bin/extract.rs:396-402` (call the constructor)
- `v6/sprefa-engine-rs/tests/executors.rs` (new cases only)
- new issue: `issuectl new -t improvement --slug engine-hosts-scip-cadence --title "engine hosts: ScipMode built by extract, cadence answered by hosts" -a chris -p normal -l engine -l refactor --description "leaky-types review rows 15 and 19"`; tick it as its own commit.
FORBIDDEN: `v6/sprefa-extract/src/types.rs`, `src/lang/**`, `src/0_move.rs`, `src/move_*.rs`, `src/scip*.rs`, everything under `v6/sprefa-store`, `v6/sprefa-engine-rs/src/executors/**`.

## Row 15, ScipMode (signatures first)
Measured: `hosts.rs:979` and `hosts.rs:1022` CONSTRUCT `ScipMode::Off` / `ScipMode::Load(&index)` inside two near-identical `ResolveRequest` literals; `bin/extract.rs:399-401` builds the mode from two CLI flags.
```rust
// project.rs, next to `pub enum ScipMode`
impl<'a> ScipMode<'a> {
    /// The one place a (index path, build flag) pair becomes a mode.
    pub fn from_flags(index: Option<&'a Path>, build: bool) -> ScipMode<'a>
    // Some(path) => Load(path); (None, true) => Build; (None, false) => Off
}
```
- `bin/extract.rs:399-401` becomes `ScipMode::from_flags(cli.scip_index.as_deref(), cli.scip_build)`.
- `hosts.rs`: the two `ResolveRequest` literals collapse into ONE private fn `resolve_request<'a>(paths, arms, root, index: Option<&'a Path>) -> ResolveRequest<'a>` whose only mode expression is `ScipMode::from_flags(index, false)`. After the change `git grep -n 'ScipMode::' v6/sprefa-engine-rs/src` prints ZERO lines; `git grep -n 'ScipMode::' v6/sprefa-extract/src` prints only `project.rs` (`from_flags` body + `load_scip` match) and `Default`.

## Row 19, ExecutorCadence
Measured: `cadence()` is already a method on `IHostExecutor` (`hosts.rs:46`, `clock.rs:31`, `watch.rs:50`). The one remaining comparison outside hosts is `run.rs:797` (`cadence_for_plan(..) == ExecutorCadence::Continuing`).
```rust
// hosts.rs, replaces the pub `cadence_for_plan` if nothing else calls it (measure with git grep)
pub fn plan_is_continuing(plan: &HostPlanData, adapter_rows: &[HostAdapterRow]) -> bool
```
- `run.rs:797` calls `hosts::plan_is_continuing(plan, adapter_rows)`; `run.rs` drops `ExecutorCadence` from its `use` line and its comment at `run.rs:5` says "continuing" without naming the enum.
- Receipt: `git grep -n 'ExecutorCadence' v6/sprefa-engine-rs/src` prints only `hosts.rs` and `executors/*.rs`.

## Fail-first tests (`tests/executors.rs`, new cases)
1. `scip_mode_from_flags_covers_all_three_arms`: Off / Load / Build, asserted with `matches!`.
2. `continuing_plans_come_from_the_executor_answer`: a plan routed to a clock executor is continuing, a plan routed to an extract executor is not, through `plan_is_continuing`.
Write each test first, run it, paste the failing line in the commit body, then make it pass.

## Receipts (PR body)
- `cargo test -p sprefa-engine-rs` and `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background), 0 failures, counts pasted.
- `bash v6/sprefa-engine-rs/grade.sh` on `946460d75` and after: paste both counts, they must match.
- The two `git grep` receipts above, pasted verbatim.
- `git diff 946460d75 --stat` shows only owned files; `cargo fmt`; no `eprintln!` in `src/**`; 10-second law.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Descriptive identifiers, never single letters. Issue tick as its own commit. No `unwrap()` in new non-test code.

## Delivery
One PR against `origin/main`, title `engine hosts: ScipMode built by extract, cadence answered by hosts`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, both grep receipts>"`.
