# Modules / dot access / catalog — design session record (2026-08-03)

Everything here is a STANCE, amendable by Chris at will. Nothing in this doc
is a law unless he later says so.

## The design, one breath

There is no module system. There are catalog rels. Every declared thing
(module, rel, column, generic instance) is a row; nesting is a parent edge;
the dotted path is DERIVED by transitive closure, never stored. Identity is
an int id, spelling is edges, so renames edit one row. Import = demand for a
module instance. Laziness = the existing demand clocking. Generic module
args = demand keys; static args monomorphize to instance rows at compile
time; the set of tables is static, runtime only adds rows.

```
__catalog_rel(rel_id, parent_id, local_name, kind).   % kind: rel|column
__catalog_instance(instance_id, rel_id, args_digest).
```

There is no module kind. A "module" is informal speech for a rel/0 that has
child rels. Any relation can nest children in principle (a rel/N's children
close over its columns as demand keys); v1 permits nesting under rel/0 only,
and lifting that to rel/N later is purely additive.

Existing machinery this rides on (verified in the 2026-08-03 dot-access
plans, three independent lanes): ref(T) int-id dictionary joins (the dot
lowering, shipping), registry.pl decl facts (the catalog's compile-time
half), demand/magic-set (laziness), type-IR facts+emitters plan (the first
catalog emitter), monomorphization ladder (instances).

## Decisions taken 2026-08-03 (Chris)

1. Catalog is MATERIALIZED into the store as __catalog_* tables (not
   compile-time-only). Current stance: user rules read it, nothing derives
   into it; Chris reserves amendments to this at any time.
2. Sequencing: rides ON the type-IR MVP. Catalog emitter first, dot surface
   second.
3. From the dot-access duel questions, resolved by this design: dots YES;
   one containment relation (child-rel names and column names share one
   namespace per parent, collision refused at decl); no `::` needed; no
   traits (contracts become catalog rows if ever wanted).
4. Import unbundling: referencing anything in a module IS the demand; alias
   sugar (`use a.b as c`) is out of v1.
5. Relative paths (`..`, `self`): out of v1. Absolute + local only.
6. M3c shadowing: SHADOW (Chris, 2026-08-03). Nearest name wins silently;
   a shadowed outer name stays reachable, always, by spelling its full path.
7. Modules do not exist as a kind (Chris, 2026-08-03): a "module" is a rel/0
   with children. The catalog knows only rel and column. Nesting under rel/N
   is the reserved future generalization (children close over parent
   columns); v1 nests under rel/0 only.
8. Dotted heads: YES (Chris, 2026-08-03). A rule head may be a dotted path
   (`a.b(x) <- ...`), contributing rules to a nested rel from outside its
   block; multiple files contributing to one rel = ordinary datalog union.
   v1 default (amendable): a dotted head CONTRIBUTES to a rel the path's
   home block declares; it does not CREATE new paths from outside. Creation
   stays in the block, so the catalog row's home is always one obvious file.
9. Dotted member reference/destructure in bodies: YES (Chris, 2026-08-03).
   A bound row variable's columns read by dot (`F.at`), chains follow ref
   columns hop by hop (`F.at.repo`), each hop = one dictionary join (the
   shipping ref(T) lowering). Resolution per M3d: bound-variable-first, so
   `F.at` is member access when F is bound in the body, path access
   otherwise.
   rx: member = map(row => row.at); chain = the decode join per hop, keyed
   on the int id, SEARCH-not-SCAN receipted.

10. Block-under-rel is the extension surface (Chris, 2026-08-03, direction
    not spec): future constructs are desugarings into nested rels + catalog
    rows, via the existing term_expansion machinery; nothing new enters the
    engine core. First candidate: REL-LEVEL MATCH — not a new construct and
    not rust inspiration: the body matching sugar dl already has
    (enum_match/unification), lifted to rel position. Parent rel's demand
    key = the scrutinee, each arm = a child rel, patterns reified as rows
    (a routes table). Semantics INHERIT from the existing body match; the
    lift itself invents nothing.

    ```
    rel route(msg) {
      ping(x) <- msg = ping(x).
      pong(y) <- msg = pong(y).
    }
    ```
    rx: partition/groupBy over the scrutinee stream, one leg per arm.

    Forks the LIFT opens (body-match semantics themselves are already
    settled by the enum_match lab and are not being redesigned):
    - overlap across arms-as-rels: body match's existing behavior carries
      over (set semantics; = uncut prolog under findall). If a committed
      choice is ever wanted, prolog cut survives into set semantics only as
      its negation translation (arm N fires iff no matching arm with
      ordinal < N; ordinal = a column on the routes table, priority stays
      data). Not v1.
    - exhaustiveness: with patterns as rows and enums in the catalog,
      totality = a JOIN (arms against variants; missing variant = a derived
      refusal row). The check is a query, not a compiler feature.
    - routes static vs data-driven: static patterns desugar to WHERE at
      compile (free, v1); patterns arriving as data rows = runtime pattern
      interpretation, a real interpreter cost, later if ever.

11. PARKED direction (Chris, 2026-08-03, "some day"): v8-hidden-class
    promotion for json values. A decode shape used repeatedly = a hidden
    class; upconvert stable shapes to materialized rels (columns = keys,
    typed captures already carry the types), moving that state from the
    blob plane (no inner clocks) to the row plane (full tick lineage).
    Catalog makes shapes rows, so promotion is a catalog operation and can
    be heuristic-automatic later. No spec, no lane, direction only.

## Open (one word each)

- Instantiation recursion beyond memo-identical args (M4e): v1 stance =
  refuse non-identical self-instantiation; revisit on a real program.

## Case ledger (full enumeration lives in this session's chat log 2026-08-03)

F1 static-tables: RESOLVED (catalog written at compile, dyn-DDL never).
F2 name root: RESOLVED by construction (int id + parent edges; declared
   spelling re-resolves; fs moves don't churn ids).
F3 module queryability: RESOLVED (meta-querying catalog rels; no default
   export).
M1 param kinds: scalar static -> mono; scalar data-driven -> args become
   leading columns (magic set); rel-valued -> static names only, else
   refuse; type-valued -> type-IR step f.
M2 identity: global memo on evaluated ground args.
M3 resolution: left-to-right walk over declared containment; bound-var-first
   for Row.col vs path; one namespace per parent.
M4 laziness: unreferenced = never lowered; two-phase checking (well-formed
   always, arg-typing at first instantiation); teardown = keep-forever with
   digest guards; no mid-tick table creation.
M5 doors: SQL mangling `a__b__c__<digest>`; scip descriptor path = the
   derived url (type-IR pkg field becomes real); existing flat rels = root
   children, zero migration.

Refusals introduced: module_name_collision, container_and_leaf,
non_static_rel_arg, growing_instantiation_cycle, unresolvable_path.

## Next concrete steps

1. Type-IR MVP (~190 LOC, plans in sprefa-plan-typeir/PLAN2.md) with its 2
   pending calls, emitting the spine catalog.
2. Generalize that emitter's facts into __catalog_rel rows.
3. Dot surface in the parser (25-50 lines + terminator disambiguation),
   resolving against the catalog.
4. ARCH task/3 rows for the above — to be added when the main tree is not
   shared with a parallel session (rows drafted in the dot-access lane
   plans).
