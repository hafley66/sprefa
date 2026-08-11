# fix/jsonlist-emit: kind_schema/6 gains the json_list clause (round-trip gap G3)

## From POKEAPI_ROUNDTRIP_REPORT.md (v6/dl/fixtures/) gap G3: a program
## carrying any json_list(T) column makes 4_emit_jsonschema.pl and
## 5_emit_openapi.pl return NO document, because kind_schema/6 has no
## json_list clause. This blocks emit-back for the generated pokeapi
## program (gen dl6 compiles fine; the emitters die on it).

## The fix
1. kind_schema/6 in v6/prolog/compile/4_emit_jsonschema.pl: json_list(T)
   renders `{"type": "array", "items": <schema of T>}`, recursing for
   nested json_list(json_list(T)) and using the existing scalar mapping for
   int/text/float/bool/json elements.
2. Verify 5_emit_openapi.pl picks it up through the shared path (the two
   emitters share kind rendering; if openapi has its own clause table, add
   the same row).
3. Fail-first fixture: a small dl6 with json_list(int), json_list(text),
   nested json_list(json_list(int)) columns; emit both docs; assert array
   items types. Regenerate compile/out for it and pokeapi-adjacent goldens.
4. Receipt in the commit message: emit-back on
   v6/tsv2/gen/pokeapi_gen.dl6 (regenerate it first via
   `cd v6/tsv2 && npx tsx scripts/openapi_to_dl6.ts` if absent — check the
   script's own usage line) now returns a document; paste the definition
   count.

## Files you own
- v6/prolog/compile/4_emit_jsonschema.pl, 5_emit_openapi.pl
- new fixture + compile/out regeneration
Nothing else. parse_dl.pl, lower.pl, 0_*.pl are all off limits.

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

## Commit rail (commit-or-report)
Up to 2 commits, prefix `prolog:`. Blocked -> FAILURE-REPORT-JSONLIST.md,
exact command + output, exit NONZERO. Exiting 0 with a dirty tree or red
gates is a defect. NEVER --no-verify. Comment budget: max 2 consecutive
comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
