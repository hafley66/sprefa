# Lane queue (user rules 2026-08-29: at most 2 live lanes; opus off; terra = `--preset terra`, glm53 = `--preset glm53` for opus-tier, glm53f for sonnet-tier)

| order | lane | brief | preset | note |
|---|---|---|---|---|
| live | fix-extract-go-residual-5 | go-residual-5.FIX.BRIEF.md | opus (already running, last opus) | |
| live | fix-extract-ts-ns-iface | ts-namespace-iface-destructure.FIX.BRIEF.md | opus (already running, last opus) | |
| 1 | fix-extract-rust-paths-3 | rust-paths-3.FIX.BRIEF.md | terra | `--reclaim`; partial work in `git stash list` of the worktree |
| 2 | bench-extract-ratchet | ../extract-bench-2026-08-29/RATCHET.BRIEF.md | glm53 | |
| 3 | fix-extract-speed | ../extract-bench-2026-08-29/SPEED.BRIEF.md | glm53f | PR #581 |
| 3b | fix-extract-speed-2 | ../extract-bench-2026-08-29/SPEED2.BRIEF.md | glm53f | go lock convoy + parse once; spawns when #581 merges |
| 4 | bench-extract-scip-informed | ../extract-bench-2026-08-29/SCIP-INFORMED.BRIEF.md | terra | |

## Harness receipts (2026-08-29 evening)
| preset | runs | outcome |
|---|---|---|
| opus | many | reliable; banned by user (cost) |
| terra | 1 | acp handshake failed: Internal error |
| sonnet | 2 | acp handshake failed: Internal error |
| glm53 (ccz, z.ai plan) | 5 | 1 completed (ratchet, PR #580); 4 died rc=1 at 15 to 20 min, `[claude-code:unrecognized_model] {"model":"glm-5.3"}` the only stderr line, conversation file 36 bytes, no error text |
| glm53f (opencode, openrouter) | many | completes; cargo-fmt churn and partial gate counts, sanded by the coordinator |
Fallback order until terra is fixed: glm53f, then glm53 for short tasks only.
