# REPORT4: click a strip row -> read that session's turns (router-wired)

Branch lab/dock-strip (== instant main). No commits. Nothing written outside
this worktree. Never `just dev`. Deviations recorded, never improvised.

## 1. Turn source recon (recorded before building)

Reads (layout + minimal columns) with receipts, before any build:

### claude: transcript jsonl, `~/.claude/projects/<cwd '/'->'-'>/<sessionId>.jsonl`
Receipt (`ls ~/.claude/projects`): dirs are the cwd encoded by replacing every
non-alphanumeric char with `-`, e.g. `-Users-chrishafley-projects-claude-research`.
One `<uuid>.jsonl` per conversation. NDJSON, one record per line. First line of a
real file is often the opener, e.g. a `type:"mode"` record; turn records are
`type:"user"` / `type:"assistant"`. Confirmed receipt from
`~/.claude/projects/-Users-chrishafley-projects-claude-research/<uuid>.jsonl`:
`{"type":"mode","mode":"normal","sessionId":"c56f..."}` as a leading non-turn
line. Turn-relevant fields read by the existing ledger reader
(`src-tauri/src/ledger.rs` `read_claude` -> `AiMessage { role, preview, ts }`): the
`message.content` (a plain string or a block array of `{type:"text",text}` /
`{type:"tool_result"}`), `timestamp` (ISO), `uuid`, and the injection classifiers
`isMeta` / `promptSource` / `origin`. The session sidebar's turn model consumes
this same data via `warmTurns`/`read_ai_messages` through `favorites.ts` and
derives previews + role labels in `src/0_sessionSidebarModel.ts`.

### opencode: sqlite `~/.local/share/opencode/opencode.db` (verified schema)
Receipt (`.schema` on this machine, db 3.3 GB):
- `message(id text PK, session_id text NOT NULL, time_created int NOT NULL,
  time_updated int NOT NULL, data text NOT NULL)`; index
  `(session_id, time_created, id)`. `data` is JSON, e.g.
  `{"role":"user","time":{"created":...},"agent":...,"model":...}`.
- `part(id text PK, message_id text NOT NULL, session_id text NOT NULL,
  time_created int, ..., data text NOT NULL)`. `data` is JSON with a `type`
  (`text`/`reasoning`/`tool`) and `text`; a tool part carries `state.input` /
  `state.output`. The existing reader (`ledger.rs` `read_opencode`) joins the
  message role with its parts' text to build a preview.
- `session_message(id, session_id, type, seq int, time_created, ..., data)` with
  `UNIQUE(session_id, seq)`; present but not needed for turn text (message+part
  carry the content). Read-only open (SQLITE_OPEN_READ_ONLY | NO_MUTEX).

Ordering columns used: opencode `message.time_created` (asc -> oldest first);
claude the jsonl line index (asc -> oldest first). Both therefore yield
chronological turns (newest last when rendered downward).

### codex / kimi: recon only, no reader this lane
- codex: `~/.codex/sessions/<Y>/<M>/<D>/<rollout>.jsonl`, first line `session_meta`
  carries `id` + `cwd` (per `harness.rs`).
- kimi: `~/.kimi-code/sessions/<workspace>/session_<id>/state.json` (carries
  `workDir`) plus a `wire.jsonl` for the main agent (per `harness.rs`).

Reported for completeness; CONTRACT4 builds readers for claude and opencode only.

## 2. Build

[filled after building]

## 3. Proofs

[filled after building]

## 4. Gates

| gate | command | output | exit |
| --- | --- | --- | --- |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` | pending |  |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/plugins/harnessTrace/` | pending |  |
| cargo | `cargo test --manifest-path src-tauri/Cargo.toml harness` | pending |  |
| e2e mint | `playwright test e2e/dock-strip-in-tab.spec.ts --update-snapshots` | pending |  |
| e2e verify | `playwright test e2e/dock-strip-in-tab.spec.ts` | pending |  |
| e2e old | `playwright test e2e/dock-strip.spec.ts` | pending |  |

## 5. Deviations

[filled at end]
