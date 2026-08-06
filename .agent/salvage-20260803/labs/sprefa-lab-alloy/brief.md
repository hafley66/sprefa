# Lab: alloy semantics in prolog — decl/ref/check/render codegen spike

Worktree lab BRANCHED OFF EXISTING CODE for interop (base 92756b54): reuse
the repo's swipl setup and read real source files; this is not an isolated
toy. If reality deviates from this brief, STOP and record it in REPORT.md.

## Base verification (FIRST action)
```bash
cd /Users/chrishafley/projects/sprefa-lab-alloy
git merge --ff-only 92756b54dc0cb633e9636234f5358f3324be1ebf && git rev-parse HEAD
```

## Ownership
Create files ONLY under `v6/prolog/labs/alloy_semantics/` plus `REPORT.md`
at the worktree root. Never commit or push.

## Bounded reference reads (NEVER walk whole dirs; named files only)
- `~/projects/claude-research/skills_archive/alloy-core/SKILL.md`
- `~/projects/claude-research/skills_archive/alloy-languages/SKILL.md`
- `v6/prolog/compile/2_emit_cli_inventory.pl` (emit precedent)
- `v6/prolog/labs/openapi_codegen/emit_openapi.pl` (emit precedent)
- `v6/sprefa-store/src/spine.rs` lines 300-420 (real schema, transcribe 3
  tables only: strings, node, edge)
- `v6/sprefa-store/js/src/engine/spine.ts` lines 60-105 (ts twin)

## Build (all under v6/prolog/labs/alloy_semantics/)
1. `0_facts.pl` — 3 spine tables as `table/2` + `column/8` facts, plus a
   deliberate CROSS-FILE reference: declare that the rust `edge` struct
   references the `node` struct id type, and the ts emission splits across
   two virtual files so one must import from the other.
2. `1_collect.pl` — derive `decl(Id, Kind, TargetFile)` and
   `ref(FromFile, Id)` facts from 0_facts. No text yet.
3. `2_check.pl` — invariant goals: every ref has exactly one decl; no two
   decls share a rendered name per target; every declared import is used.
   Failures throw `codegen_refused(<named reason>)`, mirroring the
   compiler's unmapped_feature style.
4. `3_render.pl` — term-tree emitters (NO string concat mid-tree: build
   terms like interface(Name, [field(N,T)|...]), fold to text at the end)
   for BOTH targets: ts interfaces (2 virtual files with the import line
   derived from ref/2, not hand-written) and rust structs (2 modules with
   the `use` line derived the same way).
5. `4_receipts.sh` — runs everything and captures:
   a. green run: both targets emitted, import/use lines present, text
      printed verbatim.
   b. SABOTAGE 1: comment out the node decl -> expect
      codegen_refused(unresolved_ref) BEFORE any text is emitted; capture.
   c. SABOTAGE 2: add a duplicate decl name -> expect
      codegen_refused(duplicate_name); capture.
   d. parity: the emitted ts for the 3 tables matches the real
      spine.ts:60-105 interfaces for those tables (diff shown; exact match
      not required, field names+types must match — report the diff).
6. `MAPPING.md` — the 1-1 table: alloy/tsp concept (component tree, refkey,
   binder/scope, context, mono-morphization if the skills mention it) ->
   the prolog construct used HERE (term tree, decl/ref atoms, the check
   pass, accumulator). One row per concept, each row citing the skill file
   line AND the lab file line that implements it. Concepts with no
   implementation here get status=unmapped, not a fake row.

## REPORT.md
- verbatim receipts a-d (swipl command + full output each)
- MAPPING.md summary: N mapped, M unmapped
- what broke / what surprised, if anything (facts only)
