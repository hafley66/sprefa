# Point-free lab — header (planner-seeded contract)

User direction (2026-07-30): "it does read like point free rxjs yet [sic —
does NOT], what is minimum set of syntax moves and semantics to get us
operationality there. lab it out."

Coordinator's candidate move set (the lab's hypotheses to CONFIRM, AMEND, or
REFUTE — not to assume):

- **M1 `scan(Acc, Seed, Expr)`** in an edge head: pure expansion to the
  shipped two-arm fold (`not(head)` base arm + `pre(head)` step arm). Kills
  the init/step tax every fold pays today (jsonscan, csp cursor step,
  golden's keyed folds).
- **M2 `seq(name)`** ordinal minting (ALREADY RULED, stream card 1b,
  confirmed-widened by the csp census: 52 of 94 rules were the cursor
  template): `Ordinal := seq(q)` expands to the 4-rule cursor block.
- **M3 `|>` anonymous stages**: a body stage chain where each stage becomes
  a compiler-minted intermediate rel (`__stage` naming), exactly as match
  arms become rules. The rel still exists for observability/grading; the
  author just doesn't name it.

Grading law for ALL three: sugar output must be BYTE-IDENTICAL (tick log,
both doors) to the hand-desugared program — the match-block precedent
(sha-graded sugar-vs-desugar). A sugar that changes any tick log is a
semantics change and gets REFUTED, not adjusted.

## Questions

- Q1 CONFIRM the mapping table (coordinator's claim, verify each row with a
  receipt program): merge = multi-rule head; filter/map = body; scan = M1;
  withLatestFrom = latest; switchMap = key(1); distinctUntilChanged =
  boundary diffing; toArray = json_group_array; pairwise = finalize+read.
  For each: a minimal dl6 program + its rx one-liner, graded.
- Q2 THE CORPUS: take 8-10 real examples from rxjs documentation patterns
  (counter, drag-drop state, debounced search, polling with backoff,
  running average, pairwise diff, buffered batch, rate limiter). Write each
  in dl6 TODAY (as-is), then with M1/M2/M3 as HAND-DESUGARED pairs (the
  sugar spelled in a comment, the desugar as the running program). Table:
  program x (rules today, rules with moves, deleted %).
- Q3 WHERE DOES `|>` BREAK: stages that join (two sources), stages that
  need their own retention/key, stages referenced twice (diamond). Name
  each break with a rule: what forces a stage to be NAMED.
- Q4 the ORDER question: rx reads source-first, dl6 rule bodies already
  read source-first after the arrow — is head-last spelling
  (`event(P) |> ... |-> total(...)`) worth anything beyond aesthetics?
  Price both spellings against the parser (registry/SYNTAX precedent), do
  not wire either.
- Q5 minimality: is any of M1-M3 derivable from the other two? Is there a
  FOURTH move the corpus demands that the coordinator missed? (The
  json_pattern_expand arc — brace pattern inline in a body atom — is
  already filed; treat it as M0, assume it lands, do not implement it.)

## Named slots

- slot_scan_spelling (head-position `scan(Acc, Seed, Expr)` vs bind-position
  `Acc := scan(Seed, Expr)`), slot_seq_scope (per-rel vs per-name cursor),
  slot_stage_naming (`__stage_N` determinism across recompiles — grading
  needs stable names), slot_stage_retention (what keep/key a minted stage
  gets: none? inherit? forbidden?), slot_pipe_word (`|>` vs `.` vs
  something already in the SQL/prolog/rx pool per ruling
  vocabulary_tiebreak = sqlite_first_then_sql_standard).

## Receipts required to land

- The Q1 mapping table with per-row graded receipts. The Q2 corpus with the
  rules-deleted census. Q3 break rules named. Per-slot answers.
- Every sugar hypothesis graded sugar-vs-desugar byte-identical OR refuted
  with the diverging tick log shown.
- Verdict `plans/2026-07-30-point-free-verdict.md`; lab files under
  `v6/prolog/labs/point_free/**`, die on landing (last-copy hash recorded).

## Fences

- Writes ONLY under `v6/prolog/labs/point_free/**` + the verdict doc.
- NO compiler/oracle/registry/parser edits — this lab prices and grades via
  hand-desugared programs through the EXISTING doors; wiring is follow-up.
- A concurrent lane owns `v6/prolog/labs/type_matrix/**`; never touch it.
