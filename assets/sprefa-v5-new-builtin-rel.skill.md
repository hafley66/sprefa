---
name: sprefa-v5-new-builtin-rel
description: Checklist for adding a new lazy built-in relation to the v5 dl engine (~/projects/sprefa).
---

# Adding a new built-in lazy relation to v5

Canonical reference: `ChangedKind` in `src/rels/git.rs`. Every step below has a direct parallel there.

`engine.rs` no longer exists as one file (superseded 2026-06-30 refactor): the
engine is `src/engine/mod.rs` (~5600 lines) + `src/engine/tick.rs` +
`src/engine/extract.rs`. Run `cargo build`/`cargo test` from the **repo root**,
not `v5/` — v5 was lifted to the repo root 2026-07-01.

---

## Does your family fit the trait?

`trait RelKind` (`src/rels/mod.rs:60`) covers a family with a **no-arg,
whole-set `refresh(eng) -> Result<bool>`** that self-diffs against what's
stored and returns `Ok(false)` on a steady-state no-op. If your relation needs
a **delta refresh** keyed off changed paths (spine/node/module), **extracted
args** (every/clock intervals), or **always runs** regardless of use
(builtin/type/call/dataflow/doc/daemon/effect), it does NOT fit yet — see the
module doc comment at the top of `src/rels/mod.rs` and
`plans/2026-06-30-engine-breakdown-proposal.md` for those still-in-engine
families. The checklist below is for the trait-shaped case, which is now most
new relations.

---

## Checklist

### 1. Pick (or add) the submodule

Families are bucketed by kind in `src/rels/{git,analysis,catalog,propose,scip,embed}.rs`.
Put a git-worktree-derived relation in `git.rs`, a static-analysis one in
`analysis.rs`, etc. A genuinely new bucket is a new `src/rels/<bucket>.rs` +
a `mod <bucket>;` line in `src/rels/mod.rs`.

### 2. Write the unit struct + impl

```rust
/// <What the relation tracks and when it's useful.>
pub struct StagedKind;

impl RelKind for StagedKind {
    fn rels(&self) -> &'static [&'static str] {
        &["staged"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "staged".into(),
            cols: vec![col("path", Type::Path)],
            ..Default::default()
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in staged-files relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let out = Command::new("git").arg("-C").arg(&eng.root)
            .args(["diff", "--cached", "--name-only"]).output()?;
        let mut paths: Vec<String> = if out.status.success() {
            String::from_utf8_lossy(&out.stdout).lines()
                .map(|l| l.to_string()).collect()
        } else { Vec::new() };
        paths.sort(); paths.dedup();
        let existing: Vec<String> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\" FROM {} ORDER BY \"path\"", tbl("staged")))?;
            s.query_map([], |r| r.get::<_, String>(0))?.filter_map(|x| x.ok()).collect()
        };
        if existing == paths { return Ok(false); }
        let rows: Vec<Vec<Value>> = paths.into_iter().map(|p| vec![Value::Text(p)]).collect();
        eng.refresh_rel("staged", &["path"], &rows)?;
        Ok(true)
    }
}
```

`used(prog)` and `dirty(changed)` have default impls (lazy `rels_used` gate;
"yes, every incremental tick" respectively) — only override `dirty` if your
family should NOT re-run on every `tick_paths` call (see `ScipKind`, which
gates on `index.scip` being in the changed set).

**Hard rule** (unchanged): the `Db` seam (`db.rs`) is plural-only.
`insert_rows` / `refresh_rel` take a slice. A per-row write loop fires the
tick N+1 counter and is a bug.

### 3. Register in `rel_kinds()`

`src/rels/mod.rs:89` — add `&StagedKind` to the static slice:

```rust
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[&ChangedKind, &ChangedLineKind, &CreatedKind,
      &AgentKind, &DlDiagKind, &TypeShapeKind, &TypeLggKind, &CatalogKind,
      &ScipKind, &ProposeExtractKind, &ProposeCloneKind, &EmbedKind,
      &StagedKind]
}
```

That's it — the five former call sites (`tick`, `tick_paths`,
`declare_builtins`, `all_builtin_decls`, the reserved-name guard in
`declare_all`) all loop `crate::rels::rel_kinds()` and pick the new family up
for free. No `*_RELS` const, no `*_rel_decls()` fn, no `*_rels_used()` gate,
no hand-written bail arm, no wiring line in `tick`/`tick_paths` to add or keep
in sync.

### 4. Reserved names (still true, now enforced generically)

Reserved names a user program can NEVER declare (a `.dl` author hitting one
must rename): `repo`, `rev`, `content`, `file`, `string`, `ref` (the byte-span
spine), `type_edge`, `type_edge_rev`, `type_entity`, `type_sig`, `type_link`
(the `TYPE_RELS` array — all five guard together), the module-graph rels, plus
every name any `RelKind` in the registry owns. Discovered in practice
2026-06-12: a deck-row program wanting `ref(node, panel, locator)` had to use
`node_ref` — anim's atlas-db loader reads `rel_node_ref` for exactly this
reason.

### 5. Write an e2e test

Pattern: `tests/it/discover.rs` or `tests/it/rule_edit.rs`, registered as a
module in the single integration harness `tests/it/main.rs`. Use
`env!("CARGO_BIN_EXE_dl")` to locate the binary, a `tempdir`-style sandbox,
and `--root`/`--db` flags. Assert on stdout of a `? rel(...)` query.

---

## Families that still live directly in the engine (not `RelKind`)

- **Delta refresh**: spine (`_where_bytes`/`_strings`/`_files`), `node`/`child`
  (CST), `module_edge`/`module_edge_rev` and friends.
- **Extracted args**: `every`/`clock` (temporal intervals parsed from decl args).
- **Always-run, `()` return**: `builtin`, `type_edge`/`type_entity`/`type_sig`/
  `type_link`, `call_edge`, `df_node`/`df_edge`/dataflow, `doc_comment`/`doc_tag`,
  daemon-state (`program`/`head`/`rev_advanced`), the effect runtime.

These are candidates for further staged `RelKind`-style extensions per
`plans/2026-06-30-engine-breakdown-proposal.md`, not yet done.

---

## Queued candidates (worked examples)

| Relation | Command | Columns |
|---|---|---|
| `staged(path)` | `git diff --cached --name-only` | `path: path` |
| `changed_line(path, line)` — DONE, see `ChangedLineKind` in `src/rels/git.rs` | `git diff -U0 HEAD` hunk headers (`@@ -a,b +c,d @@`) | `path: path, line: int` |

---

## Anchor drift log (auto-generated, do not hand-edit)

Hand-maintained line numbers go stale fast. dl indexes its own anchors through
its tree-sitter `ast` backend, so the table regenerates from source instead of
rotting.

Regenerate + read (from the **repo root**, not `v5/`):

    cargo build -q --bin dl
    target/debug/dl examples/gen-engine-anchors.dl --root .
    cat examples/_auto-doc/engine-anchors.md   # the live anchor table

`examples/gen-engine-anchors.dl` matches `const *_RELS` (the remaining
in-engine registries), `refresh_*_rel` (populators), the `Temporal`/`AggFn`
enums, the temporal runtime seams (`load_carry`/`rebuild_next`/`rebuild_async`/
`drain_effects`/`drain_streams`), `stratify_diags`, and the effect output-arity
seam (`split_outputs`/`split_tsv`/`async_effect_arity`) — every "where does X
live" anchor this skill needs, structural (one row per real declaration, never
a comment or call-site that merely names the symbol). Add a category by adding
one `feature_at(...) <- scan(...), ast(...)` rule. The generated artifact is
versioned with the code it indexes; the convergent `gen` sink writes nothing
when bytes already match. NOTE: this anchor table does not yet cover the
`RelKind` submodules under `src/rels/` — for those, grep `impl RelKind for` and
`rel_kinds()` directly.
