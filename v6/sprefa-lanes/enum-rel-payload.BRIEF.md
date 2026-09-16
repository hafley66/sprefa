# fix/enum-rel-payload: rel-typed variant payload fields in payload enums

## Ruled by user 2026-08-10: dispatch. A payload-enum variant field may name a
## declared relation as its type, same as any plain column already can.

## The defect (traced, receipts in hand)
```
rel tree(tree_id: int, name: text) key(1).
rel grade(ripe(subject: tree) ; bruised(reason: text)).
```
compile_dl6 throws `column_type_unknown` (0_type_plane.pl:128).

Cause chain, each link verified 2026-08-10:
1. Parse accepts it: enum variant fields take bare idents in type position
   (parse_dl.pl:769-781).
2. Enum expansion emits `col_type(grade_ripe, subject, tree)`
   (0_enum_expand.pl:185, variant_col_type).
3. Storage resolution needs `type_def(tree,...)`; those are synthesized for
   relation-valued column types by normalize_relation_value_decls
   (parse_dl.pl:988-998), whose collector `declared_column_type_name`
   (parse_dl.pl, three clauses: col_type, sh_decl, bind_decl) never walks
   `enum_decl` variant fields. No type_def, so 0_type_plane.pl:128 throws.

Proof the downstream machinery already works: add a decoy plain column
`rel unrelated(subject: tree, note: text).` and the SAME enum compiles;
emitted DDL stores grade_ripe.subject as INTEGER ref, catalog row
`"grade_ripe": [null, "tree"]`.

## The fix
1. Add a `declared_column_type_name/2` clause walking enum_decl variant
   fields: for `member(enum_decl(_, VariantTerms), Decls)`, each variant
   term's fields are `Column: TypeName` pairs (enum_decl_variant_term/2 and
   enum_field_column_name/2 in the same file show the walk); yield TypeName
   when `\+ scalar_column_type(TypeName)`.
2. Fail-first conformance fixture: the two-decl program above, arrivals into
   grade via both variants, a read back through grade_tag, plus a retraction.
   Confirm the oracle and the emitted program agree.
3. TEXT_DOOR twin if the existing enum fixtures there have one; follow the
   set's conventions.
4. Guard the typo path stays a named error: a variant field typed `treee`
   (undeclared) must still throw column_type_unknown, as a fixture.
5. Decision row in v6/prolog/conformance/rulings.pl (follow the file's row
   format): enum_variant_rel_payload; user 2026-08-10, oneOf mapping needs
   variant payloads to reference relations.

## Files you own
- v6/prolog/compile/parse_dl.pl: ONLY the declared_column_type_name clause
  block near :988. An in-flight lane owns the pre/2 spelling region
  (~:1372); touch nothing outside your clause block or the merge breaks.
- new conformance/TEXT_DOOR fixture files
- v6/prolog/conformance/rulings.pl (one appended row)
Do NOT touch 0_enum_expand.pl, 0_type_plane.pl, 0_program_check.pl,
lower.pl: the receipts above prove they already handle the shape.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
Manifest: new fixtures compiled, zero bucket flips elsewhere.

## Commit rail (commit-or-report)
Up to 2 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT-ENUM-PAYLOAD.md,
exact command + output, exit nonzero. NEVER --no-verify. Pre-commit hook
fails >2 consecutive comment lines in any touched hunk, including legacy
blocks: one-line comment edits only.

## Style
Comments state only constraints the code cannot show, max 2 consecutive
lines. Banned words, prose and identifiers: provenance, substrate,
load-bearing, regime, refusal. dl variable names descriptive, never
single-letter. Follow each file's existing style.
