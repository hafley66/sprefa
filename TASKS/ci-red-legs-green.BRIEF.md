# Brief: CI-KNOWN-RED legs fixed or deleted (card issues/ci-red-legs-green, epic productionize-rust-door, size M)

Repo `~/projects/sprefa`. Base = origin/main sha the coordinator gives you (do not rebase). Worktree: `git -C ~/projects/sprefa worktree add ~/projects/sprefa-worktrees/<branch-dir> -b <branch> <sha>`; first action inside: `git log -1` = sha, tree clean; then `just boop-start` if it exists (it builds the extractor into the shared target; do not rebuild by hand). Never touch the main tree at `~/projects/sprefa`. Never `git stash`. Read `CLAUDE.md` standing laws and style laws first: no eprintln, banned words provenance/substrate/load-bearing/regime/refusal/honest/ground*, no em dashes, build-vs-buy paragraph before any hand-rolled infra, 10-second law, failure ledger, comment budget. Measure three times, never once. You MAY spawn ONE sonnet subagent for mechanical parts with disjoint files stated in its prompt; you review and test every line; no further fan-out. PR: `gh pr create --base main` with 1-3 plain sentences, `## Reading order`, `## Tests` (name, input, expectation, printed before); no words gate/leg/receipt/door/probe/refusal; do NOT merge; tick the AC you land on the card in your worktree and commit the card with the code.

Branch `fix/ci-red-legs-green`.

## What exists
`.github/CI-KNOWN-RED.md` is the allowlist `just green-all` judges against. Rows today (re-read the file; it goes stale): `1_extraction-clock-golden.sh` (`62 !== 59`), `just typecheck` (golden-flex.ts union too complex, relation_id_access), `flagship-flow.sh` (needs the v5 release binary), tsv2-test 4 failures (`hostDecode.test.ts:144`, two `bopCheck` exit-code tests, `edge-body negation SEARCHes`), memory-soak (`sqlite_page_count_flat`), lsp-diags (needs the v5 `dl` binary). Chris: "I DO NOT WANT TO RUN V5 ANYTHING ANYMORE" (CLAUDE.md user decisions), so any leg that needs v5 is deleted or ported, never kept.

## Deliverables, one commit per leg, each commit message = leg / before / after
1. `1_extraction-clock-golden.sh`: find the source of 62 vs 59 (extractor output vs the fixture's expected count; compare `DL_EXTRACT_BIN` output rows against the golden by hand); fix at the source; the number in the script is never edited to match.
2. `just typecheck`: `golden-flex.ts` union shape from `7_emit_ts_types.pl` (a union too complex to represent) and `relation_id_access`; fix the emitter (alias the union, or emit an interface per variant), never a tsconfig flag; regenerate `compile/out/**`.
3. `flagship-flow.sh`: port to the v6 Rust door (`emit_rust_harness` / the sidecar adapters) or delete it with the reason; same for `lsp-diags.sh` (v5 `dl` binary).
4. tsv2-test 4 failures: re-diagnose each (the file says the stated "needs gen_emitted/" cause is WRONG); fix or delete with reason.
5. memory-soak: re-measure three times; if the ceiling is wrong, say why with numbers, else fix the leak.
6. `.github/CI-KNOWN-RED.md` ends empty, or lists only legs moved to a named `optional` just group with a one-line reason each. `just green-all` runs three times back to back on your tree; all three agree; paste the three summaries in the PR.

## Files owned
`.github/CI-KNOWN-RED.md`, `justfile`, `v6/justfile`, `v6/tsv2/scripts/*.sh`, `v6/tsv2/scripts/memory-soak.ts`, `v6/tsv2/tests/**`, `v6/prolog/compile/7_emit_ts_types.pl`, `v6/prolog/compile/out/**`, `v6/tsv2/gen_emitted/**`, `v6/sprefa-extract/**` only if leg 1 proves the extractor is the source (say so), the card. Do NOT touch `parse_dl_dcg.pl`, `lower.pl`, `emit_rust.pl`, `serve.rs`.

## Tests to run at the end
`just green-all` x3, `cd v6/tsv2 && bash scripts/sweep.sh`, `cd v6 && just plunit`, `cd v6/prolog/conformance && swipl -g go -t halt go.pl`, `swipl -g go -t halt v6/prolog/ARCH.pl`. Pre-commit as in the other briefs (`DL_EXTRACT_BIN`, `pnpm install --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js`).
