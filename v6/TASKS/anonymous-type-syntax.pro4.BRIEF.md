# Anonymous Type Syntax

Implement `@anonymous-type-syntax` from `issues/anonymous-type-syntax/item.md` against current `main`.

Read first:

- `plans/2026-08-18-relational-type-schema-wrappers-and-literals.md`
- `issues/anonymous-type-syntax/item.md`, including its decision notes
- `v6/dl/fixtures/golden-flex.dl6`
- the current parser, printer, `1_expansion.pl`, generic expansion, enum expansion, and Tree-sitter DL6 grammar

Required first slice:

```prolog
product_type([field(Name, TypeExpr), ...])
sum_type([variant(Name, [field(Name, TypeExpr), ...]), ...])
```

Products and sums must parse anywhere `type_expr` is legal and reach parse, print, reparse term equality plus second-print byte equality. Sum payload fields remain named and accept complete type expressions. Empty products/sums receive named diagnostics.

Identity is assigned only after module resolution and concrete generic substitution:

```prolog
anonymous(OwnerSemanticTypeId, SitePath, SpecializedShape)
```

`SitePath` is the recursive member-name path plus wrapper/application argument ordinals. Unrelated declarations must not change it. This source syntax elides a name; the compiler later materializes an ordinary generated `type_decl`. Full runtime construction/storage belongs to `@anonymous-product-values` and `@anonymous-sum-values`.

Update the Prolog parser/printer and Tree-sitter grammar/node types together. Materialize or register anonymous sums early enough that enum context sees them. Preserve literal AST through generic substitution. Add named recursion outcomes consistent with the issue card. Generated visibility must use semantic kind/origin/reachability rather than `__` substring tests.

Do not add a general annotation system. Do not broaden arrow arity. Do not implement runtime product or sum construction in this card.

CI means parser/compiler/build/tests. Add focused fixtures for nested products, named sum payloads, generic specialization, module-qualified owner identity, printer fixpoint, CST shape, and cycle diagnostics. Run focused CI and one full compiler suite. Review the complete diff, commit with `Refs-Issue: @anonymous-type-syntax`, do not push. If the existing expansion order cannot support the identity law without a phase split, stop and report the exact call graph and smallest phase change before editing around it.
