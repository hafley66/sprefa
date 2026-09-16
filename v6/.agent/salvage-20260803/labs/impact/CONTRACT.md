# CONTRACT: impact analysis — demand-driven laziness vs the engine as built

Analysis lane, branch lab/impact-lazy at feb14d8d, worktree
/Users/chrishafley/projects/sprefa-impact-lazy. Read-only outside this
worktree; deliverables are documents at this root. No subagents. No
pushes.

## Deliverables (two-doc law; without the unga doc this is undelivered)
1. IMPACT.md — the full analysis, file:line receipts for every claim.
2. IMPACT.visual.human.unga.md — plain words, ascii diagrams, zero
   citations.

## User rulings (verbatim where possible, 2026-08-03, binding)
- "we need the language to be lazy... rxjs is lazy... nothing runs till
  its asked."
- External events are typed clock-world rows: "an event will cause a
  trigger and its in the body and its typed in clock world even tho its
  host event or event matcher from edb".
- "we need a generic way to receive events from outside world... not
  just host."
- The canonical composition (pre-commit trigger AND every-1s after it
  first fires): first pre_commit, then switchMap to a merge of
  interval(1000) and the pre-commit edge again, share with NO reset =
  lazy init one global; the clock system must be able to express and
  check this shape (a new pre-commit must still fire after the switch).
- "we are literally subscribing to everything" is the defect: today the
  engine subscribes to all sources always.

## Ground truth already established (do not redo, DO extend)
RECON-QUERY.md at this root: the system as built is 100% eager. Queries
lower to dead metadata (emit_ts.pl:310-314), queryPlans never read,
every rule reruns every tick (runIncrementalTick), arrivals push via one
seam (POST /arrivals, binds, hosts), DDL fully eager, oracle evaluates
everything and never consults queries. Its "smallest change set" (3
bullets, end of file) is the starting skeleton; your job is the impact
of actually doing it.

## The analysis must cover, each section with receipts
1. EVERY eager assumption in the codebase, enumerated: the tick cascade,
   sql_rule_order strata, host-demand rows generated unconditionally,
   boot statements, conformance fixture semantics (final/1 asserts the
   union of ALL rels — what does a fixture MEAN under demand?), the
   Prolog oracle's evaluate-everything loop, TEXT_DOOR byte-identity
   goldens, golden-flex's self-gating coverage, the store's
   materialization, serve's binds (interval/watch spinning regardless of
   demand). For each: does demand-driven evaluation break it, change its
   meaning, or leave it alone — and what is the migration.
2. THE EDGE-PLANE TENSION, resolved as options with prices: `<+` rels
   under laziness. Before first demand: dropped vs buffered at ingress
   vs persisted via store materialization (module-catalog stance 1).
   After first demand: share-no-reset (never re-cold) per the user's
   ruling. State what each option does to conformance semantics and to
   the pre-commit example.
3. GENERIC EVENT INGRESS as a language construct: today interval/watch/
   sh are built-ins and POST /arrivals is the generic transport. Design
   the decl surface for a typed external event source (the git pre-commit
   as the worked example, entering as a typed EDB row in clock world),
   with its pure-rxjs lowering, consistent with the two arrows and with
   registry.pl's construct table. Show the user's composition in dl and
   rx, and specify what the clock checker must prove about the
   first-then-merge shape (the second-event-must-still-fire hazard).
4. DEMAND CLOSURE MECHANICS: queries as the only subscribe roots
   (single `?` surface, see recon), imports riding the same demand plane
   (module-catalog ruling: import = demand, module args = demand keys,
   eagerness = a standing query only). What the compiler can prune
   statically (rels outside every query cone) vs what must be dynamic
   (demand keys). Where the cone computation lives (analyze.pl is the
   natural host — say so or better).
5. WHAT DOES NOT CHANGE: name the invariants that survive (byte-identity
   of the non-query pipeline per the recon's claim, the arrows'
   semantics inside a demanded cone, the store schema). Be precise;
   "nothing else changes" claims need receipts.
6. MIGRATION LADDER: smallest landable steps, each with its gate, each
   leaving the battery green. State explicitly which existing fixtures
   would need a standing query added to keep meaning what they mean.

## Style laws
rxjs/prolog/SQL vocabulary only; banned words provenance/substrate/
load-bearing/regime; descriptive dl variable names in every snippet;
every dl snippet carries its rx lowering; comment budget n/a (docs).
Present forks with recommendations and prices; decide nothing the user
has not ruled.
