# SCIP occurrences + local binding names (S1 / S2)

## STATUS: PLANNED → implementing on `feat/scip-occurrences`

Two logged agent complaints:

- **S1** `scip_ref` has no line/column → positional tag→symbol mapping impossible
  from `scip_ref` alone (an alias fix leaked into Python because of it).
- **S2** `scip_name` returns the canonical export name, not the local binding →
  aliased/default imports silently fail a name-based join.

## Triage: what the importer keeps vs drops

`src/scip_import.rs` reads every `Occurrence`, but at the **rel layer**
(`src/rels/scip.rs`) only `scip_def`/`scip_ref`/`scip_edge`/`scip_fn_edge`/
`scip_callee_type`/`scip_local`/`scip_impl`/`scip_name` are emitted.

- `Occurrence.range` (`scip_import.rs:176-177`, `parse_range` :266): decoded to
  `((sl,sc),(el,ec))`, 0-based. Both the 4-element `[sl,sc,el,ec]` and 3-element
  `[sl,sc,ec]` (single-line) forms handled. **BUT** only `(file, sl, sc, symbol)`
  survives into `ScipRows.occ_spans` (:39) — end position DROPPED, and
  `occ_spans` is **never emitted as a relation**. Its only consumer is
  `propose.rs` (the in-process clone kernel), read via `scip_import::load`.
- `Occurrence.symbol_roles` (`is_def` :292): consulted for def-vs-ref
  branching, then DROPPED — no role ever surfaces.
- The def/ref/edge rows carry a trailing `repo` col (cross-root fix, 2026-07-03);
  `occ_spans` does NOT carry repo.

So S1 is "surface the range + role we already decode"; nothing new to parse.

SCIP range encoding confirmed in `parse_range`: 4-el or 3-el (el==sl). Test
fixtures (`tests/it/scip_import.rs`, `scip.rs` unit tests) build `Occurrence`
with an explicit 4-el `range`. The vendored `scip` crate exposes no
`position_encoding` field; columns are UTF-16 code units (LSP/SCIP default) —
identical to bytes/chars for ASCII identifiers (the overwhelming majority).

## Rel shapes shipped (both NEW, pure addition — `scip_def`/`ref`/`edge` untouched)

    scip_occurrence(file: Path, symbol: Text, line: Int, col: Int,
                    end_line: Int, end_col: Int, role: Text, repo: Text)
    scip_binding(file: Path, symbol: Text, local_name: Text,
                 line: Int, col: Int, repo: Text)

Two rels, not one 9-col rel, because:

- `scip_occurrence` is a **zero-IO** projection of data already decoded in
  `rows()` — low risk, always correct, answers S1 fully.
- `scip_binding` is the **content-derived** local name (S2). Isolating it keeps
  the content-read cost and the stale-index caveat off a user who only wants
  positions, and gives it a natural `(file,line,col)` join back to
  `scip_occurrence`.

`line`/`col`/`end_line`/`end_col` are the raw SCIP **0-based** values (matches
`occ_spans`/`fn_edge` attribution; documented in the decl doc — a sharp edge vs
`type_entity`'s 1-based lines).

`role` ∈ `{definition, reference}` — closed text vocabulary from the
`symbol_roles` bitmask (`Definition` bit set → `definition`, else `reference`;
Import/Read/Write collapse into `reference`). **Plain `Type::Text`, no brand:**
the brand machinery (`Col.brand`, `type X <: Y`) exists, but NO builtin kind
column uses it today (`git_ref.kind`, `op_catalog.kind`, `type_edge.kind` are
all plain text). Following colocated consistency, `role` stays plain text.

## S2 local name: how obtained + rev caveat

The local binding text IS the source text at the occurrence range (per the
complaint). Obtained by the **importer reading file content at load** (option a),
NOT a ref-spine join (option b): the ref-spine is byte-offset keyed while SCIP is
line/col keyed — a join would need a line→byte map per file. Direct slice is
simpler and honest.

Rev honesty: SCIP is **WORK-only** (the index reflects the on-disk tree at index
time; `read_content` for `rev=="WORK"` reads disk). `scip_binding` slices the
current WORK content of `root.join(relative_path)`. If the index is stale vs a
later edit, the slice can mis-name — the same staleness the whole importer
already carries. Documented in the decl doc. One read per distinct file (cached),
never per-occurrence — N+1-free.

`display_name` on `SymbolInformation` is NOT used for the local name: it carries
the canonical/declared name, which is exactly what fails S2 for an alias
(`import { foo as bar }` → display_name "foo", source text "bar"). Content-slice
is indexer-agnostic.

## Type signatures

    // src/scip_import.rs
    pub struct ScipRows {
        ...                                             // unchanged fields
        /// (file, symbol, start_line, start_col, end_line, end_col, role, repo)
        /// 0-based; role ∈ {definition, reference}; repo = doc origin.
        pub occurrences: Vec<(String, String, i32, i32, i32, i32, String, String)>,
    }

    /// definition/reference from the SCIP symbol_roles bitmask.
    fn role_label(roles: i32) -> &'static str

    /// Pure UTF-16 slice of one source line [start_col, end_col). Testable.
    fn slice_local_name(line_text: &str, start_col: i32, end_col: i32) -> String

    /// Local binding names for a batch of occurrences under one on-disk root.
    /// Reads each distinct file ONCE (cache), slices single-line ranges.
    /// Returns (file, symbol, local_name, line, col, repo). Skips empties /
    /// multi-line / unreadable.
    pub fn local_bindings(
        occurrences: &[(String, String, i32, i32, i32, i32, String, String)],
        root: &Path,
    ) -> Vec<(String, String, String, i32, i32, String)>

## Pseudo-code

    // rows() — inside the existing per-occurrence loop (2nd pass, :173):
    //   if let Some(((sl,sc),(el,ec))) = parse_range(&occ.range) {
    //       occ_spans.insert((path, sl, sc, symbol));            // unchanged
    //       occurrences.insert((path, symbol, sl, sc, el, ec,
    //                           role_label(occ.symbol_roles).into(),
    //                           doc_repo(path)));                // NEW
    //   }
    // (local symbols keep flowing to `locals`; occurrences records EVERY occ,
    //  incl. locals, so a detector can see them — matches occ_spans breadth.)

    // local_bindings():
    //   let mut cache: HashMap<&str, Option<Vec<String>>> = ...; // file -> lines
    //   for (file, symbol, sl, sc, el, ec, _role, repo) in occurrences {
    //       if sl != el { continue; }                            // single-line only
    //       let lines = cache.entry(file).or_insert_with(|| read root.join(file));
    //       let Some(line_text) = lines?.get(sl) else continue;
    //       let name = slice_local_name(line_text, sc, ec);
    //       if !name.is_empty() { out.push((file, symbol, name, sl, sc, repo)); }
    //   }
    //   dedup

    // scip.rs refresh() — per-input loop (:93), BEFORE merge (root known here):
    //   let rows = load(path, root, slug)?;
    //   all_bindings.extend(local_bindings(&rows.occurrences, root));
    //   all.occurrences.extend(rows.occurrences);
    //   ...
    // emit: scip_occurrence from all.occurrences, scip_binding from all_bindings.
    // empty-inputs branch clears both new rels too.

## Instance lifetimes / state

- `ScipRows` — per-`load` value, one per input index, merged into `all` in
  `refresh`. Stateless; sorted before return.
- `local_bindings` file cache — local to one `local_bindings` call (one input
  index). Dropped at return. No engine-lifetime state added.
- The two new rel tables (`rel_scip_occurrence`, `rel_scip_binding`) live in the
  engine DB like every other builtin; refreshed whole-set by `ScipKind::refresh`.

## Storage / reads / writes

- **Writes**: `eng.refresh_rel("scip_occurrence", …, &occ_rows)` and
  `("scip_binding", …, &bind_rows)` — batched `insert_rows`, one call each. No
  per-row writes.
- **Reads (new)**: `local_bindings` reads WORK content of each occurrence's file
  once via `std::fs` on `root.join(file)` (same as `read_content(root,"WORK",…)`).
- **Uniqueness**: `occurrences` is a `HashSet` (dedup identical occ); bindings
  deduped as a set. `scip_occurrence` unique on the full tuple; `scip_binding`
  unique on `(file,symbol,local_name,line,col,repo)`.

## Registration / gating (checklist)

1. `ScipKind::rels()` += `"scip_occurrence"`, `"scip_binding"` → reserved-guard +
   `changed_source_rels` cover them (mod.rs:3434 loops `rel_kinds()`).
2. `ScipKind::decls()` += two `RelDecl { group: "scip", doc, .. }` → catalog +
   `declare_builtins` + `all_builtin_decls` cover them.
3. `refresh()` empty-inputs branch clears both; populated branch emits both.
4. `used(prog)` (default `rels_used(self.rels())`) now also fires the family when
   only `scip_occurrence`/`scip_binding` is referenced — correct.
5. `dirty()` unchanged (gates on `index.scip` in the changed set) → editing
   source never forces a SCIP reload; the new rels refresh with the family.
6. **`extract_input_digest`**: NO change. It folds `scip_ref` because the
   type/call/df extraction families consume SCIP *resolution*. `scip_occurrence`/
   `scip_binding` are user-facing query rels; no extraction family reads them, so
   they can't cause a false family-skip. They stay fresh via `ScipKind` (which
   has no per-family digest-skip — always reloads when used+dirty).
7. **magic-rel-audit**: both names are catalogued (via `decls()`), so literal
   `refresh_rel("scip_occurrence"…)` calls anti-join clean against `rel_catalog`.
8. `scip_want` multi-root merge: `refresh` already loops `index_inputs()`
   per-repo; occurrences + bindings computed per-input with that input's root, so
   multi-repo attribution (the `repo` col) is preserved for both new rels.

## Tests

- Unit (`scip_import.rs`): `parse_range` 4-el + 3-el; `role_label` def/ref;
  `slice_local_name` ASCII + UTF-16 (emoji-prefixed line); `rows` emits
  `occurrences` with end pos + role + repo; multi-repo occurrence tagging.
- e2e (`tests/it/scip_import.rs`, existing synth-index pattern):
  - occurrences land with correct line/col/end, role distinguishes def vs ref;
  - aliased-import binding: `app.ts` on disk = `import { foo as bar } …`, occ of
    canonical `foo` symbol over `bar`'s range → `scip_binding` row with
    `local_name == "bar"` joinable to the canonical symbol;
  - multi-repo attribution preserved (two indexes, distinct repo tags);
  - **regression**: `scip_def`/`scip_ref`/`scip_edge` byte-identical output
    with the new rels present (pure addition).

## Docs

- CHANGELOG `[Unreleased]` → `### Added`.
- Generated zones (`docs/reference/relations.md`, README builtin-rels block,
  `docs/reference/magic-rels.md` unaffected) regen via the generators, NOT
  hand-edited.

## Deferred

- Content-read cost lands on any scip-using program's full tick (the family
  refreshes together). Per-subrel `used`-gating would need the program threaded
  into `refresh`; not worth it (reads are file-cached, WORK-only, index-gated).
- UTF-8/UTF-32 SCIP position encodings: assumed UTF-16; the vendored crate
  exposes no `position_encoding`. Identifiers are ASCII so it doesn't bite.
