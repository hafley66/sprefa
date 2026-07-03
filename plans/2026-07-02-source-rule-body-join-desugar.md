# Source-rule body-join auto-desugar

Goal: a source rule (scan/match/ast/sg/json-file/cmd/comment) may carry relational
atoms in its body; the frontend desugars it into the hidden source-rel + derived-rule
split the author writes by hand today. Pure AST transform — no evaluator changes.

The motivating shape (watch-ext.dl, currently two rels):

```
rel ext_src(path: file, hash: text).
ext_src(path, hash) <- scan("WORK", "editors/vscode-dl/src/*.ts", path, rev),
  file(_, "WORK", path, content_id), content(content_id, hash).
```

Today this errors at extraction: "head var `hash` is not bound by any source op"
(src/engine/mod.rs:5597). The hand fix is `ext_seen` (scan only) + `ext_src`
(derived join). The desugar makes the engine perform that split.

## What it unlocks for authoring

- One-rule enrichment: scan + content-hash join (the watch-ext shape) without a
  named intermediate rel.
- Capture-time joins to builtin rels (`file`, `content`, `type_entity`, ...) and
  to user rels — config-driven scan filtering (`scan(...), allowlist(path)`),
  negation (`scan(...), !vendored(path)`).
- The "step 0" boilerplate rel disappears from user programs; the extraction-vs-
  fixpoint phase boundary becomes an engine detail instead of an authoring rule.
- The mixed source/derived bail stays (it guards a real hazard); this desugar is
  how the engine satisfies it on the user's behalf.

## Code anchors

- `Rule::is_source()` — src/ast.rs:370 (scan/match/ast/sg/ast_yaml/cmd/comment,
  file-form json/jsonp `rev.is_some()`).
- Extraction evaluator: source rules bind rows ONLY from source-op captures; body
  `Pos` atoms fall through `_ => {}` (src/engine/mod.rs, the loop ending ~5583).
  The one sanctioned Pos atom today is the repo-fan seed:
  `repo(r, _, _), scan(r, rev, glob, path, rev_out)` — the error text at
  mod.rs:5597 documents it. THIS PATTERN MUST SURVIVE UN-DESUGARED
  (tests/it/data_driven_scan.rs covers it).
- Mixed source/derived bail: src/engine/tick.rs:~115 ("relation '{rel}' is
  written by both a source rule ... and a derived rule").
- Frontend desugar precedent: `desugar_effects` in src/frontend.rs (called from
  `load_program` at :43 and `load_program_set` at :110; named-arg normalization
  runs before it per the comment at :123). The new pass slots in immediately
  after `desugar_effects` at both call sites.
- Typecheck entry: `typecheck::check_and_normalize` (src/typecheck.rs:595) runs
  after the frontend, so it sees the already-split rules and the synthesized
  hidden decls — no typecheck changes for the happy path.

## Type signatures

```rust
// src/frontend.rs

/// Split every source rule that carries relational body atoms into a hidden
/// source rel + a derived join rule. Runs after desugar_effects at both
/// load_program and load_program_set call sites.
fn desugar_source_body_joins(items: &mut Vec<Item>);

/// True iff this rule must split: is_source(), has >=1 BodyItem::Pos/Neg that
/// is not the repo-fan seed for one of its own source ops, and is not an
/// effect (@async/@stream), @next, or gen rule (those keep today's semantics
/// and today's errors).
fn rule_needs_split(rule: &Rule) -> bool;

/// Vars bound by the rule's source ops (scan slots, match/ast/sg captures,
/// span args, cmd out, json captures), in first-appearance order. Reuse the
/// same slot-walk the extraction evaluator does; do NOT re-derive it ad hoc.
fn source_captured_vars(rule: &Rule) -> Vec<(String, Type)>;

/// `format!("{head}__src{n}")`, n = index among the head rel's rules needing
/// split, in program order.
fn hidden_rel_name(head: &str, n: usize) -> String;

/// (hidden source rule, rewritten derived rule). The hidden rule's head =
/// the captured vars that are USED downstream (in the original head, in the
/// surviving body atoms, or in cmps that mix source and join vars). The
/// derived rule keeps the original head verbatim and the original spans.
fn split_rule(rule: &Rule, hidden: &str) -> (Rule, Rule);
```

## Pseudo-code

```rust
fn desugar_source_body_joins(items) {
    // pass 1: group rules by head rel; find rels where ANY rule needs a split.
    // pass 2: for each such rel H:
    //   - every SOURCE rule of H splits (even join-free ones -> hidden rel +
    //     trivial copy rule), so H ends up purely derived. Otherwise the
    //     tick-time mixed bail fires on the rel we just rewrote.
    //   - derived siblings of H are untouched.
    //   for each splitting rule (program order, n = 0..):
    //     hidden = hidden_rel_name(H, n)
    //     caps   = source_captured_vars(rule)
    //     used   = caps.filter(v => v in head-vars ∪ vars(surviving atoms/cmps))
    //     synthesize Item::Rel decl: rel hidden(used...) with caps' types,
    //       group "internal" (RelDecl.doc = ""), same file/pos as the rule.
    //     source half: hidden(used...) <- [source ops + repo-fan seed atoms +
    //       cmps referencing ONLY source vars]
    //     derived half: H(original head terms) <- hidden(used...),
    //       [remaining Pos/Neg atoms, remaining cmps, assignments]
    //   replace the original rule items in place (order preserved).
}
```

Cmp placement: a cmp whose vars are all source captures stays in the source half
(row filter at extraction, matches today's behavior for pure source rules); any
cmp touching a join var moves to the derived half. Assignments (`x = expr(...)`)
always move to the derived half.

Repo-fan detection: a `Pos { rel: "repo", .. }` atom whose first term is a var
occupying the repo slot of one of the rule's scan ops stays in the source half
and does not by itself trigger a split.

## Reserved-name guard

Hidden names match `^<rel>__src\d+$`. Add a check in `check_and_normalize`'s
decl pass: a USER-written rel decl or rule head matching `__src\d+$` errors
("reserved for the source-rule desugar"), same shape as the existing `ref`/
`diag` reserved-name guards. The frontend synthesizes its decls after parse, so
the guard must fire only on user-authored items — run it before the desugar or
mark synthesized decls (a bool on RelDecl or the "internal" group) and skip them.

## Instance lifetimes

- The split exists only in the loaded AST: every load (one-shot, daemon,
  --check, LSP) re-runs the desugar deterministically. Nothing about the split
  is persisted except the hidden rel's rows.
- `rel_<H>__src<n>` tables live the normal source-rel lifecycle: reconcile
  digests, `retract_paths` on --changed, drop-when-removed (the existing
  removed-rel cleanup; tests/it/where_removed.rs is the pattern). Reordering
  rules in the program renumbers `n` -> old table dropped, new one built on the
  next tick; the source-rule digest change makes that a normal cold refresh.
- The derived half rides `rebuild_derived` + `affected_derived` scoping
  unchanged.

## Storage layout, reads/writes, uniqueness

- Layout: one SQLite table `rel_<H>__src<n>` per split rule, columns = the
  `used` capture subset, types from the source-op slot map (scan path slot =
  path/file, rev = text, captures = text, span args = int, cmd out = text).
- Writes: `reconcile_sources` only (it is a source rel like any other).
- Reads: the derived half's SQL in `rebuild_derived`; nothing else. Hidden rels
  are excluded from `rel_catalog`'s default listing via the "internal" group
  (follow how builtin groups render; keep them visible under a flag if the
  catalog has one, else just exclude).
- Uniqueness: (H, n) is unique by construction within one program; the reserved
  suffix guard keeps users out of the namespace; two programs merged by
  `dl a.dl b.dl` concatenate items BEFORE the desugar runs (both load paths call
  it once, after merge), so numbering stays collision-free.

## Error-path changes

- The extraction-time error at src/engine/mod.rs:5597 becomes unreachable for
  the join case; keep it as an internal backstop but reword: unbound head var =
  engine bug after the desugar, and point at the desugar.
- A head var bound by NOTHING (neither source captures nor body atoms) must
  still error loudly — after the split it lands in the derived half where the
  normal typecheck/derived binding diagnostics catch it. Add an explicit test;
  if the derived path's message is unclear, add a typecheck diag on the
  synthesized rule that names the ORIGINAL rule's position.

## Proof (all must pass; report each)

New e2e file `tests/it/source_body_join.rs` (+ `mod source_body_join;` in
tests/it/main.rs — NB that file may have local edits on main; in the worktree
just add the line):

1. `scan_join_binds_head_vars`: fixture repo; single rule
   `ext_src(path, hash) <- scan(...), file(_, "WORK", path, cid), content(cid, hash).`
   Rows equal the hand-split twin program's rows (run both, compare sets).
2. `sibling_source_rules_all_convert`: rel with two source rules, one needing a
   split — no mixed-rel bail, union of both rules' rows correct.
3. `changed_tick_retracts`: edit a scanned file via the --changed path; the old
   (path, hash) row is gone, the new one present (retraction rides the hidden
   source rel).
4. `cmp_on_join_var_filters`: a `hash != "x"`-style cmp on a joined var lands in
   the derived half and filters rows.
5. `negation_in_source_body`: `scan(...), !skip(path)` with `skip` a fact rel.
6. `repo_fan_survives`: the `repo(r,_,_), scan(r, ...)` pattern still works and
   does NOT split (also: tests/it/data_driven_scan.rs stays green).
7. `reserved_hidden_name_rejected`: user program with `rel foo__src0(x: text).`
   fails --check with the reserved-name diag.
8. `truly_unbound_head_var_still_errors`: head var bound by nothing errors with
   a message naming the var.

Suites: `cargo test --quiet --test it` (currently 451/0/4 + the new file) and
`cargo test --quiet --lib` (199/0/1) both green. `cargo run --quiet --bin dl --
--check --root .` on this repo: clean (3 pre-existing info diags only).

Dogfood: collapse .dl/watch-ext.dl's `ext_seen`/`ext_src` pair into the single
rule; update its header comment; verify the daemon still queues exactly one
`ext_built` effect per source edit (edit one watched file, check pending_effect
count moves by 1).

Docs: update the "one rel = one rule kind" style note in CLAUDE.md (the hazard
and bail remain; note the frontend now performs the split for source rules) and
the scan-rule restriction sentence in README's DSL section if present. Do not
touch autogen zones.
