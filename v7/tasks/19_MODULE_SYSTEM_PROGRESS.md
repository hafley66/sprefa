# DL7 module system progress

## Current map

```text
source files
    │ one dl7_unit per file
    ▼
stable module owners
    │ lower independently
    ├───────────────┐
    ▼               ▼
prelude basement    source basement
    │ callable rows │ local rows only
    └──────┬────────┘
           │ ordinary alias edges
           ▼
      merged basement
           │
           ├── path traversal + proof
           ├── visible-name checks
           ├── module-cycle checks
           ▼
      existing checker
           ▼
      comptime fixpoint
           ▼
      runtime program
```

## Milestones

| # | State | Receipt |
|---:|---|---|
| 1 | done | `b844f6da2` separate-unit loader |
| 2 | done | `7c82520cb` source-stable module owners |
| 3 | done | `a429ef1a4` independent lowering wrappers |
| 4 | done | `a429ef1a4` owner-preserving basement merge |
| 5 | done | `86b81c064` synthetic prelude module |
| 6 | done | `4029fc865` segment traversal with exact proofs |
| 7 | done | `4029fc865` local and imported visibility diagnostics |
| 8 | done | `4029fc865` host-supplied module cycle checking |
| 9 | done | `4029fc865` `Option.some.value` sum/product fixture |
| 10 | open | prefix import, alias, and export syntax |

`cd0071d90` proves that an importer alias and exporter edge target the same
type identity. `86b81c064` supplies exporter relation signatures and return
edges while nested importer expressions lower, without copying those
declarations into the importer basement.

## Implemented APIs

```prolog
load_dl7_units(+Paths, -Units, -Diagnostics).
lower_units(+Units, -ModuleBasements, -ModuleOrigins, -Diagnostics).
lower_units_with_exporter(+ExporterUnit, +ImporterUnits,
                          -ModuleBasements, -ModuleOrigins, -Diagnostics).
install_module_aliases(+Exporter, +Importers,
                       +Basements0, +Origins0, -Basements, -Origins).
merge_module_basements(+ModuleBasements, +ModuleOrigins,
                       -Program, -Origins).
resolve_path(+StartOwner, +Segments, +Edges,
             -Target, -Proof, -Diagnostics).
check_visible_name_collisions(+LocalEdges, +Imports, -Diagnostics).
check_module_cycles(+Imports, -Diagnostics).
compile_units(+Units, -CompiledUnit, -Diagnostics).
```

## Open surface work

```text
prefix syntax decision
    -> reader rows for imports and exports
    -> source positions on import rows
    -> resolve_module_graph orchestration
    -> dotted and aliased source fixtures
    -> positioned ambiguity and cycle diagnostics
```

No parser syntax was selected by the implementation branch.

## Gates

```text
SWI Prolog:  34 / 34
Tree-sitter:  1 / 1
```

CI coverage changed by adding four deterministic module-system SWI tests.
Tree-sitter coverage is unchanged because milestone 10 remains open.
