# Diet SCIP + usage/dead-code rails (bug + improvement)

Filed 2026-07-07 from a downstream consumer (a Rust game repo, `~/projects/games/smash`,
workspace at `rust-sim/`). Goal there: a `dl --check` rail that flags **dead scaffolding**
— `pub` symbols referenced only from `#[cfg(test)]`/`*_tests.rs`. Regex `scan`/`match`
can't do it (no symbol resolution); `scip_*` can, but four friction points block it.

## 1. Diet SCIP mode (the headline ask)

`dl index` on that workspace produced a **158 MB** index in **~102 s** because it indexes
**every dependency** (all of `godot-core`, the full rust-analyzer moniker set):
`scip_def=94670, scip_ref=55767, scip_fn_edge=158464` — the overwhelming majority is
third-party.

Usage / dead-code / call-graph rails only need **first-party (workspace member) symbols +
refs**. Ask: a `--diet` (or default-diet) index mode that keeps only workspace-crate
definitions and the refs among them, dropping dependency internals. Should be dramatically
faster and smaller, which is what makes a scip-backed rail viable in a pre-commit
`dl --check` gate at all. (The `scip diet` vs `scip full` split was mentioned as already a
concept — if diet exists, it is not the `dl index` default and was not discoverable from
`dl index --help`/`dl doctor`.)

## 2. Reference-doc arity drift (bug)

`docs/reference/relations.md` (generated from `rel_catalog`) lists:
- `scip_def(symbol, file)` — but the engine rejects 2 cols; actual is **3**:
  `(moniker, file, repo)` e.g. `("rust-analyzer cargo smash_core 0.1.0 geo/Geometry#",
  "core/src/geo.rs", "smash")`.
- `scip_ref(file, symbol, def_file)` — but the engine rejects 3 cols; actual is **4**.

The generated reference is a version behind the engine catalog. Either regen is stale or
the catalog metadata's arity is wrong. Cost me two failed rule compiles to discover by
probing.

## 3. Index-root vs check-root mismatch

`dl index` writes `<crate-root>/.dl/index.scip` and `scip_*` "load automatically for
`dl … --root <crate-root>`". But a repo whose Cargo workspace lives in a **subdir**
(`rust-sim/`) runs its rails and `dl --check` from the **repo root** (where `.dl/*.dl` +
the `.githooks/pre-commit` live). A scip-backed rail therefore cannot slot into the
existing repo-root `dl --check` gate — the index isn't found from that root.

Ask: let a parent `--root` discover a child crate's `index.scip`, or let `--check` /
a rail point at the index path explicitly, so scip rails work from the repo root where the
rest of the rails already run.

## 4. Moniker folding for usage rails (improvement)

A first-cut "pub symbol referenced only by tests" rail (prototype below) produced real
hits (`stage/mod.rs::platform_top`, `geo.rs` `ShapeCastHit.normal1`/`time_of_impact` —
all genuinely test-only) **mixed with false positives** (`tune.rs` `KNEEMAN`, `from_char`,
`roster`, `resolve` — heavily used in prod). The FPs come from moniker resolution:

- trait-impl method refs resolve to the **trait** moniker, not the impl's;
- **cross-crate** refs (the `shell`/`net` crates using a `core` symbol) aren't folded in
  when the rail only scopes one crate's files;
- **macro-expanded** refs aren't captured as occurrences.

Ask: a documented recipe (or helper relation) for "symbol reachable only from `cfg(test)`"
that folds trait/impl monikers and spans crates — this is the canonical dead-code query and
would make it a first-class rail rather than a hand-rolled join every consumer re-derives.

## Prototype (the join that works today, modulo the FPs above)

```dl
rel test_file(f: text).
test_file(f) <- scip_ref(f, _, _, _), f =~ /_tests\.rs$/.

rel def_here(sym: text, f: text).
def_here(sym, f) <- scip_def(sym, f, _), f =~ /core\/src\//, !test_file(f).

rel test_ref(sym: text).
test_ref(sym) <- scip_ref(rf, sym, _, _), test_file(rf).

rel real_ref(sym: text).
real_ref(sym) <- scip_ref(rf, sym, df, _), rf != df, !test_file(rf).

rel flagged(name: text, f: text).
flagged(nm, f) <- def_here(sym, f), test_ref(sym), !real_ref(sym), scip_name(sym, nm).
? flagged(name, f).
```

Priority: (1) diet index is the unlock (makes scip rails cheap enough to gate on);
(2) doc arity is a quick correctness fix; (3) root discovery unblocks the gate;
(4) moniker folding is the quality pass that removes the FPs.
