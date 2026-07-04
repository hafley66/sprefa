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

Families are bucketed by kind in
`src/rels/{git,analysis,catalog,propose,scip,embed,perf,querylog}.rs`. Put a
git-worktree-derived relation in `git.rs` (also home to `GitRefKind`/
`RevBehindKind`, not just `changed`/`changed_line`/`created`), a
static-analysis one in `analysis.rs`, an engine-telemetry one in `perf.rs`
(pattern: `rel_count`/`stmt_ms`), a request-history one in `querylog.rs`
(pattern: `query_log`), etc. A genuinely new bucket is a new
`src/rels/<bucket>.rs` + a `mod <bucket>;` line in `src/rels/mod.rs`.

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
            group: "changed",
            doc: "git-staged paths (git diff --cached --name-only)",
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

**`group`/`doc` on the decl are REQUIRED** (2026-07-02, replaces the old
`builtin_rel_docs()` tuple registry): `rel_catalog` and the generated README
table read them off the decl, and a built-in decl with an empty `doc` fails
`rel_catalog::every_builtin_relation_is_documented`. Avoid `|` in the doc (it
renders inside a markdown table cell).

**Hard rule** (unchanged): the `Db` seam (`db.rs`) is plural-only.
`insert_rows` / `refresh_rel` take a slice. A per-row write loop fires the
tick N+1 counter and is a bug.

### 3. Register in `rel_kinds()`

`src/rels/mod.rs:89` — add `&StagedKind` to the static slice:

```rust
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[&ChangedKind, &ChangedLineKind, &CreatedKind, &GitRefKind, &RevBehindKind,
      &AgentKind, &DlDiagKind, &TypeShapeKind, &TypeLggKind, &CatalogKind,
      &ScipKind, &ProposeExtractKind, &ProposeCloneKind, &EmbedKind, &PerfKind,
      &QueryLogKind, &StagedKind]
}
```

(That's the real list as of this writing, `src/rels/mod.rs:117`; check it
directly before copying, it grows every time a family lands.)

That's it — the five former call sites (`tick`, `tick_paths`,
`declare_builtins`, `all_builtin_decls`, the reserved-name guard in
`declare_all`) all loop `crate::rels::rel_kinds()` and pick the new family up
for free. No `*_RELS` const, no `*_rel_decls()` fn, no `*_rels_used()` gate,
no hand-written bail arm, no wiring line in `tick`/`tick_paths` to add or keep
in sync.

### 4. Reserved names (still true, now enforced generically)

Reserved names a user program can NEVER declare (a `.dl` author hitting one
must rename): `repo`, `rev`, `content`, `file`, `string`, `ref` (the byte-span
spine), the module-graph rels (`MODULE_RELS`), the seven-member type-graph
family (`TYPE_RELS`: `type_edge`/`type_edge_rev`/`type_entity`/
`type_entity_rev`/`type_sig`/`type_link`/`type_link_rev`), the call-graph
family (`CALL_RELS`: `call_def`/`call_def_rev`/`call_site`/`call_edge`/
`call_edge_rev`/`call_name`/`call_kind`), the dataflow family
(`DATAFLOW_RELS`, 13 members incl. `df_node`/`df_node_repo`/`df_edge`/
`df_arg`/`df_param`/`df_field` and their `_rev` twins), `doc_comment`/
`doc_tag` (`DOC_TEXT_RELS`), `dl_diag`, plus every name any `RelKind` in the
registry owns; the bail message on a redeclare attempt names the exact
family, so grepping `*_RELS` in `src/engine/mod.rs` is the authoritative list
when this paragraph goes stale again. Discovered in practice 2026-06-12: a
deck-row program wanting `ref(node, panel, locator)` had to use `node_ref`,
anim's atlas-db loader reads `rel_node_ref` for exactly this reason.

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
- **Always-run, `()` return**: `type_edge`/`type_entity`/`type_sig`/`type_link`,
  `call_edge`, `df_node`/`df_edge`/dataflow, `doc_comment`/`doc_tag`,
  daemon-state (`program`/`head`/`rev_advanced`), the effect runtime.

These refresh BODIES still live in `Engine::refresh_module_rels` /
`refresh_type_rels` / `refresh_call_rels` / `refresh_dataflow_rels` /
`refresh_doc_rels` / `refresh_spine_rels` in `src/engine/extract.rs`. A new
member of one of these (a new type-graph kind, a new dataflow node shape) is
still a hand-edit inside the existing refresher, not a new `RelKind`/
`ExtractFamily` impl.

DISPATCH for six of these (`module`/`type`/`call`/`dataflow`/`doc`/`spine`) DID
move to a registry as of the 2026-07-02 engine-trait-refactor Phase R1:
`trait ExtractFamily` (`src/rels/extract_family.rs`) mirrors `RelKind` in
shape (`name`/`rels`/`decls`/`reserved_msg`/`used`/`refresh(&mut Engine)`) and
`extract_families()` is what `tick`/`tick_paths` loop over now instead of a
hand-written fan-out. It is a SEPARATE trait from `RelKind` (not a case that
"fits" the `RelKind` checklist above) because these refreshers take `&mut
Engine` (they mutate per-file fact caches), most carry a persisted
`extract:<family>` input digest for the perf-gap-A warm-tick skip (`type`/
`call`/`dataflow`/`doc`; `module`/`spine` always re-derive), and `decls()`/
`reserved_msg()` still delegate to the free-fn decl tables + hand-written
guard in `engine/mod.rs` rather than owning them (R1 is dispatch-only; body
relocation into `src/rels/` is deferred Phase R2, per
`plans/2026-07-02-engine-trait-refactor-v2.md`). `node` (CST) stays fully
hand-dispatched, not even in `ExtractFamily`: it must run before `spine` and
its incremental form has a different signature.

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
