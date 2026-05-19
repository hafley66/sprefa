# Cons / calling unification — one primitive for list + kwarg; root = implicit `{}`

Status: BUILDABLE 2026-05-19 — 3 forks decided + ROUTE A grammar-
validated GO (zero regen, zero new conflict). 6-step build order in
`## Build order`. `## DECIDED v2` = the locked decisions;
`## Consolidated feedback` = the audit trail. Not coded. Promotes
the frozen design in `chat_log/20260516.1.sprf-block-cons-container-op-application-design.md`
to an actionable plan. Sibling: `plans/2026-05-18-callable-value.md`
(callable-Value = `apply` over a callable; this generalizes the *args*
side of that same `apply`).

## Three requirements (user, 2026-05-19)

- **R1 — calling figured out.** One call convention: an op/rule is
  applied to a single cons-list (ordered cells, each positional or
  keyed) plus a container flag. `apply` (callable-value) takes this.
- **R2 — list + kwarg = ONE primitive.** A positional list and a kwarg
  bag are the same structure: a list of cons cells. Bare cell =
  positional; `k: v` cell = keyed. No separate "args vs kwargs".
- **R3 — top-level sprf = implicit unlabeled `{}` body.** The `.sprf`
  file root IS the `{}` merge container, with NO labels/keys at the
  top level (every top-level cell is an unlabeled arm).

## DECIDED v2 (2026-05-19, locked — supersedes Open Questions)

**D-R3 — root = spelled `{}`, TOTAL SOURCE ORDER, fanout-only.**
Root is modeled/spelled `{}` and fanouts the (empty) seed to every
top-level arm, but execution + lower-time rule binding keep STRICT
source order. `{}` means "fanout the seed", NOT "unordered". Empty
root = unit `1` (clean no-op), not `{}`'s `0`. Implementation =
pure lower-time relabel: grammar FROZEN, `_stmt := pipe ';'` and
`source_file := repeat(_stmt)` UNCHANGED, `;` stays a mandatory
terminator at the grammar level; the "implicit `{}`" is a
walk/lower model tag only. Zero parse change ⇒ zero-regression on
all 36 examples by construction (no corpus parse-diff needed).

**D-TY — value-space types.** The cons cell carries
`ty: Option<Value>` (a value-space type per the 2026-05-19
types-in-value-space ruling), NOT `Arc<str>`. `col_types` /
`resolve_dot` widen toward `Value`. This honors the latest ruling;
the `()`=product/struct framing in `## Relation to other tracks`
is reconciled to value-space (a struct-type IS a Value whose dots
are its fields), NOT `DotTable.ty`-as-name.

**D-LIT — standalone `()`-cons-list literal IS in scope.** A
cons-list may appear as a first-class value at pipe-step position
(not only glued after an op name), so `apply` can take a literal
`ConsList`. This is the one real grammar item; it LR-conflicts
with `parenthesized` and needs an explicit resolution (designed in
`## Grammar: standalone cons-list`, validated separately). Regen
risk accepted by the user.

**D-Q1 — Python rule.** All positional cells strictly before any
keyed cell; keep the existing `lower/positional-after-kwarg` error
verbatim. Zero-regression (it is already an error today).

**D-Q3seq — `?`-surface fix is STEP 0.** Before any Cons typing:
`x?: t.i64` must survive lexing. Today `split_keyword_arg` sets
`key="x?"`, `is_ident("x?")` is false ⇒ the whole cell becomes one
positional slot and `?` is swallowed (walk.rs:557). The decl-mark
is recovered by the classifier BEFORE the keyword-colon split and
emitted as a reserved `decl` cell inside the cell's value-cons-list
(NOT a struct field — see the corrected `Cons` below). `Key::Index`
cells are ineligible for typed columns (no name to key `col_types`).
`Cons`:

```rust
// CORRECTION 2026-05-19 (user): NO `decl`/`ty` struct fields — that
// betrays "cons is bottom". Pure cell only; cons-of-cons via a
// ConsList-valued value. decl/ty are RESERVED-NAME CELLS, not fields.
pub enum Key { Name(Arc<str>), Index(u32) }
pub struct Cons { pub key: Option<Key>, pub value: Value } // ONLY shape
pub enum Container { Ordered /* () */, Merge /* {} */ }
pub struct ConsList { pub cells: Vec<Cons>, pub flag: Container }
// ValueKind gains a ConsList variant (a cons-list IS a value) so a
// cell's value can itself be a cons-list ⇒ recursion bottoms at
// Atom/Pipe/Callable.

// A declaration column is a cell whose VALUE is a sub-cons-list:
//   x?         Cons{ Some("x"), ConsList[ {"decl", true} ] }
//   x?: t.i64  Cons{ Some("x"), ConsList[ {"decl", true}, {"ty", t.i64} ] }
// `decl` is read at CLASSIFY time (binding vs read); `ty` is a
// value-space value read at lower. Both addressed by the SAME
// resolve_dot walk — this IS the existing DotTable:
//   DotTable.map  (value.rs:26) = a named cons-list, already
//   DotTable.ty                  = the `ty` cell, already
//   resolve_dot   (ctx.rs:272)   = the cons-of-cons walker, already
// Typed columns therefore FOLD INTO dots/resolve_dot, not beside it.
```

**D-Q3owner — owner canonicalization.** Keyed and positional
spellings of the same call must canonicalize to ONE positional
ordering BEFORE `RuleInvokeAssign`/`cache_key` (rule.rs:172-237),
so `f(1,2)` and `f(b:2,a:1)` collapse to one memo-owner identity
(else double-MATERIALIZE, runtime agent Q3 caveat).

## Grammar: standalone cons-list (the D-LIT item)

`_pipe_step` is `op_invocation | parenthesized | dsl_body`;
`parenthesized := seq('(', $.pipe, ')')`. A bare `(x?, y?)` value
at pipe-step collides with `parenthesized` on the leading `(`, and
`conflicts:` does not cover it ⇒ `tree-sitter generate` fails.

Two routes; the plan ships ROUTE A, keeps ROUTE B as the escape:

- **ROUTE A (recommended, conflict-free): the `()`/`{}` value
  literal is an OP, not a bare-paren production.** The frozen
  design already says `cons` is the bottom op and "everything is an
  op". So a standalone cons-list value is written `cons( … )`
  (and the `()`/`{}` sugar desugars to `cons`/`merge` ops in the
  walker). This reuses the EXISTING `op_invocation` + glued
  `paren_slot` path — zero new production, zero LR conflict, zero
  regen. `apply` takes the resulting `ConsList` value. This still
  satisfies D-LIT (a first-class cons-list value at value
  position) without the bare-paren ambiguity.
- **ROUTE B (only if bare `(x?, y?)` with NO opener is required):**
  add a `cons_list` rule and a `conflicts: [[$.parenthesized,
  $.cons_list]]` entry (GLR) OR an external scanner that peeks
  balanced-paren content for a top-level `,` vs `>`. Real
  tree-sitter regen + conflict-resolution work; higher risk.

Decision: ship ROUTE A; ROUTE B is a follow-up only if a literal
bare-paren value is later demanded.

**VALIDATED 2026-05-19 (grammar agent): ROUTE A = GO.** `cons`/`merge`
are ordinary `op_name` (grammar.js:166, no reserved table); `cons(…)`
is a normal `op_invocation` via glued `token.immediate('(')`
`paren_slot` (grammar.js:100), item-set-disjoint from plain `'('`
`parenthesized` (grammar.js:78) ⇒ ZERO new `conflicts:` entry, ZERO
regen. The cell list already exists: `split_paren_args` (parse.rs:322)
+ `classify_call_arg`/`split_keyword_arg` (walk.rs:538-585) →
`CallArg{keyword,value}`. Desugar = a classify/dispatch-time branch
(NOT a source rewrite, which would lose spans): a
`match lower_name { "cons"|"merge" => … }` arm inserted just before
the generic `reg.lower_call_at` at walk.rs:498-499, building
`ConsList{cells,flag}` from the already-classified args; `{}` reaches
it via the existing `op.block` path, `()` via `paren_slot`. ROUTE B's
only delta is opener-less bare-paren *spelling* — capability-equal,
ergonomics-only. STEP 0 (`?`-decl-mark, walk.rs:557) is the one real
prerequisite and is a classify-layer fix orthogonal to grammar.

## Build order (each step compiles + gate green; worktree-isolated)

0. **STEP 0 — `?`-decl-mark survives lexing.** `classify_call_arg`/
   `split_keyword_arg` (walk.rs:538-585): recover a trailing `?` on
   the key BEFORE `is_ident`/colon-split and emit it as a reserved
   `decl` cell in the cell's value-cons-list (no struct field). RED
   test: `rule(:P, x?: t.i64)` yields a cell whose value-list has
   `decl`+`ty`, not a swallowed slot. No grammar edit.
1. **`Cons` type.** value.rs: `Key`, `Cons{key,value}` (pure cell,
   no decl/ty fields), `Container`, `ConsList`, and a `ValueKind`
   `ConsList` variant so a value can be a cons-list (cons-of-cons).
   `CallArg` = `type CallArg = Cons` alias; mechanical fanout (35
   refs/7 files, compiler-enforced), then delete the alias.
2. **D-Q1 binder.** `normalize_call_args` (registry.rs:318-377):
   `match arg.keyword` → `match cons.key`; keep
   `lower/positional-after-kwarg` error verbatim.
3. **D-R3 root relabel.** Lower/walk: model root as
   `Container::Merge` (fanout the seed) WITHOUT reordering; grammar
   FROZEN, `;` stays a terminator. Empty root = unit `1`. Verify the
   36 examples + recursion/retraction tests byte-stable (they must
   be — zero parse change).
4. **ROUTE A `cons`/`merge` ops + apply(ConsList).** The walk.rs:498
   branch → `ConsList` value; generalize callable-value
   `apply(…Vec<Value>)` → `apply(…ConsList)` with D-Q3owner
   canonicalization (one positional order before
   `RuleInvokeAssign`/`cache_key`, rule.rs:172-237).
5. **D-TY value-space `ty`.** Wire the `ty` cell (in a cell's
   value-cons-list) into `set_col_type`/`resolve_dot` per the
   value-space ruling; replace reify's `# reify types:` comment seam
   with real typed cols.
6. **`{` IS the op (the bash-`[[` rule).** Bare `{ … }` at pipe-step
   lowers as the merge op — the `{` token itself is the op name
   (NO invented `merge` keyword). Grammar: add `brace_block` to
   `_pipe_step` alts (mirror of `brace_slot`, plain `{`). No
   `conflicts:` entry needed — there is no brace-rival rule at
   pipe-step (validated by the grammar agent: `{`-collision-free).
   Walker desugar: synthesize a merge-op invocation whose cells are
   the already-classified slot cells ⇒ `ConsList{flag:Merge}` value.
   `( … )` analog is DEFERRED (parenthesized conflict, real grammar
   work); for now, bare seq is `cons( … )`.
7. **`&` is the current cursor (the input sigil).** Reserved name
   `&` = a Value view of the upstream cursor at the current step.
   Pre-bound by the walker at every step boundary. `&.value`,
   `&.at`, `&.terms.X` resolve through the EXISTING `resolve_dot`
   (ctx.rs:272) — no new dot machinery; the cursor is wrapped as a
   Value whose `DotTable.map` exposes its fields. DSL: `${&.value}`
   classified by the existing dsl-interp branch; whole-cursor
   reference is `${&}`. RED test: `` `lol` > { x: str`${&.value}.lol2` } ``
   yields `{x: "lol.lol2"}`.

Dependency: step 4 generalizes callable-value's `apply`; land/merge
`feat/callable-value` (c79c47f8, GREEN) BEFORE step 4, or rebase cons
work on it.

## Grounding (real code today)

- `CallArg { keyword: Option<Arc<str>>, value: Value }` (value.rs:50)
  is ALREADY a proto-cons cell: `keyword=None` ⇒ positional,
  `keyword=Some` ⇒ keyed. R2 is partly done at the value layer.
- `walk.rs`: `classify_arg` / `split_keyword_arg` /
  `find_top_level_keyword_colon` / `classify_slot` = the existing
  per-comma-segment cons classifier.
- `registry.rs::normalize_call_args` = the existing positional-fill +
  kwarg-by-name + variadic binder; `positional-after-kwarg` is an error
  today.
- Grammar (`grammar.js`): `_slot_body`/`paren_slot`/`brace_slot` are
  ALREADY one opaque-token production for `()` and `{}`. BUT root is
  `source_file := repeat(_stmt)`, `_stmt := pipe ';'` — there is **no**
  root container; `;` is the only top-level separator. `()` is
  overloaded: `parenthesized` (sub-pipe grouping `(a > b)`) AND
  `paren_slot` (op args).

## Layer 1 — type signatures

```rust
// value.rs — promote CallArg to the cons primitive
pub enum Key { Name(Arc<str>), Index(u32) }
pub struct Cons { pub key: Option<Key>, pub value: Value } // None=bare→index by pos
pub enum Container { Ordered /* () */, Merge /* {} */ }
pub struct ConsList { pub cells: Vec<Cons>, pub flag: Container }

// the ONE call convention (generalizes callable-value apply)
fn apply(ctx:&LowerCtx, reg:&Registry, c:&CallableRef, args: ConsList)
    -> Result<Value, LowerError>;

// CallArg => Cons (type alias during migration, then delete CallArg)
```

## Layer 2 — pseudo

```
classify_slot          : unchanged (still per-comma-segment)
split_keyword_arg seg  : k:v  -> Cons{ key:Some(Name(k)), value }
bare seg               :       -> Cons{ key:None, value }   // index = position
normalize_call_args    : same logic, retyped to ConsList; the
                         positional-after-kwarg rule is now a POLICY
                         decision (Q1), not a grammar fact.

# root
source_file := implicit_merge_body
implicit_merge_body := repeat(_stmt)            # tag Container::Merge
   assert: every top-level cell is UNLABELED (no Key::Name at root)  (R3)
   `;` retained as the cell separator inside the implicit {}  (migration)

# () disambiguation (Q4)
(a > b > c)  = parenthesized sub-pipe   (contains `>`)
(x?, y?)     = Ordered cons-list        (comma cells, no top-level `>`)
   decided positionally on first top-level token class, never lexically
```

## Layer 3 — instance lifetimes

| type | lifetime |
|---|---|
| `Cons` / `ConsList` | one lower call; built by the walker per op_invocation |
| `Container` flag | set at the opening bracket token; immutable |
| root `Merge` body | the whole compile; one per `.sprf` file |

## Layer 4 — storage / sequence / uniqueness

- positional cell index = its position among `key:None` cells.
- keyed cell uniqueness = `Key::Name` unique within one `ConsList`
  (existing `lower/duplicate-arg`).
- root `{}` = fanout: every top-level arm receives the same (empty)
  seed; arm order = source order; NO label dispatch at root (R3).
- `;` = cell boundary in the implicit root `{}` (NOT a terminator
  anymore); EOF closes the implicit body.

## Open questions = the attack surface for feedback

- **Q1 calling** — under one cons-list, is `positional-after-kwarg`
  still an error, or legal (Python-style only-positional-before-kw vs
  free order)? What does `normalize` do with `f(1, x: 2, 3)`?
- **Q2 root-as-{}** — `{}` = merge/fanout/unordered per frozen design,
  but recursion (main 37bb93a5) + retraction owner-scoping + the
  `;`-ordered statement model may depend on top-level eval order. Does
  relabeling root to a merge container change eval/owner semantics or
  break the recursion fixpoint? Blast radius on 36 `v4/examples/*.sprf`
  + the test corpus.
- **Q3 the `?` decl-mark** — in `x?: t.i64`, where does `?` live: on
  the `Cons`, the `Key`, or the `Value`? How does it compose with
  callable-value typed columns and `set_col_type`?
- **Q4 `()` collision** — `parenthesized` sub-pipe vs `Ordered`
  cons-list share `(`. Is the positional disambiguation
  (`>` ⇒ sub-pipe, `,`-cells ⇒ cons-list) sound and tree-sitter-able
  without a grammar fork?
- **Q5 migration** — `;` semantics change (terminator → cell sep).
  Every shipped example/test uses `;`. Is a no-op-for-existing-files
  guarantee achievable, and how is it verified?

## Relation to other tracks

- callable-value (`plans/2026-05-18-callable-value.md`): this is the
  *args* side of the same `apply`; land callable-value first, then
  generalize its `Vec<Value>` arg to `ConsList`.
- types-in-value-space + reify `# reify types:` comment: under cons,
  `rule(:P, x?: t.i64)` is just `cons(:x, t.i64)`+decl-mark; the
  separate "typed-col grammar fix" dissolves into the cons classifier.
- dots/types/nesting (rustling-questing-falcon): `()`=product/struct
  is the type model this assumes.

## Consolidated feedback (2026-05-19, 3 agents: grammar / runtime / type-model)

Strong convergence. The three lenses independently land on the same faults.

| Item | Sev | Lens consensus | Evidence |
|---|---|---|---|
| **R3 root = *unordered* `{}` merge** | FATAL ×3 | source order is load-bearing | exec index-ordered app.rs:1860; recursion clears+recomputes `{name}_facts` before dependents app.rs:1730-1869 (37bb93a5); decl-before-use rule binding ctx.rs:239-265 + walk.rs:204; `;` is a grammar terminator grammar.js:57-60; "no-op" claim false |
| **Q3 `x?: t.i64` decl-mark** | FATAL/BLOCKED | does not lex as a kwarg OR a typed col today | `split_keyword_arg` key=`"x?"`, `is_ident("x?")==false` ⇒ None ⇒ whole thing → ONE positional slot, `?` swallowed (walk.rs:557). callable-value plan H8 was also wrong; corrected. |
| **Q4 `()` collision** | non-issue ×3 | the collision does not exist | `paren_slot`=`token.immediate('(')` glued to op name vs `parenthesized`=free `(` at pipe-step — disjoint by gluing, grammar.js:78/100. Proposed `>`-vs-`,` rule would REGRESS into content-lookahead (frozen invariant 2). DROP Q4. |
| **Real grammar hole (H2/H5)** | FATAL | standalone `()` cons-list VALUE has no production | `_pipe_step`=op\|parenthesized\|dsl only; adding a `(`-initial alt LR-conflicts with `parenthesized`; `conflicts:[]` doesn't cover it ⇒ `tree-sitter generate` fails. Needed for R2 `apply(ConsList)` literal. |
| **Q1 positional-after-kwarg** | design | keep Python rule + existing error = zero-regression | real binder constraint not grammar; `f(1,x:2,3)` has no defined slot for `3` (registry.rs:318-377) |
| **Q3 memo-owner canonicalization** | design | keyed vs positional spelling of one call must canonicalize to ONE owner | else double-MATERIALIZE (not double-fire); rule.rs:172-237 `arg_keys`/`cache_key` |
| **D3/Q5 type-model 3-way conflict** | design/BLOCKED | plan's `()`=product/struct is in NEITHER sibling | contradicts 2026-05-19 types-in-value-space ruling (ty-as-name was wrong, types are VALUES); rustling-questing-falcon still carries `DotTable.ty:Arc<str>`. Ruling says "Ask before reviving". |

Blast radius (measured): `CallArg` 35 refs / 7 files (rename mechanical, compiler-enforced); `ArgKind`/`ArgSig`/`validate_call`/`normalize_call_args` 158 refs (semantic, not mechanical); 36 `v4/examples/*.sprf` ALL `;`-terminated; 56 `v4/tests/`, 4 directly at-risk (`dots_chained`, `dots_nested_rules`, `recursion_fixpoint`, `intra_row_self_eq`). Recursion FIXPOINT itself is order-independent (set-iterated, structural SCC guard) — only cross-statement sequencing + lower-time rule visibility break under unordered root.

### Buildable subset (what survives the feedback)
- **R1/R2 cons unification: GO**, with explicit policy decisions (Q1 Python-rule, Q3 owner canonicalization). `CallArg`→`Cons` rename is mechanical.
- **R3: only as a lower-time RELABEL** — root may be SPELLED `{}` and FANOUT the seed, but stays a TOTAL SOURCE ORDER for execution + rule binding. `{}` ≠ "unordered". Empty root = unit `1`, not `{}`'s `0` (carve the exception or root is `()`-flavored when empty). Grammar FROZEN; zero grammar edit; cons-classify stays in the walker over opaque `_slot_body` (already true).
- **DROP Q4** (collision doesn't exist). If a standalone `()` cons-list value literal is wanted for `apply`, that is a separate, real grammar item (new production + `conflicts:` entry / scanner) — not in the buildable subset.
- **Q3 surface fix FIRST**: make the `?` decl-mark survive lexing before any Cons typing. (SUPERSEDED by the corrected pure `Cons{key,value}` + cons-of-cons model in `## DECIDED v2` — `decl`/`ty` are reserved CELLS, not struct fields; `Key::Index` still ineligible for typed cols; `ty` is value-space.)
- **Type model**: reconcile with the 2026-05-19 types-in-value-space ruling (adopt value-space types) OR get explicit user sign-off to supersede it.
