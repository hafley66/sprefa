# Lane: `boop db spend` reports flat-rate plan tokens as money

## Base
`git merge --ff-only 0b672fc1` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/boop-spend-flat-rate-split`.
`fix/boop-price-sync` (commit `b7d8e8d6`) added the code you are fixing. Merge or
cherry-pick it first; without it there is no `spend` command.

## The bug, measured 2026-08-12

`boop db spend` multiplies tokens by list price for EVERY model and prints one
total column. Models on flat-rate plan harnesses therefore read as spend.

```sql
select m.value, count(*), sum(u.cost_usd_recorded)
from agent_usage u join dict_model m on m.id = u.model_id group by 1;
```

| model | calls | cost_usd_recorded | reality |
|---|---:|---|---|
| gpt-5.6-sol | 42809 | NULL, all 42809 rows | ChatGPT plan, codex harness, flat rate |
| claude-opus-5 | 31507 | NULL | Claude plan, flat rate |
| claude-sonnet-5 | 26588 | NULL | Claude plan |
| claude-fable-5 | 22847 | NULL | Claude plan |
| claude-opus-4-8 | 19705 | NULL | Claude plan |
| glm-5.2 | 11484 | 0.0 | z.ai coding plan, flat rate |
| deepseek-v4-flash-0731 | 21116 | **45.74** | openrouter, metered, the only real money |

The report currently prints `gpt-5.6-sol ... 9994.86` as a total. Real metered
spend across the whole store is the deepseek row plus openrouter's own figure
($79.51 all time, $58.61 this month, from `/api/v1/key`).

## Scope, exactly

1. Split the report into metered and plan-billed. A model whose
   `cost_usd_recorded` is NULL or 0.0 across all its rows is plan-billed.
2. Plan-billed rows show token counts and keep the dollar columns BLANK, or show
   them under a column whose header says the number is notional list price.
   Never in the same total column as metered money.
3. The grand total counts metered rows only.
4. Keep cache reads broken out as their own column. That split is the whole
   reason the table exists: flash4 is 79% cache reads and pro4's cache reads are
   4.4x cheaper, which nearly cancels a 5.4x input-rate gap.

## Anchors
- store: plain SQLite at `~/.agent/boop.db`
- `v6/boop/src/price.rs` (new, from `b7d8e8d6`), `v6/boop/src/usage.rs`,
  `v6/boop/src/main.rs` (`boop db price sync`, `boop db spend`)
- `agent_usage` columns: `input_tokens`, `output_tokens`,
  `cache_create_5m_tokens`, `cache_create_1h_tokens`, `cache_read_tokens`,
  `cost_usd_recorded`
- `model_price` has 23 rows; 8 `dict_model` spellings are unpriced
- inspect the real schema before writing; do not trust this list

## Laws
- boop NEVER reinvents SQLite or SQL. `boop db "<sql>"` is the query surface.
  Canned reports are named SQL, visible and deletable. No query-flag DSL.
- Surrogate keys: INTEGER ids, natural TEXT keys once in a dictionary with
  UNIQUE. Read `.claude/skills/sql-relational-design` first.
- Infra is bought, never built.
- The 10-second law: the current report is 0.28s over ~220k rows. Keep it there.
- No `eprintln!` in src/**, `tracing` only. CLI-UX lines carry `@eprintln-ok`.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Files you own
`v6/boop/**`, plan doc `plans/2026-08-12-boop-spend-flat-rate-split.md`.

## Files you must NOT touch
`v6/prolog/**`, `v6/sprefa-engine-rs/**`, `v6/justfile`, any Cargo.toml outside
`v6/boop/`. Other lanes own those.

## Gates
`cargo test --no-fail-fast` in `v6/boop`. Two failures pre-exist:
`lane::tests::a_gpt_model_names_the_codex_harness` and
`lane::tests::an_unnamed_harness_never_guesses_opencode`. Measure three times.

## Report
The new table as it actually prints, the metered grand total, and the test counts.
