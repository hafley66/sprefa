# PLAN: rip cass's transcript-reading into instant

Motive: cass's four walls (stale batch index, parent-bucket workspace scoping,
no parent links, wrong kimi tree) are all index-layer choices. The part that
knows HOW to read every harness's transcript format is exactly what instant
wants to own. Source: github.com/Dicklesworthstone/coding_agent_session_search
(brew tap, v0.6.22 installed, 0.6.23 upstream).

## Step 0 (gates everything): license + fork-vs-PR call

- Read the repo LICENSE. Permissive (MIT/Apache) = rip is legal with
  attribution; copyleft = depend-don't-rip; no license = neither, upstream PRs
  only.
- Cost the upstream alternative honestly: PRs for exact-folder scope + a watch
  mode + parent links in schema. If the author takes them, no rip needed.
  (Build-vs-buy law: both paths written down before any bespoke code.)

## What to rip (and what not)

| take | leave |
| --- | --- |
| format parsers: claude jsonl (+subagents dirs), opencode.db schema reads, codex rollouts, kimi trees | the batch indexer + its sqlite schema |
| workspace/session extraction logic (fix scoping to exact cwd while porting) | the TUI |
| | the search CLI (instant fronts its own) |

## Integration shape (instant)

- Rust module in src-tauri beside harness.rs: parsers feed BOTH the live
  strip (today's direct reads, unchanged contract) and a NEW local index.
- Index = instant's own sqlite (favorites.db precedent) with FTS5 for text
  search — bought from sqlite, never bespoke search.
- Freshness = fs-watch ingest (instant already has claimFsWatch plumbing),
  no batch cycle. Kills wall 1.
- Workspace = exact cwd column. Kills wall 2. Parent links come from the
  harness.rs subagent walk. Kills wall 3. kimi-code tree read natively.
  Kills wall 4.
- ledger.rs keeps shelling to cass until the new index reaches parity, then
  the cass binary dependency drops.

## Gates

- Parity fixture: for N real sessions on this box, ripped parsers emit the
  same (session id, cwd, ts) rows cass's index has for them.
- Freshness proof: a session started during the test appears in the index
  before the test ends (the exact case cass fails today).
- Search proof: FTS5 query finds a known phrase in a known transcript.

## Open (user word)

1. Step 0 verdict once LICENSE is read: rip, depend, or upstream-PR.
2. Does the ripped layer live in instant only, or as its own small crate so
   sprefa/the bus can read the same index (the "reuse someday" note)?
