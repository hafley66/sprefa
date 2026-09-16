# CONTRACT: plan — file body as first binding scope, rel-as-module, import-as-demand

Plan lane, branch lab/plan-modscope at feb14d8d, worktree
/Users/chrishafley/projects/sprefa-plan-modscope. READ-ONLY outside this
worktree. No commits to code; deliverables are documents at this worktree
root. No subagents.

## Deliverables (two-doc law; a plan without the unga doc is undelivered)
1. PLAN.md — receipts, file:line citations, the full design.
2. PLAN.visual.human.unga.md — plain words, ascii diagrams, ZERO
   citations. Chris reads only this one.

## User rulings (verbatim, binding, 2026-08-03)
- "we have to formalize the body text of a dl6 top level file because
  that is the first local binding scope concept, and rel-as-module must
  nail that part. really just ruby metaclasses but in the types for
  later (remember all dots, if its under something its a dot away)"
- "we do not demand anything until we query it, we also can create
  conditions for eagerness, but in reality i want as lazy as possible"
- "compiler is checking that lazy but we are lazy (we can make all table
  defs up front but that is it). we need a different phase to force
  subscribing. right now ?- i thought was the subscribe operation
  semantically"
- "if its in the file its a literal question so its a subscribe aka
  whatevs" — AND the user then asked for this to be verified against the
  system as built ("or am i wrong"). A parallel flash recon lane is
  reading the v6 pipeline's actual `?-` handling; its REPORT.md will land
  at /Users/chrishafley/projects/sprefa-recon-query/REPORT.md. Wait for
  it / poll for it before finalizing the query-semantics section, and
  reconcile the plan with what IS, not what either of us believes.

## Grounding (read these)
- 2026-08-03-module-catalog-ruling.md at this worktree root (11 stances:
  catalog rels __catalog_rel(id,parent,name,kind)/__catalog_instance,
  dotted path derived via closure never stored, identity=int id,
  import=demand, module args=demand keys, static tables invariant,
  materialized into store, modules don't exist = rel/0 with children,
  dotted heads contribute cross-file union, shadow nearest-wins with
  full-path escape, block-under-rel via term_expansion).
- The landed dot_get surface: worktree
  /Users/chrishafley/projects/sprefa-dots-land (branch lab/dots-land,
  commits 4caf66f5/e88a9b01) — head+body member access desugaring to
  decode at phase 44. Module paths will share the dot parse shape;
  the plan must state how `mod.rel` atoms and `Row.field` reads
  disambiguate (catalog lookup vs bound-variable, per the ruling's
  bound-var-first stance).
- v6/prolog/registry.pl (the existing in-repo catalog precedent),
  v6/dl/LANG.md (the two arrows <- and <+), v6/prolog/1_expansion.pl
  (phase list), golden-flex.dl6 (query forms in use).

## The plan must nail, in order
1. FILE SCOPE: a .dl6 file body = the body of an anonymous rel/0; every
   top-level decl is its child, one dot away; file gets a catalog row.
   Spell out: what is bound in file scope, decl vs rule vs query,
   name resolution (nearest enclosing wins, full dotted path reaches
   outer), what two files in one compile see of each other.
2. REL-AS-MODULE: nesting surface (additive rel/N children closing over
   parent columns per the ruling), the metaclass reading: a rel's
   catalog row is both instance and type carrier; the type IR sees the
   same tree.
3. IMPORT AS DEMAND with the user's laziness rulings: compile
   total/eager (all DDL + checks up front), zero clocking until a query;
   `?-` as the subscribe (pending recon confirmation); demand flows
   query-to-body magic-set style across files; eagerness = a standing
   query only (an `eager` spelling desugars to one), never a second
   mechanism.
4. THE LATE-SUBSCRIBER EDGE: `<+` log rels demanded late — reconcile
   with stance 1 (materialized store rows = read persisted + live
   continuation); state exactly what a late importer of an edge-plane
   rel observes.
5. Every construct shown carries its pure-rxjs lowering (law); dl
   variable names descriptive; vocabulary rxjs/prolog/SQL only; banned
   words (provenance/substrate/load-bearing/regime) excluded.
6. Phasing ladder: smallest landable step first (catalog rels emitting
   rows for declared rels + a conformance fixture querying its own
   catalog), then nesting, then demand wiring, then dotted heads.
   Each step names its gates.

## Open questions to PRESENT (not decide): list every fork with a
recommendation and its price, the way the dot-access plans did.
