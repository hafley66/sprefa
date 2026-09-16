# Lane: two modules declaring the same rel name collapse to one catalog row

## Base
`git merge --ff-only 0b672fc1` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/catalog-two-module-collapse`.

## The bug, measured 2026-08-12 (probe, rc=0, silent)

Two modules each declaring `rel item` produce ONE catalog row. The column set
degrades to `col1: text` and one module's columns are lost. Swapping the `use`
lines changes nothing. A single-module control is correct.

```
input                              expected                 actual
module a: rel item(sku: text).     __rel (1,"item",mod=a)   __rel (1,"item")
module b: rel item(qty: int).      __rel (2,"item",mod=b)     cols: col1 TEXT
```

`grep -rln 'use "' --include=*.dl6 .` returns **0 files**. No fixture in the
tree compiles two modules. That is why this survived.

## Second bug, same file family

`type_name/2` is not injective. Identical 4 lines at
`compile/7_emit_ts_types.pl:61-64` and `compile/8_emit_rust_types.pl:61-64`.

```
http_response -> split "_" -> [http, response] -> HttpResponse
httpResponse  -> split "_" -> [httpResponse]   -> HttpResponse
```

Two catalog rows, one emitted identifier. The second `export interface
HttpResponse` silently overwrites the first. rc=0.

## Scope, exactly

1. Trace the two-module collapse to its line. Report the throw or merge site.
2. Fix it so both modules keep their own row and their own columns.
3. Add a fixture that compiles two modules. There is none today.
4. For `type_name/2`: add COLLISION DETECTION ONLY. Two rels rendering to one
   identifier must raise a named compile error. Do NOT invent a renaming scheme
   (prefix, numeric suffix, mangling). That is a language decision the user owns
   and has not ruled on.

## Anchors
- `v6/prolog/lower.pl:836-839` `__rel.rel_id`, `:844` the key
- `v6/prolog/lower.pl:1389` module row (name + hash), `:1395` `rel_module_map/3`
- `v6/prolog/lower.pl:1885` `__str` dictionary, UNIQUE, on by default
- `v6/prolog/compile.pl:165` `default_intern_mode(dict)`
- `v6/prolog/compile/7_emit_ts_types.pl:17` and `8_emit_rust_types.pl:17` both
  DISCARD `_ModuleId`

## Gates, three runs each, never from the whole gate
```
cd v6/tsv2 && bash scripts/sweep.sh     # RUN identical / wrong / rejection
just conformance                        # 392 PASS / 0 FAIL
swipl -g go -t halt v6/prolog/ARCH.pl   # 7 PASS
```
`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the real gate,
and it is stale by 9 rows. Do not chase anything listed there.

## Files you own
`v6/prolog/lower.pl`, `v6/prolog/compile/7_emit_ts_types.pl`,
`v6/prolog/compile/8_emit_rust_types.pl`, new fixtures under `v6/dl/fixtures/`,
plan doc `plans/2026-08-12-catalog-two-module-collapse.md`.

## Files you must NOT touch
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/**`, `v6/boop/**`,
`v6/prolog/compile/parse_dl.pl`, `parse_dl_dcg.pl`, any `Cargo.toml`,
`v6/justfile`. Other lanes own those.

## Laws
- Doubt yourself before asserting. Cite the throw site or say you did not find it.
- A compiler error for an unbuilt construct is "TODO", never "refusal", in prose.
- Surrogate keys: INTEGER ids, natural TEXT keys once in a dictionary with
  UNIQUE. A composite TEXT PRIMARY KEY is a DEFECT. Read
  `.claude/skills/sql-relational-design` first.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.
- Language design happens with the user in the room. Report forks, do not rule.

## Report
The throw site, the fix, the new fixture name, and the three gate outputs.
