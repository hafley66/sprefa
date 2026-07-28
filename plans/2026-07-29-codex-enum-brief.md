# CODEX BRIEF: semicolon enum variants in rel decls (sol-class)

Ruling being landed (rulings.pl `enum_decl_in_rel` + `enum_variant_separator`
+ `decl_column_spelling`, read the exact rows first):

```prolog
rel body(page(view: view) ; redirect(to: text)).
```

Design authority: plans/2026-07-28-types-as-rels-verdict.md, the enum-shape
slot and "The DDL all three spellings generate" section. The lab already
proved all spellings expand to the SAME tables: one rel per variant
(`body_page`, `body_redirect`) plus a DERIVED tag view. Spelling (c), plain
rels, is the desugared form and is what the expansion produces.

## The centralized move (user directive: no new machinery where old exists)

The enum decl is SUGAR. It desugars to constructs the whole pipeline already
understands: N per-variant rel decls (colon-typed per wave 2) + one derived
tag-view rule. Requirements:

1. The term form RETAINS the sugared decl (round-trip G1 is `=@=` exact;
   print_dl must reproduce the semicolon text).
2. ONE shared expansion predicate (place it where analyze.pl and the
   conformance engine can both consult it; name it in the summary). Both the
   oracle engine and the tsv2 compile pipeline consume the EXPANSION — zero
   new constructs reach engine.pl's evaluator or lower.pl's statement
   emitters beyond what plain rels already exercise.
3. Registry rows + generated SYNTAX.md section via 1_emit_registry_docs.pl.
4. Variant columns use wave-2 colon types; variant rel names are
   `<rel>_<variant>` per the verdict's DDL section.

## Fixtures (the grade is the existing harness, nothing new)

Add conformance fixtures exercising: (a) enum decl parses, variant rows
write and read back through the tag view; (b) two variants of the same rel
coexist and the tag view unions them; (c) a variant name colliding with an
existing rel name is a named refusal. Sweep + roundtrip + conformance are
the graders; report per-fixture movement (existing movement must be zero).

## Laws

Worktree /Users/chrishafley/projects/sprefa-codex-enum, branch
codex/enum-variants, base sha = the commit adding this brief (verify with
`git rev-parse HEAD` and `git log -1 --format=%s` naming this brief;
READ-ONLY git only — your sandbox cannot write git metadata; leave the tree
dirty, the coordinator commits). Descriptive variables; no em dashes;
banned words provenance, substrate, load-bearing, regime. Full conformance
at most 3 runs. If a piece cannot land within the laws, STOP it and name
the crack.

## Final summary shape

File list; the expansion predicate name and home; fixture count before and
after; conformance/roundtrip/sweep/plunit/tsv2/import-gate results;
per-fixture movement; cracks.
