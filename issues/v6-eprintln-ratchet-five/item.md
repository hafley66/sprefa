---
created: 2026-08-21
updated: 2026-08-21
type: task
status: open
priority: normal
epic: usurp-v4-v5
---

## Description

Five `eprintln!` sites in `v6/*/src/**/*.rs` carry no `@eprintln-ok` waiver.
CLAUDE.md: "eprintln never comes back. No `eprintln!` in `src/**`; `tracing`
only. Rare CLI-UX lines carry `@eprintln-ok`."

## The five, measured 2026-08-21

| site | what it prints | verdict |
|---|---|---|
| `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:89` | harness usage / arg error | waive: CLI-UX contract |
| `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:306` | harness run failure | waive: CLI-UX contract |
| `v6/sprefa-extract/src/bin/extract.rs:281` | top-level CLI error before exit | waive: CLI-UX contract |
| `v6/sprefa-extract/src/bin/extract.rs:592` | top-level CLI error before exit | waive: CLI-UX contract |
| `v6/sprefa-store/src/engine.rs:75` | `[cascade] {ms} {head}` timing line | **convert to `tracing`**: machinery narration in a library, not a CLI-UX contract |

Four are exactly what the waiver word exists for and want a comment, not a
rewrite. The fifth is the real finding.

## How this was measured

The v5 rail that used to answer this cannot run on v6 (@v5-rail-eprintln-blocked),
so its rule was applied by script: a hit is waived when `@eprintln-ok` appears
on the hit line or the line immediately above.

```bash
grep -rn 'eprintln!' v6/*/src --include=*.rs   # 17 hits
# minus the 12 carrying @eprintln-ok on the hit line or the line above
```

## Gate

```bash
cd v6/sprefa-store && cargo test --release
cd v6 && timeout 600 just rust-grade
```
