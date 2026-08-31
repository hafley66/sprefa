# DL7 filesystem products and colon traversal

## Goal

Compile a project filesystem into the same node and edge graph used by DL7
types, then let ordinary Datalog rules traverse that graph through `:/4`.

The issuectl epic is `dl7-module-system`. Its first task is
`dl7-module-parity`.

## Decisions

1. A source file has the stable owner `module(file(CanonicalPath))`.
2. A project root and each containing directory have the owner
   `module(directory(CanonicalPath))`.
3. Every file and directory module is a product node.
4. Filesystem containment is an ordinary edge:
   `:(ContainingModule, SegmentLabel, ChildModule, Ordinal)`.
5. Numeric author-order prefixes remain in paths and identities but are
   removed from semantic segment labels.
6. A source module owns its top-level binds through the existing edge model.
7. `:/2` remains lexical bind syntax.
8. `:/4` is the canonical edge relation usable in rule heads and bodies.
9. A bare identifier in argument 2 of `:/4` is a constant edge label. Other
   positions retain normal expression lowering.
10. Cross-module access is an explicit relational join with variables.
11. The existing dependency checker owns positive recursion, negation, and
    aggregate stratification after all project rows merge.
12. User modules receive no implicit sibling aliases.
13. Dot syntax, `::`, import/export syntax, punning, keyword arguments, and
    first-class edge identity remain deferred.

## Type signatures

```prolog
load_dl7_project(+Root, +Paths, -Project, -Diagnostics).

install_project_graph(+Project, +Basements0, +Origins0,
                      -Basements, -Origins, -Diagnostics).

compile_dl7_project(+Root, +Paths,
                    -CompilerRows, -RuntimeProgram, -Diagnostics).
```

The reader result is:

```prolog
dl7_project(CanonicalRoot, Units)
```

Each unit keeps its own canonical file origin and content hash.

## Instance timeline

```text
Root + Paths
    -> canonicalize root and files
    -> parse one immutable unit per file
    -> lower each unit under its file module owner
    -> derive root and directory product nodes
    -> derive deterministic containment edges
    -> merge all basements and source origins
    -> check relation signatures and dependencies
    -> run the comptime fixpoint
    -> retain the checked runtime program
```

## Storage, reads, writes, and uniqueness

- Canonical paths identify filesystem module nodes.
- Content hashes identify source revisions without changing module identity.
- Filesystem edges are unique by `(Owner, Label)` and `(Owner, Ordinal)` under
  existing checker rules.
- Ordinals are dense and deterministic after sorting children by semantic
  label and canonical identity.
- A cross-module colon goal reads existing `:/4` rows.
- Userland generated edges use the same `:/4` relation. Dense ordinal
  allocation for generated edges is deferred because it requires a relational
  operation over the owner's existing edge set.
- Physical table and symbol names remain emitter decisions.

## Source projection

Given:

```text
project/
  0_accounts.dl7
  1_consumer.dl7
```

The filesystem graph contains:

```prolog
product(ProjectRoot).
product(AccountsModule).
product(ConsumerModule).
:(ProjectRoot, accounts, AccountsModule, 0).
:(ProjectRoot, consumer, ConsumerModule, 1).
```

The accounts file contains:

```dl7
(: User
   (* (: id int)
      (: name text)))
```

The consumer file can query the module edge and then its type edge:

```dl7
(<- (found_user ?UserType)
    (: accounts User ?UserType ?Index))
```

The module owner is inferred by ordinary lexical resolution of `accounts`.
The rule body then joins the `User` edge owned by that module. The resulting
`?UserType` is the existing type node identity.

## Removed parallel model

The previous branch introduced host predicates for path lists, import rows,
visibility precedence, and import-cycle checking. They had no production
caller after colon traversal became the module model. Their module and four
direct tests are removed in this wave.

Prelude exposure remains in `0a_module_lowerer.pl`. It supplies ambient type
constructors while lowering source expressions and does not create sibling
module imports.

## Verification

1. A virtual nested path proves root, directory, and file products plus exact
   dense ordinals.
2. A two-file project proves filesystem edges, source-owned type edges, and a
   derived cross-module fact through a colon body goal.
3. The complete V7 SWI suite and Tree-sitter corpus run before merge.

## Deferred work

- Generated-edge ordinal allocation.
- First-class edge identity for metadata on edges.
- Optional chained traversal syntax.
- Module surface controls if a later use case requires them.
