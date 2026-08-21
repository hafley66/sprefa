# Read-only phase review for applicative type annotations

Review `plans/2026-08-19-applicative-type-annotations.md` against current main.

Deliver exact files, predicates, and phase positions for:

- retaining `annotated_type(Type, Applications)` through parsing, imports, generics, and anonymous type minting;
- invoking the existing compiler-relation evaluator with implicit `Target` and one `return:type`;
- retaining site and sequence evidence without runtime DDL/facts;
- feeding `key(Target) -> Target` evidence into current key normalization;
- keeping option, enum, relation values, semantic IDs, and type emitters correct.

Find semantic collisions or existing machinery that should be reused. Do not edit code. Write `REPORT.md`, then run `boop tell-parent --kind completion --body "type-annotation-phase-review done"`.
