# Variable-name signal extraction

Date: 2026-06-26. Companion to `2026-06-26-cross-domain-decomposition-
techniques.md`. The prior surveys cover structural and semantic axes. This
note adds a third: **lexical repetition of identifier names.** rust-analyzer
already emits local-variable names in the SCIP index; sprefa currently filters
them out (`usable_symbol` in `scip_import.rs:147`). Flipping that filter
opens a new signal layer.

## The signal

Same identifier name appearing across many functions or files.

- `parser_state` declared in 8 fns but never as a type: missing-type smell.
- `{db, repos, root}` parameter trio in >=5 fns: context-object smell.
- `MAX_RETRIES = 5` literal in 6 files: missing shared constant.
- `parse_X`, `parse_Y`, `parse_Z` family: missing module/type for the family.

## The taste problem

Not all repetition is a smell. Humans love idiomatic shape repetition (`?`
chains, `match` arms), love naming-family repetition (`parse_*`, `build_*`),
hate copy-paste code blocks, hate repeated parameter lists, hate same long
name across files (the missing-type signature).

The discriminator is **structural vs semantic repetition**:

- Same shape + same meaning = idiom.
- Same shape + divergent meaning = copy-paste.
- Same meaning + no shared structure = missing abstraction.

The third case is the high-value one. Sprefa detects it via raw name
frequency. Hand-tuned filters handle idiom rejection. N-rater labeling
(the methodology from the prior note) handles cases in between.

## Filter heuristics

| Filter | Cuts | Why |
|---|---|---|
| Length >= 6 | `i, j, n, x, s, t, k, idx` | generic short names |
| Length >= 10 | `len, val, err, ret, ctx, db` | common medium names |
| Cross-file count >= 5 | local idioms | repetition must span files |
| Not in `scip_def` (not a type/struct/fn) | already-named concepts | don't suggest what exists |

Strongest single filter: long name + cross-file + not already a type. Catches
missing-type smell with low noise.

## Relations

```dl
rel scip_local(fn: text, name: text).    # local var or param declared in fn
```

One relation. Params and lets come through the same path; if RA distinguishes
them in the moniker shape, split into `scip_param` and `scip_local` later.

## Recipes

### `missing-type.dl`

Names of length >= 10 appearing as locals in >=5 files, not already a type.

### `context-object.dl`

Parameter multisets recurring across >=5 fns with >=3 long-name members.

### `param-fan-out.dl`

Fns declaring >=10 locals (parameter-count proxy; will refine if RA
distinguishes params).

## Extraction plan

`v5/src/scip_import.rs`:

1. Add `pub locals: Vec<(String, String)>` field to `ScipRows`.
2. Add a third pass after the existing two: for each def occurrence whose
   symbol starts with `local `, attribute to the enclosing fn (same
   predecessor search as `enclosing_fn`), strip `local ` prefix and any
   shadowing suffix (`#N`), collect `(fn_moniker, name)`.
3. Unit test: tiny SCIP index with a local def inside a fn; assert the
   tuple is collected.

`v5/src/engine.rs`:

1. Add `"scip_local"` to `SCIP_RELS`.
2. Add declaration in `scip_rel_decls`.
3. Add refresh path in `refresh_scip_rels`.

## Caveats

- **Name shadowing**: RA disambiguates with `#N` suffix (`foo`, `foo#1`).
  Strip the suffix.
- **Built-in types**: `Vec`, `HashMap`, `Option` appear in `scip_def` and
  are filtered by the `!existing_type(name)` check.
- **Generics**: `T`, `E`, `K`, `V` filtered by length.
- **Test code**: tests have many similar names (`result`, `expected`); may
  need to filter test files separately.

## Open questions to resolve by running

- What is the actual length distribution of locals in sprefa's own index?
  Drives the length threshold.
- How often does a long local name appear in N files? Drives the cross-file
  threshold.
- Does RA emit params with a distinguishable moniker shape, or are they
  indistinguishable from lets?

## Results (run on sprefa's own index, 2026-06-26)

### Implementation wrinkle resolved

Initial extraction grabbed numeric IDs, not source names: rust-analyzer's
local monikers are opaque (`local 0`, `local 1`, ... `local 9186`). The
source-level variable name lives in `SymbolInformation.display_name`, a
separate field per-document on `doc.symbols`. Added a `display_names()` helper
that builds a symbol -> display_name map; the locals pass now joins through
it. Tests updated to populate `doc.symbols` with the `SymbolInformation`.

### Length distribution

After the display_name join, 7418 locals extracted from sprefa's own
codebase. Length distribution:

| Length | Count |
|---:|---:|
| 2 | 526 |
| 3 | 1209 |
| 4 | 1658 |
| 5 | 619 |
| 6 | 292 |
| 7 | 398 |
| 8 | 235 |
| 9 | 168 |
| 10 | 188 |
| 11 | 81 |
| 12 | 32 |
| 13 | 27 |
| 14 | 4 |
| 15+ | 39 |

Length 4 dominates (`name`, `path`, `file`, `rule`). The 10+ char range has
371 names, a substantial pool. Sample 10-char locals from sprefa src:
`alloc_rows`, `callee_sym`, `cond_edges`, `der_digest`, `dirty_rels`,
`group_list`, `head_binds`, `param_refs`, `path_by_id`, `repo_sinks`,
`seed_rules`, `to_extract`, `to_retract`, `unresolved`.

### `missing-type.dl` output

At thresholds (length >= 10, fns >= 5, files >= 3): **20 hits**, all real
identifiers, no noise. Top hits sorted by frequency:

| name | n_fns | n_files | interpretation |
|---|---:|---:|---|
| `label_span` | 96 | 98 | appears in nearly every fn; near-universal. The strongest single signal in the codebase. Suggests a missing fundamental type. |
| `foreground` | 13 | 13 | daemon-mode flag, possibly over-extracted |
| `type_diags` | 12 | 12 | typed diagnostic collection |
| `canon_files` | 12 | 12 | canonicalized-path collection |
| `source_rels` | 11 | 11 | **context-object pair with `source_rules`** |
| `derived_rels` | 7 | 7 | **context-object pair with `derived_rules`** |
| `derived_rules` | 7 | 7 | paired |
| `source_rules` | 8 | 8 | paired |
| `tick_label` | 9 | 9 | |
| `touches_cfg` | 9 | 9 | **triple with below two** |
| `touches_git` | 9 | 9 | triple |
| `touches_program` | 9 | 9 | triple |
| `watcher_start` | 9 | 9 | timestamp |
| `is_shutdown` | 8 | 8 | bool flag |
| `is_subscribe` | 8 | 8 | bool flag |
| `subscriber_stream` | 8 | 8 | stream concept |
| `snippet_queries` | 5 | 5 | |
| `where_bytes` | 6 | 6 | |
| `backoff_idx` | 5 | 5 | |
| `write_roots` | 5 | 5 | |

### Co-occurrence check (context-object validation)

The single-name missing-type recipe surfaces candidates; the context-object
smell requires the same names to co-occur in the same fns. Probed directly:

| cluster | per-name fns | fns with all members | fns with >= 2 |
|---|---:|---:|---:|
| `touches_cfg` + `touches_git` + `touches_program` | 9 / 9 / 9 | 3 | 7 |
| `derived_rels` + `derived_rules` | 7 / 7 | 3 | n/a |
| `source_rels` + `source_rules` | 11 / 8 | not probed | n/a |

The `touches_*` triple is the strongest context-object signal: 3 fns declare
all three booleans in the same scope, 7 declare at least two. The co-occurrence
is real but not universal — some fns use only one. The cluster is a candidate
for a `TouchMask` struct.

### What the recipe does and does not catch

**Catches:**
- Missing fundamental types (`label_span` universal; `subscriber_stream`).
- Co-occurring concept pairs/triples that want to be a struct (`touches_*`,
  `*_rules` + `*_rels`).
- Repeated collection patterns (`snippet_queries`, `canon_files`).

**Misses:**
- Concepts whose name varies across fns (`parser_state` vs `parse_state` vs
  `parser_ctx`). No fuzzy matching; would need embeddings or edit distance.
- Concepts where the local name is short but the type is missing. Filter
  discards names < 10 chars, so e.g. `ctx` repeated 50 times is invisible.
- Concepts declared as fields on existing types (correctly filtered, but
  also misses the "this should be a field on a new type" case).

### Build notes

Discovered mid-experiment: cargo builds were thrashing swap with 16GB RAM and
default parallelism. `RUSTFLAGS="-C debuginfo=0" cargo build -j 4` dropped a
full 399-crate clean build from "ongoing for an hour" to **39 seconds**.
Debuginfo generation is the main per-crate memory cost; cutting it makes
rustc use ~5x less memory. Worth keeping in the working-conventions skill.

### Next steps

1. **`context-object.dl` recipe.** Group fns by parameter-name multiset;
   flag clusters of >=5 fns sharing >=3 long-name params. Generalizes the
   ad-hoc co-occurrence probe above.
2. **`param-fan-out.dl` recipe.** Fns declaring >= N locals (parameter-count
   proxy). Already supported by `scip_local`; just needs the recipe.
3. **N-rater labeling.** Run the 20 hits through 5 cold-read subagents asking
   "is this a real missing type or noise?" Tune thresholds against Fleiss'
   kappa on the labels. This is the path from "20 candidates" to "validated
   refactoring recommendations."
4. **Fuzzy matching for name variants.** Cheap approach: cluster by name
   stem (longest common prefix >= 6 chars). Better approach: token-edit
   distance. Best approach: embeddings, but heavier infra.

### Reproducibility

- Code: `v5/src/scip_import.rs` (`ScipRows.locals`, `display_names()`,
  locals pass, 3 new unit tests).
- Wiring: `v5/src/engine.rs` (SCIP_RELS now 6 entries; `scip_local` declared
  in `scip_rel_decls`; refresh path in `refresh_scip_rels`).
- Recipe: `v5/examples/missing-type.dl`.
- Run: `SPREFA_SCIP_INDEX=index.scip ./v5/target/debug/dl v5/examples/missing-type.dl --root . --db /tmp/dl-mt.db --no-daemon`
