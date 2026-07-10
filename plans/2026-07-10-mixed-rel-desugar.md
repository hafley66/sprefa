# Mixed source+derived rels: desugar the split instead of bailing

## Context

A rel headed by both a source rule (scan/match/ast/sg/json/cmd/comment) and a
derived rule bails today (tick.rs:157-170), and the term-extract twin of the
same hazard bails right below it (tick.rs:188-201). The reason is tick-phase
plumbing, not language semantics: `reconcile_sources` fills source rows
incrementally (tracked in `_prov`, retracted per path), while
`rebuild_derived` (mod.rs:5420) starts every derived rel with `DELETE FROM
rel` + recompute, so shared tenancy silently drops the reconciled rows. The
bail was the honest cheap fix; the documented workaround is the manual
split-then-union (examples/anim-self.dl pin/fpin -> span_of).

The complaint this plan answers: the bail makes users stratify ENGINE
INTERNALS. Classic datalog's EDB/IDB separation gives it cover, but Soufflé
already allows `.input` + rule-headed relations, and dl itself already
allows ground facts + derived rules on one rel. The restriction is
idiosyncratic and it keeps biting (ledger 2026-06-13; agent-sharp-edges;
the gh-cache @next-carry routing).

## The shape: synthesize the manual pattern, do not touch tick phases

Key realization that keeps this small: the desugar needs NO new tick
machinery. Rewrite the program before rule classification so the engine
sees exactly what a user writing the manual pattern produces:

- Source heads of a mixed rel `orders` rewrite to a hidden twin
  `orders__src` (an ordinary source rel: reconcile fills it, `retract_paths`
  (mod.rs:3712) path-keys it, `_prov` tracks it — all unchanged).
- Derived heads rewrite to `orders__drv` (an ordinary derived rel).
- The visible rel `orders` becomes a plain derived rel with two synthesized
  union rules: `orders(...) <- orders__src(...).` and
  `orders(...) <- orders__drv(...).`
- Body reads are NOT rewritten. Everything (other rules, recursion through
  the mixed rel, `?` queries, `@next` carries, the panel, closures) reads
  the visible rel, which the ordinary fixpoint keeps correct — Tarjan
  components (`rel_components`) order the union after its twins, and a
  recursive read of `orders` inside one of its own derived rules is just a
  recursive component like any other.
- Same rewrite for the extract+derived case: extract heads -> `__src` twin
  (eval_extract_rules fills it before the fixpoint, which is already the
  required order), derived heads -> `__drv`. This ALSO retires the
  "term-extract cannot feed a @next carry" restriction for free — the twin
  IS the routing gh-cache does by hand.

Both bails then become unreachable for the supported cases and are kept
only for the excluded combinations (below).

## Type signatures

```rust
// src/engine/tick.rs (or a new src/engine/desugar.rs) — runs on the Program
// AFTER parse/typecheck, BEFORE rule classification in tick(). Pure.
struct MixedRel {
    visible: String,      // "orders"
    src_twin: String,     // "orders__src"
    drv_twin: String,     // "orders__drv"
    kind: MixedKind,      // SourceDerived | ExtractDerived
}

fn desugar_mixed_rels(prog: &Program) -> Result<(Program, Vec<MixedRel>)>;
//   classify each rule's head kind (reuse the existing source/extract/
//   derived classification in tick.rs:120-150);
//   mixed = a rel with heads in >1 class (facts count as derived-side: they
//     already coexist with derived rules today; a fact on a mixed rel lands
//     in the __drv twin);
//   for each mixed rel:
//     bail-if-excluded (lattice key/merge decl, @in/@out port, rev-twin
//       builtin, reserved sink) — the OLD bail text, narrowed;
//     synthesize RelDecl for both twins (cols/types cloned from the visible
//       decl; twins carry a `synthesized: true` marker or just the __ name
//       convention);
//     rewrite head.rel on each rule to its twin (spans untouched — diags
//       keep pointing at user source);
//     append the two union rules (synthetic span = the rel decl's span);
//   returns the rewritten Program + the mapping for diagnostics/telemetry.
```

Rewrite happens once per tick entry (tick + tick_paths share the chokepoint
where rules are classified; put the desugar immediately before that split
so both paths see the same program). It is pure and cheap; no caching.

## Instance lifetimes

- Twin tables: created by the ordinary `declare` path when the rewritten
  decls flow through the program declare; live in the db like any rel;
  dropped by nothing special (a program edit that unmixes the rel leaves
  orphan `rel_orders__src` tables — same lifecycle as any renamed rel
  today, acceptable).
- `Vec<MixedRel>`: per-tick, used for error attribution and (optionally)
  a `dl_diag` info lint naming the desugar; not stored.
- `_prov` rows for the src twin: identical lifecycle to any source rel.

## Storage / sequence / uniqueness

- Storage: two extra real tables per mixed rel (`rel_orders__src`,
  `rel_orders__drv`) plus the visible `rel_orders` now being
  derived-rebuilt. Set semantics dedup the union exactly as the manual
  pattern does.
- Write order per tick: reconcile -> `orders__src` (incremental);
  eval_extract_rules -> `orders__src` (extract case); fixpoint ->
  `orders__drv` then `orders` (component order guarantees twins first).
- Uniqueness: `__` twin names collide with nothing user-writable if the
  desugar REJECTS user rels containing `__src`/`__drv` suffixes (add to the
  reserved-name guard; grep first that no in-tree .dl uses such names).
- Attribution/digests: twins are ordinary rels so `seed_rel_digests`,
  `affected_derived` scoping, `rel_count`/`stmt_ms` all just work; the
  only polish is telemetry display mapping `orders__drv` back to `orders`
  via the MixedRel table (nice-to-have, stage D4).

## Exclusions (first cut, keep the narrowed bail)

1. Lattice `key(...)`/`merge(...)` on a mixed rel: the upsert semantics of
   the union step need a real design (which side wins a key collision is
   order-dependent). Bail with the old message + "lattice rels cannot be
   mixed yet".
2. `@in`/`@out` port rels: ports already have their own head bail
   (tick.rs:172-186); mixing stays refused.
3. Reserved builtins/sinks (diag, hover_note, demand sinks, rev twins):
   already guarded by reserved-name checks; the desugar never sees them.
4. Rev-aware user rels: none exist as decls today (rev twins are builtin);
   nothing to do, note only.

## Catalog / panel hygiene

- `rel_catalog` and the panel's `_node`/`_edge` PRAGMA discovery must skip
  twins: filter `name NOT LIKE '%\_\_src' ESCAPE '\'` etc. at the catalog
  fill (one site) and check whether the panel discovery needs the same
  (its LIKE pattern `rel_%_node` only matches twins if someone mixes a rel
  literally named `x_node`; add the filter anyway).
- `dl --rows orders` and `? orders(...)` read the visible rel: no change.

## Stages

- **D1 (M)**: `desugar_mixed_rels` + source+derived path + twin-name
  reserved guard + catalog filter. Flip tests/mixed_source_derived.rs from
  asserting the bail to asserting the union rows survive a changed-path
  retick (the original silent-loss repro, now the positive e2e: scanned
  rows persist, derived rows rebuild, a file edit retracts only the
  scanned side). Keep one bail test for the lattice exclusion.
- **D2 (S)**: extract+derived path (same rewrite, extract classification)
  + the @next-carry-through-extract e2e (the gh-cache shape without the
  manual split) + retire that bail.
- **D3 (S)**: docs sweep — the sharp-edges skill section, CLAUDE.md style
  note ("One rel = one rule kind" becomes "mixed heads desugar; lattice
  mixing still refused"), book chapter if it states the law, anim-self.dl
  comment (the manual split stays valid, no longer mandatory).
- **D4 (S, optional)**: telemetry display mapping twins -> visible name;
  `dl_diag` info lint showing the desugar happened (directive-visibility
  precedent: dl's own magic shows itself).

## Verification

- The flipped mixed_source_derived e2e (D1) is the core proof: mixed rel,
  full tick, `--changed` retick after editing the scanned file, row set
  exact at every step.
- Recursive mixed rel test: a derived rule on the mixed rel that reads the
  mixed rel (self-recursive through the union) reaches fixpoint with
  source rows as seeds.
- Existing suites green; magic-rel audit green (twins go through declared
  decls); `--parse-only` unaffected (desugar is post-typecheck — verify
  diags on mixed-rel rules still position on user spans).

## Critical files

- src/engine/tick.rs:120-201 (classification + both bails — the desugar
  slots immediately before this and deletes/narrows the bails)
- src/engine/mod.rs:3515 (reconcile_sources), 3712 (retract_paths), 5420
  (rebuild_derived), 5243 (eval_extract_rules) — read-only context; none
  should change
- src/engine/mod.rs reserved-name guard region (add the `__src`/`__drv`
  suffix rejection beside the existing guards)
- tests/mixed_source_derived.rs (flips), tests/it/ for the new e2e
- examples/anim-self.dl, examples/gh-cache.dl (docs-only comment updates,
  D3)
