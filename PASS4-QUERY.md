# LANE boop — PASS 4: prove the relational model, CLI querying, idle perf

User word, verbatim: "i should be able to hit up boop and get all messages
with where/filters xyz and then i will parse those for links etc. dont make
boop do that yet, prove the relational model the cli querying and perf at
idle etc."

You are pass 4. PASS4-CONTROL.md is DEFERRED to pass 5 (user reprioritized);
read it only for context. NORTH-STAR-CODEGEN.md and QUERY-SURFACE.md still
bind: emit raw rows, zero interpretation.

## MANDATORY FIRST READS (repo law, every agent, before any schema)

- .claude/skills/sql-relational-design/SKILL.md
- .claude/skills/sqlite-costs/SKILL.md
Surrogate INTEGER keys; paths and session ids go through dictionary tables;
a composite TEXT primary key is a DEFECT.

## Scope

1. **The tables land in SQLite** at `~/.agent/boop.db`: the base tables of
   QUERY-SURFACE.md (agent_session, agent_turn, agent_touch, agent_span,
   agent_cmd, agent_fetch, agent_skill, agent_pr, agent_edge, agent_live),
   dictionary-encoded per the skills.
2. **`boop sync`**: tail every harness forward from stored offsets into the
   db. Incremental: a second run with nothing new writes nothing.
3. **Query flags** on `chat` and `events`, reading the db:
   `--harness --session <id-or-nickname> --role --since --until --turn-from
   --turn-to --path <prefix> --limit --format ndjson|text`.
   Raw rows out. The caller parses links. NO derivation verbs.
4. **Idle proof**: `boop follow` (tail loop) runs on filesystem notification
   or a coarse poll; steady state near-zero CPU.

## Explicitly OUT (do not build)

Link extraction, git/gh classification, dir derivation, control/spawn verbs,
any column that interprets a message body.

## Receipts REQUIRED in REPORT.md (numbers, not adjectives)

- ingest: events/sec and db bytes for one full `boop sync` of this machine's
  claude transcripts; second-run no-op time.
- query: wall ms for 3 canned queries (all turns of one session; all touches
  of one path prefix; all sessions in a cwd), each with
  `EXPLAIN QUERY PLAN` output showing SEARCH, never SCAN.
- idle: CPU%% and RSS of `boop follow` sampled over 60s with no new events.
- 10-second law: every command above finishes under 10s or you STOP and
  report the number instead of normalizing it.

## Gates

cargo build, cargo test, cargo clippy -- -D warnings. tmux-touching tests use
a throwaway socket (`tmux -L boop-test-$$`), teardown kills that server.
Commit on green: `boop: PASS — pass 4 relational store + query flags + idle`.
Done-report, ALWAYS last action, success or not:
`bus hail --to fable-main --kind result --body "boop pass4 done: <one line>"`.
If reality deviates from this brief, STOP and report; do not improvise.
