# fix/anyof-emit-phase: option columns emit anyOf, not leaked __opt_* helpers

## RESPAWN NOTE (2026-08-11): attempt 1 exited rc=0 with a dirty tree and no
## commit, a defect. Its partial diff is banked at
## sprefa-lanes/anyof-attempt1.patch (4_emit_jsonschema.pl +64,
## 0_option_expand.pl +10, sweep.pl +5; 5_emit_openapi.pl untouched).
## Consult it, trust nothing untested. Finishing means COMMITTING green work
## or a FAILURE-REPORT-ANYOF.md with nonzero exit; rc=0 any other way is the
## defect that killed attempt 1.

## Ruled by user 2026-08-11 (pokeapi round-trip arc; decorations/meta stay out).

## The defect, already diagnosed (v6/dl/POKEAPI_SHAPE_REPORT.md:39)
The fixture carries 129 option(T) columns; the emitted
compile/out/pokeapi_shape.schema.json contains ZERO nullable anyOf entries,
because option expansion runs before the catalog the emitters read, so
4_emit_jsonschema.pl:column_schema/3 sees the expanded __opt_* helper
relations instead of option columns. 224 definitions emit where the source
had 212 schemas: the extra 12ish are compiler-internal helpers leaking into
a public schema.

## The fix
1. The emitters (4_emit_jsonschema.pl, 5_emit_openapi.pl, both ~110 lines)
   must render an option(T) column as the nullable anyOf shape
   (`anyOf: [{<T schema>}, {type: "null"}]`) and must NOT emit __opt_* or
   companion helper relations as public definitions. Two viable shapes,
   pick whichever fits the pipeline with the smaller diff and say which in
   the commit message:
   a. emit from pre-expansion decl information (the author plane keeps
      option(T) on the column), or
   b. keep emitting from the catalog but carry an option-origin mark for
      minted helpers so the emitter folds them back and hides them.
2. Same treatment in 5_emit_openapi.pl components.
3. Receipt: regenerate compile/out for pokeapi_shape and a small option
   fixture. pokeapi_shape.schema.json shows 129 anyOf nullable properties,
   definition count matches source schema count, zero __opt_ strings in
   schema.json or openapi.json. Paste the three grep counts in the commit
   message.

## Files you own
- v6/prolog/compile/4_emit_jsonschema.pl, 5_emit_openapi.pl
- whatever single seam carries the option-origin info to them (smallest
  diff wins; do NOT restructure expansion phase order, 1_expansion.pl's
  ordering comments explain why it is what it is)
- compile/out/* regenerated artifacts
Do NOT touch parse_dl.pl, print_dl.pl (another lane owns them),
0_generic_expand.pl (another lane), lower.pl.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
Known-red on main, NOT yours, ignore if they are the only failures:
text-door/roundtrip rel-element-list family (column_type_unknown(fighter_summary),
fail(not_variant) x3).

## Commit rail (commit-or-report)
Up to 2 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT-ANYOF.md, exact
command + output, exit NONZERO. Exiting 0 with uncommitted work or red gates
is a defect. NEVER --no-verify. Comment budget: max 2 consecutive comment
lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
