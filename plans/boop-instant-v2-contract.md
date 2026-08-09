# boop query contract for the four instant v2 views

What instant calls instead of parsing transcripts itself. One call per view,
from the CLI or from the lib. No UI work here, no HTTP door, no IPC layer.

All output below is live, run 2026-08-09 against `~/.agent/boop.db` (schema 5,
1689 sessions, 230953 turns, 117219 usage rows, two harnesses).

## Reaching it from a Rust host

`boop` ships a lib target. `cargo run --example instant_views` is a working
outside consumer of exactly this surface:

```rust
use boop::{FactKind, FactQuery, GroupBy, UsageQuery};
let store = boop::open_default()?;                      // ~/.agent/boop.db
let rows  = store.query_facts(FactKind::Touch, &filter)?; // serde_json::Value per row
```

```
$ cargo run --example instant_views
view 1 subagent readout: 14 sessions
view 2 external shells: 500 sessions
view 3 network: 50 rows
view 4 file sidebar: 50 rows
  usage by harness: "claude" calls=95730
  usage by harness: "opencode" calls=21489
```

---

## View 1: subagent table readout

| | |
|---|---|
| CLI | `boop db status --window <minutes>` |
| lib | `Store::query_status(window_ms, now_ms)` |
| rollup | `Store::usage_report(None, &UsageQuery { session, rollup_subtree: true, .. })` |

Rows carry `session`, `nickname`, `harness`, `cwd`, `parent_session`,
`last_turn_ts`, `turns`, `calls_in_window`, `tokens_in_window`, plus `lane` and
`state` layered from the registry and tmux.

```
$ boop db status --window 120
{"session":"259999e5-...","harness":"claude","parent_session":null,"turns":413,
 "calls_in_window":141,"tokens_in_window":31683803,"lane":null,"state":"unknown"}
{"session":"ses_0185ff416ffe7Nd2O5MYndbPN5","nickname":"kind-moon","harness":"opencode",
 "cwd":"/Users/chrishafley/projects/sprefa-lanes/prolog-dead-sweep","parent_session":null,
 "turns":6,"calls_in_window":4,"tokens_in_window":25469,"lane":"prolog-dead-sweep","state":"live"}
```

The tree is `parent_session` per row, not a nested shape, so a caller folds it
however it renders. The true cost of a parent is the subtree rollup:

```
$ boop db usage --session 259999e5-... 
  calls=162  output=114899  cache_read= 24945628
$ boop db usage --session 259999e5-... --rollup-subtree
  calls=746  output=523718  cache_read=112244105
```

**Gap**: `state` is `unknown` for any session with no registry route, which is
every hand-started claude session (only lanes boop spawned have routes). The
row says `unknown` rather than guessing `dead`.

---

## View 2: external shells

| | |
|---|---|
| CLI | `boop db session list` and `boop beep lane list` |
| lib | `Store::query_sessions(None, limit)` |

One tree regardless of harness: the same `agent_session` rows carry both.

```
$ sqlite3 ~/.agent/boop.db "SELECT dict_harness.value, COUNT(*) FROM agent_session
    JOIN dict_harness ON dict_harness.id=agent_session.harness_id GROUP BY 1;"
claude    1369
opencode   320
```

Spawn edges resolve for both:

```
child_harness  edges
claude         1081
opencode         51
```

**Gaps**:
- 288 of 1369 claude sessions have no `cwd`, because the first transcript
  record does not always carry one. Grouping by project drops those.
- A shell lane with no transcript and no opencode session is invisible: boop
  only knows agents that write a store. `beep lane list` still shows it from
  the registry, so the view needs both calls, not one.

---

## View 3: network viewer

| | |
|---|---|
| CLI | `boop db fetch list [--like <url-prefix>] [--session] [--since]` |
| lib | `Store::query_facts(FactKind::Fetch, &FactQuery { .. })` |

```
$ boop db fetch list --limit 2
{"session":"f8260935-.../agent-a5ed98260810502ac","url":"https://code.claude.com/docs/en/plugins.md",
 "domain":"code.claude.com","kind":"fetch","turn":5,"ts":1783604691853,"query":null}
```

Row shape is `(session, turn, ts, url, domain, kind, query)`. `kind` is `fetch`
or `search`; a search has no url or domain and carries its `query` instead.

```
kind    rows
fetch   3475
search  1274
```

**Closed this arc**: searches were invisible. Measured over 120 transcripts,
`WebFetch` was 213 calls all captured and `WebSearch` was 117 calls none
captured, so roughly a third of network activity was missing. `agent_fetch` now
carries `kind_id`, nullable `url_id`/`domain_id`, and a `query` payload column.

**Remaining gap**: 119 of the same 120 transcripts' `Bash` commands embed a URL
(`curl`, `gh api`). Those are network activity that no fetch row records, and
extracting a URL from a shell string is a different kind of claim than reading
a tool argument. Not done; named so it is not mistaken for coverage.

---

## View 4: file sidebar during a session

| | |
|---|---|
| CLI | `boop db touch list --session <id> [--like <path-prefix>] [--since]` |
| lib | `Store::query_facts(FactKind::Touch, &FactQuery { session, since, like, limit })` |

```
$ boop db touch list --session 259999e5-... --limit 2
{"session":"259999e5-...","path":".../chat_log/LATEST.md","verb":"Read","turn":6,"ts":1786282376799}
```

`verb` is the icon: `Read`/`read` versus `Write`/`Edit`/`write`/`edit`, spelled
as the harness spelled it.

The acceptance query, most recently touched markdown across every harness:

```sql
SELECT dict_path.value AS file, dict_harness.value AS harness,
       dict_verb.value AS verb, MAX(agent_touch.ts) AS last_touch
FROM agent_touch
JOIN dict_path    ON dict_path.id = agent_touch.path_id
JOIN dict_verb    ON dict_verb.id = agent_touch.verb_id
JOIN agent_session ON agent_session.session_id = agent_touch.session_id
JOIN dict_harness ON dict_harness.id = agent_session.harness_id
WHERE dict_path.value LIKE '%.md'
GROUP BY dict_path.value, dict_harness.value
ORDER BY MAX(agent_touch.ts) DESC LIMIT 14;
```

```
/Users/.../sprefa-lanes/prolog-dead-sweep/BRIEF.md                      claude    Write  13:44:03
/Users/.../sprefa/plans/2026-08-09-viewport-facts-design.md             claude    Write  13:27:10
/Users/.../sprefa-lanes/opt3vl-bench/plans/2026-08-09-option-vs-3vl.md  opencode  edit   13:19:34
/Users/.../sprefa-lanes/opt3vl-bench/REPORT.md                          opencode  write  13:18:32
/Users/.../sprefa-lanes/boop/QUERY-SURFACE.md                           claude    Read   13:17:06
/Users/.../sprefa-lanes/opt3vl-bench/BRIEF.md                           opencode  read   12:40:03
```

Both harnesses interleave in one time-ordered list with no per-harness branch,
because tool-name matching is case-insensitive and `filePath` is read beside
`file_path`.

```
harness   md_touches  distinct_files
claude    4911        1923
opencode  1075         505
```

**Gap**: `agent_touch` records the tool call, not the file's mtime, so a file
edited outside an agent never appears. That is the intended meaning of the
table (what an agent touched), stated so the view does not present it as a
filesystem watcher.

---

## Schema gaps, collected

| gap | effect | where |
|---|---|---|
| 288 of 1369 claude sessions have no `cwd` | project grouping drops them | view 1, 2 |
| no registry route means `state=unknown` | hand-started sessions show no liveness | view 1, 2 |
| shell lanes with no store are unknown to boop | must also read `beep lane list` | view 2 |
| URLs inside `Bash` commands are not fetch rows | ~119 per 120 transcripts uncounted | view 3 |
| `agent_touch` is tool calls, not file mtimes | non-agent edits invisible | view 4 |
| `agent_live` is current-state only, never history | "died within the window" unanswerable | view 1, 2 |

The last one has a design already written: `agent_live_span` in
`plans/boop-self-id-and-status.md`, the same interval shape as `agent_visible`
in `plans/boop-tmux-visibility.md`.
