# Catalog rows for same-named module relations

## Context

`v6/prolog/lower.pl:1431-1436` keyed `rel_module_decl/2` by relation name.
`first_per_key/3` retained one module assignment for every repeated local
name. `catalog_rel_rows/10` then emitted one catalog declaration row for the
single `RelPlans` entry. The text door merged the two declarations into one
physical relation plan and inferred `col1: text`.

`v6/prolog/compile/7_emit_ts_types.pl:61-64` and
`v6/prolog/compile/8_emit_rust_types.pl:61-64` render both `http_response`
and `httpResponse` as `HttpResponse`. Each emitter accepted the duplicate
identifier.

## Decisions

1. Catalog declarations retain one row per `(module hash, local relation
   name, arity)` occurrence. The physical relation plan remains one entry,
   because its table and boundary names use the relation name.
   - Rejected alternative: relation-name mangling in emitted storage. It
     changes the language-level relation naming contract.

2. `catalog_rel_plans/4` reads declaration groups ending in
   `rel_module_decl/2`, expands repeated names for declaration-row emission,
   and preserves the matching module hash. `catalog_rel_module_ids/3`
   resolves the hash through the existing integer module-id dictionary.
   - Rejected alternative: a composite text catalog key. `__rel.rel_id`
     remains the integer surrogate key.

3. Type emitters raise
   `unsupported_construct(type_name_collision(TypeName, Modules))` before
   rendering declarations. The payload identifies the rendered identifier
   and the module id plus local relation name for the first collision pair.
   - Rejected alternative: suffixes, prefixes, and mangling. Identifier
     spelling remains a language decision.

## Design

### Signatures and body shape

```prolog
catalog_rel_plans(+Decls, +RelPlans, -CatalogRelPlans, -CatalogRelModules).
catalog_rel_module_ids(+CatalogRelModules, +HashIdMap, -CatalogRelModulesWithIds).
check_type_name_collisions(+RelRows).
```

`catalog_rel_plans/4` walks the declaration sequence, captures each file's
`col_type/3` rows at its `module_decl/2` boundary, and expands only a
relation name that appears under multiple module hashes. Each expanded plan
has its own declared column list. `catalog_rel_rows/11` consumes the matching
module entry and writes that module's integer id into the relation and column
rows.

`check_type_name_collisions/1` maps every renderable relation row to
`TypeName-module(ModuleId, LocalName)`, sorts by `TypeName`, and throws when
two adjacent entries share the key.

### Instance timeline

1. The text door resolves module files leaf first and emits each file's
   `module_decl/2` and `rel_module_decl/2` rows.
2. Program planning retains one physical relation plan for the shared
   relation name.
3. Catalog declaration planning expands the repeated module declarations.
4. Catalog rows receive consecutive integer `rel_id` values. The existing
   physical-plane rows attach to the first declaration row.
5. Type rendering validates the catalog rows before output text is built.

### Stored identities and reads

`__rel` continues to use `rel_id INTEGER` as its key. Module hashes appear
once in module rows and resolve through `HashIdMap` to `module_id INTEGER`.
The duplicate declaration rows reference those integer ids. The emitted
storage relation remains keyed by its established relation name.

## Verification

Fixture files:

- `v6/dl/fixtures/catalog-two-module-collapse.dl6`
- `v6/dl/fixtures/catalog-two-module-collapse-a.dl6`
- `v6/dl/fixtures/catalog-two-module-collapse-b.dl6`

The entry fixture imports both modules. Its emitted catalog contains two
`item` relation rows: the A row owns `sku: text`; the B row owns `qty: int`.

The collision receipt constructs catalog rows for `http_response` and
`httpResponse`. Both type emitters throw
`unsupported_construct(type_name_collision('HttpResponse', ...))`.

Required gates run three times each:

```sh
cd v6/tsv2 && bash scripts/sweep.sh
just conformance
swipl -g go -t halt v6/prolog/ARCH.pl
```

## Staffing

One Codex lane. Worktree:
`.boop-worktrees/fix/catalog-two-module-collapse`. Base SHA:
`0b672fc11ef2d73478a72849c62d921074f460b4`. Owned files are
`v6/prolog/lower.pl`, both type emitters, the three fixture files, and this
plan document.
