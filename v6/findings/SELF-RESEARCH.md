# SELF-RESEARCH — we mined our own conversation history (READ FIRST, then don't redo)

This is the meta-capability that matters more than any single result below: on
2026-07-22 we stopped guessing and **researched our own past sessions** to recover
decisions we had already made and kept re-deriving. It was round 6 of re-litigating
the same counting-vs-DRed question. The fix was not another debate — it was
**treating our conversation history as a searchable corpus**.

## The problem this solves

Findings, decisions, and Big-O measurements keep getting re-discovered because
nothing indexes them. A new session starts cold and re-derives settled calls. The
entire point of this app (datalog-over-code, facts you query instead of re-find) is
to make that impossible — and we had not turned it on ourselves.

## The method (the literal research, reproducible)

Two corpora, both queryable RIGHT NOW with plain tools:

1. **Curated session summaries** — `chat_log/*.md` (226 files, one per session).
   Mined by **8 parallel haiku passes** (cheap; they read "the junk" so the
   coordinator does not) into `v6/findings/SESSION-DIGEST.md` — every
   salsa/DRed/dd/timely/dataflow/async-sync/Z-set/SCC/fixpoint/Big-O discussion,
   distilled to snippets, each sourced to its `chat_log/` file. 78 of 226 relevant.

2. **Raw Claude Code transcripts** — the actual turn-by-turn conversation:
   `~/.claude/projects/-Users-chrishafley-projects-sprefa/*.jsonl` (67 files,
   577MB). NEVER read whole; `rg` them:
   ```bash
   rg -i -c 'PATTERN' ~/.claude/projects/-Users-chrishafley-projects-sprefa/*.jsonl \
     | sort -t: -k2 -rn | head            # rank files by hit count
   rg -i -o '"[^"]*PATTERN[^"]*"' <that-uuid>.jsonl | head   # pull phrases, not the file
   ```
   This is how we found a PAST session where the assistant had already caught the
   exact detour: "DRed is a thing *I* dragged in... needed ONLY if a recursive
   relation can contain cycles." We re-dragged it anyway. Hence this doc.

3. **Decision docs** — `plans/`, `v6/plans/` (dated `YYYY-MM-DD-topic.md`),
   `v6/ARCHITECTURE.md`. `grep -rniE 'PATTERN' chat_log/*.md plans/ v6/plans/`.

## What it recovered (so you don't re-derive it)

The retraction model was DECIDED in `v6/plans/2026-07-19-v6-table-design.md:344-368`
and pinned in `v6/DECISIONS.md`: counting weights (support count), NO DRed, cycles
by SCC-scoped nested fixpoint. salsa + dd rejected as resident (the v5 36GB swap
leak). All of salsa/SCC/reachability/dd is ONE counting cascade; prune varies
(digest / weight / reached). The self-research also surfaced already-measured
numbers we'd forgotten: 130,000x SCC speedup, WITH-RECURSIVE 720x slower than
incremental, wavefront = |Δoutput| universal lower bound, dd 215 B/node -> aborts
at 1.5GB while the on-disk store survives, 220 ns/row rederive.

## The forward capability (what this should BECOME)

This was done by hand with `rg` + haikus. The real fix, and the app's own dogfood:
**sprefa/dl indexes agent messages as queryable facts** (search by provider,
topic, decision-status), so the pin and the digest self-surface at session start
instead of a human remembering to open them. Until then: this file + `DECISIONS.md`
+ `SESSION-DIGEST.md` are the manual index, and the `rg` recipes above are how you
extend it.

Artifacts: `v6/DECISIONS.md` (settled calls + re-find commands),
`v6/findings/SESSION-DIGEST.md` (the lineage timeline), this file (the method).
