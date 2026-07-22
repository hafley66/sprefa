# v6 HYPOTHESES — the idea ledger (don't lose a good one)

Every promising-but-not-yet-settled idea, with its source and status. This is the
counterpart to `v6/DECISIONS.md` (settled) — decisions go there, LIVE HYPOTHESES
go here. Add a row the moment a good idea appears; a hypothesis that only exists
in a transcript is one we will lose.

Status: `untested` · `promising` (partial evidence) · `in-flight` · `rejected`
(with receipt) · `promoted` (moved to DECISIONS.md).

| # | hypothesis | source | status | notes / next test |
|---|---|---|---|---|
| H1 | **Soft-delete / tombstone instead of hard delete-at-zero**: keep the `weight=0` row, filter by `weight>0`, sweep later — unify the retract plane with the temporal/durable plane so "what was alive at rev N" falls out for free. | 2026-07-22 session (user latent thought) | untested | trade: table never shrinks under churn until a sweep; temporal plane already gets durability cheaper via close-interval. G9 scanning transcripts for prior discussion. Test: soft-delete variant of `retract`, measure disk growth vs sweep cost. |
| H2 | **mmap-KV engine** (redb / heed-LMDB / sanakirja): compiled cursor traversal + flat, evictable, disk-backed RAM — lands between dd (speed) and sqlite (RAM). | 2026-07-22, memory-first reframe | in-flight (G8) | objective is LOWEST resident RAM at correctness, speed second. Must prove flat RAM is real (file grows, RSS bounded). |
| H3 | **SQLite as a raw B-tree, cascade driven from compiled Rust** (drop the bytecode VM, keep on-disk paging): removes the ~10x interpreter tax while keeping constant/evictable RAM. | 2026-07-22, "raw metal" question | untested | overlaps H2 (mmap KV is the clean version). Test only if H2's store isn't enough. |
| H4 | **SCC DAG early-out**: collapse `retract_scc` to a single counting pass when the cut's cone has no cyclic survivor (most real cuts are mostly-acyclic → SCC ≈ counting). | 2026-07-22, G6 | promising (open) | G6 tried 3 variants, reverted (receipts in EXPERIMENT-G6-RESULT.md); the early-out itself did not land. Biggest remaining perf lever. |
| H5 | **cache_size knob trade curve under 1GB**: quantify RSS vs disk read/write vs speed as `PRAGMA cache_size` is squeezed, find the smallest cache that finishes each scale under 1GB, then drive it down. | 2026-07-22, 1GB-budget reframe | in-flight (G10) | the golden-data bench (v6/labs/AGENTS.md). |
| H6 | **Interactive drill-down living map via D2 → cytoscape** (`explorer.jsx` progressive expand + `AtlasPanel` fold/unfold + the CSS-anchor backend) instead of static Mermaid. | 2026-07-22, map-recursion wish | untested (tooling exists) | Mermaid stays for the committed doc; D2→cytoscape when you want to click into it. Pipeline already built in ~/projects/anim. |
| H7 | **sprefa/dl indexes agent messages (by provider) so findings self-surface** — the real fix for re-derivation; the pin/digest stop depending on someone re-reading them. | recovered 2026-07-22 (SELF-RESEARCH.md) | untested (aspirational) | `tools/chat-find.sh` (G9) is the manual stopgap; this is the productized version. |

## How a hypothesis moves

`untested` → run the cheapest experiment that could falsify it → `promising` or
`rejected` (write the receipt). A `promising` one that survives adversarial re-run
and a measured win → `promoted` (move the row's conclusion into `v6/DECISIONS.md`
and leave a one-line pointer here). Never delete a rejected row — the receipt is
why we don't re-try it.
