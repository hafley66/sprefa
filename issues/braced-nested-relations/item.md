---
created: 2026-08-23
updated: 2026-08-23
type: feature
status: done
priority: normal
labels:
- area:dl6
- component:parser
- intent:syntax
assignee: codex
closed: 2026-08-23
closed_by: codex
---

# Add brace syntax for nested relation declarations

## Description

## Context

DL6 currently expresses relation containment with dotted declaration paths:

```dl6
rel A(a: int).
rel A.B(b: int).
rel A.B.C(c: int).
```

`parse_dl_dcg.pl` flattens each path with `module_path_name/2` and retains
`rel_path_decl(Ref, Segments)`. `0_dot_expand.pl` then adds the implicit parent
reference, rewrites rule calls, and leaves the catalog and storage pipeline one
canonical representation.

Add a brace spelling for the same declaration graph:

```dl6
rel A(a: int) {
    rel B(b: int) {
        rel C(c: int).
    }.
}.
```

The parser output for these programs must be variant-equivalent before path
collision resolution and term-equivalent afterward. No brace-specific semantic
term may reach expansion or lowering.

Relevant implementation seams:

- `v6/prolog/compile/parse_dl_dcg.pl:351-370` owns statement collection and source sites.
- `v6/prolog/compile/parse_dl_dcg.pl:557-620` owns relation declarations.
- `v6/prolog/compile/parse_dl_dcg.pl:776-779` owns path flattening and `rel_path_decl/2`.
- `v6/prolog/0_dot_expand.pl:310-430` owns nested parent capture.
- `v6/prolog/print_dl.pl:386-390` already prints retained paths in dotted form.
- `v6/prolog/compile/test/plunit_tests.pl:7062-7435` owns dotted-path and parent-capture tests.

## Surface Contract

1. A concrete relation declaration may end with `.` or with a declaration block followed by `.`.
2. The parent declaration exists independently of its children. Its signature and modifiers have their current meaning.
3. A block contains zero or more `rel` declarations. Child blocks recurse to arbitrary parser depth.
4. A child path is the enclosing path concatenated with the child's authored path. Inside `A`, `rel B.C().` denotes `A.B.C`.
5. Rules, queries, imports, uses, interfaces, and other statement kinds remain outside relation blocks.
6. Rules refer to nested relations by their full dotted path, such as `A.B.C(X)`.
7. The first slice permits concrete relation owners and children, including zero-column declarations, modifiers, keys, and relation arrows already accepted by the concrete declaration branch.
8. Enum declarations, generic relation templates, and arrival/host declarations do not own or appear as children in this slice. A brace after one receives a named refusal or a parse error pinned by a test.
9. The canonical printer emits dotted declarations. Preserving brace layout requires syntax provenance that the semantic program currently discards and stays outside this issue.

Canonical lowering:

```text
rel A(a: int) { rel B(b: int) { rel C(c: int). }. }.

  parse
    col_type(A/1, a, int)
    col_type(A__B/1, b, int)
    rel_path_decl(A__B/1, [A, B])
    col_type(A__B__C/1, c, int)
    rel_path_decl(A__B__C/1, [A, B, C])

  existing nested-parent expansion
    A(a)
    A__B(parent: A, b)
    A__B__C(parent: A__B, c)
```

## Parser Signatures and Body Shape

```prolog
%! rel_stmt(-Decls)// is semidet.
%  Preserve the public statement parser and delegate with an empty path prefix.

%! rel_stmt_in(+Prefix, -Decls, -DeclSites)// is semidet.
%  Parse one concrete rel declaration, prepend Prefix to its local dotted path,
%  emit the existing declaration terms, then parse either `.` or a child block.

%! rel_decl_end(+Path, -ChildDecls, -ChildSites)// is semidet.
%  On `.`, return empty children. On `{`, parse declaration-only children under
%  Path, consume `}.`, and concatenate children in source/dependency order.

%! nested_rel_stmts(+ParentPath, -Decls, -DeclSites)// is semidet.
%  Repeatedly parse `rel_stmt_in/3` until `}`. Reject every other statement kind
%  at its own token.
```

`DeclSites` carries one `decl_site(RemainingInput, OwnDecls)` per authored
relation. `statements//3` records these only after the enclosing parse succeeds,
then attaches the flattened declaration list to `prog/2` or `program/3`. This
keeps `parse_dl_line_for_reason/2` pointed at the child declaration rather than
the opening line of its outer parent.

## Instance Timeline

1. Parser prepass enters each block recursively and records column order under the same flattened name used by dotted syntax.
2. The real parse produces ordinary declaration terms and per-declaration source sites.
3. `resolve_module_path_collisions/2` applies the existing mangle/digest policy.
4. `normalize_relation_value_decls/2` sees the same declarations as the dotted program.
5. `expand_nested_parent_refs/4` processes shallow paths before deep paths and adds parent identities.
6. Catalog, SQLite, TS, Rust, JSON Schema, and OpenAPI consumers receive the existing canonical relation plans.

Brace scope has source-parse lifetime only. It creates no compiler-fixpoint or runtime instance.

## Storage, Reads, Writes, and Uniqueness

- Storage additions: zero tables, columns, catalog row kinds, or runtime fields.
- Compiler IR additions: zero persistent terms. `rel_path_decl/2` remains the path carrier.
- Parse-time state: the existing two parser passes reset and repopulate column-order and source-statement facts.
- Path uniqueness and flat-name collision handling remain owned by `resolve_module_path_collisions/2` and existing declaration validation after flattening.
- A mixed dotted/brace duplicate reaches the same existing duplicate or arity checks as two dotted declarations.
- Parent identity uniqueness remains the existing implicit leading relation reference and authored key-position shift.

## Implementation Sequence

1. Add fail-first parser tests proving three-level brace input currently fails and defining exact dotted equivalence.
2. Split the concrete relation parser into declaration-core and declaration-terminator productions without changing the emitted terms for dotted input.
3. Add prefix-aware recursive child parsing and declaration-only block refusal behavior.
4. Preserve one source location per nested declaration.
5. Add parser, expansion, collision, zero-column, modifier/key, and diagnostic tests.
6. Add a roundtrip receipt showing brace input canonicalizes to dots and reparses to the same program.
7. Compile equivalent brace and dotted fixtures and compare lowered plans and target artifacts byte-for-byte.

Brace lowering remains in the parser. The expanded deep-reference scope also touches qualified relation/type resolution and query preparation so every authored dotted path reaches the same canonical flat identity.

## Decisions

- Brace nesting is parser sugar over the landed dotted-path relation model.
- Blocks provide declaration path scope only. They provide no rule-variable scope or implicit qualification for rules.
- Dotted syntax remains accepted and remains the canonical printer output.
- A period follows every closing relation block: `}.`.
- Concrete relation declarations form the initial owner/child set.
- Generic-template ownership, enum ownership, arrival ownership, and brace-preserving formatting remain separate syntax work.

Rejected alternatives:

- A new nested-relation AST term duplicates `rel_path_decl/2` and sends a second representation through every consumer.
- Implicitly qualifying rules inside blocks adds lexical rule scope and name-resolution behavior to a declaration-sugar issue.
- Reprinting braces from semantic paths invents author layout after comments and grouping boundaries have been erased.

### 2026-08-24T02:27:03Z · @codex

2026-08-23 scope expansion: verify deep dotted paths in every relation and type reference position, including declarations, rule heads and bodies, column types, constructor applications, relation-valued terms, direct compiler-relation annotation applications, and canonical printing. The superseded `is` surface is outside this feature.

### 2026-08-24T03:03:37Z · @codex

Deep compiler-relation annotations use the ordinary colon type position, for example rel holder(value: namespace.types.identity(int)). The legacy is clause receives no brace or deep-path extension in this issue.



## Acceptance Criteria

- [x] `rel A() { rel B() { rel C(). }. }.` parses successfully.
- [x] The three-level brace program and its dotted spelling produce variant-equivalent parsed programs.
- [x] Expansion gives `B` a leading `parent: A` reference and `C` a leading `parent: A.B` reference.
- [x] Zero-column children retain one implicit parent column.
- [x] Child modifiers, authored keys, and relation arrows retain dotted-syntax behavior.
- [x] A dotted local child path appends to its enclosing path.
- [x] Flat-name digest collisions produce the same names for equivalent brace and dotted programs.
- [x] Rules outside the block can read and contribute to the nested relation through its full dotted path.
- [x] A child diagnostic reports the child's source line and column.
- [x] Non-relation statements inside a block fail at the contained statement.
- [x] Unsupported owner/child declaration forms have pinned failures.
- [x] Printing a parsed brace program emits canonical dotted declarations and reparses to a variant of the same program.
- [x] Equivalent brace and dotted fixtures produce byte-identical lowered plans and TS, Rust, JSON Schema, and OpenAPI artifacts.
- [x] Existing dotted-path tests remain unchanged and green.
- [x] No brace-specific term survives parsing.

## Verification

Focused tests:

```bash
cd v6/prolog/compile
swipl -q -l test/run_plunit.pl -g "run_tests(module_path_decls)" -t halt
swipl -q -l test/run_plunit.pl -g "run_tests(braced_nested_relations)" -t halt
```

Project gates:

```bash
cd v6
just plunit
just roundtrip
just conformance
just typegen-golden
```

CI coverage change: add parser, source-location, canonical-roundtrip, expansion-equivalence, and target-equivalence coverage. Runtime execution coverage remains unchanged because both spellings share one lowered plan.

## Tests Run

- `braced_nested_relations`: 26 passed, 0 failed.
- Relevant deep-path regression units (`module_path_decls`,
  `annotation_surface`, `anonymous_type_syntax`, `compiler_relations`,
  `query_order_tail`, `hosts_wiring`, `declaration_query_parity`, and
  `mount_door`): 177 passed, 0 failed.
- Full PLUnit: 1,114 passed, 0 failed, 0 timed out.
- Typegen golden: holds.
- Conformance: 443 passed and 2 existing Unicode character-count fixtures
  failed (`reverse_reverses_characters`, `length_counts_characters`).
- Roundtrip: the feature branch and an archive of base
  `b37ca81cfd551e16314a1c2189e40ed3add445c0` produced the same 437/445 G1
  result, the same eight G1 failures, and the same G2 `ghcacher.dl6` parse
  failure at `use http.`.

## Implementation Notes

Implementation runs in the isolated `feature/braced-nested-relations` worktree created from `b37ca81cfd551e16314a1c2189e40ed3add445c0`.

## Staffing

- Implementation: isolated compiler worktree at `/private/tmp/sprefa-braced-nested-relations`.
- Base SHA: `b37ca81cfd551e16314a1c2189e40ed3add445c0`.
- Focused parser suite budget: 60 seconds.
- Full PLUnit budget: repository default 600 seconds.
- Roundtrip, conformance, and typegen gates use their repository-configured budgets.

## Resolution

### 2026-08-24T03:05:33Z · @codex

Brace declarations and deep dotted relation/type references compile through the existing flattened relation identities. All acceptance items are covered by compiler tests.
