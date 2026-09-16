# BRIEF: the two runtimes packaged as libs, and a plan for the generated types

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha. If a procedural line in this brief seems to forbid
otherwise-correct work, the work wins: note the conflict and keep going.

## One sentence
`v6/sprefa-engine-rs` and the `v6/tsv2` runtime should each be consumable as a
standalone library by an outside project; today both are entangled with their
app shells, and the compiler's generated type artifacts reach neither.

## What exists, measured. Verify each line before building on it.

| fact | evidence |
|---|---|
| the compiler emits per-program TS interfaces, Rust serde structs, and JSON Schema | `v6/prolog/compile/9_emit_type_artifact.pl` wrapping `4_emit_jsonschema.pl`, `7_emit_ts_types.pl`, `8_emit_rust_types.pl` |
| the artifacts land beside the program in `compile/out/*.types.{ts,rs}` | run any sweep, or list the directory |
| NOTHING consumes them: emitted programs import runtime modules only | `v6/tsv2/gen_emitted/<any>.ts` import lines |
| engine-rs is already a crate with a bin harness | `v6/sprefa-engine-rs/Cargo.toml` |
| engine-rs links sprefa-extract in-process | `v6/sprefa-engine-rs/Cargo.toml:17` |
| tsv2 mixes runtime, serve, cli, tests in one package | `v6/tsv2/package.json` |

## Deliverables, in order

1. **engine-rs as a lib**: a `lib.rs` public surface (runtime boot, tick
   driver, host executor registry), the harness/cli behind a cargo feature so a
   consumer builds the lib without them. Do NOT rename the crate, do NOT touch
   version/publish metadata. Prove it: a `tests/` integration test (or doc
   example) that uses the crate only through `use sprefa_engine_rs::...`.
2. **tsv2 runtime as a lib**: an `exports` map in `v6/tsv2/package.json`
   exposing `./runtime` (and only it) as the library entry; a barrel
   `runtime/index.ts` re-exporting the public modules
   (`1_incremental`, `3_subscribe`, `diff`, `2_boot`, `types`). MOVE NO FILES;
   existing imports must keep working. Prove it: a tiny script outside the
   package dir importing through the exports map, run it, paste the output.
3. **PLAN doc, no implementation**:
   `plans/2026-08-13-generated-types-as-lib-surface.md`, TOC first. How the
   `.types.ts` / `.types.rs` artifacts become each emitted program's public
   typed surface. At least these forks, priced with citations: (a) the emitter
   writes an import of the adjacent types file into the emitted program,
   (b) the artifacts ship as a sibling module the consumer imports directly,
   (c) rust: a generated `mod` include per program. Say what each costs in
   emitter changes (`emit_ts.pl` / `emit_rust.pl` are FORBIDDEN files to you),
   what it costs the graded goldens, and which functions gain compile-time
   checking. The user rules on the fork; you price it.

## Files you own
- `v6/sprefa-engine-rs/**` EXCEPT `src/hosts.rs` (live-host surface stays put)
- `v6/tsv2/package.json`, `v6/tsv2/runtime/index.ts` (new)
- `plans/2026-08-13-generated-types-as-lib-surface.md` (new)

FORBIDDEN: `v6/prolog/**`, `v6/sprefa-extract/**` (another lane owns it),
`v6/tsv2/serve/**`, `v6/tsv2/tests/**` (read-only), `v6/tsv2/gen_emitted/**`,
`CLAUDE.md`.

## Validation, run and paste verbatim, each three times
```bash
cd v6/sprefa-engine-rs && cargo test 2>&1 | tail -3
bash v6/sprefa-engine-rs/grade.sh 2>&1 | tail -3        # count must not move
cd v6/tsv2 && npm test 2>&1 | tail -8                   # only the known-red failures, .github/CI-KNOWN-RED.md
node <your outside-import smoke script>
```

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" banned in prose.
- Comments state only constraints the code cannot show.
- New TS classes declare interfaces in the package's `types.ts`; `I` prefix.
- No `eprintln!` in src/**; `tracing` only.
- Docs open with a table of contents; tables and lists over prose.

## Report format
Zero-context coworker brief, every claim `path:line`. COMMIT your work; a lane
that exits without committing has not delivered.
