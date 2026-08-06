# Lane: CST query lowering breakdown (planning only, NO production code)

Worktree /Users/chrishafley/projects/sprefa-plan-cstlower, branch
plan/cst-lowering-breakdown, base 2eceb836. FIRST action: `git merge --ff-only
2eceb836` — failure = STOP, write PLAN.md saying so. Deliverable: PLAN.md at
the worktree root. Do NOT commit, do NOT write outside this worktree, do not
spawn subagents. STOP-and-record on any contradiction with reality.

## Ruled and settled (2026-08-02, user-ratified — do not re-litigate)

- Lowering: pattern sugar -> ts_query term -> QUERY TEXT STRING to the host.
- Demand: LAZY via cst_need(path, content_hash, lang); eager = same rel with a
  files(P) demand-set. Compact type/call/df planes stay eager.
- Caching: engine-side effect_cache; request cols += grammar_hash+query_text.
- Executor: sprefa-extract (ast-grep-core linked); long-lived NDJSON stdio
  `extract --serve` protocol (id-tagged jobs, {id,done}, cancel line).
- Read plans/2026-08-02-cst-query-rulings.md IN FULL (mermaid + 7-stage worked
  example + the surface-spelling disambiguation note) and
  plans/duels-2026-08-02/duel-a-flash.md + duel-a-kimi.md.

## The work to break down

A. Phase-2 runner: today dl6 REFUSES tree_sitter_query
   (unsupported_host_execution_phase_2; find the marker near SYNTAX.md:330 and
   the refusal in the compiler — VERIFY locations). The build = make
   sprefa-extract execute ts queries (port shape precedent: v5 run_ts at
   src/eval.rs:1047 area — verify) + wire the dl6 host contract + effect_cache
   request columns. Break this into steps with files, LOC, gates. The
   extract-side entry: v6/sprefa-extract (astgrep.rs:171-199 emits named nodes
   only — verify what query execution support exists vs needs adding).
B. `extract --serve`: the ruled NDJSON protocol as its own step ladder
   (spawn-per-file was the 87x wall; CRAWL-BENCH 40.7 vs 3540.9 files/s is the
   receipt — cite where that bench lives if you can find it, else UNVERIFIED).
C. Surface-spelling DECISION PACKET (do not decide; the user rules):
   option (a) quoted pattern parsed at compile (v5 `ast` precedent: compiler
   parses, refuses unmapped shapes, binds @captures) vs (b) native S-expr in
   the dl6 grammar. Cost each: parser LOC (DCG precedent parse_dl.pl:95-120,
   :1464-1483 per the kimi duel — verify), langium/JS-door impact
   (v6/dl/grammar/dl.langium), LSP/textmate surface, and the DIAGNOSTICS
   dimension: all 7 dl6 expanders carry zero source positions (1608 lines,
   finding sugar_spans_absent in chat_log/20260802.2.opus-flash-fleet-*.pl),
   so quoted patterns are opaque to squiggles without inner-position mapping
   while native spelling gets spans for free. Quantify what inner-mapping
   would cost if (a) wins. End the packet with a table the user can rule from.
D. Interactions: trivia/A14 (separate trivia rel vs CST-native comments) and
   metavariable semantics — state where in the ladder each MUST be ruled and
   what proceeds without them.

## PLAN.md must contain

1. Verified-receipts section first (every location claim above checked, with
   file:line as found today; unverifiable = flagged).
2. Step ladders for A and B with per-step files/LOC/gate and failure modes.
3. The decision packet C as a self-contained section.
4. ARCH-style task/5 rows (read v6/prolog/ARCH.pl for shape) for the ladder.
5. What tonight's other arcs feed in: effect_cache response-side gap (lane A
   2026-08-02 found the schema has NO response-side col), and the type-IR
   lane's SCIP-identity ruling (symbol strings as fact ids) if it touches the
   cst fact planes — one paragraph max.

Style: no em dashes; never provenance/substrate/load-bearing/regime; dl
variable names descriptive; every .dl snippet you show carries its rx lowering
per repo law, or state why none is shown.
