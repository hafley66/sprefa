# boop analytics + control surface: REST spec, clap mapping, token-usage schema

Design doc. No source code was touched. Companion files:
`plans/2026-08-09-boop-openapi.yaml` (the spec),
`plans/2026-08-09-boop-analytics-PLAN.visual.human.unga.md` (the plain-words doc).

## Table of contents

1. [Position and measured baseline](#1-position-and-measured-baseline)
2. [Research: what other tools measure, and from what](#2-research-what-other-tools-measure-and-from-what)
   - 2.1 [ccusage](#21-ccusage)
   - 2.2 [Claude-Code-Usage-Monitor](#22-claude-code-usage-monitor)
   - 2.3 [sniffly](#23-sniffly)
   - 2.4 [Claude Code OpenTelemetry export](#24-claude-code-opentelemetry-export)
   - 2.5 [OpenTelemetry GenAI semantic conventions](#25-opentelemetry-genai-semantic-conventions)
   - 2.6 [Langfuse public API](#26-langfuse-public-api)
   - 2.7 [LiteLLM pricing map](#27-litellm-pricing-map)
   - 2.8 [Multi-harness session managers](#28-multi-harness-session-managers)
   - 2.9 [What boop steals, and from whom](#29-what-boop-steals-and-from-whom)
3. [Two blocking defects found in the current ingest](#3-two-blocking-defects-found-in-the-current-ingest)
4. [Schema: the token-usage tables](#4-schema-the-token-usage-tables)
5. [Ingest design](#5-ingest-design)
6. [SQL sketch per analytics endpoint](#6-sql-sketch-per-analytics-endpoint)
7. [The REST surface](#7-the-rest-surface)
8. [clap to REST, both directions](#8-clap-to-rest-both-directions)
9. [Design decisions and the reason for each](#9-design-decisions-and-the-reason-for-each)
10. [Lint output](#10-lint-output)
11. [Open questions](#11-open-questions)

---

## 1. Position and measured baseline

Store: `~/.agent/boop.db`, 88.7 MB, measured 2026-08-09.

| table | rows |
|---|---|
| agent_session | 1312 |
| agent_turn | 140475 |
| agent_cmd | 61978 |
| agent_touch | 32218 |
| agent_fetch | 2965 |
| agent_pr | 667 |
| agent_skill | 306 |
| agent_edge | 0 |
| agent_live | 0 |

Schema source: `~/projects/sprefa-lanes/boop/v6/boop/src/ident.rs:562-670` (the `SCHEMA`
const). 15 `dict_*` tables, every fact table keyed `(session_id, turn)` `WITHOUT ROWID`,
integer surrogate keys throughout. The store already obeys
`.claude/skills/sql-relational-design/SKILL.md`.

Token usage in the store today: **zero columns**. The doc comment at
`src/main.rs:138` ("Acknowledge unacked lanes via cass and stamp token usage") on the
`Sweep` verb is stale; nothing in `ident.rs` reads or writes a token count.

Transcript corpus census, run 2026-08-09 over `~/.claude/projects/**/*.jsonl`:

| measure | value |
|---|---|
| transcript files | 1368 |
| of those, under a `subagents/` directory | 1076 (78.7%) |
| records carrying `message.usage` | 189999 |
| distinct `(message.id, requestId)` pairs | 94943 |
| duplication ratio | **2.00x** |
| records with `isSidechain: true` | 119127 (62.7%) |
| distinct model names | 8 |

Models seen: `claude-sonnet-5` 49590, `claude-opus-5` 44674, `claude-opus-4-8` 44387,
`claude-fable-5` 39749, `glm-5.2` 7896, `claude-haiku-4-5-20251001` 3526, `glm-4.7` 124,
`<synthetic>` 53. Full scan in Python: 4.8 s wall, so a Rust ingest pass over the whole
corpus is comfortably inside the 10-second law.

A live usage record, verbatim shape:

```json
{"type":"assistant","message":{"id":"msg_...","model":"glm-5.2","usage":{
  "input_tokens":8031,"cache_creation_input_tokens":0,"cache_read_input_tokens":13952,
  "output_tokens":239,"service_tier":"standard","speed":"standard",
  "cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":0},
  "server_tool_use":{"web_search_requests":0,"web_fetch_requests":0}}},
 "requestId":null,"timestamp":"2026-08-09T03:02:51.740Z","isSidechain":false}
```

Note `requestId: null` on the glm rows: 5521 of 6894 records in a 25-file sample carried a
`requestId`, every one carried `message.id`. The dedup key must tolerate a null request id.

---

## 2. Research: what other tools measure, and from what

### 2.1 ccusage

Repo `ccusage/ccusage` (was `ryoppippi/ccusage`). Rewritten in Rust; the TypeScript
`apps/ccusage/src/*.ts` paths from the older docs 404 today. Layout is one crate per
concern plus one adapter crate per harness, 16 adapters
(`rust/adapters/{claude,codex,opencode,amp,droid,codebuff,hermes,pi,goose,openclaw,kilo,kimi,qwen,copilot,gemini,common}`).
That is the same shape as boop's `registry.rs` + `harness/` trait.

| file | what it does |
|---|---|
| `rust/crates/ccusage-core/src/types.rs` | `UsageEntry` / `UsageMessage` / `TokenUsageRaw` / `TokenCounts` / `ModelBreakdown` / `UsageSummary`. `TokenCounts` has exactly four buckets: input, output, cache_creation, cache_read. `cache_creation_token_count()` prefers the `cache_creation.{ephemeral_5m,ephemeral_1h}_input_tokens` split and falls back to the flat `cache_creation_input_tokens`. |
| `rust/crates/ccusage-core/src/cost.rs` | `calculate_cost_for_usage` with a three-way `CostMode` (`Display` = only the harness-recorded `costUSD`; `Calculate` = always tokens x rate; `Auto` = recorded, else computed). `CACHE_CREATE_1H_INPUT_MULTIPLIER = 2.0`. Two-stage long-context pricing: a per-model `long_context_threshold` selects a tier for the whole request, not marginally. |
| `rust/crates/ccusage-core/src/pricing.rs` | `LITELLM_PRICING_URL` = the BerriAI raw JSON; `MODELS_DEV_API_URL` = `https://models.dev/api.json` as fallback; a build-time embedded snapshot (`include_str!(OUT_DIR/litellm-pricing.json)`); `DEFAULT_LONG_CONTEXT_THRESHOLD_TOKENS = 200_000`, `OPENAI_LONG_CONTEXT_THRESHOLD_TOKENS = 272_000`. |
| `rust/crates/ccusage-core/src/build.rs` | Downloads `model_prices_and_context_window.json` at build time behind the `fetch-litellm-pricing` feature, compacts it, writes `litellm-pricing.json`. Offline builds read a pinned local path from an env var. |
| `rust/adapters/claude/src/lib.rs:98-215` | Deduplication. `usage_dedupe_hash(message_id, request_id)` hashes the pair; `push_deduped_entry` keeps a `FxHashMap<u64, SmallIndexVec>` of hash to index and `should_replace_deduped_entry` decides whether a later record replaces an earlier one with the same key. A second index keyed on `(message_id, None)` catches sidechain rows that lost their request id. |
| `rust/crates/ccusage/src/blocks.rs:53-107` | `identify_session_blocks(entries, session_duration_hours)`. Sort by timestamp; a new window opens when `entry.ts - block_start > duration` **or** `entry.ts - last_entry.ts > duration`; the new window's start is `floor_to_hour(entry.ts)`; an idle stretch longer than the duration is emitted as its own `is_gap` row. |
| `rust/crates/ccusage/src/blocks.rs:567-604` | `calculate_burn_rate`: `tokens_per_minute = total_tokens / minutes(first..last)`, a second rate over input+output only for the indicator, `cost_per_hour = cost / minutes * 60`. `project_block_usage` extrapolates the open window: `total = current + rate * remaining_minutes`. |

Report shapes: `daily`, `weekly`, `monthly`, `session`, `blocks`, `statusline`, `mcp`.
Shared args `--since` / `--until` / `--json`. `UsageSummary` carries `models_used` and
`model_breakdowns` on every row, plus `first_activity` / `last_activity` on session rows.

### 2.2 Claude-Code-Usage-Monitor

Repo `Maciek-roboblog/Claude-Code-Usage-Monitor`, Python, `src/claude_monitor/`.

| file | what it does |
|---|---|
| `core/plans.py:51-95` | `PLAN_LIMITS` hard-codes per-plan ceilings: Pro 19000 tokens / \$18 / 250 messages; Max5 88000 / \$35 / 1000; Max20 220000 / \$140 / 2000; Team marked `unverified: True` with guidance to prefer the official statusline; Custom 44000 / \$50 / 250. `COMMON_TOKEN_LIMITS = [19000, 88000, 220000, 880000]`, `LIMIT_DETECTION_THRESHOLD = 0.95`. |
| `core/p90_calculator.py:17-48` | Derives a ceiling from history instead of a plan name. Keep past windows that are neither gaps nor active and whose total reached `>= limit * 0.95` for any known limit; take the ninth decile (`quantiles(hits, n=10)[8]`); fall back to all closed windows, then to a floor. |
| `core/calculations.py:34-92` | Same burn-rate and projection algebra as ccusage, plus `calculate_hourly_burn_rate` which sums every session's tokens landing inside a trailing one-hour window rather than inside one billing window. |
| `data/aggregator.py`, `data/analyzer.py`, `data/warehouse.py` | Read, aggregate, persist. `monitoring/orchestrator.py` + `monitoring/session_monitor.py` drive the refresh loop; `ui/` is a Rich terminal dashboard. |

What boop takes: the p90-derived ceiling (a plan name is a guess; the ninth decile of your
own history is a measurement) and the trailing-window burn rate as a separate thing from
the in-window burn rate.

### 2.3 sniffly

Repo `chiphuyen/sniffly`, Python + FastAPI, reachable. Structure from
`repo-structure.md`: `sniffly/core/{processor,stats,global_aggregator,constants}.py`,
`sniffly/api/{data,data_loader,messages,share}.py`,
`sniffly/utils/{memory_cache,local_cache,cache_warmer,pricing}.py`.
Endpoints: `/api/global-stats`, `/api/dashboard-data`, `/api/messages`, `/api/refresh`,
`/api/cache/status`. It derives error categories (its headline finding was that
"content not found" is 20-30% of Claude Code errors), interruption rate, tool usage
counts, per-command completion times, and a two-tier cache (in-memory LRU over a
file-backed store).

Not substituted; it was reachable.

What boop takes: the `/api/refresh` idea (a cheap change-detection door that answers in
milliseconds when nothing moved) is already boop's `sync_cursor` offset check, so nothing
new is needed. The error-category taxonomy is out of scope for this arc; noted in open
questions.

### 2.4 Claude Code OpenTelemetry export

Docs: `https://code.claude.com/docs/en/monitoring-usage`.

Enabled by `CLAUDE_CODE_ENABLE_TELEMETRY=1` plus `OTEL_METRICS_EXPORTER` /
`OTEL_LOGS_EXPORTER` / `OTEL_EXPORTER_OTLP_PROTOCOL` / `OTEL_EXPORTER_OTLP_ENDPOINT`.

Metrics, verbatim from the docs table:

| Metric Name | Description | Unit |
|---|---|---|
| `claude_code.session.count` | Count of CLI sessions started | none |
| `claude_code.lines_of_code.count` | Count of lines of code modified | none |
| `claude_code.pull_request.count` | Number of pull requests created | none |
| `claude_code.commit.count` | Number of git commits created | none |
| `claude_code.cost.usage` | Cost of the Claude Code session | USD |
| `claude_code.token.usage` | Number of tokens used | tokens |
| `claude_code.code_edit_tool.decision` | Count of code editing tool permission decisions | none |
| `claude_code.active_time.total` | Total active time | s |

Standard attributes on every metric and event: `session.id`, `app.version`,
`app.entrypoint`, `organization.id`, `user.account_uuid`, `user.account_id`, `user.id`,
`user.email`, `terminal.type`.

Events (`OTEL_LOGS_EXPORTER`): `claude_code.user_prompt`, `.assistant_response`,
`.tool_result`, `.api_request`, `.api_error`, `.api_refusal`, `.api_request_body`,
`.api_response_body`, `.tool_decision`, `.permission_mode_changed`, `.auth`,
`.mcp_server_connection`, `.internal_error`, `.plugin_installed`, `.plugin_loaded`.
Event-level attributes include `prompt.id`, `message.uuid`, `client_request_id`,
`workspace.host_paths`, `workflow.run_id`, `workflow.name`.

Beta tracing (`CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1` + `OTEL_TRACES_EXPORTER`) exports
spans `claude_code.interaction` (root, one per prompt), `.llm_request`, `.tool`,
`.tool.blocked_on_user`, `.tool.execution`, `.hook`, and propagates W3C `TRACEPARENT`
into Bash subprocesses and HTTP MCP requests.

Two things the OTEL door has that the transcript does not: `active_time.total` and
`lines_of_code.count`. Two things it does not have that the transcript does: the text of
what happened, and every harness that is not Claude Code. boop reads transcripts, so it
gets the second pair and must derive the first pair itself (active time from turn
timestamps, lines of code from `agent_span`).

### 2.5 OpenTelemetry GenAI semantic conventions

Repo `open-telemetry/semantic-conventions-genai`, moved out of the main semconv repo.

`model/gen-ai/metrics.yaml` metric names: `gen_ai.client.token.usage` (histogram,
`{token}`), `gen_ai.client.operation.duration` (s), `gen_ai.server.request.duration`,
`gen_ai.server.time_per_output_token`, `gen_ai.server.time_to_first_token`,
`gen_ai.invoke_workflow.duration`, `gen_ai.invoke_agent.duration`,
`gen_ai.invoke_agent.inference_calls` (`{inference_call}`),
`gen_ai.invoke_agent.tool_calls` (`{tool_call}`), `gen_ai.execute_tool.duration`.

`gen_ai.client.token.usage` requires `gen_ai.token.type` (`input` / `output`) and takes
`gen_ai.response.model` + `gen_ai.provider.name` from the `metric_attributes.gen_ai` group.

Attributes relevant here, from `model/gen-ai/registry.yaml`:
`gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`,
`gen_ai.usage.cache_read.input_tokens`, `gen_ai.usage.cache_creation.input_tokens`,
`gen_ai.usage.reasoning.output_tokens`, `gen_ai.request.model`, `gen_ai.response.model`,
`gen_ai.provider.name`, `gen_ai.operation.name`, `gen_ai.conversation.id`,
`gen_ai.agent.id` / `.name` / `.version`, `gen_ai.tool.name` / `.call.id` / `.type`,
`gen_ai.workflow.name`.

Two spec notes that change the arithmetic: `gen_ai.usage.cache_read.input_tokens` and
`gen_ai.usage.cache_creation.input_tokens` both say "SHOULD be included in
`gen_ai.usage.input_tokens`", and the token-usage note says "When systems report both used
tokens and billable tokens, instrumentation MUST report billable tokens." Claude
transcripts do the opposite of the first: `input_tokens` there **excludes** the cached
buckets (an 8031-input record alongside 13952 cache-read is the normal shape). So boop's
stored columns are the Anthropic-shaped four, and any OTEL-shaped export is a computed
view, never the storage. That is written down here because getting it backwards
double-counts every cached token.

Also there is a reference conformance scenario named `claude-agent-sdk` in
`reference/scenarios/claude-agent-sdk/`, so the mapping from Claude Agent SDK to these
conventions is already specified upstream.

### 2.6 Langfuse public API

Spec fetched from `https://cloud.langfuse.com/generated/api/openapi.yml` (472 KB,
OpenAPI 3.0.1). 69 paths. Resource model: `/api/public/traces`, `/observations`,
`/sessions`, `/scores`, `/models`, `/datasets`, `/prompts`, `/metrics`, `/ingestion`,
`/otel/v1/traces`.

Pagination is offset style: every list takes `page` (starts at 1) and `limit`, and every
paginated body is `{ data: [...], meta: utilsMetaResponse }` where `utilsMetaResponse` is
`{page, limit, totalItems, totalPages}`, all four required.

`GET /api/public/traces` filters: `userId`, `name`, `sessionId`, `fromTimestamp`,
`toTimestamp`, `tags` (array, AND semantics), `version`, `release`, and
`orderBy` as a single string `"[field].[asc|desc]"`.

`GET /api/public/metrics` is one endpoint taking a JSON-encoded `query` string with
`{view, dimensions[], metrics[{measure, aggregation}], filters[{column, operator, value,
type, key}], timeDimension{granularity}, fromTimestamp, toTimestamp, orderBy[], config{}}`.
Granularity enum: `minute | hour | day | week | month | auto`. Aggregations include
`count | sum | avg | p95 | histogram`.

What boop takes: the `{data, meta}` envelope and the time granularity enum. What boop
rejects: offset pagination (a log that grows under you skips and repeats rows across
pages) and the JSON-blob-in-a-query-string metrics API (it cannot be typed in OpenAPI and
cannot map to clap flags). boop uses `limit` + opaque `cursor` and a flat `group_by` enum.

### 2.7 LiteLLM pricing map

`https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json`,
one flat JSON object, **2988** model entries, fetched 2026-08-09. Per-entry shape for
`claude-opus-4-5`:

```json
{"input_cost_per_token":5e-06,"output_cost_per_token":2.5e-05,
 "cache_creation_input_token_cost":6.25e-06,
 "cache_creation_input_token_cost_above_1hr":1e-05,
 "cache_read_input_token_cost":5e-07,
 "litellm_provider":"anthropic","max_input_tokens":200000,
 "max_output_tokens":64000,"mode":"chat","prompt_cache_min_tokens":4096}
```

Rates are **per token**, not per million. There are separate one-hour cache-write fields
(`cache_creation_input_token_cost_above_1hr`), so the 2.0x multiplier ccusage hard-codes is
only a fallback for models that omit it. Provider-prefixed duplicates exist
(`anthropic.claude-opus-4-5-20251101-v1:0` for Bedrock at different rates), so lookup needs
alias resolution: ccusage keeps `model_aliases.rs` for that and strips 8-digit date
suffixes (`MODEL_DATE_SUFFIX_DIGITS = 8`).

Maintenance model: the file is community-maintained in the LiteLLM repo and updated by PR
as providers change prices. `models.dev/api.json` is the second source. Neither is
authoritative for Anthropic subscription plans, which are not per-token billed at all.

### 2.8 Multi-harness session managers

Found and skimmed for verb ideas:

| project | verbs / keys |
|---|---|
| `YoanWai/agent-manager` | `n` new session, `space` send a prompt to the selected session or spawn into a group, `enter` focus, `ctrl+r` diff review, `x` kill, `v` revive, `R` restart with fresh context, `f` fork into a named conversation, `T` shell tab, `alt+w` toggle worktree, `ctrl+q` detach, `c`/`C` comment a diff line and send comments back as a prompt |
| `timoclsn/agents-dashboard` | one-shot snapshot, watch, TUI. Detects agents by walking each tmux pane's process tree for `claude`/`codex`/`opencode` binaries, then classifies state by matching pane content against spinner and prompt patterns |
| `Ark0N/Codeman` | spawns agents in persistent tmux sessions, streams the terminal to a browser |
| `asheshgoplani/agent-deck` | TUI session manager, pools MCP processes over unix sockets across sessions |
| `Dicklesworthstone/ntm` | named tmux manager, tiles agents across panes, TUI command palette |

Verb ideas worth adopting: **revive** (a dead session's conversation is resumable, which
boop's `Capabilities.resume` already models), **fork** (branch a session into a new named
one), **restart with same config, fresh context**, and **pane capture as a first-class
read** (every one of these tools reads the pane; boop has `tmux.rs` capture but no verb).
Only pane capture is in this spec (`GET /lanes/{lane}/pane`); revive and fork are open
questions because they need adapter support that does not exist yet.

Note on state detection: agents-dashboard classifies liveness by regexing pane text.
boop's `agent_live` reads `~/.claude/sessions/<pid>.json` for pid, exact tmux pane and
busy/idle instead. That file exists (2 present today) and is the better source; the
regex approach is what boop's own `instant` consumer already got burned by
(`QUERY-SURFACE.md`, the replacement map).

### 2.9 What boop steals, and from whom

| stolen thing | from | where it lands |
|---|---|---|
| daily / session / model report shapes | ccusage `UsageSummary` | `GET /usage?group_by=day\|session\|model` |
| the four token buckets, with the 5m/1h cache-write split | ccusage `TokenCounts` + `TokenUsageRaw::cache_creation_token_count` | `agent_usage` columns |
| `(message.id, requestId)` dedup with last-write-wins | ccusage `usage_dedupe_hash` + `should_replace_deduped_entry` | `dict_request` + the upsert rule |
| the three-way cost mode | ccusage `CostMode` | `?cost_mode=auto\|calculate\|display` |
| gap-aware billing windows with hour-floored starts | ccusage `identify_session_blocks` | `GET /usage/blocks` |
| burn rate and projection algebra | ccusage `calculate_burn_rate` / `project_block_usage` | `GET /usage/burn-rate` |
| p90-derived token ceiling instead of a plan name | Claude-Code-Usage-Monitor `p90_calculator` | `?token_limit=p90` |
| trailing-window burn rate as distinct from in-window | Claude-Code-Usage-Monitor `calculate_hourly_burn_rate` | `?window_minutes=` |
| per-token rate table with 1h cache-write fields | LiteLLM `model_prices_and_context_window.json` | `model_price` table |
| `{data, meta}` envelope, time granularity enum | Langfuse | every list response |
| RFC 7807 errors, `limit`+`cursor` | general REST practice | every operation |
| pane capture as a read verb | agent-manager / agents-dashboard | `GET /lanes/{lane}/pane` |

Nothing here is bespoke where a common shape exists. The one genuinely odd operation is
`POST /lanes/{lane}/messages` (hail), covered in section 9.

---

## 3. Two blocking defects found in the current ingest

Both are prerequisites; the usage table inherits `(session_id, turn)` from them.

**D1: `turn` is a per-sync-run counter, not a stable session ordinal.**
`ident.rs:392` declares `let mut turn = 0u64;` inside `sync_session`, which runs once per
incremental sync from the stored byte offset. The second sync of a growing transcript
starts numbering at 1 again and collides with rows already stored. `add_turn`
(`ident.rs:110-120`) is `INSERT OR IGNORE`, so the colliding new rows are silently
dropped. Fix: persist the high-water mark (a `next_turn` column on `agent_session`, or
read `MAX(turn)` once per session at sync start) and seed the counter from it.

**D2: `turn` skips ordinals.** `*turn += 1` fires for every content block
(`ident.rs:472`), but `add_turn` runs only for `text` and `tool_use` blocks
(`ident.rs:474-495`). `thinking`, `tool_result` and image blocks burn an ordinal without
writing a row. Measured: of 1312 sessions, **1293 have `COUNT(*) < MAX(turn)`** and only
14 are exact. Gaps are harmless for a key but make `turn` useless as a count. Fix is a
choice, not a bug per se: either only increment when a row is written, or accept gaps and
never present `turn` as a count.

**D3 (minor, same family): `agent_edge.model_id` references a `dict_model` table that
does not exist.** `ident.rs:654` declares the column; there is no
`CREATE TABLE dict_model` in `SCHEMA` and no `dict_model` in the live db's `.tables`.
`add_edge` (`ident.rs:207`) never writes it. This design creates `dict_model` and the
column becomes real.

---

## 4. Schema: the token-usage tables

Read against `.claude/skills/sql-relational-design/SKILL.md`: integer surrogate keys
everywhere, every natural key stored once in a dictionary with `UNIQUE`, no composite TEXT
primary key, booleans as INTEGER 0/1, atomic columns.

```sql
-- New dictionaries. dict_model closes defect D3.
CREATE TABLE IF NOT EXISTS dict_model        (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_service_tier (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS dict_price_source (id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);

-- The composite natural key of an LLM call lives here ONCE, and only here.
-- request_id is NOT NULL with '' for absent, because SQLite treats NULLs in a
-- UNIQUE index as distinct and would let the same message land twice.
CREATE TABLE IF NOT EXISTS dict_request (
  id         INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL,
  request_id TEXT NOT NULL DEFAULT '',
  UNIQUE (message_id, request_id)
);

-- One row per deduplicated assistant record. Keyed (session_id, turn) like every
-- other fact table, per QUERY-SURFACE.md.
CREATE TABLE IF NOT EXISTS agent_usage (
  session_id             INTEGER NOT NULL,
  turn                   INTEGER NOT NULL,
  ts                     INTEGER NOT NULL,
  request_ref            INTEGER NOT NULL,   -- dict_request.id
  model_id               INTEGER NOT NULL,   -- dict_model.id
  service_tier_id        INTEGER,            -- dict_service_tier.id
  input_tokens           INTEGER NOT NULL DEFAULT 0,
  output_tokens          INTEGER NOT NULL DEFAULT 0,
  cache_create_5m_tokens INTEGER NOT NULL DEFAULT 0,
  cache_create_1h_tokens INTEGER NOT NULL DEFAULT 0,
  cache_read_tokens      INTEGER NOT NULL DEFAULT 0,
  is_sidechain           INTEGER NOT NULL DEFAULT 0,
  cost_usd_recorded      REAL,               -- the harness's own costUSD, when present
  PRIMARY KEY (session_id, turn)
) WITHOUT ROWID;

-- The dedup constraint AND the upsert conflict target.
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request   ON agent_usage(request_ref);
CREATE INDEX        IF NOT EXISTS idx_usage_ts        ON agent_usage(ts);
CREATE INDEX        IF NOT EXISTS idx_usage_model_ts  ON agent_usage(model_id, ts);

-- Rate table. Not a fact table: one row per model, replaced in place.
-- Rates are USD per MILLION tokens; LiteLLM's per-token values are multiplied
-- by 1e6 at ingest so the numbers are readable and REAL keeps full precision.
CREATE TABLE IF NOT EXISTS model_price (
  model_id                INTEGER PRIMARY KEY,  -- dict_model.id
  input_per_mtok          REAL NOT NULL,
  output_per_mtok         REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok     REAL NOT NULL,
  source_id               INTEGER NOT NULL,     -- dict_price_source.id
  fetched_ts              INTEGER NOT NULL
);
```

Design notes on the shape.

- **Five token columns, not four.** ccusage collapses the 5m and 1h cache writes into one
  `cache_creation_tokens` and then re-derives the 1h portion for pricing. Splitting at
  storage keeps the arithmetic exact and costs one integer column per row.
- **`cost_usd_recorded` is nullable and separate from any computed cost.** Computed cost
  is never stored: it is a join against `model_price`, so a rate correction retro-fixes
  history instead of needing a rewrite.
- **`is_sidechain` is stored, not derived.** 62.7% of records carry it; every analytics
  query wants to filter on it, and the alternative (walking `agent_edge` per row) is a
  join for a fact the source already gives.
- **No `project` column.** Grouping by project is `JOIN agent_session ON session_id` then
  `GROUP BY cwd_id`. `agent_session.cwd_id` already exists and already has an index
  (`idx_session_cwd`).
- **Size estimate.** 94943 deduplicated records today; 11 integer columns plus a REAL,
  `WITHOUT ROWID` with a 2-column integer key. Well under 10 MB, against an 88 MB db.

---

## 5. Ingest design

Where it hangs: `ident.rs::project_line`, which already walks every transcript record and
already has the `assistant` branch. It reads `message.usage` and emits one `agent_usage`
row per assistant record.

Which turn the row carries: the ordinal of the record's **first** emitted block. A usage
object belongs to the whole API response, not to one content block, and the first block is
the only ordinal that exists before the loop runs. This is a constraint, so it is stated
here and in the response schema rather than left implicit.

The dedup rule, taken from ccusage and corrected by its own issue #888:

```
intern (message.id, requestId ?? "") -> request_ref
INSERT INTO agent_usage (...) VALUES (...)
ON CONFLICT (request_ref) DO UPDATE SET
  output_tokens          = excluded.output_tokens,
  input_tokens           = excluded.input_tokens,
  cache_create_5m_tokens = excluded.cache_create_5m_tokens,
  cache_create_1h_tokens = excluded.cache_create_1h_tokens,
  cache_read_tokens      = excluded.cache_read_tokens,
  ts                     = excluded.ts
WHERE excluded.output_tokens >= agent_usage.output_tokens;
```

Last-write-wins guarded by a monotone `output_tokens`. ccusage's original rule kept the
**first** record for a key, and its issue #888 reports that recent Claude Code transcripts
write an intermediate usage snapshot first and the final `output_tokens` last, so
first-wins undercounts. The `WHERE` guard makes replay order irrelevant: whichever record
carries the largest output count wins, and re-running `sync --rebuild` converges.

Measured stakes: 189999 raw records against 94943 distinct keys. Counting raw records
overstates every number by 2.00x.

Batching: `sync` already wraps the pass in one transaction (`store.begin()` /
`store.commit()` in `run_follow`, `main.rs:582-591`). Usage rows ride that transaction. No
per-row commit, per the N+1 law.

Pricing refresh: a separate door, not part of ingest. `PUT /pricing/models/{model}` writes
a `manual` row. Fetching LiteLLM is a build-time embed plus an optional refresh; it is
network IO and must never sit inside the ingest path.

---

## 6. SQL sketch per analytics endpoint

Every sketch is answerable over the tables in section 4 plus what already exists. `:name`
marks a bound parameter.

**Common cost expression**, used verbatim in each aggregate below:

```sql
-- cost_usd, honouring ?cost_mode
CASE :cost_mode
  WHEN 'display' THEN COALESCE(usage.cost_usd_recorded, 0.0)
  ELSE COALESCE(
    CASE WHEN :cost_mode = 'auto' THEN usage.cost_usd_recorded END,
      usage.input_tokens           / 1e6 * price.input_per_mtok
    + usage.output_tokens          / 1e6 * price.output_per_mtok
    + usage.cache_create_5m_tokens / 1e6 * price.cache_write_5m_per_mtok
    + usage.cache_create_1h_tokens / 1e6 * price.cache_write_1h_per_mtok
    + usage.cache_read_tokens      / 1e6 * price.cache_read_per_mtok)
END
```

**`GET /usage`, no `group_by` (totals).**

```sql
SELECT SUM(usage.input_tokens)           AS input_tokens,
       SUM(usage.output_tokens)          AS output_tokens,
       SUM(usage.cache_create_5m_tokens) AS cache_create_5m_tokens,
       SUM(usage.cache_create_1h_tokens) AS cache_create_1h_tokens,
       SUM(usage.cache_read_tokens)      AS cache_read_tokens,
       SUM(<cost expr>)                  AS cost_usd,
       COUNT(*)                          AS record_count,
       MIN(usage.ts) AS first_ts, MAX(usage.ts) AS last_ts
FROM agent_usage AS usage
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
WHERE usage.ts >= :since AND usage.ts < :until;
```

**`GET /usage?group_by=model`** (ccusage's model breakdown).

```sql
SELECT dict_model.value AS model_name, <the same SUMs>, <cost expr>
FROM agent_usage AS usage
JOIN dict_model ON dict_model.id = usage.model_id
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
WHERE usage.ts >= :since AND usage.ts < :until
GROUP BY usage.model_id
ORDER BY cost_usd DESC;
```

**`GET /usage?group_by=day&timezone=...`** (ccusage's `daily`).

```sql
SELECT DATE(usage.ts / 1000, 'unixepoch', :tz_modifier) AS bucket, <SUMs>, <cost expr>
FROM agent_usage AS usage
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
WHERE usage.ts >= :since AND usage.ts < :until
GROUP BY bucket ORDER BY bucket;
```

`week` / `month` / `hour` swap the `DATE(...)` expression for
`STRFTIME('%Y-W%W', ...)`, `STRFTIME('%Y-%m', ...)`, `STRFTIME('%Y-%m-%dT%H', ...)`.
`:tz_modifier` is `'localtime'` or an explicit `'+HH:MM'`; SQLite has no IANA zone table,
so the daemon resolves the IANA name to a fixed offset for the requested range and passes
that. Stated as a constraint because a range spanning a DST change needs bucketing in Rust
instead.

**`GET /usage?group_by=session`** (ccusage's `session` report).

```sql
SELECT dict_session.value AS session_name,
       agent_session.nickname, dict_cwd.value AS project,
       <SUMs>, <cost expr>,
       MIN(usage.ts) AS first_activity, MAX(usage.ts) AS last_activity,
       GROUP_CONCAT(DISTINCT dict_model.value) AS models_used
FROM agent_usage AS usage
JOIN agent_session ON agent_session.session_id = usage.session_id
JOIN dict_session   ON dict_session.id = usage.session_id
JOIN dict_model     ON dict_model.id  = usage.model_id
LEFT JOIN dict_cwd  ON dict_cwd.id    = agent_session.cwd_id
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
WHERE usage.ts >= :since AND usage.ts < :until
GROUP BY usage.session_id
ORDER BY last_activity DESC;
```

**`GET /usage?group_by=project` / `=harness` / `=hour`**: same body, `GROUP BY
agent_session.cwd_id` / `agent_session.harness_id` / the hour expression.

**`GET /usage?rollup=subtree&session=X`** (no upstream tool does this).

```sql
WITH RECURSIVE subtree(session_id) AS (
  SELECT id FROM dict_session WHERE value = :session
  UNION
  SELECT edge.child_session_id
  FROM agent_edge AS edge JOIN subtree ON edge.parent_session_id = subtree.session_id)
SELECT <SUMs>, <cost expr>
FROM agent_usage AS usage
JOIN subtree ON subtree.session_id = usage.session_id
LEFT JOIN model_price AS price ON price.model_id = usage.model_id;
```

`UNION` (not `UNION ALL`) terminates on a cycle. This is the endpoint that pays for
`agent_edge`: 78.7% of transcript files are subagent transcripts, so a root session's real
cost is invisible without the rollup.

**`GET /usage/blocks`.** The window rule is sequential (a record opens a new window when
either `ts - window_start > W` **or** `ts - previous_ts > W`), so pure SQL needs a
recursive walk:

```sql
WITH RECURSIVE
  ordered AS (
    SELECT ROW_NUMBER() OVER (ORDER BY ts) AS seq, ts, session_id, turn
    FROM agent_usage WHERE ts >= :since AND ts < :until),
  walk(seq, ts, session_id, turn, window_start, prev_ts) AS (
    SELECT seq, ts, session_id, turn, ts / 3600000 * 3600000, ts
    FROM ordered WHERE seq = 1
    UNION ALL
    SELECT next.seq, next.ts, next.session_id, next.turn,
           CASE WHEN next.ts - walk.window_start > :window_ms
                  OR next.ts - walk.prev_ts     > :window_ms
                THEN next.ts / 3600000 * 3600000
                ELSE walk.window_start END,
           next.ts
    FROM ordered AS next JOIN walk ON next.seq = walk.seq + 1)
SELECT walk.window_start, MIN(walk.ts) AS first_ts, MAX(walk.ts) AS last_ts,
       <SUMs over agent_usage>, <cost expr>
FROM walk
JOIN agent_usage AS usage
  ON usage.session_id = walk.session_id AND usage.turn = walk.turn
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
GROUP BY walk.window_start ORDER BY walk.window_start;
```

The sketch is the contract. **The implementation folds the window in Rust** over a single
`ORDER BY ts` scan (which `idx_usage_ts` serves), because a recursive CTE is row-at-a-time
over ~95k rows and the 10-second law does not leave room to find out how slow that is in
production. Gap rows are emitted by the same fold: when a window closes because of
inactivity, a synthetic `is_gap` row covers `last_ts .. next_ts`. Both the CTE and the
fold must produce identical output on the same input; that equality is the test.

**`GET /usage/burn-rate`.**

```sql
SELECT MIN(usage.ts) AS window_first, MAX(usage.ts) AS window_last,
       SUM(usage.input_tokens + usage.output_tokens
         + usage.cache_create_5m_tokens + usage.cache_create_1h_tokens
         + usage.cache_read_tokens)                        AS total_tokens,
       SUM(usage.input_tokens + usage.output_tokens)        AS billable_tokens,
       SUM(<cost expr>)                                     AS cost_usd
FROM agent_usage AS usage
LEFT JOIN model_price AS price ON price.model_id = usage.model_id
WHERE usage.ts >= :window_start;
```

Then, in Rust, guarding a zero span:
`tokens_per_minute = total_tokens * 60000.0 / (window_last - window_first)`,
`cost_usd_per_hour = cost_usd * 3600000.0 / (window_last - window_first)`,
`projection.total_tokens = current + tokens_per_minute * remaining_minutes`.

**`?token_limit=p90`.** Over closed, non-gap windows from `/usage/blocks`:

```sql
-- window_totals is the block query's per-window total_tokens, closed windows only
SELECT total_tokens FROM window_totals
WHERE total_tokens >= :threshold_fraction * :nearest_known_limit
ORDER BY total_tokens
LIMIT 1 OFFSET (SELECT CAST(COUNT(*) * 9 / 10 AS INTEGER) FROM window_totals);
```

Fall back to all closed windows when the filtered set is empty, then to a floor.

**`GET /usage?follow=true` (realtime).** Rides the existing `follow` loop
(`main.rs:568-594`), which already polls session mtimes on a one-second tick and syncs
changed files inside one transaction. After each tick the daemon re-runs the aggregate
above with `:since = window_start` and emits one NDJSON or SSE frame. For the turn stream
the cursor is the key, not a timestamp:

```sql
SELECT session_id, turn, ts, role_id, said FROM agent_turn
WHERE (session_id, turn) > (:cursor_session, :cursor_turn)
ORDER BY session_id, turn LIMIT :limit;
```

Row-value comparison on the `WITHOUT ROWID` primary key is an index seek, not a scan.

**`GET /turns` and every fact list.** Same shape, cursor on the `(session_id, turn)`
primary key, `LEFT JOIN` the relevant `dict_*` to materialise TEXT at the read boundary
(never as a stored column, per the design law).

**Row counts to assert in tests.** The formerly-quadratic rule applies: each aggregate gets
an `EXPLAIN QUERY PLAN` test asserting `SEARCH` on `idx_usage_ts` or `idx_usage_model_ts`,
never `SCAN agent_usage`, and a statement-count test on the ingest path (one `INSERT` per
deduplicated record, no per-row `SELECT` beyond the dictionary interns).

---

## 7. The REST surface

`plans/2026-08-09-boop-openapi.yaml`, OpenAPI 3.1.0, 34 operations, 6 tags.

Cross-cutting decisions:

| concern | choice |
|---|---|
| pagination | `limit` (1..1000, default 100) + opaque `cursor`; body `{data, meta:{next_cursor, count}}` |
| time filters | `since` / `until` as RFC 3339 `date-time`, half-open `[since, until)` |
| sorting | `order=asc\|desc` on the row's natural key only; no free-form `orderBy` string |
| errors | RFC 7807 `application/problem+json`, declared as a `4XX` range response on every operation |
| streaming | `follow=true` on the collection; `Accept` picks `application/x-ndjson` (default) or `text/event-stream` |
| partial update | `PATCH` with `application/merge-patch+json` |
| idempotent write | `PUT` for a price row (keyed by model name), `POST` for lanes and messages |
| auth | bearer token from `~/.agent/boop-token`, loopback bind only |

Bulk delete: `DELETE /lanes?state=dead` rather than a `POST /lanes/prune` action. The
prune case is a conditional bulk delete and nothing more, so it is `DELETE` with a filter.
It answers `409` when tmux is unreachable, because the current `Prune` verb already
refuses in that case ("it cannot tell live from dead").

---

## 8. clap to REST, both directions

Naming rule: the CLI noun is singular, the path segment is its plural. The CLI verb is the
HTTP method (`list` = GET collection, `get` = GET item, `create` = POST, `patch` = PATCH,
`delete` = DELETE). Subcommand nesting is path nesting.

### `boop beep` (control) -> REST

| clap | method + path | operationId |
|---|---|---|
| `boop beep harness list` | `GET /harnesses` | harnessList |
| `boop beep harness get <harness>` | `GET /harnesses/{harness}` | harnessGet |
| `boop beep lane list [--state] [--harness]` | `GET /lanes` | laneList |
| `boop beep lane create --lane --cwd ...` | `POST /lanes` | laneCreate |
| `boop beep lane get <lane>` | `GET /lanes/{lane}` | laneGet |
| `boop beep lane patch <lane> --tmux ...` | `PATCH /lanes/{lane}` | lanePatch |
| `boop beep lane delete <lane>` | `DELETE /lanes/{lane}` | laneDelete |
| `boop beep lane delete --state dead` | `DELETE /lanes?state=dead` | laneDeleteMany |
| `boop beep lane route <lane>` | `GET /lanes/{lane}/route` | laneRouteGet |
| `boop beep lane pane <lane> [--lines]` | `GET /lanes/{lane}/pane` | lanePaneGet |
| `boop beep lane message list <lane>` | `GET /lanes/{lane}/messages` | messageList |
| `boop beep hail <lane> --body ...` | `POST /lanes/{lane}/messages` | hailCreate |
| `boop beep message ack [--lane] [--max-age-days]` | `POST /message-acks` | messageAckCreate |
| `boop beep ps` | `GET /processes` | processList |
| `boop beep ps <lane>` | `GET /processes/{lane}` | processGet |

### `boop db` (analytics) -> REST

| clap | method + path | operationId |
|---|---|---|
| `boop db session list` | `GET /sessions` | sessionList |
| `boop db session get <id>` | `GET /sessions/{sessionId}` | sessionGet |
| `boop db turn list [--follow]` | `GET /turns` | turnList |
| `boop db turn get <session> <turn>` | `GET /sessions/{sessionId}/turns/{turn}` | turnGet |
| `boop db chat list [--follow]` | `GET /chat` | chatList |
| `boop db touch list [--path] [--verb]` | `GET /touches` | touchList |
| `boop db command list [--program]` | `GET /commands` | commandList |
| `boop db fetch list [--domain]` | `GET /fetches` | fetchList |
| `boop db skill list` | `GET /skills` | skillList |
| `boop db pr list` | `GET /prs` | prList |
| `boop db span list [--path]` | `GET /spans` | spanList |
| `boop db edge list [--direction]` | `GET /edges` | edgeList |
| `boop db usage [--group-by day\|model\|session\|...]` | `GET /usage` | usageList |
| `boop db usage blocks [--window-hours] [--active]` | `GET /usage/blocks` | usageBlockList |
| `boop db usage burn-rate [--window-minutes]` | `GET /usage/burn-rate` | usageBurnRateGet |
| `boop db price list [--source]` | `GET /pricing/models` | priceList |
| `boop db price set <model> --input-per-mtok ...` | `PUT /pricing/models/{model}` | priceSet |
| `boop db sync create [--rebuild]` | `POST /syncs` | syncCreate |
| `boop db sync-cursor list` | `GET /sync-cursors` | syncCursorList |

### REST -> clap (the inverse table)

| method + path | clap |
|---|---|
| `GET /harnesses` | `boop beep harness list` |
| `GET /harnesses/{harness}` | `boop beep harness get <harness>` |
| `GET /lanes` | `boop beep lane list` |
| `POST /lanes` | `boop beep lane create` |
| `DELETE /lanes` | `boop beep lane delete --state <state>` |
| `GET /lanes/{lane}` | `boop beep lane get <lane>` |
| `PATCH /lanes/{lane}` | `boop beep lane patch <lane>` |
| `DELETE /lanes/{lane}` | `boop beep lane delete <lane>` |
| `GET /lanes/{lane}/route` | `boop beep lane route <lane>` |
| `GET /lanes/{lane}/pane` | `boop beep lane pane <lane>` |
| `GET /lanes/{lane}/messages` | `boop beep lane message list <lane>` |
| `POST /lanes/{lane}/messages` | `boop beep hail <lane>` |
| `POST /message-acks` | `boop beep message ack` |
| `GET /processes` | `boop beep ps` |
| `GET /processes/{lane}` | `boop beep ps <lane>` |
| `GET /sessions` | `boop db session list` |
| `GET /sessions/{sessionId}` | `boop db session get <id>` |
| `GET /sessions/{sessionId}/turns/{turn}` | `boop db turn get <session> <turn>` |
| `GET /turns` | `boop db turn list` |
| `GET /chat` | `boop db chat list` |
| `GET /touches` | `boop db touch list` |
| `GET /commands` | `boop db command list` |
| `GET /fetches` | `boop db fetch list` |
| `GET /skills` | `boop db skill list` |
| `GET /prs` | `boop db pr list` |
| `GET /spans` | `boop db span list` |
| `GET /edges` | `boop db edge list` |
| `GET /usage` | `boop db usage` |
| `GET /usage/blocks` | `boop db usage blocks` |
| `GET /usage/burn-rate` | `boop db usage burn-rate` |
| `GET /pricing/models` | `boop db price list` |
| `PUT /pricing/models/{model}` | `boop db price set <model>` |
| `POST /syncs` | `boop db sync create` |
| `GET /sync-cursors` | `boop db sync-cursor list` |

34 operations, 34 CLI paths, no orphan on either side.

### Migration from today's 16 verbs

| today | becomes | note |
|---|---|---|
| `harnesses` | `beep harness list` | |
| `sessions` | `db session list` | |
| `events` | `db turn list` | `--follow` replaces a second verb |
| `chat` | `db chat list` | `--all` becomes "no `--session` filter" |
| `tail` | `db turn list --session X --follow` | byte-offset tailing becomes an ingest detail |
| `sync` | `db sync create` | |
| `follow` | `db sync create --forever`, or `boop serve` | see open questions |
| `list` | `beep lane list` / `beep lane message list` | today's `list` returns both; split |
| `measure` | `beep ps` | |
| `dispatch` | `beep lane create` | |
| `lane` | `beep lane create` | `lane` and `dispatch` collapse into one create |
| `hail` | `beep hail` | name kept |
| `resolve` | `beep lane route` | |
| `adopt` | `beep lane patch` | |
| `sweep` | `beep message ack` | |
| `prune` | `beep lane delete --state dead` | |

`lane` and `dispatch` are today two spellings of one spawn (`lane` is documented as "the
dispatch wrapper (now via the trait)", PASS5-REPORT step 4). There is one create scenario,
so there is one `create`.

### The one clap attribute this needs

`boop db usage` is both a leaf (`--group-by model`) and a parent (`usage blocks`). clap
supports that with, on the `usage` command:

```rust
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
```

with the subcommand field typed `Option<UsageCmd>`. Without those two attributes the
parser rejects one of the two forms. Called out because it is the only place the 1:1
mapping needs a non-default clap setting.

---

## 9. Design decisions and the reason for each

| decision | reason |
|---|---|
| `hail` keeps its name; `POST /lanes/{lane}/messages` looks ordinary | The path is a plain sub-resource create because that is what the mailbox write is. The CLI name stays because the operation is not only a row write: it injects text into a live terminal owned by another process, and the response carries a delivery outcome (`injected` / `queued_for_next_spawn` / `unsupported`) that no ordinary create has. A verb called `message create` would hide the side effect. |
| `lane` + `dispatch` collapse to one `create` | Only one create scenario exists, per the standing instruction not to invent a bespoke verb for it. |
| `prune` becomes `DELETE /lanes?state=dead` | It is a conditional bulk delete. Nothing about it is special except the refusal when tmux is unreachable, which is a `409`, not a new verb. |
| `sweep` becomes `POST /message-acks` | The selection is across lanes, so the resource is the ack, not one message. `POST` because an ack is created, and repeated acks are naturally idempotent by the `acked` flag. |
| cursor pagination, not Langfuse-style `page`/`limit` | The turn table grows while you page it. Offset paging skips and repeats rows. The cursor is the `(session_id, turn)` primary key, so the next page is an index seek. |
| one `/usage` with `group_by`, not five report endpoints | ccusage's `daily`, `weekly`, `monthly`, `session` differ only in the bucket expression. One endpoint with an enum is the common REST shape and one clap flag. |
| `blocks` and `burn-rate` stay separate paths | Neither is a `GROUP BY`. Windowing is sequential and gap-aware; burn rate is a rate over a trailing span with a projection. Folding them into `group_by` would be a lie about what they compute. |
| store the Anthropic four-bucket shape, expose OTEL names as a view | The GenAI convention says cache buckets are *included in* `input_tokens`; Claude transcripts *exclude* them. Storing the convention shape would need a lossy conversion at ingest and would double-count on the way back out. |
| five token columns (5m and 1h cache writes split) | Pricing differs per bucket and LiteLLM carries `cache_creation_input_token_cost_above_1hr`, so the split is real money. One extra integer column per row. |
| computed cost is never stored | A rate correction retro-fixes history through the join. Only the harness's own `costUSD` is stored, in `cost_usd_recorded`. |
| `dict_request(message_id, request_id)` rather than TEXT columns on `agent_usage` | The composite TEXT natural key lives once, with `UNIQUE`, exactly the `sym(path, name)` shape the design law prescribes. `agent_usage` carries only the integer. |
| `request_id NOT NULL DEFAULT ''` | SQLite treats NULLs in a UNIQUE index as distinct. With NULL allowed, the 1373-of-6894 records that have no request id would each insert twice. |
| last-write-wins on `output_tokens`, not first-wins | ccusage first-wins, and its own issue #888 reports recent transcripts write an intermediate snapshot first and the final count last. The `WHERE excluded.output_tokens >= ...` guard also makes replay order irrelevant. |
| billing windows folded in Rust, CTE kept as the spec | The recursive CTE is row-at-a-time over ~95k rows. The 10-second law does not permit finding out how slow that is under load. Both must agree on the same input; that is the test. |
| `rollup=subtree` exists at all | 1076 of 1368 transcript files are subagent transcripts. Every upstream tool attributes usage to the file it was found in, so a coordinator session's real cost is invisible. `agent_edge` already carries the parent link. |
| `token_limit=p90` alongside a numeric limit | Plan ceilings are guesses that go stale (the monitor's own table marks Team `unverified`). The ninth decile of your own closed windows is a measurement. |
| the daemon resolves IANA zones to fixed offsets | SQLite has no zone table. A range crossing a DST change needs bucketing in Rust; stated so it is not discovered later. |
| `GET /lanes/{lane}/pane` is new | Every session manager surveyed reads the pane, `tmux.rs` can already capture it, and there is no verb. Read-only, so it is cheap. |
| bearer token on loopback | Any local process could otherwise spawn lanes and read every transcript through the daemon. |

---

## 10. Lint output

Run from the repo root, 2026-08-09.

```
$ npx -y @redocly/cli lint plans/2026-08-09-boop-openapi.yaml
No configurations were provided -- using built in recommended configuration by default.

validating plans/2026-08-09-boop-openapi.yaml...
plans/2026-08-09-boop-openapi.yaml: validated in 27ms

Woohoo! Your API description is valid. 🎉

$ npx -y @redocly/cli lint --extends=recommended-strict plans/2026-08-09-boop-openapi.yaml
validating plans/2026-08-09-boop-openapi.yaml...
plans/2026-08-09-boop-openapi.yaml: validated in 26ms

Woohoo! Your API description is valid. 🎉
```

`@redocly/cli` 2.46.0, node v24.15.0. Zero errors and zero warnings on both the default
`recommended` ruleset and `recommended-strict`.

---

## 11. Open questions

1. **`boop serve`.** The 1:1 mapping presumes a daemon. Where does it hang: a root verb
   (`boop serve --port 8420`), or under `beep`? It serves both trees, so a root verb is
   the accurate placement, but that makes a third top-level command beside `beep` and
   `db`.
   Needs a call.
2. **`follow` vs `serve`.** Today's `follow` is an infinite ingest loop with no exit. If
   `serve` runs ingest on its own tick, `follow` has no separate reason to exist. Same
   question as the standing `bop-run idle-exit vs rail receipts` item in CLAUDE.md.
3. **D1 and D2 (section 3) are prerequisites, not part of this arc.** Should the turn
   high-water fix land first as its own PR, or inside the usage arc? Landing usage on top
   of an unstable `(session_id, turn)` key means the usage rows inherit the collision.
4. **Turn attribution of a usage row.** Section 5 attaches usage to the first block's
   ordinal. The alternative is a dedicated ordinal space (`agent_usage.turn` counted
   separately from `agent_turn.turn`), which breaks the `QUERY-SURFACE.md` join contract.
   First-block is proposed; it needs the user's word.
5. **Pricing refresh policy.** Build-time embed (ccusage's approach, reproducible, stale
   between releases) or a cached fetch with a TTL (fresh, needs network and a failure
   path)? Build-time embed plus `PUT /pricing/models/{model}` for corrections is proposed.
6. **`glm-5.2` and `glm-4.7` have no LiteLLM rate row under those names** (7896 + 124
   records). They are z.ai models reached through a Claude-compatible endpoint. Cost for
   them is `null` with the model listed in `unpriced_models` until a manual rate is set.
   Confirm that is the wanted behaviour rather than a zero.
7. **Subscription billing is not per-token.** Every cost number here is a per-token
   reconstruction, which is what an API bill would have been, not what was actually paid
   on a Max plan. Should the API say so in a field (`cost_basis: "api_rate_card"`)?
8. **Error taxonomy from sniffly** (content-not-found, interruption rate, tool failure
   classes) is not in this design. It needs `tool_result` records, which
   `project_line` currently drops. Separate arc.
9. **Other harnesses.** Only `claude` is registered (`registry.rs::discover`). ccusage
   ships 16 adapters and parses codex / opencode / kimi token counts today; their usage
   record shapes differ (codex emits cumulative `total_token_usage` rows, which its own
   issue #884 reports as an overcounting trap). Worth reading
   `rust/adapters/codex/src/parser.rs` before writing boop's codex adapter.
10. **`active_time` and `lines_of_code`**, which Claude Code's OTEL export has and the
    transcript does not, are derivable (turn timestamp gaps under a threshold;
    `agent_span` line ranges). Not in this spec. Worth a later `GET /usage?group_by=` peer
    resource such as `GET /activity`.
