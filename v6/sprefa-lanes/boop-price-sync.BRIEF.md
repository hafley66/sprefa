# BRIEF: populate boop's model_price from litellm plus openrouter

Small task. One subcommand, two sources, a report. Do not redesign the store.

## Base
Confirm the base with `git log --oneline -1` before your first commit. Branch
`fix/boop-price-sync` off origin/main.

## The defect

`boop db "select count(*) from model_price"` returns **0**. The table exists with
the right columns and nothing has ever filled it, so boop cannot answer "what did
this cost". I priced a comparison by hand today from a web page; that should be
`boop db`'s job.

The schema already there, do not change it:

```sql
CREATE TABLE model_price (
  model_id INTEGER PRIMARY KEY,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_write_5m_per_mtok REAL NOT NULL,
  cache_write_1h_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL NOT NULL,
  source_id INTEGER NOT NULL,
  fetched_ts INTEGER NOT NULL
)
```

`model_id` joins `dict_model`. `source_id` joins `dict_price_source`, which
already exists. Note the units: **per million tokens**, while both upstream
sources publish **per token**. Convert on the way in.

## Two sources, and you need both

**1. litellm** (what ccusage uses):
`https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json`

Measured today: 3003 keys, an object keyed by model name. Relevant fields per
entry: `input_cost_per_token`, `output_cost_per_token`,
`cache_read_input_token_cost`, `cache_creation_input_token_cost`,
`litellm_provider`, `mode`. A `sample_spec` key documents the schema; skip it.

**2. openrouter** `https://openrouter.ai/api/v1/models`

litellm alone is NOT enough, measured today: it has 160 deepseek keys and its v4
entries are only `azure_ai/deepseek-v4-pro`, `azure_ai/deepseek-v4-flash` and
`fireworks_ai/...`. The spellings this machine actually runs,
`deepseek/deepseek-v4-flash-0731` and `deepseek/deepseek-v4-pro-0813`, appear
only in openrouter's list. Verified prices from openrouter today, per token:

| model | prompt | completion | cache read |
|---|---|---|---|
| `deepseek/deepseek-v4-flash-0731` | 0.00000008 | 0.00000018 | 0.000000016 |
| `deepseek/deepseek-v4-pro-0813` | 0.000000435 | 0.00000087 | 0.000000003625 |

Note pro's cache read is CHEAPER than flash's. If your sync produces a row that
disagrees with those six numbers, your parser is wrong, not the source.

## Deliverable

`boop price sync`, or whatever spelling fits the existing CLI shape, that:
- fetches both sources
- matches each row in `dict_model` to a price, recording WHICH source matched in
  `source_id` so a later reader knows
- writes per-million-token values with `fetched_ts`
- prints a summary: how many models priced, how many unmatched, and the
  unmatched names

Matching is the real work. `dict_model` holds spellings like `claude-opus-5`,
`deepseek/deepseek-v4-flash-0731`, `gpt-5.6-terra`, `kimi-code/k3`, and
`unknown`. Decide how aggressive to be, and prefer leaving a model UNPRICED over
guessing a wrong match. State your matching rule in the plan doc.

Then one canned report, named SQL, visible and deletable, per the standing law
that boop never invents query DSLs: spend by model over a time window, joining
`agent_usage` to `model_price`.

## Constraints
- boop never reinvents SQLite or SQL. `boop db "<sql>"` stays the query surface.
- Infra is bought, never built. Use whatever HTTP and JSON crates boop already
  depends on; add nothing if an existing dep covers it.
- Cache the fetched JSON so a sync is not a network round trip every run. Say
  where and for how long.
- Offline behaviour: a sync with no network must fail clearly, never write
  partial or zero prices over good ones.

## Acceptance, paste the output
1. `boop price sync` runs and reports counts
2. `boop db "select count(*) from model_price"` is greater than zero
3. the six deepseek numbers above round-trip correctly as per-mtok values
4. the spend report returns a plausible number for a real day of usage
5. `cargo test` passes; report the counts. Two `lane::tests` failures are known
   and pre-existing, do not chase them
6. `.github/CI-KNOWN-RED.md` allowlists the red green-all legs; read it before
   calling any leg broken

## File ownership
YOURS: `v6/boop/**` and one plan doc. Everything else READ ONLY.

## Style laws
- No em dashes. Banned in prose and identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`.
- "refusal" banned in prose; say TODO or not built yet.
- Comments state only constraints the code cannot show. No dates, no narrative.
- No sycophancy, no negative parallelism ("not X, Y").

## Worktree setup, before your first commit
Try `just boop-start` first; it may now exist. Otherwise:
```
(cd v6/sprefa-extract && cargo build --release --features cli --bin extract)
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
