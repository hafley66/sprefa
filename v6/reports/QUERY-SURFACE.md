# boop query surface v2 (user word, 2026-08-08): the full table catalog

Goal, verbatim: "parse messages without giving a fuck about anything other
than session name/nickname/id and then we know its relational graph of
pointers is there."

Contract: every table keys on (session) or (session, turn). Give boop a
session id or nickname; every other fact is a join away. No consumer ever
re-parses a transcript.

## Base tables (boop emits these, raw projections, no interpretation)

```
agent_session(session: text, harness: text, nickname: text, cwd: text,
              branch: text, started_ts: int).
agent_live(session: text, pid: int, tmux_pane: text, status: text).
agent_edge(parent_session: text, child_session: text, edge_kind: text,
           agent_type: text, model: text).
agent_turn(session: text, turn: int, ts: int, role: text, said: text).
agent_touch(session: text, turn: int, ts: int, path: text, verb: text).
agent_span(session: text, turn: int, path: text, line_start: int, line_end: int).
agent_cmd(session: text, turn: int, ts: int, program: text, argline: text).
agent_fetch(session: text, turn: int, ts: int, url: text, domain: text).
agent_skill(session: text, turn: int, skill: text).
agent_pr(session: text, turn: int, pr_url: text).
```

Sources already measured: `pr-link` is a NATIVE claude record type (33 in one
session); `gitBranch` rides every record; `~/.claude/sessions/<pid>.json`
carries pid + exact tmux pane + busy/idle (one reader on the whole machine
today); `file-history-delta.trackingPath` is write ground truth (zero readers).

## Derived (dl6 rules, never boop code)

```
agent_git(Session, Turn, GitVerb, Argline) <-
  agent_cmd(Session, Turn, _, "git", Argline), ...;   % commit/push/merge...
agent_gh(Session, Turn, Argline)  <- agent_cmd(..., "gh", ...);
dir_touch(Session, Dir)           <- agent_touch + dir_of  (needs str revival)
touch_node/touch_edge             -> flow-panel layer, free via naming
```

git/gh live in DERIVED space on purpose: boop records the command; what
"counts as git usage" is a rule you can change without touching Rust.

## The instant replacement map (the quasi-busted consumer)

| instant today | busted how | replaced by |
|---|---|---|
| Touched-files pane | regexes file_path out of a 400-char truncated blob (ledger.rs:359 + 0_sessionSidebarModel.ts:24) | agent_touch |
| live-session detection | ps+lsof rogue scan + 5s tmux poll | agent_live (reads the pid registry) |
| message side panel | own jsonl parser (one of 5 copies) | agent_turn |
| session identity | 3 different head-window guesses (128/10/8 lines) | agent_session |
| cass for history | unhealthy, 3.8-day stale, 5s commit floor | boop tail is the live path; cass optional for deep history |

## str namespacing rides the module-dot lane

User: `use("str"). str.rtrim / str.split`. Verdict per construct, throw sites
cited in the triage doc: rtrim/replace = scalar expression/5 rows, land AS
str.-qualified from birth. split is TABLE-VALUED: one input row -> N rows;
its lowering seam is the same one spread/1 already owns (split to a json
array, json_each over it). So: rtrim/replace first, split immediately after
through the spread seam, all three inside module `str`, nothing lands in the
flat global namespace.
