# Lane: resolve type_name collisions with a module prefix

## Base
`git merge --ff-only 506ab5d8` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/catalog-two-module-collapse` (already exists,
already carries the half you are finishing).

## What already landed, commit `506ab5d8`

The catalog no longer collapses two same-named rels from two modules. Keys are
now `(module hash, local name, arity)`. Both type renderers gained a collision
CHECK that throws `unsupported_construct(type_name_collision(...))`.

## What you are finishing

The user's decision, verbatim: **module prefix**. Not a numeric suffix, not an
error. Replace the throw with a prefix.

`type_name/2` is not injective. Identical 4 lines at
`compile/7_emit_ts_types.pl:61-64` and `compile/8_emit_rust_types.pl:61-64`:

```
http_response -> split "_" -> [http, response] -> HttpResponse
httpResponse  -> split "_" -> [httpResponse]   -> HttpResponse
```

Two catalog rows, one emitted identifier, second silently overwrites the first.

## Scope

1. On collision, prefix the emitted identifier with the module. The catalog
   already carries what you need: `lower.pl:1389` emits a module row with name
   and hash, `rel_module_map/3` at `:1395`. Both renderers currently DISCARD
   `_ModuleId` at their line 17.
2. Prefix ONLY on collision. A rel with no collision keeps its bare name, so
   the 392-fixture corpus stays byte-identical.
3. The two colliding rels in the fixture must produce two distinct interfaces
   in BOTH targets, TypeScript and Rust.
4. Add a fixture that collides. Keep the existing collision fixtures.

## The user's framing, carry it

"modules have file paths so how are you gonna have a collision not solved by
local symbol table parsing". Two rels in two modules are two symbols. The
prefix exists for the case where one target's identifier space cannot hold both,
never as the identity. Identity stays the integer id.

## Anchors
- `v6/prolog/lower.pl:1389` module row, `:1395` `rel_module_map/3`
- `v6/prolog/lower.pl:836-839` `__rel.rel_id`, `:844` the key
- `compile/7_emit_ts_types.pl:17` and `8_emit_rust_types.pl:17` discard `_ModuleId`
- `plans/2026-08-12-catalog-two-module-collapse.md` the prior half

## Gates, three runs each, never from the whole gate
```
cd v6/tsv2 && bash scripts/sweep.sh    # RUN identical must not drop
just conformance                       # 392 PASS / 0 FAIL
swipl -g go -t halt v6/prolog/ARCH.pl
bash v6/sprefa-engine-rs/grade.sh      # must not fall below 280
```
`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the real gate and
is stale by 9 rows. Do not chase anything in it.

## Files you own
`v6/prolog/compile/7_emit_ts_types.pl`, `compile/8_emit_rust_types.pl`,
`v6/prolog/lower.pl`, fixtures under `v6/dl/fixtures/`, and the plan doc
`plans/2026-08-12-catalog-two-module-collapse.md`.

## Files you must NOT touch
`v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/**`, `v6/boop/**`, any
`Cargo.toml`, `v6/justfile`. Other lanes own those.

## COMMIT YOUR WORK
Six lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING. Commit on the branch before you exit. An uncommitted tree is an
undelivered lane.

## Laws
- Language design belongs to the user. The prefix is already decided; implement
  it, do not re-open it.
- Comments state only constraints the code cannot show. No dates, no narrative.
- A compiler error for an unbuilt construct is "TODO", never "refusal".
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Report
The prefix spelling you chose, the two emitted interfaces side by side, and the
four gate outputs.
