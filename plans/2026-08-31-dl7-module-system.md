# DL7 module ownership and path resolution

## Context

PR 602 established the V7 reader, one-module lowering, relational expression
flow, and userland type algebra. `compile_dl7/4` in
`v7/src/2_comptime/2_compiler.pl` still concatenates the prelude and one user
file into one reader unit. `lower_datalog/4` then creates one
`module(unit(Origin, ContentIdentity))` owner for every bind in that combined
unit.

The DL6 donor implementation lives in `v6/prolog/use_resolve.pl`,
`v6/prolog/0_dot_expand.pl`, and `v6/prolog/executor_modules.pl`. The completed
audit at `v7/audit/results/4_SCOPE.md` identifies its self-contained kernel as
path collection, collision checking, scope-tree construction, and path
traversal. DL6 flattened semantic names with `__`; V7 already has structured
owner/name/target edges and can postpone target-specific spelling until layout
or emission.

The issuectl epic is `dl7-module-system`; its first task is
`dl7-module-parity`.

## Decisions

1. Every source file receives one stable module owner derived from its
   canonical source identity. Content hashes remain revision data and do not
   rename the declaring module.
2. Modules, scopes, products, relations, and sums remain graph nodes. A module
   name is an ordinary `:/4` edge from its containing module.
3. A brace or nested bind contributes a name prefix only. No implicit parent
   column or key position is generated.
4. Qualified lookup walks ordinary edges one segment at a time:
   `Owner -> Name -> Target -> NextName -> Target`.
5. An imported alias is an ordinary edge in the importing module whose target
   is the existing exporting module or exported node. Imported declarations
   retain their declaring owner and identity.
6. Local edges resolve before imported aliases. Two visible candidates for one
   unresolved local name produce a positioned ambiguity diagnostic.
7. Module dependency cycles are checked before alias edges are installed.
   Positive rule recursion inside a module remains governed by the Datalog
   dependency checker.
8. Structured semantic identities survive through checking and comptime.
   Target emitters may project them to escaped or `__`-joined names.
9. The compiler-owned prelude remains one synthetic module initially. Every
   authored module receives its prelude edge set through the same resolver
   input rather than source-text concatenation.
10. Dot projection returns an edge target. Sum alternatives and product fields
    therefore use the same traversal. A sum alternative may target a product,
    allowing `Option.some.value` to traverse two ordinary edges.
11. Source files may be loaded and compiled as separate units before import
    surface syntax is selected. This creates an executable host API and tests
    for ownership, merging, resolution, and diagnostics.
12. Node source closure and comptime fixpoint closure are separate timestamps.
    Their user-visible relation shape remains a separate decision.

<!-- todo(decision): Select prefix import, alias, and export spelling without adding a second declaration language. -->

<!-- todo(decision): Select whether `::` exposes an edge identity while `.` projects its target. -->

<!-- todo(decision): Select the user-visible rows for source closure and comptime fixpoint closure. -->

## Type signatures

```prolog
load_dl7_units(+Paths, -Units, -Diagnostics).

compile_units(+Units, -CompiledUnit, -Diagnostics).

lower_units(+Units, -ModuleBasements, -Origins, -Diagnostics).

lower_units_with_exporter(+ExporterUnit, +ImporterUnits,
                          -ModuleBasements, -ModuleOrigins, -Diagnostics).

merge_module_basements(+ModuleBasements, +ModuleOrigins,
                       -Program, -Origins).

resolve_path(+StartOwner, +Segments, +Edges,
             -Target, -ResolutionProof, -Diagnostics).

check_visible_name_collisions(+LocalEdges, +ImportSpecs, -Diagnostics).

check_module_cycles(+ImportSpecs, -Diagnostics).
```

The remaining surface-integration signatures are:

```prolog
resolve_module_graph(+Modules, +ImportSpecs,
                     -AliasEdges, -Dependencies, -Diagnostics).
```

The initial public entrypoint remains compatible:

```prolog
compile_dl7(+EntryPath, -CompilerRows, -RuntimeProgram, -Diagnostics).
```

Its body loads the prelude module plus the entry module and delegates to
`compile_units/3`.

## Instance timelines

### File loading

```text
canonical path
    -> read one file
    -> parse one dl7_unit
    -> stable module owner from canonical path
    -> content hash retained as revision metadata
```

### Module compilation

```text
separate units
    -> lower each unit under its own owner
    -> collect module names, local edges, import specifications, and exports
    -> check dependency cycles and visible-name collisions
    -> install alias edges targeting existing nodes
    -> merge graph rows, declarations, facts, and rules
    -> run the existing checker and comptime fixpoint
```

### Qualified lookup

```text
StartModule + [Alias, Type, Variant, Field]
    -> StartModule --Alias--> ExportModule
    -> ExportModule --Type--> TypeNode
    -> TypeNode --Variant--> PayloadProduct
    -> PayloadProduct --Field--> FieldType
```

The resolution proof records every traversed edge so ambiguity and missing
segment diagnostics can point to the failing segment and both candidate
definitions.

## Storage, reads, writes, and uniqueness

- A module owner is unique by canonical source path within one compiler
  invocation and remains stable across content edits.
- Content hashes identify source revisions and reader caches.
- Local graph edges retain the existing keys `(Owner, Name)` and
  `(Owner, Index)`.
- Imported aliases add edges to the importing owner. Their targets are reused
  node identities from the exporting module.
- Import dependencies are unique by `(Importer, Imported, Alias)` before cycle
  checking.
- Export visibility tags identify an existing edge by
  `(Owner, Name, Target, Index)` and do not duplicate the edge relation.
- Path resolution reads only module nodes, visibility tags, and `:/4` edges.
- Runtime layout reads resolved node identities. SQL, Rust, and other emitters
  choose physical names independently.

## DL6 reuse map

| DL6 predicate or fixture | V7 treatment |
|---|---|
| `declared_path/3` | Replace flat declaration pairs with `:/4` edge reads. |
| `check_path_collisions/1` | Adapt to duplicate visible `(Owner, Name)` targets. |
| `decl_scope_tree/2` | Replace the temporary trie with graph traversal over owners. |
| `resolve_path/3`, `descend/3` | Preserve segment-by-segment lookup and proof order. |
| `collect_all/8` | Replace text splicing with separate `dl7_unit` loading. |
| `mount_decl/4`, `module_edge_decl/4` | Represent mounts and aliases as module-targeting edges plus provenance. |
| `resolve_rel_path_rule/3` | Resolve prefix path forms before the existing expression lowerer. |
| `resolve_qualified_type/3` | Use the same resolver in type and relation positions. |
| `resolve_enum_arm_term/3` | Use ordinary sum-alternative edges. |
| `scip_namespaces.test.pl` | Preserve declaring identity and collision oracles; drop flat-name assertions. |
| `7_module_path*.pl` | Port alias, wrapper-position, unresolved-path, and element-position cases. |
| `4_braced_nested_relations.test.pl` | Port deep path equivalence without the DL6 brace surface. |

## Ten milestones

1. Add a numbered module loader that returns separate `dl7_unit` values.
2. Make module owner identity depend on canonical source identity rather than
   content identity.
3. Lower a list of units independently and retain unit-to-owner provenance.
4. Merge independently lowered graphs while preserving relation keys and
   source origins.
5. Replace prelude text concatenation with a synthetic prelude unit.
6. Add edge-based `resolve_path/6` with exact traversal proofs.
7. Add duplicate local-name and ambiguous visible-name diagnostics.
8. Add dependency-cycle checking over explicit host-supplied import rows.
9. Prove sum-alternative and product-field traversal in one consolidated
   module fixture.
10. Connect the selected prefix import/export syntax and port the DL6 alias and
    cycle oracles.

Milestones 1 through 9 do not depend on the unresolved surface spelling.

## Implementation status

Milestones 1 through 9 are implemented on `feature/dl7-module-system`.
Milestone 10 remains open because prefix import, alias, and export spelling is
still a user decision. The executable receipts are listed in
`v7/tasks/19_MODULE_SYSTEM_PROGRESS.md`.

## Verification

- One fixture loads two files with distinct module owners and equal local
  names without identity collapse.
- An alias resolves to the exporting module's existing target node.
- `Option.some.value` traverses sum and product edges through one resolver.
- Missing segments report the source node and failing segment index.
- Ambiguous visible names report both candidate owner/name edges.
- Import cycles report the complete canonical module path.
- Existing V7 SWI tests remain green.
- Existing V7 Tree-sitter corpus remains green until milestone 10 adds syntax.
- `extract --resolve --family call` records the migrated DL6 predicate seams.

## Staffing

- Branch: `feature/dl7-module-system`.
- Worktree: `.boop-worktrees/feature/dl7-module-system`.
- Base: merged `origin/main` at `7961b9efd`.
- High-reasoning lane owns graph identity, merge semantics, and final review.
- Medium lanes may implement isolated loader, cycle-checker, or fixture tasks
  after signatures are committed.
- Test budget: focused SWI after each milestone, complete V7 SWI and
  Tree-sitter gates before PR.
