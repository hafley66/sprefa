# fix/typedecl-mirror: expansion must rewrite the ref-target schema mirror

## The defect (residual G1, 17 pokeapi columns; diagnosed 2026-08-11)
2-line repro, fails today:
```
rel item(item_id: int, note: option(text)).
rel box(subject: item).
```
`unsupported_construct(column_type_unknown)`. Remove the ref (`box(label:
text)`) and the identical item rel compiles.

Mechanism, verified: normalize_relation_value_decls (parse_dl.pl:988)
synthesizes `type_decl(item, Specs)` as a schema mirror for any rel used as
a column type, copied PRE-expansion, so the mirror carries `option(text)`.
Phase-5 generic expansion rewrites col_type rows only; nothing writes
type_decl terms (grep 0_generic_expand.pl, 0_option_expand.pl). Post-
expansion checks walk mirror specs too (0_program_check.pl:759-760,
declared_column_type_use) and 0_type_plane.pl:128 throws on the stale
`option(text)` name.

## The fix
In 0_generic_expand.pl, the same pass that rewrites a rel's col_type rows
rewrites any type_decl(RelName, Specs) mirror of that rel to the SAME
post-expansion column types. Pattern to follow: 0_enum_expand.pl:71-80
retarget_enum_column_types. Keep it one mechanism: derive the mirror's new
specs from the rewritten col_type rows rather than re-running type logic.

## Fixtures, fail-first
1. The 2-line repro above compiles green; box.subject emits INTEGER ref;
   item's option column behaves identically to the unreferenced control
   (arrival with and without the optional value, plus a retraction).
2. Same with `items: list(text)` instead of option (the list flavor).
3. A rel that is a ref target, a list element target, AND carries generics
   at once (the pokeapi shape that produced the 17 gap rows).
4. Rerun the strict converter check: cd v6/tsv2 &&
   npx tsx scripts/openapi_roundtrip_check.ts — the kind known-gap count
   must DROP from 17; put old->new in the commit message. Do not edit the
   converter; if any column still gaps, its throw site goes in the report.

## Files you own
- v6/prolog/0_generic_expand.pl (the mirror rewrite)
- new conformance fixtures + TEXT_DOOR twins if the set has them
- v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md refresh + gen/pokeapi_gen.dl6
  regen if the check run changes them
Do NOT touch parse_dl.pl, 0_program_check.pl, 0_type_plane.pl: the mirror
synthesis and the checks are correct once the mirror tracks the rewrite.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate (all green, no exceptions)
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just golden-flex
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```

## Rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> FAILURE-REPORT-MIRROR.md, exact command + output, exit
  NONZERO. rc=0 with a dirty tree or red gates is a defect.
- NEVER --no-verify. Up to 2 commits, prefix `prolog:`. Comment budget: max
  2 consecutive comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
dl variable names descriptive, never single-letter.
