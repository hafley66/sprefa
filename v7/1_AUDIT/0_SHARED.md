# DL6 to DL7 predicate audit protocol

Read the assigned DL6 files and their direct callers, callees, tests, and
immediately preceding predicate comments. This is a read-only code audit. The
only authored change is the assigned report file.

V7 assumptions:

- source extension: `.dl7`
- implementation: SWI-Prolog
- source syntax: Lisp-shaped cons trees with explicit `?Variable` spelling
- `:` is the kernel binder form
- ordinary parenthesized forms are applications
- scopes, products, namespaces, and relation/type nodes are represented through
  owner/name/target/ordinal edges where the existing semantics permit it
- compile-time and runtime retain relational fixpoints
- `sprefa-engine-rs` remains in place; V7 may revise the compiler-side IR while
  preserving the execution-plan fields the engine consumes
- no implicit declaration or callable inference
- DL6 source compatibility and a maintained DL6 frontend are outside scope

For every predicate that materially participates in the assigned slice, emit
this report block:

```prolog
% File: path:line
% Existing comment: exact summary of the comment immediately above, or `none`
% Signature: predicate_name(Arguments...)
% Called by: direct predicate names and entry points
% Calls: direct semantic dependencies
% Tests: exact test/fixture paths
% V7 class: extract | adapt | oracle | drop
% Parser coupling: none | term-shape | token/CST | surface-policy
% Preserved law: one sentence describing observable behavior
% DL7 seam: expected input and output term shapes
```

Classification meanings:

- `extract`: predicate body can move with module/import cleanup
- `adapt`: semantic law stays while input term shape or scope threading changes
- `oracle`: preserve tests, fixtures, or contract while replacing code
- `drop`: DL6-only syntax or compatibility behavior

Finish with:

1. predicate counts by class
2. exact canonical term shapes entering and leaving the slice
3. hidden dynamic predicates, global flags, assertion order, cuts, tabling, or
   module-state dependencies
4. the smallest self-contained extraction boundary
5. the first dependency that forces adaptation instead of extraction
6. unresolved questions requiring a V7 language ruling

Do not modify DL6 files. Do not run full test suites. Commit only the assigned
report with subject `v7 audit: <slice>`.
