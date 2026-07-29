# CODEX BRIEF: latest() in edge bodies (sol-class; review B1, golden plan phase 3 opener)

Lower latest(Atom) in edge-rule bodies. Design review receipt: the
emitted negation path already reads the BASE table while triggers
read the frontier; latest is structurally that minus the NOT EXISTS
wrapper (a sampled read of current state, rx withLatestFrom). The
model reading (TICK-MODEL.md section 4): latest = the N -> (0|1)
coercion; this arc ships the missing coercion operator.

Scope: analyze.pl flips edge_body_with_latest off (only for the
supported shape: latest around a plain positive rel atom; anything
wider keeps a named refusal); lower.pl/emit_ts.pl emit the base-table
sampled join on the incremental AND naive paths; registry/SYNTAX
status row for latest updates truthfully. Semantics authority: the
oracle already RUNS these programs (body.pl latest in edge ctx);
byte-identity against it is the whole grade, never re-derive
semantics.

Fixtures: the 6 edge_body_with_latest bucket fixtures
(marker_stops_backlog_replay is the flagship: the compiler currently
REFUSES the correct program while ACCEPTING the backlog-replaying
wrong one -- your summary shows that inversion dying). Expect
compiled +6 (or fewer with named reasons), identical growth to
match, ZERO movement elsewhere, both modes.

Grades: conformance 135/0 (may grow only if you add fixtures);
sweep both modes with exact movement table; TEXT_DOOR all-compiled;
roundtrip ALL PASS; plunit growth with fail-first receipts; tsv2
tests + gate; tsgo. Statement-count/EXPLAIN receipt on the sampled
join (SEARCH not SCAN where an index exists).

Laws: codex no-commit flow (git READ-ONLY, tree stays dirty). FIRST
ACTION verify HEAD = dispatch sha, STOP on mismatch. A concurrent
lane owns v6/tsv2/runtime + serve; do not touch runtime files (your
emitted-SQL changes live in gen shapes, not runtime edits) -- if a
runtime edit seems required, STOP AND REPORT. No em dashes; banned
words provenance, substrate, load-bearing, regime. Summary: movement
table, the inversion receipt, EXPLAIN receipt, grades, cracks.
