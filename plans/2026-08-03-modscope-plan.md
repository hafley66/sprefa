# PLAN: file body as first binding scope, rel-as-module, import-as-demand

Lane: plan. Branch `lab/plan-modscope` at `feb14d8d`, worktree
`/Users/chrishafley/projects/sprefa-plan-modscope`. No code changed; this document and
`PLAN.visual.human.unga.md` are the deliverables. No subagents were used. Every claim
about the system as built carries a `file:line` receipt I opened and read; cross-worktree
receipts name the worktree.

Binding inputs: `CONTRACT.md` (this root), the user rulings quoted there (2026-08-03),
`CONTRACT-ADDENDUM.md` (this root, the 2026-08-03 laziness rulings; ruling 3 carries a
correction, applied in sections 3.1, 3.2, and 3.4),
`2026-08-03-module-catalog-ruling.md` (this root, the 11 stances), and the recon lane's
verdict at `/Users/chrishafley/projects/sprefa-recon-query/REPORT.md`, which this plan
waited for and reconciles against in section 0.

Vocabulary: rxjs/prolog/SQL words only. Banned words excluded. dl variables in examples
are descriptive.

---

## 0. What `?` does today (recon reconciliation, settled before anything below)

The user asked: "if its in the file its a literal question so its a subscribe aka
whatevs... or am i wrong". The recon lane traced the whole pipeline. Verdict, quoted
from `REPORT.md:205-220` and confirmed by my own reads: **as built, the belief is FALSE,
with one true fragment.**

- The parsed surface is a SINGLE question mark, `? Name(args).`, not `?-`
  (`v6/prolog/compile/parse_dl.pl:977-978`: `query_stmt(query(Atom), ...)` consumes
  `lit_dcg(`?`, ...)`). A file can hold many query lines
  (`parse_dl.pl:348` folds each onto the `Queries` list; four of them at
  `v6/tsv2/goldens/multirepo_crawl/0_multirepo_crawl.dl6:113-116`).
- A query line becomes `query(Atom)`, rides the decl stream
  (`v6/prolog/1_host_expand.pl:59`), and lowers to inert metadata
  `{ rel: "name", arity: N, snapshot: "current" }` (`v6/prolog/emit_ts.pl:310-314,
  419-422`). The atom's ARGUMENTS are dropped at emission.
- The runtime never reads `queryPlans` (grep over `v6/tsv2/runtime/` and
  `v6/tsv2/serve/` finds no consumer; only the field declaration at
  `v6/tsv2/runtime/types.ts:438`). Every rule is recomputed every tick
  (`v6/tsv2/serve/3_engine.ts:191` into `runIncrementalTick`,
  `v6/tsv2/gen_emitted/native_ts_query_term.ts:376-396`). Evaluation is driven by
  pushed world arrivals (`v6/tsv2/serve/4_http.ts:426`, `v6/tsv2/serve/2_binds.ts:4-5`,
  `v6/tsv2/serve/1_hosts.ts:690`), never by queries.
- Queries are not typechecked: `analyze.pl`, `0_program_check.pl`, `0_type_plane.pl`,
  `lower.pl` contain no `query(` term at all; the compile-time gates run over
  `prog(Decls, Rules)` only (`v6/prolog/compile.pl:128-135`), and the query plans the
  expansion produces are discarded on the compiler path (`compile.pl:105-107`).
- The oracle matches: `v6/prolog/conformance/engine.pl:503-531,558-560` evaluates all
  rules every tick and reports the union of all rels; no `query(` term appears in
  `engine.pl` or `level_eval.pl`.
- TRUE fragment: all table defs are created up front at boot regardless of queries
  (`v6/tsv2/serve/3_engine.ts:220-232` runs the full `ddl` array; the array CREATEs
  every rel including unqueried ones, `native_ts_query_term.ts:133-193`).
- The only read a query-shaped name gets today is the pull API
  `LiveEngine.rows(rel)` (`3_engine.ts:127-134`, a one-shot SELECT over
  `finalSelect`, re-run per HTTP `GET /idb/:rel` at `4_http.ts:429`).

Also on record: the rxjs lifecycle words `subscribe/1`, `unsubscribe/1`, `complete/1`,
`error/1` are RESERVED registry rows (`v6/prolog/compile/registry.pl:44-47`), and
golden-flex notes "their kernel forms are next/finalize on the demand rel"
(`v6/dl/fixtures/golden-flex.dl6:76-77`).

**Consequence for this plan.** Sections 1-2 describe constructs that are new but
compatible with what is built. Section 3 (import-as-demand) is NOT a description of the
current system; it is the target semantics per the user's rulings, and section 6 prices
the delta as the recon's smallest-change list (`REPORT.md:222-237`) sequenced onto the
phasing ladder. Where the plan and the built system disagree, the plan says so in the
open. The user's model ("compile eager, nothing clocks until a query, `?` is the
subscribe") is adopted as the TARGET, not reported as the present.

---

## 1. FILE SCOPE: the file body is the first binding scope

### 1.1 The construct

A `.dl6` file's body text IS the body of an anonymous rel/0, the FILE REL. Every
top-level declaration in the file is a child of that rel, one dot away. The file gets
one catalog row, parented at the root row:

```
__catalog_rel(rel_id, parent_id, local_name, kind).   % kind: rel | column
__catalog_instance(instance_id, rel_id, args_digest).
```
(shape per `2026-08-03-module-catalog-ruling.md:17-19`; stance: materialized into the
store as ordinary tables, user rules read it, nothing derives into it, ruling doc
:33-35. There is no module kind, ruling :48-51; a "file" row is a rel/0 row whose
`local_name` is the file's spelling.)

One root row, id 0, parents every file row in a compile. The dotted path of anything is
DERIVED by transitive closure over `parent_id`, never stored (ruling :11-14): the file
`orchard`'s rel `tree` has path `orchard.tree`; a column of it is
`orchard.tree.tree_id`. All dots: if it is under something, it is one dot away.

rx: the catalog is two ordinary tables whose rows arrive once at boot; the path closure
is a recursive datalog rule over `parent_id` (SQL: a `WITH RECURSIVE` over one indexed
table; rx: nothing per-tick, the closure rows are computed at compile and stored).

### 1.2 What is bound in file scope

File scope binds exactly the local names of the file's top-level declarations: rel
names, enum names, `sh` host names, `bind` names. Nothing else. Rule variables are
clause-local prolog bindings and never enter file scope; a query line binds nothing.

The three statement kinds a file body can hold (parse receipts: `parse_dl.pl:540-547`
dispatches each statement to `decl_list | rule | query`; `parse_dl.pl:132-137` emits
`prog(Decls, Rules)` or `program(Decls, Rules, Queries)`):

- **decl** (`rel tree(tree_id: int).`, `enum`, `sh`, `bind`): creates catalog rows
  (one rel row + one column row per column), creates DDL up front, creates NO demand.
  This is the whole eager half: tables and checks exist before anything clocks.
- **rule** (`head <- body.`, `head <+ body.`): contributes one clause to the rel its
  head resolves to. The head name resolves from file scope (section 1.3). The two
  arrows keep their shipped semantics: `<-` level (maintained view, retracts),
  `<+` edge (fires on arrivals, appends, never retracts) (`v6/prolog/LANG.md:29-35`,
  exercised throughout `golden-flex.dl6:285-424`).
- **query** (`? tree(TreeId, Species, Site).`): a standing subscription to a rel
  resolvable from file scope. Today inert (section 0); target semantics in section 3.

rx: a decl is a `CREATE TABLE` plus one catalog row insert at boot. A rule is a stream
definition (cold: nothing flows until subscribed). A query is `.subscribe()` on the
named stream. File scope itself lowers to NOTHING at runtime: it is a compile-time
scope, erased into int ids before emission.

### 1.3 Name resolution

Per ruling stances 6 and 9 (:46-47, :60-65) and the shipped dot lowering:

- **Nearest enclosing wins, silently** (SHADOW). A name lookup walks outward from the
  use site: enclosing block, file, root. The first match binds.
- **A shadowed outer name stays reachable, always, by spelling its full dotted path.**
- **Bound-variable-first**: in a rule body, `Row.field` is member access when `Row` is
  bound by that body (the shipped dot_get desugar, `0_dot_expand.pl:28-31,171-177` in
  worktree `sprefa-dots-land`: root must be a bound body variable, else the named
  refusal `unresolvable_member`). When the root is NOT a bound variable, the dot chain
  is a CATALOG path: `orchard.tree` resolves by walking catalog child edges from the
  root. One check order, two targets: bound variable -> decode join; unbound atom root
  -> catalog lookup, else `unresolvable_path` (ruling :133-134).
- One namespace per parent: a rel's child-rel names and column names share it, a
  collision is refused at decl (`module_name_collision`, `container_and_leaf`,
  ruling :42, :133-134).
- **No relative paths in v1** (ruling stance 5, :45): the spellings `..` and `self`
  are refused; a path is either absolute from the root or a local name found by the
  walk above. Nothing else parses as a path.

The disambiguation the contract asks for, stated once: `mod.rel` and `Row.field` share
one dot parse shape (`dot_get` chains, dots-land phase 44,
`sprefa-dots-land/v6/prolog/1_expansion.pl:36`). The bound-variable-first stance splits
them AFTER parse: a chain whose root the body binds is member access and desugars to
`decode` exactly as shipped (`0_dot_expand.pl:9-20`); a chain whose root is a bare atom
is a module path and resolves against the catalog at expansion time, rewriting the atom
to the int id (SQL-mangled name `a__b__c__<digest>` per ruling M5, :129-131). The two never
collide because a body cannot bind a variable spelled like an atom.

rx: member access is `map(row => row.field)` per hop, one dictionary join keyed on the
int id (ruling :64-65). Path access is erased at compile: the emitted SQL names the
mangled table directly. Zero runtime cost for paths.

### 1.4 What two files in one compile see of each other

Today the compiler takes exactly one source string per compile
(`v6/tsv2/serve/0_compile.ts:98-108`); multi-file compile is itself new work. Under
this plan:

- A compile unit is a SET of files. Each file is one root child row. File `a` and file
  `b` are siblings.
- File `a` sees file `b`'s top-level declarations ONLY by full path: `b.tree`. Local
  (unqualified) names never cross files. There is no import statement: referencing
  `b.tree` IS the demand (ruling stance 4, :44; alias sugar `use` is out of v1).
- Two files may declare same-named rels without collision: different parent ids,
  different paths. Same-named FILES at root collide (fork F1, section 7).
- A rule in file `b` may have a dotted head `a.tree_label(...) <- ...` contributing to
  a rel file `a` declares (ruling stance 8, :53-58): contribution, not creation. The
  catalog row's home is always the file that declared the rel; multiple files
  contributing rules to one rel is ordinary datalog union.

rx: cross-file reference is a plain join between two streams; multi-file compile is one
`merge` of per-file boot row batches. Union of two rule files into one head is
`merge(streamA, streamB)` upstream of the head's materialization.

---

## 2. REL-AS-MODULE: nesting, and the metaclass reading

### 2.1 Nesting surface (additive)

A block under a rel decl introduces children. v1 permits nesting under rel/0 only
(ruling stance 7, :48-51); nesting under rel/N (children closing over parent columns
as demand keys) is reserved and purely additive later.

```
rel orchard {
  rel tree(tree_id: int, species: text).
  rel picked(tree_id: int, picker: text).

  picked(TreeId, Picker) <-
    tree(TreeId, _Species),
    pick_event(TreeId, Picker, _Kilos, _Sugar).
}
```

Desugaring (the term_expansion machinery, ruling stance 10, :69-84): the block is
sugar. It flattens to ordinary top-level decls and rules, PLUS catalog rows recording
the parent edges (`tree` and `picked` get `parent_id = orchard`'s id), PLUS reference
rewrites: inside the block, `tree` resolves in block scope first (section 1.3's walk
starts at the enclosing block). After desugar, the program is term-identical in shape
to what the author could have written flat with full paths, exactly the discipline the
dot phase uses ("nothing past expansion learns a new construct",
`0_dot_expand.pl:19-20`).

The metaclass reading, "ruby metaclasses but in the types": a rel's catalog row is BOTH
the instance and the type carrier. In Ruby every object has a class and the class is
itself an object; here every rel has exactly one catalog row and that row is both the
thing you navigate (it has a parent, children, a name: the instance half) and the
carrier of the rel's shape (its column rows hang off it by the same parent edges: the
type half). There is no second meta tree, no module object next to the rel. A rel/0
with children is what other languages call a module; the catalog knows only `rel` and
`column` kinds (ruling :48-51).

The type IR sees the same tree. The type-IR MVP's fact schema
(`sprefa-plan-typeir/PLAN2.md` section 3: `table/2`, `table_symbol/2`, `column/6`) is
the compile-time projection of these same rows; the catalog emitter generalizes those
facts into `__catalog_rel` rows (ruling next steps :139-141). One tree serves the value
plane (data rows), the type plane (shape rows), and the scope plane (parent edges),
because they are one tree.

rx: a block is compile-time scope only, zero runtime footprint. Children are ordinary
cold streams. The catalog rows they emit are boot-time inserts into two tables. The
reserved rel/N generalization lowers to the demand-key form: child stream =
`parentStream.pipe(groupBy(row => key columns), ...)` with one inner subscription per
key; that is future work and costs nothing now.

### 2.2 Instances

`__catalog_instance(instance_id, rel_id, args_digest)` exists for when rels take
arguments (ruling: module args = demand keys; static args monomorphize to instance rows
at compile time, :11-14). v1 file rels and nested rel/0 blocks take no arguments, so
the instance table boots EMPTY in v1 and the first non-empty writer is the rel/N
nesting step. Static tables stay invariant: the set of tables is fixed at compile,
runtime only adds rows (ruling :11-14, F1 :116).

---

## 3. IMPORT AS DEMAND: the laziness rulings made mechanical

The rulings (CONTRACT :18-24): compile total/eager, all DDL and checks up front; zero
clocking until a query; `?` is the subscribe; demand flows query-to-body, magic-set
style, across files; eagerness is a standing query only, never a second mechanism;
"as lazy as possible". Extended by `CONTRACT-ADDENDUM.md` (2026-08-03): ruling 1
confirms this target; ruling 2 adds typed clock-world event sources (section 3.4);
ruling 3, CORRECTED, leaves refcount/teardown and before-first-demand behavior as
OPEN forks (sections 3.2 and 3.4 — an earlier draft of this section treated cold
semantics as settled, which was wrong); ruling 4 defers deep engine-migration
detail to the impact-analysis lane at `/Users/chrishafley/projects/sprefa-impact-lazy`.

Reconciled with section 0: this is the TARGET. The true fragment today (DDL up front)
stays. Everything else is new work, priced in section 6.

### 3.1 The demand root

A `? path.to.rel(BoundColumn, FreeColumn).` line is a standing subscription and the
ONLY demand root. Its atom's arguments survive to emission (today they are dropped,
`emit_ts.pl:310-314`; recon change item 1, `REPORT.md:222-228`): the query becomes a
minted derived rel carrying its projected columns, and the served runtime exposes it as
a per-query stream that re-emits on each tick's deltas (recon change item 3,
`REPORT.md:234-237`).

rx: `? display(TreeId, Note).` IS `displayStream.subscribe(row => emit(row))`. The
stream is cold until that call. Unsubscribing (file reload, query removal) is a
scope exit; WHAT that exit does to the shared pipeline (refcount teardown vs
keep-warm) is the open fork of section 3.2, recommended but not decided there.

### 3.2 Demand propagation

Demand flows backward from each query, query-to-body, magic-set style:

- A demanded rel demands every rule whose head is that rel, and each such rule demands
  every rel atom in its body. Transitively, this is a reachability closure over the
  rule graph seeded by the queries.
- A rel outside the closure is NEVER CLOCKED: no statements run for it, no working
  tables receive deltas (ruling M4, :126-128: "unreferenced = never lowered"). That
  its DDL still exists, empty and idle, is THIS PLAN'S OWN reconciliation of M4
  against F1's static-tables-invariant resolution — it is not the ruling's text, and
  if contested it becomes a fork rather than quoted law.
- Cross-file: demanding `b.tree` demands the closure inside file `b`. There is no
  import to execute; the reference itself carries the demand across the file boundary
  (ruling stance 4).
- v1 granularity is REL reachability, not column magic-sets (fork F4, section 7).

The compiler checks the lazy, statically and up front: every rel in the program is
well-formed checked whether or not it is demanded (today's gates already run over the
whole `prog(Decls, Rules)`, `compile.pl:128-135`); demand changes WHAT CLOCKS, never
WHAT COMPILES. The "different phase to force subscribing" (CONTRACT :22) is the serve
phase: compile emits the full demand-eligible graph; the runtime subscribes exactly the
query-seeded closure at boot and unsubscribes on reload.

rx: the emitted graph is a tree of Observables, and HOW demand subscribes it is an
OPEN FORK (corrected addendum ruling 3; an earlier draft asserted "ordinary rxjs cold
semantics" here as settled, which was wrong):

- **Cold-per-subscriber**: every query's subscription builds its own pipeline from
  the sources up. Two queries over the same rel run its rules twice. The last
  unsubscribe tears the branch down; a later re-subscribe rebuilds it from what the
  store holds. Price: duplicated work per watcher, and teardown/rebuild churn on
  reload.
- **Connect-once**: the first demander connects the pipeline and later demanders of
  the same rel share it (`share()`-style); each arrival is computed once no matter
  how many queries watch. What the LAST unsubscribe does is itself undecided:
  refcount (disconnect, reset the working rows) or no-reset (stay connected, keep
  the rows warm for the next subscriber). Price: teardown policy must be spelled
  per source kind, and after a full refcount teardown a re-subscribed query faces
  the section-4 late-subscriber fork all over again.

Recommendation: connect-once with refcount teardown, because the store already
shares working tables across readers and running any rel's rules twice would fight
that. PRESENTED, NOT DECIDED: the user's share-with-no-reset words describe the
pre-commit composition in section 3.4 only, not demanded sources in general.

### 3.3 Eagerness

Eagerness is a standing query and nothing else. The `eager` spelling is sugar:

```
eager roll_call(TreeId, Bucket).
```
desugars at expansion to exactly `? roll_call(TreeId, Bucket).` (one new registry row
+ one expansion clause; term-identical output). A rel the author wants hot gets a
standing question, in writing, in the file. No pragma, no flag, no second mechanism
(CONTRACT :20-24, :64-65).

rx: `eager` is `.subscribe()` retained for the life of the program. Identical stream,
identical demand.

### 3.4 Typed clock-world event sources (addendum ruling 2)

External events enter as typed clock-world rows. The shipped spellings cover three
sources (`interval`, `watch`, `sh` host decls); addendum ruling 2 wants the GENERIC
decl behind them: one surface where the author names the event rel, its column
types, and the adapter that produces rows. Worked example: a git pre-commit hook
entering as a typed EDB row.

```
event pre_commit(
  repo_path: text,
  changed_file: text,
  diff_summary: text
) via sh("git", ["hook-bridge", "pre-commit"]).
```

The decl is an ordinary file-scope decl (section 1.2): it creates catalog rows (one
rel row + one column row per column) and DDL up front, like any rel. It creates NO
demand by itself: the rows are clock-world (EDB) arrivals, and the adapter runs only
while the rel is demanded (section 3.2). A rule body reads it like any other rel; a
query demanding it subscribes the whole chain down to the adapter.

rx: the adapter is a plain Observable constructor — `new Observable(subscriber => {
spawn the bridge process; its stdout lines become subscriber.next(row); return
() => kill the process })`. Cold by construction: no subscriber, no process. In THIS
worked example the user's composition keeps the source shared with no reset once
connected (one bridge process feeding every demander); that is this composition's
shape, not a ruled default — the general refcount/reset behavior is the open fork
of section 3.2.

What a source does with occurrences that arrive before its first demand is a second
open fork (addendum ruling 3): (a) drop them — the adapter does not exist before
first demand, so nothing arrives, the lazy default; (b) buffer them in memory until
first demand; (c) store-materialize them — write to the table always, demand or not.
Recommendation: (a). Prices: (b) pays unbounded memory for rows nobody may ever
read; (c) pays write cost on every source forever and reintroduces eagerness
through the back door. Presented, not decided.

Deeper work — the adapter contract, checking `via` targets, engine wiring of source
lifecycles — is deferred by name to the impact-analysis lane at
`/Users/chrishafley/projects/sprefa-impact-lazy`, which owns laziness-vs-existing-
code in depth (addendum ruling 4). This plan fixes only the decl surface and its
consistency with the scope and demand story.

---

## 4. THE LATE-SUBSCRIBER EDGE

`<+` heads write the edge plane: occurrences fire on arrivals and never retract
(`v6/prolog/LANG.md:31-35`). The shipped retention spellings bound what the store
keeps: `log keep(all)` journals everything, `log keep(count(N))` keeps a window
(`golden-flex.dl6:246-250`), `key(1)` on an edge head keeps the current winner per key
(`golden-flex.dl6:395-398`). LANG.md already names the known consequence: "late-
subscriber backlog replay" (`LANG.md:35`).

Reconciled with ruling stance 1 (catalog materialized into the store; materialized
store rows = read persisted + live continuation, CONTRACT :67-68), the leading
reading of what a late importer of an edge-plane rel observes is:

**persisted rows, then live tail, nothing else** — this is reading (a) of fork F6
(section 6), presented as the recommendation, NOT decided here; F6 stays the single
place the fork is laid out.

- Under reading (a): at subscribe time the late importer reads the rel's CURRENT
  STORE CONTENTS — for `log keep(all)`, every occurrence since boot; for
  `log keep(count(N))`, only the retained window; for a keyed edge head, the current
  winning row per key. Occurrences that left retention before the subscribe are gone
  under (a); reconstructing them regardless is option (c) of F6.
- From then on it receives the live continuation: each new occurrence as its own
  arrival, in tick order, identical to what a from-boot subscriber sees. (This half
  is common to all three readings.)
- Under (a), the late importer cannot distinguish a rel that produced nothing from a
  rel it subscribed to late, except through the catalog and the tick log. If (a) is
  adopted, that is accepted, and it is why retention spellings are the author's tool
  for sizing the replay.

rx (reading (a)): the store read is a starting `SELECT` emitted as the first batch;
the continuation is the ordinary delta stream; the pair concatenates
(`concat(startWithRows, tail)`). ReplaySubject is NOT the model under (a): retention,
not the subscriber buffer, decides what replays. Reading (b) drops the first batch;
reading (c) replaces it with a full-history journal the rulings decline to pay for
by default.

---

## 5. Phasing ladder

Smallest landable step first. Each step names its gates. Ordering per CONTRACT :73-77
and the ruling's next-steps list (:139-144: the catalog emitter rides ON the type-IR
MVP, which is a separate lane's step a/b).

**Step 1: catalog emission for flat programs.**
The type-IR emitter's facts generalize into `__catalog_rel` rows: one row per declared
rel, one per column, one per file, parent edges, root row id 0. A conformance fixture
declares rels and then QUERIES ITS OWN CATALOG (`? __catalog_rel(...)`), graded by the
oracle like any other fixture.
Gates: (a) oracle parity for the catalog fixture on both doors; (b) the golden coverage
gate still passes with the two catalog rels registered; (c) catalog rows are written at
compile, never by rules (ruling stance 1), asserted by a fixture attempting a rule head
into `__catalog_rel` and getting a named refusal.

**Step 2: file scope and the resolution walk.**
One catalog row per compiled file, name resolution per section 1.3 (shadow
nearest-wins, full-path escape, bound-variable-first), dotted PATH ATOMS in body
position resolving against the catalog and rewriting to int ids at expansion. Still
one file per compile.
Gates: (a) unit fixtures for shadow order and full-path escape; (b) `unresolvable_path`
refusal fixtures; (c) the shipped dot fixtures (dots-land) byte-identical, since member
access is untouched.

**Step 3: nesting blocks under rel/0.**
The block surface of section 2.1, desugared via term_expansion to flat decls + catalog
rows + rewritten references. New expansion phase placed BEFORE the dot phase 44 (block
flattening produces the dotted shapes dot then reads), matching the phase-list
discipline of `1_expansion.pl:23-41`.
Gates: (a) desugar output is term-identical to the hand-written flat equivalent, tested
by `==/2` on terms; (b) round-trip printer emits the block back; (c) refusals
`module_name_collision`, `container_and_leaf` covered; (d) nesting under rel/N is a
named refusal, reserved not silent.

**Step 4: demand wiring (the section 3 target).**
The recon's smallest-change list (`REPORT.md:222-237`), sequenced: (i) query arguments
kept through emission (`1_host_expand.pl:404-410`, `emit_ts.pl:310-314,419-422`, the
emitted `IQueryPlanData` shape); (ii) the demand closure deciding what clocks — owned
by name by the impact-analysis lane at `/Users/chrishafley/projects/sprefa-impact-lazy`
(addendum ruling 4), which covers laziness-vs-existing-code in depth: host-demand
generation (`1_host_expand.pl`; an earlier draft cited `analyze.pl:124-175` here, but
that range is `event_use`/`atom_ref_args`/`guard_goal`/`bind_goal`/`tick_goal`),
statement pruning, working-table pruning, and their mechanisms and sites are that
lane's to design, not prescribed by this plan, which fixes only the section-3 target
semantics and the gates below; (iii) served standing queries: `queryPlans` consumed,
per-query streams re-emitting on deltas (`3_engine.ts`, `4_http.ts`). Plus `eager`
sugar and query typechecking (queries enter analyze; unresolvable query path is a
refusal).
Gates: (a) a fixture whose undemanded rel never clocks, asserted by absence from the
tick log and from `stats`; (b) a standing-query fixture re-emitting on new arrivals
through the served HTTP engine, golden-flex style; (c) full-program recompute preserved
when every rel is demanded (byte-parity with today's emitted tick for the existing
golden corpus, so step 4 cannot silently change demanded programs); (d) oracle vs
emitter parity on a laziness fixture: the oracle grows a demand-closure evaluation mode
and both sides agree.

**Step 5: dotted heads and multi-file compile.**
`a.b(x) <- ...` contribution per ruling stance 8, and compile units of more than one
file (section 1.4).
Gates: (a) union parity: rules contributed from outside a block produce identical rows
to the same rules written inside it; (b) contribution-not-creation: a dotted head
naming an undeclared path is `unresolvable_path`; (c) two-file compile fixture with
cross-file demand flowing (a query in file `a` clocks rels in file `b`).

Each step ships behind the repo's ordinary gates (`scripts/verify.sh`, the conformance
sweep, `just test` in tsv2) plus its own. ARCH task/3 rows for steps 1-5 are added when
the main tree is not shared with a parallel session (ruling :142-144).

---

## 6. Open forks (presented, not decided)

Each fork: the options, a recommendation, and its price.

**F1. File row naming.** Options: (a) file stem as `local_name`; (b) content digest;
(c) one row per path segment (directories as rows). Recommendation: (a) stem.
Price: two same-stem files collide at root; v1 refuses the second with a named refusal
and (c) stays available when real directory structure matters. (b) was priced and
rejected: paths become unreadable and every rename-free content change keeps the id but
the spelling no longer means anything to a human.

**F2. `?` vs `?-` spelling.** The shipped parser reads single `?`
(`parse_dl.pl:977-978`); the user's mental spelling was `?-`. Recommendation: keep `?`.
Price: one wrong mental model to correct, in the docs. The alternative (accept both)
costs two spellings for one construct, and `?-` collides with every prolog reader's
directive reflex.

**F3. Query arguments.** Options: (a) keep args, query = minted projection rel + demand
seed; (b) keep today's name+arity only. Recommendation: (a). Price: queries become
derived rels needing minted names and a place in analyze; (b) is free but cannot seed
column-level demand later and cannot project, so every consumer reads whole rows.

**F4. Demand granularity.** Options: (a) rel reachability; (b) column magic-sets (bound
columns prune body work, the full "module args = demand keys" form). Recommendation:
(a) in v1. Price: coarser laziness; a demanded rel computes all its columns' rules
even when the query binds some. (b) is the ruled direction (ruling :11-14, M1 :121-124)
but rewrites rule shapes and the checker; lands with rel/N nesting, not before.

**F5. Standing-query delivery.** Options: (a) push stream per query (subscribe
semantics); (b) keep pull-only `rows(rel)`. Recommendation: (a), with (b) kept for
`bop q`. Price: one new serve endpoint and its backpressure decision; (b) alone is
zero work but makes `?` a polite name for `curl`, which is the semantics the rulings
reject.

**F6. Late edge subscriber.** Options: (a) persisted rows then live tail (section 4);
(b) live-only from subscribe; (c) full-history replay independent of retention.
Recommendation: (a). Price: replay is bounded by retention spellings, so a late
importer of a `keep(count(N))` rel sees a window, not a history; (c) would require
keep-everything storage underneath every edge rel, priced as storage the rulings do not
want to spend by default.

**F7. `eager` spelling.** Options: (a) new registry word desugaring to a query;
(b) convention (queries in a marked file section). Recommendation: (a). Price: one
registry row + one expansion clause + golden-flex coverage line; (b) costs a
file-layout convention the parser must learn anyway and reads worse.

**F8. Query checking.** Options: (a) queries enter analyze (path resolution, arity,
column types checked up front); (b) stay unchecked (today). Recommendation: (a).
Price: query terms flow through the checker and gain refusals (`unresolvable_path`
among them); (b) keeps compile green for queries that name nothing, which is exactly
the silence this repo files as a defect.

---

## 7. Receipts index (what was actually opened)

- This worktree: `CONTRACT.md`, `CONTRACT-ADDENDUM.md` (the 2026-08-03 laziness
  rulings, ruling 3 corrected), `brief.md` (via the recon worktree root),
  `2026-08-03-module-catalog-ruling.md`, `v6/prolog/compile/registry.pl`,
  `v6/prolog/LANG.md`, `v6/prolog/1_expansion.pl`,
  `v6/dl/fixtures/golden-flex.dl6`, `v6/prolog/compile/parse_dl.pl` (:95-165, :340-352,
  :540-547, :975-990, :1400-1410), `v6/prolog/1_host_expand.pl` (:390-422),
  `v6/prolog/emit_ts.pl` (:300-320, :416-422, :595-601, :2000-2044),
  `v6/prolog/analyze.pl` (grep: no query terms), `v6/tsv2/serve/0_compile.ts` (:98-108),
  `v6/tsv2/serve/3_engine.ts` (:100-160), `v6/tsv2/serve/4_http.ts` (grep: idb routes),
  `v6/tsv2/serve/1_hosts.ts` (grep: demand), `v6/tsv2/runtime/tickLoop.ts` (grep),
  `v6/prolog/conformance/fixtures/2_hosts_wiring.pl` (:63-66).
- Cross-worktree (read-only): `/Users/chrishafley/projects/sprefa-recon-query/REPORT.md`
  (the full `?` pipeline trace and verdict, polled until present);
  `/Users/chrishafley/projects/sprefa-dots-land/v6/prolog/0_dot_expand.pl` (whole file)
  and its `1_expansion.pl:36` (phase 44);
  `/Users/chrishafley/projects/sprefa-plan-dotaccess/PLAN.md` (prior art + the
  forks-with-prices format this plan imitates);
  `/Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md` (step ladder + fact schema).
