# v6 language tiers: what to build orthogonally, in topological order

Source: 2026-07-27 corpus survey of all 163 v5 .dl programs (14,082 lines,
1,451 rel decls) + the boiling surface (v6/prolog/labs/LANG.md). Status:
AMENDED 2026-07-27 PM: tiers section replaced with AGGREGATE.md section 4
(the nine-lab reconciliation) after the user ruled Q1-Q10; see
v6/prolog/conformance/rulings.pl for the rulings the tiers now assume.

## The corpus verdict that orders everything

| feature | files (of 163) | tier |
|---|---|---|
| scan (file enumeration -> rels) | 135 | T1 |
| ? queries (snapshot asks) | 128 | T0 |
| negation ! | 112 | T0 |
| count/max/min/sum aggregation | 63/11/15/5 | T0 |
| severity diagnostics (diag rows) | 54 | T3 |
| closure / scc / node2vec | 37 / 13 / 7 | T2 |
| ast / comment / jsonp / json / sg extract | 35/33/14/10/1 | T1 |
| @async effects / sh | 12 / 7 | T5 |
| @next carries (-> keys/edges in v6) | 6 | T4 |
| clock / every | 5 / 2 | T4 |

~90% of real usage is the TIMELESS static-analysis fragment. The temporal
machinery that dominated this week's design discussion serves the daemon
minority. Build order must deliver the timeless fragment first and bolt time
on without disturbing it.

## Tiers (each orthogonal; arrows = depends-on)

T0 RELATIONAL CORE (timeless)
  enum/struct + typed rel cols + Option(T) + Key(Type) as static FD; level
  rules `<-`; stratified negation; reserved aggregate head forms
  (count/sum/min/max, bag-vs-set per R8); facts; snapshot asks `?` with
  --check exit 2; comparison + arithmetic (Int-only, truncating); `:=`
  bindings; string interpolation (name-only holes, desugar to concat,
  Display = closed {Int, Str}); named-column atoms (head = construction,
  omission error; body = pattern, omission wildcard; head values are exprs);
  `_` wildcard; pure stdlib (12 names); quote/eval-default rule; surface
  recursion (no_self_union retargeted); multi-rule heads (spec sentence);
  unit-rel idiom.
  Lowering EXISTS for the pre-amendment core: js engine v1 lowerSql + emit_ts.
  Checkers: HM/enum typing, exhaustiveness, stratification, Key-as-FD,
  aggregate group arity, range restriction via := / atom-arg rule,
  head-expression ban inside recursive SCCs. THE STRATIFIER SEES EXPRESSIONS:
  they are part of T0's checker input and cannot be bolted onto a frozen
  checker later.
  <- nothing

T1 EXTRACTION + BIND (world-fed corpus)
  `from world` rel modifier (the unbundled source keyword; canned rows and a
  bind are program-text-identical, orthogonality claim 2 MEASURED);
  bind mechanism, whose obligation family is: finiteness discharge for
  Stream-typed rels, per-emit batching, atomic single-transaction commit
  (writer-side R7); quoted DSL regions `{|lang|| ... |}` (compile-time
  parse + check; raw-text token owed to surface_dcg NOW); match(cst, pattern)
  over checked pattern values; grammar-import (node-types.json -> con facts;
  labbed; needs the target parser, not just the schema, for bare-token
  kinding). Extraction op bodies (scan/regex/comment/ast/sg/json) still owe
  surface syntax: the single largest open gap (AUDIT finding 17).
  Checkers: quoted-DSL parse/check as compile errors, pattern-vs-grammar,
  two-lowering refusal channel (a backend that cannot express a construct
  refuses, never approximates).
  <- T0

T2 GRAPH OPERATORS (on-disk algorithms)
  closure/scc (node2vec later) as operators over edge rels; recursion is
  already T0. RAM law: operators stream from sqlite, never resident.
  <- T0

T3 DIAGNOSTICS LIBRARY + CLI (milestone, not a syntax tier)
  ZERO new syntax, proven twice (timeless_rail: gate/severity/stage/exit are
  eight lines of level rules over fact tables; diag_emit: the diag_v5 view +
  one shell bind close the editor loop with no rust or extension change).
  Deliverables: std/diag library (diag decl, severity_rank, gate_threshold,
  gate_exit/check_exit rules), CLI verbs (--check exit 2 on rows, stage
  gates, LSP span rendering), the diag_v5 view contract + sqlite bind.
  Retraction reaches the editor via the reader's absence diff; no T4
  dependency even for clearing squiggles. Gate: rule A6 (ordinary rel vs
  engine sink) before rewriting this row.
  <- T0, T1 (hard: the 54-file diag corpus is extraction-fed)

T4 EDGE TIME (first temporal tier)
  `<+` edge rules (respecified: arrow = trigger, rel kind = storage);
  rel-kind declaration Set|Log (one word, six jobs: storage kind, retention
  target, event-ness, boundary-check input, R2 site, keyed-Log exclusion);
  trigger_marker (the R5 construct, one spelling); now() (kernel);
  pre (visibility per R6, within-tick chaining per R1); Key runtime
  semantics (replace -old/+new; equal-row write = no-op); occurrence
  identity per R1 (engine stamps on event rels); clock-bucket rel pattern +
  the two-salt law; retention clause on Log rels (REQUIRED, not an
  optimization); tick transaction with R7 boundary diffing (delta MULTISET
  on occurrence rels); R9 edge-write propagation + the drain scheduler;
  count-IVM port (contract: R7 + the support-count/occurrence-multiplicity
  split ruled).
  `|>` temporal pipe lives HERE AS SUGAR, adopted only under four
  conditions: (1) R5 ruled first, marker preferred, pipe generates it;
  (2) the rel-kind declaration landed (keeps the boundary check local);
  (3) R9 ruled next-tick with the drain scheduler named; (4) reserved
  namespace for generated intermediates. Pipes with edge/key cuts ship at
  T4; yield cuts additionally need the T5 effect-signature DECLARATION in
  scope, not T5 runtime.
  Checkers: pairwise body disjointness over the rules heading each keyed rel
  (REPLACES "jointly semidet per key per tick", which is neither decidable
  as quantified nor applicable to the one-rule-many-rows case), causality,
  retention presence on Log rels, fold-shape recognition (accumulate/lww/
  concat catalog; out-of-catalog steps rejected), `<+`-into-Set type error.
  Regression contract: the timeless_rail check set byte-for-byte at any
  single repo state (orthogonality claim 1, mechanically checkable).
  <- T0 (engine: count-IVM port + arrival staging table)

T5 EFFECTS
  adorned world rels (signature arrow, pending Q8), envelope enums, demand
  rows + content addressing + the two salts, shell bind with two-channel
  grammar (stdout_line + exit), STREAMING PRE-REGISTERED: Stream(Item, End)
  / Tail(Item) result wrappers land BEFORE register_lowering (they ground in
  {ground_terms, rule, external_rel} with no register dependency,
  shell_stream tier note); write effects + apply gate + dry-run (AUDIT
  finding 15, still open); checkout-style demand sinks.
  Checkers: LINK-TIME LIFETIME OBLIGATION: a bind must discharge its rel's
  finiteness claim (tail -f into a Stream-typed rel is a link error); bind
  obligation discharge for batching + atomicity; streaming retention gate.
  <- T1 (bind), T4 (edges/ticks)

T6 ASKS + MODES
  tail asks; (cardinality, lifetime) mode analysis with the new
  (multi, finite) cell; dominance/scopes (switch_map); the five ask rows
  from check_eventing (hook write, hook snapshot, LSP tail under document
  scope, commit gate, dashboard tail-with-warning). mode_lab scope:
  result-type modes, the lifetime lattice fixes (AUDIT finding 13: two
  operators, scope_min and join_max, stated; mode analysis declared a
  post-link pass), static-vs-runtime lifetime distinguished.
  <- T4, T5

T7 LAZINESS + SUB GRAPH        (unchanged)
  <- T5, T6

T8 STORAGE LOWERING            (unchanged, user-parked)
  <- T0

T9 OPTIMIZER                   (unchanged)
  <- most of the above

## Compiler self-check (cross-tier)
  The census check REPLACES voluntary registration: surface_dcg is the
  source of surface construct names; `go` fails on any parsed construct
  with no grounds chain (inverting AUDIT finding 1's quantifier). kernel.pl
  drops the dead surface_form rows (source/fact/external/register);
  checks.pl retargets no_self_union and fixes covers_enum arity matching.
  surface_dcg additionally owes: the raw-text region token (astgrep), the
  five unlexable constructs (|>, !rel, x.field, Entry {..}, match {..}),
  lexer-owned `.` with whitespace never meaning-changing, and the
  adversarial law: no single-character perturbation of a legal program may
  yield a different legal program silently (review_temporal_pipe.md:142-147).

## Shortest paths (amended)
  v6 replacing a v5 lint rail: T0(amended) + T1's one job (turn `from world`
  into bind) + T3 library. The timeless_rail program text does not move.
  Running v6 ghcacher: + T4 edges/keys/rel-kind + T5 shell effect.

## Orthogonality claims to hold the line on

1. T0 programs never mention time; adding T4 must not change any T0
   program's meaning (levels stay levels).
2. T1 extraction is bind-swappable: same program, canned rows in tests.
3. T2 operators are rel-in/rel-out; no syntax beyond an operator position.
4. Mode/lifetime (T6) is analysis only: no program text changes, only
   rejections and CLI warnings.
5. T8 changes storage, never semantics.

## Implementation reality (what exists today)

- T0: js engine v1 84/84 + emit_ts.pl green. Closest to done.
- T1: v5 rust extraction is the solved layer (per ARCH tech roles: rust =
  extraction home); grammar-import is the new part (lab in flight).
- T4: count-IVM measured in rust store (beat DRed 4-5x); port task open.
- T2: v5 closure/scc exist SQL-side; carry format decision to T8.
- T5-T7: labs + design only.

Shortest path to a running v6 ghcacher: T0 (have) + minimal T1 bind + T4
edges/keys + T5 shell effect. Shortest path to v6 replacing a v5 lint rail:
T0 + T1 + T3 only, no temporal tier at all.
