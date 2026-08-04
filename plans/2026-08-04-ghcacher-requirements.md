# ghcacher requirements (user-worded 2026-08-04) vs what stands

User words: "checkout all my repos from an org from some config file that can
be overriden by env, polls on a tiered timer, recent git activity gets polled
more frequently at max 1min each, slow ones get pulled every N/relational
minutes based on facts about it; are we caching/respecting gh 304s/etags for
PR info; get PR info as optimally as possible, REST or graphql points,
hundreds of repos, 5000 pts/hour." And: "as long as its adjustable we are
fine."

## TOC
1. [Gap table](#1-gap-table)
2. [Tiered polling: expressible today, zero new constructs](#2-tiered-polling-sketch)
3. [Rate-budget doctrine to verify](#3-rate-budget-doctrine-to-verify)
4. [Work items](#4-work-items)

## 1. Gap table

| requirement | stands today | gap |
|---|---|---|
| org repo set discovered | crawl_org.dl6 graded: local dir enumeration; gh_repos (slug list) written, ungraded | CLONE host = named absence (slug -> checkout is a write effect: cache dir, retry) |
| config file + env override | org arrives as a want_org ARRIVAL ROW (runtime-adjustable by POST, follows the no-defaults design line: a coordinate is a row) | no file/env layer; if wanted it is a tiny host that reads the file and emits want_org rows, env override = which file the host reads |
| tiered polling, hot repos <=1min, cold every N relational minutes | interval cadence is a literal in a rule body (adjustable by editing the rule); ghcacher_tick_golden grades the clock+etag loop hermetically | no tier logic anywhere; see §2, the language already carries it |
| etag/304 respected | the SHAPE is graded: current_etag key(1) latch feeds prev tag into the fetch host, only status 200 refreshes cache_view, non-200 = zero delta (goldens/ghcacher_tick_golden) | the LIVE transport (curl/gh with If-None-Match) is an unwritten host body; the golden proves the engine side only |
| PR info, point-optimal at 100s of repos / 5000 pts/hr | nothing: no PR host exists | see §3 doctrine; needs a receipts pass before any lane |

## 2. Tiered polling sketch

Zero new constructs: one fast base clock, tiers are derived rels over facts,
witness freshness makes a non-due repo cost nothing. SKETCH, surface spelling
to be priced (arithmetic/modulo forms not yet confirmed on the tick plane):

```dl6
rel repo_activity(root: text, last_commit_bucket: int) key(1).
rel due(root: text, bucket: int).

# hot tier: touched within the hour -> every tick (1-min base clock)
due(root, bucket) <-
  interval(60, bucket), repo(root),
  repo_activity(root, last_commit_bucket),
  bucket - last_commit_bucket < 60.

# cold tier: everything else -> every 30th tick, relational and editable
due(root, bucket) <-
  interval(60, bucket), repo(root),
  repo_activity(root, last_commit_bucket),
  bucket - last_commit_bucket >= 60, bucket mod 30 == 0.
```

rx lowering (per the snippet law): `interval(60_000).pipe(withLatestFrom(repoActivity$), mergeMap(tickRows => tickRows.filter(inTier)), ...)`
with the poll join downstream exactly as crawl_org.dl6 already lowers; the
fetch host only re-fires for rows whose witness (root, bucket) is new.

Adjustability, the user's stated bar: every tier boundary above is a literal
in a rule body or a fact row, never engine config. Editing cadence = editing
data or one rule.

## 3. Rate-budget doctrine to verify

Stated from memory, VERIFY against GitHub docs before any lane builds on it:
- REST conditional requests answering 304 are free against the primary limit;
  GraphQL ignores ETags entirely.
- REST and GraphQL draw from SEPARATE 5000/hr pools (GraphQL cost is
  query-shaped, not per-call); splitting detection from hydration stretches
  the budget.
- Consequent shape: cheap conditional REST GETs as the change detector
  (304s free at hundreds of repos), batched GraphQL hydration only for repos
  that actually changed. Both cadences and the split itself stay relational
  (rows + rules) per §2.

## 4. Work items

| item | shape |
|---|---|
| clone host (slug -> checkout, cache dir, retry) | design first: write-effect host class is new ground |
| config-file/env want_org feeder host | tiny, flash-shaped once specced |
| tier rels + repo_activity facts feed | sketch above -> fixture beside ghcacher_tick_golden |
| live conditional-fetch host body (If-None-Match) | small; golden already grades the engine side |
| PR-info host + rate doctrine receipts | opus receipts pass on §3 claims, then design |
