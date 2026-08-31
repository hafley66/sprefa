# DL7 module system progress

## Current map

```text
project root + source paths
          |
          v
dl7_project(Root, Units)
          |
          +--> each unit lowers under module(file(CanonicalPath))
          |
          +--> filesystem directories lower to product nodes
          |
          +--> directory/file containment lowers to :/4 edges
          v
merged basement
          |
          v
existing checker -> comptime fixpoint -> runtime program
```

Source traversal uses the same `:/4` relation as every other edge:

```dl7
(<- (found_user ?UserType)
    (: accounts User ?UserType ?Index))
```

`accounts` is the filesystem-generated module node. `User` is the edge label.
`?UserType` and `?Index` are ordinary logic variables. No dot, path resolver,
import row, or export row participates.

## Surface rulings

| Surface | Meaning |
|---|---|
| `(: Name Target)` | lexical bind syntax that creates an owned edge |
| `(: Owner Label Target Ordinal)` | canonical edge relation in rule heads and bodies |
| `?Name` | logic variable |
| bare label in argument 2 of `:/4` | constant edge label |
| numbered filesystem segment | semantic label with author-order prefix removed |

Dot projection, `::`, punning, keyword arguments, private/export declarations,
and compound edge identities remain outside this wave.

## Implemented APIs

```prolog
load_dl7_project(+Root, +Paths, -Project, -Diagnostics).
lower_units(+Units, -ModuleBasements, -ModuleOrigins, -Diagnostics).
merge_module_basements(+ModuleBasements, +ModuleOrigins,
                       -Program, -Origins).
install_project_graph(+Project, +Basements0, +Origins0,
                      -Basements, -Origins, -Diagnostics).
compile_dl7_project(+Root, +Paths,
                    -CompilerRows, -RuntimeProgram, -Diagnostics).
```

The prelude still enters lowering as an ambient exporter. That mechanism only
makes prelude type constructors available to source units. User modules do not
receive implicit aliases to sibling modules.

## Receipts

| State | Receipt |
|---|---|
| done | separate source units and stable source-owned module identities |
| done | independent lowering and owner-preserving basement merge |
| done | synthetic prelude module |
| done | project root, directory, and file product nodes |
| done | deterministic filesystem `:/4` edges with dense per-owner ordinals |
| done | author-order prefix removal from semantic filesystem labels |
| done | cross-module rule traversal through an ordinary colon goal |
| removed | host `resolve_path/6` traversal |
| removed | host import alias, visibility, and module-cycle rows |

Commits for the current wave:

- `349de4645` makes room for filesystem graphing.
- `4148743e0` compiles filesystem modules through colon edges.

## Deferred mechanics

- A userland operation that allocates the next dense edge ordinal when a rule
  generates a new owned edge.
- Edge identity as a first-class value if edge metadata requires it.
- A source shorthand for chained traversal, if one is later selected.

## Gates

```text
SWI Prolog:  32 / 32
Tree-sitter:  1 / 1
```

This wave adds two deterministic module-system SWI tests and removes four
tests whose only subject was the deleted host resolver. Tree-sitter coverage
is unchanged because no source grammar was added.
