# trigger-delta-probe

PROBE, not a landing: measure whether AFTER INSERT/AFTER DELETE triggers populating `__delta_<rel>` (and frontier staging where shape allows) cut the ghcache statement count and wall. Report numbers; the user decides landing. Base: worktree at or past f84d11409179e560da51443ddc9a156da0c031f3. Branch `probe/trigger-delta`. Push the branch and hail numbers; NO PR unless hailed to.

## Constraints (user words, 2026-08-23)
- Pure SQL only: triggers and SQL emitted by `lower.pl`. NO rusqlite feature flags (no session, no preupdate_hook).
- The oracle and `emit_ts.pl` byte-output for unchanged programs stay untouched; guard the trigger DDL behind an emit option (`emitter option or env`) so the default door is unchanged.
- Byte-identical receipts or the probe reports failure: `tests/fixtures/ghcache_ticklog_base.txt`, grade.sh on the probe path for at least the coalesce/recursive/negation fixture families.

## Read first
`v6/prolog/lower.pl` :1359 (the one existing trigger), stage/publish emission, `level_delta_insert_sql`; `v6/sprefa-engine-rs/src/incremental.rs` stage_events + write paths (which explicit statements become redundant when a trigger fires); `docs/failure-modes.md` 85-89.

## Measure, three runs each
Baseline at your base sha and probe arm: fold statements (SEAM_TALLY), wall, per-verb table (`DL_TRACE_SUMMARY=1`), on ghcache 14-tick fold and on wide_64. Statement floor arithmetic in the report: statements removed x ~25 us vs trigger row-work added.

## You own
`v6/prolog/lower.pl` (probe-gated), `v6/sprefa-engine-rs/src/incremental.rs` (probe-gated skips), new test files. Forbidden: everything else; no conformance fixture edits; main untouched.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "probe numbers: ..."`. Blocked: one line, stop.
