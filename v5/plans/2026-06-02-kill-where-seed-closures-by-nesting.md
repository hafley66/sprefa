# Kill `where`; make nesting the one way to filter (incl. seeded closures)

Branch `codex/kill-where-seed-closures`. Goal: make the query `where` clause
unnecessary, then delete it. Today `where` is sugar in every case EXCEPT one the
task flagged: seeding a point query into a closure relation. This plan shows
that even that case is already subsumed by a **literal head term**, that the only
genuinely new capability requested — *reading a closure relation from a rule
body* — is an intentional, named eval-semantics change, and how to land it
soundly without re-introducing the Θ(V²) closure or per-tick Tarjan.

## What `where` actually does today (empirically measured, not assumed)

`grep where examples/*.dl` → three shapes, all currently routed through
`Query.wheres` + `lower_query`'s `WHERE` SQL OR `pinned_value`:

| shape | example | op | mechanism today |
|---|---|---|---|
| S1 closure seed | `? reaches(s,d) where s = "X".` | Eq | `pinned_value` → `run_reaches_point` (seeded BFS over `closure_cache`) |
| S2 plain Eq filter | `? def(n,p,l) where n = "X".` | Eq | `lower_query` adds `WHERE n_col = 'X'` |
| S3 regex/glob filter | `? def(n,p,l) where n =~ "::tick".` | Match/Glob | `lower_query` adds `WHERE n_col REGEXP/GLOB ...` |

Measured on `examples/glean.dl --root <repo>`:
- S1 `? reaches(s,d) where s="Engine::tick"` ⇒ 11 rows, seeded BFS path.
- Rewritten `? reaches("Engine::tick", d).` ⇒ **identical 11 rows**, SAME seeded
  BFS path. `pinned_value` already matches `Term::Str` in the head FIRST
  (engine.rs:186-187), so a literal head term seeds the closure with zero `where`.
- S2 `? def(n,p,l) where n="Engine::tick"` ⇒ rewritten `? def("Engine::tick",p,l).`
  ⇒ identical row (the pinned column drops from the header, which is correct: it
  is a constant).
- S3 `? def(n,p,l) where n =~ "::tick"` ⇒ rewritten as a nested rule
  `rel d_tick(...). d_tick(n,p,l) <- def(n,p,l), n =~ "::tick". ? d_tick(n,p,l).`
  ⇒ identical 4 rows. `lower_rule` already lowers a `Cmp` with `Match`/`Glob`
  in a rule body (engine.rs:2266-2271 + lower.rs:108-114).

Conclusion: **`where` carries no capability the rest of the language lacks.**
- Eq (S1, S2) → literal head term. Already wired in `pinned_value` and `lower_query`.
- Match/Glob (S3) → a constraint in a nested derived rule body. Already wired.

So removing `where` is a pure surface deletion + example migration. No closure
machinery change is *required* by the migration itself.

## The one new capability the task asks for (named eval-semantics change)

The task's headline failing example is NOT a query, it is a **rule body** reading
a closure relation:

```
reaches(a,b) <- closure(e).
from_tick(b) <- reaches(a, b), a = "Engine::tick".   # ERRORS today
```

`check_stratification` (engine.rs:202) bans a derived rule body from reading any
closure head, because the closure VIEW (`rel_reaches`) is backed by
`scc_node`/`scc_edge` tables that are only populated by `rebuild_closures`, which
runs AFTER `rebuild_derived`. A rule reading the view mid-fixpoint sees empty
tables.

This IS a real, intentional eval-semantics extension (per the repo rule: do not
pretend "zero new semantics"). Naming it: **seedable closure body-read** — a
derived rule whose body joins a closure head against a bound/literal key may read
that closure, evaluated as a seeded reachability slice, not a full materialization.

### Approach decision: (a) seed-pushdown, NOT (b) full materialization

Three candidates from the task:

- **(b) materialize the closure VIEW into its base table before such rules run.**
  Rejected: that is the Θ(V²) pair table the SCC condensation (`scc.rs`,
  `count_pairs`, `reaches_from`) was explicitly built to NEVER materialize. On
  the kernel this is billions of pairs. Violates the "respect the cross-tick SCC
  condensation cache, no per-tick full Tarjan/closure" constraint.

- **(a) detect the seedable body shape and push the bound key as a SEED into the
  condensation walk.** A rule `H(b) <- reaches(a,b), a = "X".` (or `a` bound by an
  upstream body atom to a single key, or the symmetric dst-pinned form) is exactly
  `run_reaches_point(seed="X", forward=true)` already used for queries. Generalize
  that seeded-BFS emit from "print query rows" to "INSERT into the head table".
  Preserves the SCC cache; one BFS per seed; no Tarjan, no Θ(V²). **CHOSEN.**

- (c) lift the ban only for the seedable shape and let SQL evaluate the view
  src-pinned. The recursive CTE with a `WHERE a = 'X'` pushdown is correct but
  re-walks the component graph in SQL every fixpoint iteration and ignores the
  in-memory `closure_cache`. Strictly worse than (a). Rejected.

### Seedable shape (the only shape the ban is lifted for)

A derived rule R is *closure-seedable* iff:
- exactly one body atom A references a closure head H (= key in `closures` map), and
- H is 2-ary, and
- one of H's two columns is **pinned to a single literal** in R's body — either a
  `Term::Str` directly in that atom position, or a `Term::Var v` with a body
  `Cmp { v = "lit", Eq }` (mirrors `pinned_value`), and
- the other H column is a `Term::Var` that flows to the head (free), and
- R has no OTHER positive body atom that must join against H's free column
  (i.e. H is a leaf source in R; if it joins further, fall back — see below).

If a rule reads a closure head but is NOT seedable (free/free, or H joined onward
to another relation, or H pinned on both columns), keep the error but with a
sharper message: "reading a closure relation in a rule body is only supported
when one endpoint is pinned to a literal (seeded reachability); got an unpinned
read of 'H'." This keeps the unsounded full-closure read OUT (it would need
materialization we refuse to do).

For the migration of `examples/*.dl` NONE of them need the body-read at all (all
are query-level), so the seedable-body feature is additive and independently
tested; the example migration does not depend on it landing.

---

## 1. Type signatures (new / changed)

### AST (`src/ast.rs`)

```rust
// REMOVE the wheres field from Query. Head terms (literals) carry all pinning.
pub struct Query { pub head: Atom }          // was: { head, wheres: Vec<Constraint> }
```

`Constraint`, `CmpOp` (incl. `Match`/`Glob`) stay — still used in rule bodies
(`BodyItem::Cmp`). `BodyItem::Closure` stays.

### Lexer (`src/lex.rs`)

No token for `where` exists (it lexes as `Tok::Ident("where")`). Nothing to
remove in the lexer. After removal, `where` is just an ordinary identifier again
(harmless; it is not a relation name anyone declares).

### Parser (`src/parse.rs`)

```rust
fn query(&mut self) -> Result<Query>;
// pseudo:
//   expect(Question); let head = self.atom()?;
//   if peek is Ident("where"): bail!("`where` was removed; filter by a literal
//       head term (`? rel(\"X\", y).`) or a nested rule with a body constraint
//       (`r(...) <- rel(...), col =~ \"...\".`). See plans/2026-06-02-...md");
//   expect(Dot); Ok(Query { head })
```

We KEEP an explicit error on `where` (not silent acceptance) so existing programs
fail loudly with the migration recipe, rather than parsing `where` as a bare
atom and producing a confusing downstream error.

### Lowering (`src/lower.rs`)

```rust
pub fn lower_query(q: &Query, rels: &Rels) -> Result<(String, Vec<String>)>;
// pseudo: identical to today MINUS the `for c in &q.wheres { ... }` loop
//   (lower.rs:164-168). Literal head terms already become WHERE filters in the
//   existing per-term match (lower.rs:159). Match/Glob never appear in a query
//   head term, so no query-side REGEXP/GLOB lowering is lost — those move to
//   rule bodies, which lower.rs already handles for Cmp.
```

### Engine (`src/engine.rs`)

```rust
// pinned_value: drop the `q.wheres` branch; a head Var can no longer be pinned
// by a where-constraint, only a head Str literal pins.
fn pinned_value(q: &Query, pos: usize) -> Option<String>;
// pseudo:
//   match &q.head.terms[pos] { Term::Str(s) => Some(s.clone()), _ => None }

// run_query: signature unchanged; it constructs no wheres.
fn run_query(&self, q: &Query, closures: &HashMap<String,String>) -> Result<()>;

// NEW: classify a derived rule as closure-seedable.
// Returns Some((edge, seed_literal, forward, head_out_var)) or None.
struct ClosureSeed<'a> { edge: &'a str, seed: String, forward: bool }
fn closure_seed_of(rule: &Rule, closures: &HashMap<String,String>) -> Option<ClosureSeed>;
// pseudo:
//   find the single body Pos atom A whose A.rel is a closure head; else None.
//   require A.terms.len()==2.
//   pin = literal in A.terms[i] (Term::Str) OR a body Cmp{ Term::Var==A.terms[i],
//         Eq, Term::Str(lit) }. Determine which endpoint (0 or 1) is pinned and
//         that the OTHER is a Term::Var. forward = (pinned position == 0).
//   require no other Pos atom in the body joins on the free var (H is a leaf):
//         the free var may appear ONLY in this atom and in the head.
//   return ClosureSeed { edge: closures[&A.rel], seed: lit, forward }.

// NEW: evaluate a seedable closure rule by seeded BFS into its head table.
fn eval_closure_seed_rule(&self, rule: &Rule, cs: &ClosureSeed) -> Result<()>;
// pseudo:
//   let cc = self.closure_cache.get(cs.edge).ok_or(...)?;   // already condensed
//   let walk = if cs.forward { reaches_from } else { reached_by }(&cc.cond, seed_id);
//   build head rows by binding: the closure atom's free var -> each walked name,
//     the pinned var -> seed (constant); project through rule.head.terms.
//   collect Vec<Vec<Value>>, ONE self.db.insert_rows(tbl(head), cols, rows).  // no N+1
//   (head may have >2 cols only if it re-emits constants/seed; support the common
//    1- or 2-col projection; bail with a clear message otherwise.)

// check_stratification: now ALLOWS a closure body-read iff closure_seed_of is Some;
// still bans the unpinned/full read.
fn check_stratification(derived_rules: &[&Rule], closures: &HashMap<String,String>)
    -> Result<()>;
// pseudo: for each derived rule, for each body atom on a closure head:
//   if closure_seed_of(rule, closures).is_some() { continue; }
//   else bail!("...only supported when one endpoint is pinned to a literal...").
```

### tick / tick_paths wiring (`src/engine.rs`)

The seeded closure rules must run in the QUERY phase ordering sense — AFTER
`refresh_cond_cache` has the condensation ready — but they WRITE a base table
other derived rules might read. Decision: **split derived rules into two tiers.**

- Tier-0 derived rules: today's `derived_rules` MINUS closure-seedable ones.
  Evaluated by `rebuild_derived` exactly as today.
- closure-seed rules: evaluated by `eval_closure_seed_rule` AFTER
  `rebuild_closures` + `refresh_cond_cache`, BEFORE `run_query`.

Constraint to keep stratification honest: a closure-seed rule's head MUST NOT be
read by any Tier-0 rule (else Tier-0 would see it empty). Enforce: if a
closure-seed rule's head appears in another derived rule's body, bail with
"a relation seeded from a closure cannot feed another derived rule in the same
tick; query it directly". This keeps a clean two-stratum order without a general
fixpoint over closure outputs (which would re-open the materialization question).

Place the new eval loop in both `tick` (line ~687, after `refresh_cond_cache`,
before the `run_query` loop) and `tick_paths` (line ~687 analog).

---

## 2. Instance lifetimes (types holding state)

- `Engine` (engine.rs:401): unchanged shape. `closure_cache: HashMap<String,
  ClosureCache>` persists ACROSS ticks (the perf cache from 4b5056b). The seeded
  body-read READS this cache; it never rebuilds it. `recondensed` counter
  untouched (seeded reads do not recondense).
- `ClosureCache { cond, names, id, digest }`: per-edge, lives on the Engine across
  ticks; `refresh_cond_cache` is the only writer. `eval_closure_seed_rule` is a
  pure reader.
- `ClosureSeed`: transient, per-rule, lives only within one tick's eval loop.
- `Query`: now holds only `head: Atom`. No owned constraints.

## 3. Storage layout

No schema change. Closure-seed rule heads are ordinary declared tables
(`rel_<head>`), created by `declare` in `declare_all` (they are NOT closure heads
themselves, so they go through the `None => self.declare(d)` arm). They are
wiped + repopulated each tick by `eval_closure_seed_rule` (DELETE then
`insert_rows`), same lifecycle as `rebuild_derived` tables.

`_strings`/`_where_bytes`/`scc_node`/`scc_edge` unchanged.

## 4. Read/write sequence + uniqueness

Per tick (in `tick`, mirrored in `tick_paths`):
1. `declare_all` — closure heads → VIEW; seed-rule heads → TABLE.
2. `check_stratification` — accepts seedable closure body-reads, rejects unpinned.
3. `rebuild_derived(tier0)` — fixpoint over non-seed derived rules (unchanged).
4. `rebuild_closures(edges)` — condense edges into scc tables (unchanged).
5. `refresh_cond_cache(edges, dirty)` — fill `closure_cache` (unchanged).
6. **NEW**: for each closure-seed rule, `eval_closure_seed_rule` → DELETE head,
   seeded BFS, single `insert_rows`. Uniqueness: head table PK / `INSERT OR
   IGNORE` dedupes; the BFS `reaches_from`/`reached_by` already returns each
   reached node once.
7. `run_query` over all `? ...` items (unchanged; literal heads seed closures).

Uniqueness conditions:
- A seed rule's head must be a freshly-declared rel (not a builtin/closure head).
- Each (edge) is condensed at most once per tick (digest skip in step 5).
- Seeded BFS is O(reachable component subgraph), never O(V²).

---

## Migration of `examples/*.dl` and tests

All `where` query usages → rewrite in place:

| file:line | from | to |
|---|---|---|
| callgraph.dl:31-32 | `? calls(c,e) where c="lex".` / `? reaches(s,d) where s="lex".` | `? calls("lex", e).` / `? reaches("lex", d).` |
| callgraph-ast.dl:33 | `? reaches(s,d) where s="run".` | `? reaches("run", d).` |
| callgraph-sg.dl:46 | same | `? reaches("run", d).` |
| callgraph-c.dl:34-36 | `where caller/src/dst = "panic"` | `? calls("panic",e). ? reaches("panic",d). ? reaches(s,"panic").` |
| callgraph-resolved.dl:32-33 | `where caller="tick"`,`where src="run"` | literal heads |
| callgraph-typed.dl:55-56 | `where caller=...`,`where src=...` | literal heads |
| module-history.dl:26-27 | `where rev="WORK"`, `where src="v5/src/engine.rs"` | `? module_edge_rev(s,d,"WORK").` ; `? work_reaches("v5/src/engine.rs", d).` |
| typegraph.dl:30-33 | `where from=`,`where src=` (incl 4-ary `type_edge_rev where from=`) | literal heads in the right position |
| glean.dl:66-69 | Eq seeds | literal heads |
| glean.dl:74-76 | `=~`/`~~` filters | nested rules `r(...) <- def(...), col =~/~~ "...". ? r(...).` |

`type_edge_rev(from,to,kind,rev) where from="Engine"` → `? type_edge_rev("Engine", to, kind, rev).`
(4-ary; literal in pos 0).

Tests: `grep where tests/*.rs` shows ZERO query-`where` usages (only `_where_bytes`
SQL + identifiers). No test migration needed; add NEW tests instead.

## New tests

`tests/where_removed.rs` (new file):
1. `closure_seed_via_literal_head_matches`: build a small call graph fixture, run
   `? reaches("a", d).` and assert the exact reachable set (proves S1 literal-head
   seed == old `where` seed; same `run_reaches_point` path).
2. `regex_filter_via_nested_rule`: `r(n) <- def(n,...), n =~ "...". ? r(n).`
   assert filtered set (proves S3 nesting).
3. `where_keyword_now_errors`: feed `? reaches(s,d) where s="a".`; assert parse
   error mentioning the migration recipe.
4. `closure_body_read_seedable_ok`: `from_a(b) <- reaches(a,b), a="a". ? from_a(b).`
   assert it equals the seeded reachable set (proves the new body-read capability).
5. `closure_body_read_unpinned_errors`: `all(a,b) <- reaches(a,b). ? all(a,b).`
   assert the sharpened "pin one endpoint" error (proves we did NOT silently
   re-introduce full materialization).

Plus: all migrated `examples/*.dl` must still `cargo run` to the same row counts
(spot-checked via the existing example-driven flows; glean.dl verified by hand in
this session).

## Risk / stop conditions

- If `eval_closure_seed_rule`'s projection cannot cover a head shape cleanly
  (e.g. head re-emits the seed in a 3rd column), bail loudly rather than guess —
  the seedable set stays a strict, documented subset. The example migration does
  not depend on body-reads, so even if the body-read tier is descoped, `where`
  removal still lands (queries + nested rules cover every existing example).
- Keep `cargo test --release` green at each commit. RA oracle stays `#[ignore]`.
