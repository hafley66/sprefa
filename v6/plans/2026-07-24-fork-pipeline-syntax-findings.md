# Fork findings: pipeline syntax + surface rulings (2026-07-24, session forked-for-sql-pipeline-syntax)

Rejoin doc for the main session. Rulings are ALSO pinned in DECISIONS.md
(two blocks: "Surface rulings, fork session 2026-07-24" + additions).

## Ruled in this fork

1. **Comma stays unordered.** Pipe `|>` syntax SHELVED. Its semantics survive
   without the syntax: a host/temporal atom in a body is the tick-boundary cut
   (the yield point); compiler splits the rule there. Pre-effect request rel =
   the saved coroutine frame (columns = live variables); post-effect rule wakes
   on response rows. Minted intermediates = rel(0) scratch (fused into one SQL
   SELECT when pure); cross-tick stage rels = durable (the effect cache).
2. **Postfix effect sigils**: `fetch?(args)` idempotent effect (digest-cached),
   `fetch!(args)` mutation (fire-once, never replayed). Auto Result<T,E>:
   errors land as columns (QueryState shape), stream never dies. Postfix
   position = the TIMECUT marker (where the body splits across ticks).
   `!` prefix stays negation; `!x!(a)` legal; revisit = v7.
3. **Time builtins take rxjs names VERBATIM**: interval/timer/delay/
   debounceTime/throttleTime/auditTime. clock/every die as names.
   `interval!(300, bucket)` — time sources are effects. Law: no synonyms when
   an rx target name exists. Store spellings underneath (no subscription-local
   state).
4. **Slash-liberal idents** (lisp-style): `gh/pull_request` = one ident; types
   addressable as URL paths. `/` binds into idents unless spaced; division
   needs spaces; regex literals value-position only.
5. **diag = plain rel** read by the LSP plugin — no state/event kind needed.
   Diags = the FIRST RETRACTION INSTANCE (file fix -> re-extract -> facts
   retract -> diag rows die via delta plane; the DRed golden test).
   Harness-critical for agent use. `--check` = reader: severity=error -> exit 2.
6. **Type system = JSON5 shapes**: nested object/array/primitive, NO generics,
   tuples now, named shapes later, slash-path addressable. Plus Key/Min/Max
   column wrappers + base column types. That's the whole system.
7. **Inline filters**: constants in atom args (`fetch?(ep, prev, 200, etag,
   body)`) = equality filter; on host rels an output-column constant filters
   AFTER the effect (not part of the request digest). `status: =200` expr-in-
   named-arg noted for later.

## Theory acquired (for the record)

- Pipe syntax = **user-written SIPS** (sideways information passing, the
  magic-sets term). Semantically ignorable on pure atoms, REAL at host stages.
  GoogleSQL pipes / KQL / PRQL / dplyr / rx chains = the family; **DCG is the
  40-year-old precedent** for a pipeline neck lowering to plain head/body
  clauses (threads S0->S1->S2 exactly like our minted stage rels).
- Prolog's ordered comma exists for backtracking (choice points need concrete
  order; cut only means anything under order). Datalog dropped backtracking so
  it dropped order. Mixing strategy (Mercury/XSB): order is a hint on pure
  code, real only at impurity — exactly our `?`/`!` line.
- Purity = TWO-way (mode-polymorphic, replayable); effect = ONE-way (single
  mode, must-be-cached). Mercury's four orthogonal axes (type/mode/determinism
  /purity) = the unit-vector decomposition of `host`. sh/http/extract differ
  ONLY in executor DSL.
- Cut stays buried: needs backtracking, we have none.

## v3/v4 archaeology (git receipts)

- v3 last real work e499b8a0 2026-05-20; v4 last 2026-05-18/19 (memo seam,
  content-hash staleness, emit_sprf reify, source-keyed owner identity +
  dirty-source re-render loop); archived 4c9662fc 2026-07-01 ->
  ~/projects/sprefa-archive-20260701/ (v3, v4, v5cozokuzu).
- v4 src/ was React-shaped: mounted_query.rs, dirty_source.rs, memo.rs,
  chan.rs (chan discussion is a rerun), ghcache.rs (ghcacher rode along),
  cursor_codec.rs, cst/. **gen's better name is in there: render/emit/mount**
  — codegen = render-to-file, memoized by content hash, retracted like any
  derived row. Name still unruled.
- The v5 pivot to datalog-family was so design questions get prior art
  instead of raw language invention — this fork is that bet paying out.

## Still undetermined (blocks writing real programs: 1-3)

1. Scalar kernel spelling: arith, string fns, `${var}` interp, `=~`
   (probably keep-v5, never formally ruled; `/` now needs spaces).
2. Extraction ops as body atoms (jsonp etc.): PURE -> no sigil, builtin rels.
   One ruling needed: builtin pure predicates vs Composed extract-registry ops.
3. `--check` reader formalization (one line).
4. json_group_* -> JSON5 constructor terms in heads (likely, unruled).
5. gen/render port ruling. 6. use/modules/std. 7. scc() as builtin host.
8. spine column types (rides extract/E2 seam). 9. collect(var,n) batching.
10. `_` wildcard + `?` query prefix formal nod.
