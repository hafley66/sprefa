# Lane queue (user rules 2026-08-29: at most 2 live lanes; opus off; terra = `--preset terra`, glm53 = `--preset glm53` for opus-tier, glm53f for sonnet-tier)

| order | lane | brief | preset | note |
|---|---|---|---|---|
| live | fix-extract-go-residual-5 | go-residual-5.FIX.BRIEF.md | opus (already running, last opus) | |
| live | fix-extract-ts-ns-iface | ts-namespace-iface-destructure.FIX.BRIEF.md | opus (already running, last opus) | |
| 1 | fix-extract-rust-paths-3 | rust-paths-3.FIX.BRIEF.md | terra | `--reclaim`; partial work in `git stash list` of the worktree |
| 2 | bench-extract-ratchet | ../extract-bench-2026-08-29/RATCHET.BRIEF.md | glm53 | |
| 3 | fix-extract-speed | ../extract-bench-2026-08-29/SPEED.BRIEF.md | glm53f | PR #581 |
| 3b | fix-extract-speed-2 | ../extract-bench-2026-08-29/SPEED2.BRIEF.md | glm53f | go lock convoy + parse once; spawns when #581 merges |
| 4 | bench-extract-scip-informed | ../extract-bench-2026-08-29/SCIP-INFORMED.BRIEF.md | glm53f | live |
| 5 | fix-extract-rust-traits | rust-traits-4.FIX.BRIEF.md | glm53f | live |
| 6 | fix-extract-ts-codeql-gap | ts-codeql-gap.FIX.BRIEF.md | glm53f | pin sha before spawn |
| 6b | fix-extract-scip-speed | ../extract-bench-2026-08-29/SCIP-SPEED.BRIEF.md | glm53f | live: scip-informed under 10 s, go scip normal form, informed-by-default |
| 7 | fix-extract-go-residual-6 | go-residual-6.FIX.BRIEF.md | glm53f | pin sha before spawn |

## Harness receipts (2026-08-29 evening)
| preset | runs | outcome |
|---|---|---|
| opus | many | reliable; banned by user (cost) |
| terra | 1 | acp handshake failed: Internal error |
| sonnet | 2 | acp handshake failed: Internal error |
| glm53 (ccz, z.ai plan) | 5 | 1 completed (ratchet, PR #580); 4 died rc=1 at 15 to 20 min, `[claude-code:unrecognized_model] {"model":"glm-5.3"}` the only stderr line, conversation file 36 bytes, no error text |
| glm53f (opencode, openrouter) | many | completes; cargo-fmt churn and partial gate counts, sanded by the coordinator |
Fallback order until terra is fixed: glm53f, then glm53 for short tasks only.

## Night queue 2026-08-30 (user: glm53f only, 2 live max, xhigh)
| order | lane | brief | state |
|---|---|---|---|
| live | fix-extract-scip-informed-gate | ../extract-bench-2026-08-29/SCIP-INFORMED-GATE.BRIEF.md | PR #585 gate fix |
| live | fix-extract-ts-codeql-gap-2 | ts-codeql-gap-2.FIX.BRIEF.md | resumes origin/fix/extract-ts-codeql-gap |
| 3 | bench-extract-go-parity | ../extract-bench-2026-08-29/GO-PARITY.BRIEF.md | pin sha at spawn |
| 4 | bench-extract-rust-parity | ../extract-bench-2026-08-29/RUST-PARITY.BRIEF.md | after #584 merges (rust src ownership) |
| 5 | coordinator | gate-584 rerun, merge #584 | when no lane is measuring walls |
| 6 | fix-extract-scip-speed-2 | ../extract-bench-2026-08-29/SCIP-SPEED.BRIEF.md | resumes origin/fix/extract-scip-speed |
| 7 | fix-extract-go-residual-6 | go-residual-6.FIX.BRIEF.md | after go parity |

## User decisions 2026-08-30 (afternoon)
- rust resolve goes CHECKER-TIER: rust-analyzer as a library (ra_ap_load_cargo
  workspace load, ra_ap_hir/ra_ap_ide), no cargo build. Diet (syntax) stays
  the design for go and ts. RUST-GRIND.REDIRECT.md is the working spec.
- chaining bench: dropped.
- opus: allowed for the rust grind lane only; glm53f for everything else, one
  lane at a time, nice 15, watchdog on.
