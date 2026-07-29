# V6 ALPHA GOLDEN PLAN (2026-07-29, dataflow frontier per user reorientation)

Alpha finishes on CODE DATAFLOW ANALYSIS. ghcacher's live loop is
post-alpha. Receipts feeding this plan: the v5-utility gap review and
the language design review (both opus, 2026-07-29, full texts in
session task outputs; key findings folded into the ledger), the
hosts-wiring phase-1 landing (ghcacher.dl6 parses gap-free), and the
update-arm/spreading lab verdicts.

## The two structural facts the plan is built around

1. v6 is TWO DISJOINT RUNTIMES; only tsv2 is graded. v6/dl (the
   server, hosts, ingest) still evaluates DELETE-all+rebuild via
   lowerSql. The P1/P3 incremental engine does not run under the
   thing that runs. (Utility review, verified import graph.)
2. The engine is already strongest at dataflow: recursive strata
   (P2), refCount retraction with cycle guards (P3), the 1M
   competition win. The gap is feeding it (extraction, ingest speed)
   and invoking it (CLI, watcher).

## P0: correctness debts (dispatched or this-week class)

- [DISPATCHED] keyed-arrival divergence (design review A4, live bug,
  coordinator-verified): emitter PK/arrival must match
  absorb_set_arrival; fail-first fixture. Brief:
  plans/2026-07-29-keyed-arrival-divergence-brief.md (also carries
  B2's three silent-inert refusals + the gen_emitted footgun).
- SCOREBOARD.md + justfile expected-count comments refresh (stale by
  1-2 arcs; rides the next sweep-touching landing).
- json_ticklog_encoding ruling EXECUTION (canonical JSON): oracle
  encoder change + one-time regrade; unblocks json_array/json_object
  agg heads (design review C5 flags the stale registry comment). S.

## Phase 1: bridge the runtimes (alpha-critical)

Hosts phase 2: live sh + interval-bind execution in the tsv2 runtime,
and a served process whose engine IS the emitted incremental runtime
(the v6/dl server either adopts tsv2 gen modules or a thin serve
wrapper grows around the tsv2 runtime; decide by reading, not fiat).
Exit receipt: door-handwritten + one host program run LIVE under a
server with tick logs byte-identical to the oracle's schedule-fed
grading.

## Phase 2: extraction live (the dataflow feed)

- Watcher BOUGHT (chokidar / @parcel/watcher; research table first
  per standing law; i:file-watching skill is the reference).
- sg / tree-sitter / span extraction hosts executing (the fork
  verdict's HOST shape; term forms landed phase 1; sidecar or
  in-process per the UDF lab receipts).
- EXTRACTOR IS FIXED (user 2026-07-29: "what we have with
  scip/ast-grep in that sprefa-extract is what we got, we are not
  relitigating that one right now"): phase 2 wires the EXISTING
  v6/sprefa-extract binary's output through hosts; no extractor
  redesign, no new extraction tooling research.
- Program-declared file sets: the worktree-default enumeration host
  (no "WORK" atom, ruled) replacing push-only /edb/file_changed.
Exit receipt: sg-rail-class diag rail runs end to end on v6 with a
real file edit triggering the retick.

## Phase 3: the dataflow flagship

- Edge-body construct arc, ordered by the review's receipts: latest()
  in edge bodies FIRST (B1: structurally the negation path minus NOT
  EXISTS; 6 fixtures + fixes the compiler-accepts-wrong-refuses-right
  backlog-replay trap), then pre (12), negation (6), now (5),
  finalize (2, the update-arm bucket), json destructure (6, gated on
  the decode lowering).
- Graph rels + closure over the extraction feed (recursive strata are
  landed engine capability; closure() spelling research rides the
  graph-algo build-vs-buy queue item).
- PORT THE FLAGSHIP: a real v5 dataflow program (flow-interproc or a
  callgraph rail) graded against v5's own output (the same-matrix
  baseline pattern from the scale bench).
Exit receipt: flagship rail byte-graded vs v5 on a pinned corpus.

## Phase 4: daily utility

- CLI ("the bop", gates any 6.2.x push per user): registry.pl cli
  table -> commander (required target) + clap derive later; verbs
  serve/run/check/load/q; run+check boot the server in-process; exit
  codes 0 clean / 2 findings / 1 broken.
- LSP milestone X1 (utility review: nearest retirement): v5 dl --lsp
  --diag-db pointed at the v6 db (5_diag.ts diag_v5 already matches),
  watcher from phase 2, 3-5 diag rails ported.
- changed(path) rel joined against git diff (the pre-commit rail
  path; full rail parity is post-alpha XL).

## Phase 5: type pass + perf (parallel track, alpha-closing)

- Type pass hardening: every rel incl derived gets a resolved printed
  decl (kills col3 anonymous columns), open(none) fixpoint made
  total, float/bool/null RULINGS (design review B7: no avg() today),
  refusal messages get prolog:message//1 + source location (B4 -- the
  worst part of the cold-author experience, reviewer-verbatim).
- Ingest perf arc: commit_ms ~10.8ms/file is the named next cost;
  74 files/s -> target enough for rail-sized repos (v5's 7,244 is
  post-alpha; the felt gap per the utility review).

## Alpha exit criteria (all receipts, no vibes)

1. One real dataflow rail graded byte-vs-v5 on a pinned corpus.
2. LSP diags served from v6 with live re-tick on file edit.
3. ghcacher runs on the GRADED runtime (schedule-parity already held;
   live loop quality-of-life stays post-alpha).
4. CLI with exit codes; `dl6 check` usable in a hook.
5. Zero open oracle/emitter divergence class (P0 lane + a standing
   fixture rule: every oracle semantics change lands with its emitter
   fixture in the same arc).
6. Endurance + leak-soak gates green in green-all (landed 2026-07-29).

## Out of alpha (recorded so nobody relitigates)

rust backend (designed endgame, parked per user "calm down");
spreading wiring (lab verdict banked); openapi/json-schema import
(design sketched, gated on hosts phase 2 + type rulings); channel
checker (gated on arms-lab SLOT rulings); per-key retention spelling
(design review B6, needs a ruling); daemon verb parity; multi-repo
config surface.
