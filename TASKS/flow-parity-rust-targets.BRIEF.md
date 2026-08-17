# flow-parity-rust-targets (ARCH task: flow_parity_residue, size:med)

FIRST ACTION: `git merge --ff-only 046cbc510804671d2441aca36536bbd310eef485`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root, then ARCH.pl:779 (task flow_parity_residue) — that
row is the state of record and your contract.

GOAL: advance the named BLOCKER on dataflow parity: Rust call-target
resolution evidence. Current gradebook (ARCH.pl:779): targets V5 200 / V6 168,
matched 113, V5-only 87, V6-only 55. Direct flow already 2457/2457 exact;
flow_node_type 58/33; flow_param_type 35/40. The row's own closing law: "Close
with improved typed Rust resolution facts or a pinned SCIP index, not another
DL rule." You do NOT touch dl rules to close gaps.

THE FORK YOU WEIGH (this is the judgment part; write the verdict with
receipts before implementing):
(a) improve the extractor's typed Rust resolution (current resolver picks
    first same-file/unique-blob definition; V5 retains qualified method
    targets) — src is v6/sprefa-extract/src/lang/rust.rs resolve plane;
(b) pin a SCIP index for the graded corpus and join targets through it —
    `--family scip` exists (v6/sprefa-extract/src/bin/extract.rs:155-168),
    SCIP indexing is the named 10-second-law exception.
Weigh on: which closes more of the 87 V5-only rows, determinism (pinned index
vs re-resolve), and corpus portability. Pick ONE, say why in the report.

RIG: `bash v6/tsv2/scripts/flagship-flow.sh` is the referee (v5 std/flow.dl
output on the pinned corpus vs v6 flagship-flow.dl6). Run it on base FIRST and
paste the four-query table as your baseline; every claim of improvement is a
delta against that run.

VALIDATION (paste, twice each):
1. flagship-flow.sh four-query table: targets matched MUST rise from 113;
   direct flow MUST stay 2457/2457 exact; no column regresses.
2. `cd v6/sprefa-extract && cargo test` green.
3. The classifier in the rig (ARCH.pl:779: "fails on direct drift or
   empty/nonmatching node types") passes.

FILES YOU OWN: v6/sprefa-extract/src/** (resolution plane only), additive
tests, and IF you pick (b): the scip pinning wiring in
v6/tsv2/scripts/flagship-flow.sh.
FORBIDDEN: v6/dl/fixtures/flagship-flow.dl6 and every .dl6 (the row forbids
closing with DL rules), v6/prolog/**, engine-rs, tsv2 runtime.

Update ARCH.pl:779's task row comment with the new numbers as part of your
commit (that row is the ledger for this arc; append, do not rewrite history).

COMMIT plain, COMMENT_RAIL_IDLE_MS=3000, never pipe a commit, commit ONLY in
your worktree (`pwd` before every git commit).
Report: chosen fork + why, baseline table, post table, the V5-only residue
count and what class remains.
