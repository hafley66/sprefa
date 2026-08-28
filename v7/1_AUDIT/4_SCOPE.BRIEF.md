# Slice 4: scope, modules, imports, nesting, and projection

Read `0_SHARED.md` first.

Primary files:

- `v6/prolog/use_resolve.pl`
- `v6/prolog/0_dot_expand.pl`
- `v6/prolog/0_annotation_expand.pl`
- `v6/prolog/compile/test/scip_namespaces.test.pl`
- nested relation and module-path fixtures/tests

Trace how owner, scope, local name, qualified name, relation identity, and dot
projection are represented. Find every place that assumes DL6 braces, dotted
surface names, implicit parents, capitalization, or declaration order.

Write `v7/1_AUDIT/results/4_SCOPE.md`.
