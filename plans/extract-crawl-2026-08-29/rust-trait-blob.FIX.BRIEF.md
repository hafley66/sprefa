# Lane `fix-extract-rust-trait-blob` (glm53f): a trait edge points at the file that declares THAT trait

Defect (coordinator audit 2026-08-30): `v6/sprefa-extract/src/lang/rust_modules.rs`
`trait_fn_target` (grep `fn trait_fn_target`) looks up `self.trait_fns[trait_name]`,
finds the entry whose fn name matches, and returns `(sites[0].0, matched.span)`:
the FIRST entry's blob with ANOTHER entry's span. `trait_fns` is keyed on the
bare trait name. In `/Users/chrishafley/projects/rust-analyzer/crates`, 139 of
390 trait names are declared in more than one file (`grep -rhoE '^\s*(pub(\([a-z]+\))? )?trait [A-Za-z_]+' --include='*.rs' | awk '{print $NF}' | sort | uniq -c | awk '$1>1'`),
so those edges land in the wrong file. `trait_impl_target` and
`trait_default_target` carry the same bare-name key.

## First action
```
git merge --ff-only <BASE_SHA>
cd v6/sprefa-extract && nice -n 15 cargo build --release --features cli 2>&1 | tail -1
```
Read `rust_modules.rs` around `trait_fns`, `trait_impl_fns`, `type_traits`,
`trait_in_scope`, and `plans/extract-crawl-2026-08-29/rust.REPORT.md` section 20
(the #584 trait lane) before editing.

## Fix, fail-first
1. Fixture: `tests/fixtures/rust_findings/trait_blob/` with `a.rs` and `b.rs`
   each declaring `trait Shape { fn area(&self) -> u32; }` (different bodies),
   `a.rs` also `impl Shape for Sq` and a caller `fn f(s: &dyn Shape) { s.area(); }`;
   `b.rs` a caller that `use`s nothing from `a`. Test `tests/7N_rust_trait_blob.rs`
   asserts the edge from `a.rs` targets `a.rs`'s `area`, and `b.rs`'s call
   targets `b.rs`'s. Paste the HEAD failure (wrong file) in the header. Commit red.
2. Replace the `(ContentId, String, Span, bool)` tuples in `trait_fns` with the
   existing `TraitFn` plus a `blob: ContentId` field (or a `TraitFnSite` struct);
   return the MATCHED entry's blob.
3. When a trait name has entries in more than one blob: prefer the caller's own
   file, else the one `trait_in_scope(caller, trait_name)` binds, else unbound
   (an `unresolved{reason}` row, never a guess). Same rule in
   `trait_impl_target` and `trait_default_target`.
4. Commit green.

## Receipt
`nice -n 15` single-process run over the rust corpus (COMMON.md invocation),
`plans/extract-bench-2026-08-29/rust.project.py` + `bench.py` vs both rust
oracles; then `RATCHET_BUMP=1 just extract-ratchet` rust rows: recall must not
drop below 70.23 / 78.75, precision expected up from 43.32 / 42.44. Before ->
after in the PR body, plus the count of trait edges that changed file. Gate
(`nice -n 15 cargo test --release --features cli`) in background with a log;
wall-ratio flakes rerun 3x isolated.

## Ownership
`src/lang/rust_modules.rs`, `src/lang/rust.rs` (call sites of the three fns
only), `tests/7N_rust_trait_blob.rs`, `tests/fixtures/rust_findings/trait_blob/`,
`plans/extract-crawl-2026-08-29/rust.REPORT.md` (append section), RATCHET.tsv
rust rows with BUMP. NOT `scip*`, `go*`, `ts*`, `types.rs`, `project.rs`. No
`cargo fmt` on files you do not own. One extract run at a time, `timeout 60`.
No file over 1 MB. Budget 60 min; past it, post with what is green.

Push `fix/extract-rust-trait-blob`, `gh pr create --base main`, hail
`boop beep --no-wait --as fix-extract-rust-trait-blob sprefa-coordinator "rust trait blob: PR #N, precision a -> b vs ra, c -> d vs scip, n edges moved file, gate x/y"`.
Laws: no em dashes anywhere, no eprintln (tracing only), descriptive names,
comments only for what code cannot show, no words
provenance/substrate/load-bearing/regime/refusal, never "ground truth".
