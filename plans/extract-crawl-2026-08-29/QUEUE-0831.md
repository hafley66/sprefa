# Overnight queue 2026-08-31 (user-set: rust type recall turbo, 2 cleanup passes with extract move, then approved next-actions; RSS shelved; ONE lane at a time; coordinator grades everything)

| # | lane | preset | state |
|---|---|---|---|
| 1 | fix/extract-rust-type-recall | opus-max | spawning |
| 2 | bench/extract-rust-corpora | glm53f | NEXT free slot (user priority: multi-repo rust ratios) |
| 3 | chore/extract-cleanup-lang | glm53f | queued (brief at spawn time; extract move for relocations) |
| 3 | chore/extract-cleanup-bench | glm53f | queued (tests/bench + scripts area) |
| 4 | feat/extract-python-oracle | glm53f | queued (PyCG micro-suite + SWARM-CG import) |
| 5 | feat/extract-jelly-comparator | glm53f | queued (Jelly --compare-callgraphs as ts/js comparator) |

Rules: PR base main only; coordinator merges on green; gates nice -n 15; no
parallel lanes; briefs carry receipts + ownership + ceilings inline.
