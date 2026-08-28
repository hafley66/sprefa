---
created: 2026-08-24
updated: 2026-08-24
type: bug
status: open
priority: normal
---

# extract prolog corpus runs at 7 MB/s, 0.51 s for 187 files

## Description

Measured 2026-08-24 on the #456 worktree, release build: `extract <187 v6/prolog *.pl> --family call >/dev/null` = 0.51 s wall for 3.5 MB. `extract move` dry run = 0.90 s, of which this is the extraction half; soopy temp-mirror staging is the other ~0.4 s. tree-sitter parses this volume in well under 100 ms, so the loss sits in flatten/JSONL serialization or per-file setup. Next: `extract --bench` per phase on the corpus, then fix the dominant phase. Rail: a COUNT/timing test on the prolog corpus.
