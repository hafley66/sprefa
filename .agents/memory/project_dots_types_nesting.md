---
name: project-dots-types-nesting
description: "sprf dots/types/nested-rules/cond initiative — C-spine + 3 amendments, locked design rulings, plan + worktree paths"
metadata: 
  node_type: memory
  type: project
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

Active initiative (started 2026-05-18): make sprf able to import foreign
language type systems and address/compose them. Design converged via 3
swarm rounds onto the **C spine** (type = a rule whose columns are
fields; variant = name-overloads; dot-access = lower-time projection;
negation = existing antijoin) plus 3 amendments:
1. parametric per-type projection: ONE `_proj_<Ty>(OWNER?,FIELD?)` rule
   per type, NOT one rule per field (avoids O(types*fields) blowup).
2. negation/else = stratified antijoin via existing `FactRead`
   `JoinKind::Anti` (fact.rs); NO new `|` operator.
3. dotted-op disambiguation = positional (step-head+registry => op;
   else projection); NOT lexical longest-match.

Locked rulings (user, 2026-05-18):
- nested rule = **closure** (inner captures enclosing bound terms;
  `Rule.captured` snapshot of ctx.bindings).
- Value metaclass on clone = **copy-on-write** (Arc::make_mut;
  independent after clone).
- same-scope re-`?` decl = **compile error**; shadow only across nested
  scope.
- `atom_literal` keeps **dotted** (`:a.b.c` = one atom); only `op_name`
  loses greedy dots.
- bare ident = term-ref or call only (no bare rule references);
  all-caps = lint convention, zero semantics; unresolved bare ref =
  compile throw.

Build env (this machine, 2026-05-18): `.cargo/config.toml` forces
`rustflags=["-Z","threads=8"]` (nightly-only) but active toolchain is
`stable-x86_64` and the only nightly is wrong-arch
(`nightly-aarch64`). Plain `cargo build` fails with "failed to run
rustc to learn about target-specific information" (the `-Z` breaks the
stable target-probe). **Unlock: prefix every cargo invocation with
`RUSTC_BOOTSTRAP=1`** (lets stable accept `-Z`). Foreground cargo is
sandbox-blocked from rustc; run cargo with `dangerouslyDisableSandbox`
and/or `run_in_background`. Do NOT edit the repo `.cargo/config.toml`
to fix this (would leak into the diff).

Plan: `/Users/chrishafley/.claude/plans/rustling-questing-falcon.md`
(6 build steps + worktree merge; tasks 1-7).
Worktree: `/Users/chrishafley/projects/sprefa-dots`, branch
`feat/dots-types-nesting`, based on main `e47b0e95`.

Progress: Task 2 (Value = {kind: ValueKind, dots: Arc<DotTable>} CoW)
DONE 2026-05-18, gate 449/0 = baseline parity, diff audited clean,
NOT committed/merged. Pre-existing non-pass: example
`dogfood-rust-doc-target.rs` E0601 (no main; it's a rustdoc fixture
for Task 6) — fails on base too, gate scope = `--lib --tests`.
Task 3 DONE 2026-05-18, gate 449/0: ctx.rs scope_path/enter_scope/
exit_scope + path-keyed register_rule/get_rule (outward walk, bare-
lookup-compatible at top level); walk.rs wraps rule block recursion
in enter/exit; rule.rs Rule.captured + with_captured + seed_for
overlay (captured ABOVE ambient, BELOW explicit args) + cache_key/
arg_keys fold captured. Closure ENGINE only; the `outer.inner`
value SURFACE (capture-population via dot projection) is Task 5.
Task 4 model LOCKED (user, "bare words doing atom duty is the wart;
that's what :sym is for"): bare ident = NAME/term (scope-resolve or
THROW UnresolvedRef); remove classify_slot bare->Value::atom fallback
(walk.rs:708/728); `:sym` = only atom; `ident?` ANY case = decl
(drop is_caps_ident gates incl binding_graph.rs:92), same-scope
re-decl = DupDecl, nested = shadow; all-caps = lint only. Scope
sources already in binding_graph `bound`: ?-decls, rule cols,
re/split named captures, term_bind. Mirror sites: walk::classify_slot
(walk.rs:634) + binding_graph::analyze_pipe (:108). Folds Task 1
(dotted op-name head: first seg a bound term => projection else op).
Migrate v4/examples/*.sprf (30) + tests (small ~5 bare-as-atom spots).
Task 4 = highest blast radius; design in plan §Build-order 4.
Task 4a (the FLIP) DONE 2026-05-18, gate 449/0 = baseline parity,
ZERO migration needed (corpus already written :sym/CAPS-term/
registered-op style; bare lowercase only ever appeared as op names,
which the new `reg.get().is_none() && !rule_decls` gate keeps as
ops). Changed: walk.rs classify_slot (any-case term, killed
bare->Value::atom fallback, ${} gates), walk_op lone-step gate
(is_ident && not-op && not-rule), removed is_caps_ident; mirrored
binding_graph slot_terms (! = read now, lockstep), collect_term_refs
(+rule_decls param), collect_rule_decls (any-case cols),
is_literal_slot (bare ident != literal), scan_dollar_idents/
host_binder_name/host_reader_name any-case, removed is_caps_ident.
Task 4b (DupDecl) DONE 2026-05-18, gate 449/0. User ruling: "its
just a redeclare fuck it yes make it that" => redundant-redeclare
only. Impl: collect_term_refs returns (reads,binds,DECLS);
analyze_pipe carries `declared: HashMap<name,read_since>` per pipe
frame (HashSet `bound` is already per-pipe in analyze_program, NOT
accumulated — earlier worry was wrong), cloned for {block}. A `?`
decl of a name still `false` (unread since last decl) => Diag
`lang/dup-decl`; a read flips it true so a later re-`?` is legal.
decls fed ONLY from plain-step `?` slots + lone predicate `X?`
step; gated by `track_decls = !predicate && !apply &&
!rule_decls.contains(name)` so relational self-join
`hits?(FS?,..) > hits?(FS?,..)` (join binder, NOT redeclare) and
rule-query args don't false-fire (that was the only 4b regression,
2 fuser_kinds_target tests, fixed). Task 4 (flip+DupDecl) COMPLETE,
diff confined to walk.rs+binding_graph.rs (162/86), not committed.
4c de-greed dot: folded into Task 5 (done there).

Task 5 CORE DONE 2026-05-18, gate 451/0 (449 baseline + 2 new RED).
- ctx.rs: `LowerError::DotMiss{ty,key}` (+Display, +registry.rs
  map_err arm => Diag `lang/dot-miss`); `LowerCtx::resolve_dot(v,key)`
  3-step: (1) v.dots.map instance dot, (2) v.dots.ty => columns via
  `store.declared_cols(ty)` (NOT get_rule/sink_cols — decl-only
  `rule(:Ty,f?..)` only `store.declare`s, never register_rule; this
  also covers bodied rules), (3) DotMiss loud.
- walk.rs classify_slot: de-greed dot (4c). Dotted arg-slot `head.seg`
  where head is_ident && store.declared_cols(head) non-empty =>
  TYPE projection: seed term-read(head) .typed(head), fold
  resolve_dot per seg; Err => push `lang/dot-miss` Diag + None;
  non-type head falls through to existing op/inline-pipe (unchanged,
  zero corpus regression — nothing used a dotted type head before).
- RED test v4/tests/dots_nested_rules_target.rs: 2 tests
  (type_field_projection_resolves, unknown_field_is_loud_dot_miss).
DEVIATION from plan: used DIRECT column term-read projection, NOT
the synthesized `_proj_<Ty>(OWNER?,FIELD?)` rule. Amendment-1's
only purpose was avoiding O(types*fields) rule blowup; direct proj
has zero blowup too and is simpler. Equivalent for type-addressing.
Task 5 REMAINING (extensions, not core): (a) `_proj_<Ty>`
synthesized-rule form if projection must be a first-class reusable
rule (autodoc antijoin north-star); (b) `${X.field}` host-interp
routing through resolve_dot (glob/re still REJECT it, ops.rs
709/1388); (c) chained nested-type `x.a.b.c` needs field->type
metadata (Rule/schema carries only col NAMES) — single-level +
miss is what's tested. Not committed. Task 5 CHECKPOINTED by user 2026-05-18 (3 rocks
deferred: ${X.field} body routing, _proj_<Ty> reusable rule,
chained x.a.b.c) — move to Task 6.

Task 6 DONE 2026-05-18 (scoped): wrote
`v4/docs/v4-cond-recursion-negation.md` (>-as-if, recursion =
self-call + base overload + fixpoint, negation = FactRead
JoinKind::Anti / WHERE NOT EXISTS; fact.rs:142-208). RED coverage
for the drift-catch primitive (stale field ref => loud, not
silent) already green via Task 5's `unknown_field_is_loud_dot_miss`.
Full cargo-rustdoc structural-hash autodoc NORTH-STAR is
blocked-by-checkpointed-ROCK-1 (`${X.field}` in json-generated
bodies) — flagged, NOT faked. Gate g6 expected 451/0 (doc inert).
Suite gate cmd:
`RUSTC_BOOTSTRAP=1 cargo test --manifest-path v4/Cargo.toml --lib
--tests -j 2` (low -j avoids concurrent-cc link flake).

Task 7 DONE 2026-05-18 (user GO "go, one commit, d2 re-render is
intentional"). ONE commit `fe88ccd5` (18 files, 694/216) on
`feat/dots-types-nesting`. d2 re-render byte-identical (no
v4/examples/ diff — zero-migration confirmed at artifact level).
Post-commit gate re-run independently: 451/0 exit 0 (no
green-faking). Base e47b0e95 == main == origin/main → NO stale
base, clean fast-forward `git merge --ff-only` (no merge commit).
Worktree removed + pruned, branch deleted (was fe88ccd5).
INITIATIVE COMPLETE; main HEAD = fe88ccd5. Deferred 3 rocks
(${X.field} body routing, _proj_<Ty> reusable rule, chained
x.a.b.c) remain open for a future session, NOT regressions.

See [[feedback-rule-is-function-not-channel]] (rule = fn, write =
return/yield — frame projections that way), [[parallel-worktree-workflow]]
(verify-before-merge, no stale base, cleanup), and
[[sprefa-genericization-initiative]].
