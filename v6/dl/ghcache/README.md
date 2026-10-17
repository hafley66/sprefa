# ghcache: ghcacher, verbatim, in dl6

`~/projects/ghcacher` (Rust, 3339 lines under `src/`) as one dl6 program.
Every ability is mapped below, one row per ability, `original file:line -> rel
or rule`. Nothing is dropped silently: the things that do not carry over have
their own section with a throw site each.

## Contents

- [Status](#status)
- [The one transport](#the-one-transport)
- [How to run it](#how-to-run-it)
- [The rate budget](#the-rate-budget-the-whole-point)
- [The account type, and backing off a 404](#the-account-type-and-backing-off-a-404)
- [Retention](#retention)
- [Ability map](#ability-map)
- [The 16 tables](#the-16-tables)
- [Storage law](#storage-law)
- [What the language answered that the brief expected to build](#what-the-language-answered-that-the-brief-expected-to-build)
- [Deviations, each with its throw site](#deviations-each-with-its-throw-site)
- [Executors](#executors)

## Status

| leg | state |
|---|---|
| `ghcache.dl6` compiles through the Rust emitter | yes, 2.6s |
| the ETag, the 304 body, the page walk, the token, the GraphQL query | RULES, not executor code |
| `src/executors/{fetch,graphql,pulls,repos}.rs` | DELETED; `http.rs` is the whole transport |
| the six `v6/dl/ghcacher` goldens | on `http.get`, gate green, `goldens=6` |
| simulated schedule through the Rust door | `GHCACHE_RUST_DOOR_HOLDS ticks=14`, `pr_transition_open_merged=1`, COUNT receipt below |
| the account-type split and the 404 backoff | `account_ticks=14`, `org_events=0 user_events=7 ghost_calls=3 ghost_due=0 user_repos_due=1`, gate leg 2 |
| live `dl6 run` against `hafley66` (instant, sprefa, hafley-rs, hafley-rxjs) | ONE call per endpoint per bucket; a quiet bucket is 9 x 304 / bytes=0 |
| kill + restart, first poll | 8 x 304, bytes=0, out of 8 stored ETags and 8 stored bodies |
| the GraphQL pull-request batch | live: `ghcache_pull_request` holds every open PR of the four repos, `_recent` selection (#425) sees merges |
| open -> merged, live | PR #426 opened 06:29, captured open at tick 30, merged 06:31:28, `v_pr_transition` row `hafley66/sprefa 426 open merged at_tick=49` by 06:33:50; resident process alive throughout (#423 dirty set, #424 trace armed); cold-start page-walk ticks 26 and 43 took 7.3 s and 8.3 s, every other tick ~1.1 s |

Two unit bugs closed, both the same shape: a value in SECONDS compared against
a clock bucket in MINUTES.

| issue | rule | fix |
|---|---|---|
| `ghcache-dl6-poll` | `period_candidate` | `ceil(seconds / clock_granularity)` buckets; `poll_interval_seconds=60` is due every minute bucket, not every 60 |
| found live, same family | `over_budget` | `x-ratelimit-reset` is epoch SECONDS, `Bucket` epoch MINUTES; raw, every reset was in the future and one stop never released |

`gh_username` had no rule at all (`ghcache-gh-username-unsourced`), so the
org-events endpoint never polled. It is one `http.get` of `user` and a keyed
fold of `login` now.

## The one transport

```
rel http.get(url: text, headers: text, prev_etag: text, bucket: int)
  -> (status: int, response_headers: json, body: json, bytes: int).
rel http.post(url: text, headers: text, request_body: text, bucket: int)
  -> (status: int, response_headers: json, body: json, bytes: int).
```

The request IS the row. `headers` is a JSON object the program built with the
`json_object/2` aggregate over a `request_header(page_url, name, value)` rel,
so `Authorization` and `If-None-Match` reach the wire only because a rule put
them there. `src/executors/http.rs` holds no ETag map, no page walk, no token
lookup and no 304 body substitution: its only state across calls is the `ureq`
connection pool.

Three contract points a reader needs.

| point | why |
|---|---|
| the output column is `response_headers`, not `headers` | `disjoint_columns` (`1_host_expand.pl`) refuses one name on both sides of `->` |
| `headers` and `request_body` are `text`, not `json` | every identity input is concatenated into the witness digest, and `compile_concat_part` (`lower.pl:1050`) refuses a `json` piece |
| a whole-number response header is a JSON NUMBER | `decode(.., X: int)` reads a number and never a string (the no-coercions law), measured: `{"x-ratelimit-remaining":"150"}` decodes to zero rows at `: int` |

`prev_etag` shapes no header. It is demand identity, and demand identity may
not move while the question stands. `poll_state_etag(page_url, etag,
asked_etag, at_bucket) key(1)` carries the tag the last answer gave AND the tag
that answer was asked with, and `page_prev_etag` picks between them in three
exclusive arms:

| the request reads | when |
|---|---|
| `etag` | `at_bucket < Bucket`: the stored row predates this bucket |
| `asked_etag` | `at_bucket == Bucket`: this bucket's own answer already landed |
| `""` | no stored row at all: a never-polled page |

Arms, not a `coalesce`: a coalesce over a DERIVED rel lets the read and the
negation disagree for one tick, and that tick's demand is claimed before they
settle. The header and the identity column both read `page_prev_etag`, so one
tick cannot mint a demand whose header says one thing and whose identity says
another. One page is ONE wire call per bucket, changed or not, and
`page_arrival` carries `prev_etag` so writing the keyed rel is one row per
answer.

Known residual: the FIRST bucket flip after a page's tag is first stored mints
one extra demand, because the keyed write and the `not(poll_state_etag(...))`
arm land in the same tick. One extra conditional call per page per db
lifetime; the scripted gate shows it as the second bucket's second row.

Before this, the tag fed the header directly and GitHub answers one resource
with `W/"tag"` and `"tag"` depending on the request, so the two spellings
chased each other with zero wire traffic. Measured live: a period-4 cycle on
`.../events?page=3` with `rate_remaining` flat at 4967/4964/4961/4956 and
`change_log` gaining 64 rows every 6 drain ticks (failure-modes entry 77).

## How to run it

`dl6 run` landed with PR #407 and folds into the ONE db, `~/.agent/dl6.db`,
tables carrying the program's own name (CLAUDE.md 2026-08-21). There is no
per-program db flag.

```bash
swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile_dl6('v6/dl/ghcache/ghcache.dl6','/tmp/ghcache.rs',[emitter(emit_rust:emit_program)])" -g halt
DL_ADAPTERS_DIR=v6/dl/ghcache RUST_LOG=sprefa_engine_rs=info \
  timeout 60 v6/sprefa-engine-rs/target/debug/emit_rust_harness \
  /tmp/ghcache.rs v6/dl/ghcache/ghcache.schedule.json --live-hosts --final
```

## The rate budget (the whole point)

The user's ask was "i dont want my github points dying every time i poll
ineffectively". Three properties hold structurally rather than by convention.

```
tick 0   rate_state = {}                  due = {events, branches}   2 GETs, both 200
tick 1   rate_state = {rest, 4998, R}     due = {}                   0 GETs   (period 60 ∤ 1)
...
tick 60  rate_state = {rest, 4998, R}     due = {events, branches}   2 GETs, both 304, 0 body bytes
tick 61  poll_state_period = 90 (X-Poll-Interval seen)               endpoint_period = max(60, 90) = 90
tick 120 due = {}                         0 GETs   (90 ∤ 120)
tick 180 due = {events, branches}         2 GETs
--- the pool drops below rate_warn_threshold ---
tick 181 rate_warm(rest)                  endpoint_period = max(60, 90, 60*W) = 60*W
--- the pool drops to rate_stop_threshold, reset_at = 2000 ---
tick 182 over_budget(rest, 182)           due = {}                   0 GETs
...                                        (steady state until 2000)
tick 2000 not over_budget                 due resumes
```

1. **A poll that is not due derives no row in `due`**, so `gh_rest_cond` is
   never demanded and no request leaves the process. Absence of demand, not a
   suppressed call. This is the tier golden's shape
   (`v6/dl/ghcacher/ghcacher_tier_golden.dl6:43-47`).
2. **`over_budget` anti-joins into `due`.** Below `rate_stop_threshold` and
   before `rate_reset`, the poll plane is empty. `gh.rs:161-165` slept a thread
   here; nothing sleeps in this program.
3. **`endpoint_period` is a `max` over candidates**, so `X-Poll-Interval` and
   the warn-band stretch can only SLOW polling. No rule can shorten a period
   below the configured floor, because a shorter candidate loses the max.

## The account type, and backing off a 404

An `[[org]]` config row names an OWNER, and GitHub answers a different endpoint
set depending on whether that owner is an org or a user. Two endpoint families
differ by account type, and until 2026-08-24 only one of them knew it.

| family | org account | user account |
|---|---|---|
| repo discovery | `orgs/<owner>/repos` | `users/<owner>/repos` |
| the events firehose | `users/<me>/events/orgs/<owner>` | `users/<owner>/events` |

`not_an_org(owner) key(1)` is the switch, folded from the `/orgs` 404 and never
retired. Both families read it, in exclusive arms, so the watched SET is
untouched: hafley66 stays watched whole and only the spelling adapts.

Two defects, both measured in `~/.agent/dl6.db` over 1,427 minute buckets:

| what | rows | why |
|---|---|---|
| `users/hafley66/events/orgs/hafley66` 404 | 1,422 | the events family had no user-account arm |
| `orgs/hafley66/repos?per_page=100` 404 | 24 | `not_an_org` read its 404 off `rest_response`, which has a 200 arm and a 304 arm and NO OTHER, so the rule was statically dead and `ghcache_not_an_org` was empty |

The backoff is `retryWhen` with a delay, spelled as rels. `miss_streak` counts
consecutive 404s on PAGE 1 of an endpoint, keyed on the endpoint; the two
`miss_prior` arms are exclusive on `at_bucket`, the `page_prev_etag` shape, so a
bucket's extra drain ticks cannot run the counter up. Any non-404 answer resets
the streak to 0. At `miss_threshold(3)`, `endpoint_cooling` anti-joins into
`due` until `miss_cooloff(60)` buckets past the LAST miss, so a probe that 404s
again pushes the resume bucket out by another cool-off instead of resuming every
bucket. A permanently dead endpoint costs 3 calls, then 24 a day.

### The match form

Five clause-pairs in this program are `match` blocks, one per branch this arc
touched. The scrutinee parses as a HEAD atom, so it is full arity or
`partial_head` (`parse_dl_dcg.pl:1400`); the arm guard is an ordinary body, so
it carries rel reads, `not(...)` and `:=` beside its comparison.

| scrutinee | arms |
|---|---|
| `org_owner` | `watched_global` `org_repos` / `user_repos` |
| `org_config` | `watched_global` `org_events` / `user_events` |
| `page_response` | `endpoint_miss` / `endpoint_hit` |
| `endpoint_miss` | `miss_next`, prior streak / no stored row |
| `page_response` | `call_candidate`, 304 / not 304 |

Every other branch in the file is left as clauses: this arc did not touch them.
The `call_candidate` graphql arm stays a clause too, because its scrutinee is
`pr_batch_response` and a match block has one.

## Retention

Three telemetry logs are bounded; `pr_transition` is the record of what changed
and stays `keep(all)`. The Ns are one measured rate times 1,440 buckets.

| rel | rows/bucket measured | keep | hours |
|---|---:|---:|---:|
| `engine_tick_cost` | 93.23 (243 executors) | 140000 | 25.0 |
| `change_log` | 22.29 | 34000 | 25.4 |
| `call_log` | 10.71 | 16500 | 25.7 |

`call_log` had three `<+` arms and `retention_head_conflict_risk`
(`0_program_check.pl:666`) refuses two or more on a bounded log head, so the
three fold into a `call_candidate` LEVEL rel first and one edge arm stamps it.
`call_candidate` carries `bucket`, which is the granularity the three arms
already had through `page_response`, so an identical answer in a later bucket is
still a positive delta and still one log row.

The prune is `DELETE ... WHERE rowid NOT IN (SELECT rowid ... ORDER BY rowid
DESC LIMIT N) RETURNING`, run at tick end. MEASURED at the `engine_tick_cost`
bound: 59 ms at 150,093 rows deleting 93, against a tick interval of ~13 s.

## Ability map

### the conditional GET and its bookkeeping

| ghcacher | dl6 |
|---|---|
| `gh.rs:65-68` `with_etag` -> `If-None-Match` | `poll/3` carries `prev_etag`; `gh_rest_cond`'s second input |
| `gh.rs:90` `is_not_modified` | `Status == 304` guard on `call_log`'s cache-hit arm |
| `gh.rs:92-110` etag/last-modified/poll-interval/rate accessors | `gh_rest_cond` output columns, `executors/fetch.rs:151-176` |
| `gh.rs:174-269` `call` | `rest_response/11` |
| `gh.rs:180-189` `--paginate` | `executors/fetch.rs:follow_link_next`, `pages` output column |
| `gh.rs:392-404` `parse_paginated_body` | same function, array concat |
| `gh.rs:271-330` `graphql` | `pr_batch_response/8`, `executors/graphql.rs` |
| `gh.rs:378-390` `inject_rate_limit` | `graphql.rs:query_for`, unconditional `rateLimit` selection |
| `gh.rs:143-172` `throttle_if_needed` | `over_budget/2` + `rate_warm/1` + `endpoint_period/2` |
| `gh.rs:147-154` latest call_log row | `rate_state/3`, a rel keyed on `api_type` |
| `gh.rs:161-165` stop band (sleep to reset) | `over_budget/2`, `not(...)` into `due/3` |
| `gh.rs:166-168` warn band (sleep 10s) | `rate_warm/1` -> the stretch `period_candidate` |
| `db.rs:190-225` `log_call` | `call_log/8` `log keep(all)` |
| `db.rs:136-186` `get/set_poll_state` | `poll_state_{etag,modified,period,polled,changed}` + `poll_state/6` |
| `db.rs:173-175` `COALESCE(excluded.x, poll_state.x)` | not writing a keyed rel: it keeps the value it had |
| `db.rs:245-266` `log_change` | `change_log/4`, fed by `change_candidate/3`'s positive delta |

### config

| ghcacher | dl6 |
|---|---|
| `config.rs:151-177` search order | `config_candidate/2` rows, `best_config_rank(min(...))` |
| `config.rs:142-149` `load` | `config_doc/2` through the `toml_json` host |
| `config.rs:16-41` `[global]` | `global_setting/8` |
| `config.rs:64-79` `[[repo]]` | `repo_config/9` |
| `config.rs:83-98` `[[org]]` | `org_config/7`, `org_exclude/2` |
| `config.rs:19` `poll_interval_seconds` | `global_setting.poll_period`, the floor candidate |
| `config.rs:21` `org_repo_discovery_interval_seconds` | its own `period_candidate` arm, `org_repos`/`user_repos` only |
| `branches_poll_interval_seconds` (optional, default 60) | `branches_period_setting/1`, coalesced to `poll_period` in the `branches` arm |
| `config.rs:25-27` warn/stop thresholds | `api_tier/3` |
| `config.rs:105-108` `fs_owner`/`fs_alias` | `watched_repo.fs_alias`, `checkout_task.dest_root` |

### sync

| ghcacher | dl6 |
|---|---|
| `sync/mod.rs:43-104` `discover_org_repos` | `watched_global(_, _, 'org_repos')` + `discovered_repo/3` |
| `sync/mod.rs:66-90` the `/orgs` 404 -> `/users` fallback | `not_an_org/1`, minted from `endpoint_miss/2` |
| `sync/events.rs:117-225` the org-events endpoint, per account type | `watched_global(_, _, 'org_events')` and `watched_global(_, _, 'user_events')`, exclusive on `not_an_org/1` |
| `gh.rs` had no equivalent: a 404 repeated forever | `miss_streak/3` + `endpoint_cooling/2`, anti-joined into `due/3` |
| `sync/mod.rs:114-131` `org_to_repos` | the second `watched_repo_seen` rule |
| `sync/mod.rs:196-199` configured then discovered | two `watched_repo_seen` rules, union |
| `sync/mod.rs:242-248` full sweep vs targeted | `pr_due/2`'s two arms |
| `sync/mod.rs:274-276` dirty repos | `dirty_repo/1` |
| `sync/mod.rs:363-378` `should_skip_full_sweep` | `not(pr_ever_synced(RepoRef))` |
| `sync/events.rs:16-103` `sync` | `repo_event_seen/6` -> `repo_event/6` |
| `sync/events.rs:67` `INSERT OR IGNORE` | the keyed fold: an identical write is a zero delta |
| `sync/events.rs:26-34` poll-interval skip | `endpoint_period/2`'s server candidate |
| `sync/events.rs:117-225` `sync_org` | `watched_global(_, _, 'org_events')` |
| `sync/events.rs:227-233` `pr_number_from_event` | two `dirty_pr/2` rules |
| `sync/events.rs:249-259` `pr_numbers_from_branch` | `dirty_pr/2` joining `open_pr_head/4` on `head_ref` |
| `sync/events.rs:105-112`, `:235-247` CI sha -> PR | three `dirty_pr/2` rules joining `open_pr_head/4` on `head_sha` |
| `sync/branches.rs:7-88` `sync` | `candidate_branch/3` -> `matched_branch/3` -> `branch/4` |
| `sync/branches.rs:90-95` `matches_glob` | the two `matched_branch/3` rules (exact, then `*` prefix) |
| `sync/prs.rs:60-145` `sync_batch` | `pr_batch_member/3` -> `pr_batch_member_keyed/4` -> `pr_batch/4` -> `pr_batch_response/8` |
| `sync/prs.rs:58` `BATCH_SIZE` | `batch_size(20)`, `BatchIndex := Ordinal / RowsPerCall` |
| `sync/prs.rs:71-84` alias building | `group_concat(RepoSlug, " ")` + `graphql.rs:query_for` |
| `sync/prs.rs:147-198` `sync_targeted` | the `dirty_repo` arm of `pr_due/2` |
| `sync/prs.rs:7-41` `PR_FIELDS` | `graphql.rs:PR_FIELDS`, verbatim |
| `sync/prs.rs:200-324` `upsert_pr` | `pull_request_seen/19` -> `pull_request/19` |
| `sync/prs.rs:210-214` state mapping | `graphql.rs:pr_state` |
| `sync/prs.rs:326-345` `upsert_review` | `pr_review_seen/7` -> `pr_review/7` |
| `sync/prs.rs:347-383` `upsert_status_check` | `pr_status_check_seen/7`, union flattened in `graphql.rs:status_check` |
| `sync/prs.rs:274-289` labels delete-and-replace | `pr_label/4`, keyed fold |
| `sync/prs.rs:292-311` requested reviewers | `pr_requested_reviewer/3`; user/team fold in `graphql.rs:flatten` |
| `sync/notifications.rs:10-61` `sync` | `watched_global("notifications", ...)` -> `notification_seen/11` |
| `sync/notifications.rs:101-158` `upsert_notification` | `notification/11` |

### checkout

| ghcacher | dl6 |
|---|---|
| `checkout.rs:112-168` `checkout_all` | `checkout_task/4` -> `checkout_answer/3` -> `checkout/5` |
| `checkout.rs:170-230` `force_update_default_branch` | inside the `repo_checkout` executor (soopy) |
| `checkout.rs:353-374` `run_fetch_pr_heads` | `pr_head_mirror/3` through `repo_mirror_pr_heads` |
| `checkout.rs:12-20` concurrency cap | `apply_daemon_budget` in the runtime, not a program column |
| `checkout.rs:70-75` `is_dirty` gate | `want_sha`'s FRESHNESS role: an unmoved branch re-asks on a known witness |

### reader views

| ghcacher | dl6 |
|---|---|
| `schema.sql:195-206` `v_open_prs` | `open_pr/11` + `pr_approvals/3` + `pr_changes_requested/3` + `pr_comment_count/3` |
| `schema.sql:208-216` `v_unread_notifications` | `unread_notification/8` |
| `schema.sql:218-232` `v_recent_events` | `recent_event/5` |
| `schema.sql:234-245` `v_rate_limit` | `call_log/8` query, ordered |
| `demo.sh:74-82` rate pools | `rate_pool/4` |
| `query/*.rs` filter flags | `?` queries; the runtime's `/idb/:rel` is the read surface |

## The 16 tables

| # | schema.sql | dl6 rel | key |
|---:|---|---|---|
| 1 | `repo` | `repo/2` (a relation-valued TYPE) | content-interned |
| 2 | `branch` | `branch/4` | `key(1, 2)` |
| 3 | `pull_request` | `pull_request/19` | `key(1, 2)` |
| 4 | `pr_review` | `pr_review/7` | `key(1, 2, 3)` |
| 5 | `pr_comment` | `pr_comment/10` | `key(1, 2, 3)` |
| 6 | `pr_status_check` | `pr_status_check/7` | `key(1, 2, 3)` |
| 7 | `pr_label` | `pr_label/4` | `key(1, 2, 3)` |
| 8 | `pr_requested_reviewer` | `pr_requested_reviewer/3` | `key(1, 2, 3)` |
| 9 | `repo_event` | `repo_event/6` | `key(1, 2)` |
| 10 | `notification` | `notification/11` | `key(1)` |
| 11 | `call_log` | `call_log/8` | `log keep(count(16500))` |
| 12 | `change_log` | `change_log/4` | `log keep(count(34000))` |
| 13 | `poll_state` | five keyed projections + `poll_state/6` view | `key(1)` each |
| 14 | `checkout` | `checkout/5` | `key(1, 2)` |
| 15 | `worktree` | NOT CARRIED, see below | |
| 16 | the four views | `open_pr`, `unread_notification`, `recent_event`, `rate_pool` | |

## Storage law

`.claude/skills/sql-relational-design` asks for INTEGER surrogate ids, natural
keys once in a dictionary with UNIQUE, and no composite TEXT primary key. The
emitter already gives all three and the program adds the fourth. Measured on the
emitted DDL of a probe with this exact shape:

```sql
CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)
CREATE TABLE "p1_pull_request" ("__id" INTEGER PRIMARY KEY, "repo_ref" INTEGER NOT NULL,
  "number" INTEGER NOT NULL, "title" INTEGER NOT NULL, "author" INTEGER NOT NULL,
  UNIQUE ("repo_ref", "number"))
```

Every TEXT column in every rel is an INTEGER into `__str`; `repo_ref` is one
INTEGER for the whole `(owner, name)` pair because `rel repo(owner, name)` is
used as a COLUMN TYPE, which makes it a content-interned dictionary
(`SYNTAX.md:231-232`). `schema.sql:83-88` and `:90-94` are the original's two
composite TEXT primary keys (`pr_label`, `pr_requested_reviewer`); both become
`("__id" INTEGER PRIMARY KEY, ..., UNIQUE (...))` here with zero TEXT in the key.

## What the language answered that the brief expected to build

The brief asked for a `json.rows(body) -> (index, element)` executor "if the
language has no column-plane JSON element read". **It has one**, and it is
labelled "the gh-cache flagship" in `SYNTAX.md:276`. No executor was written.

```dl6
pull_request(Number, Title, Author) <-
  resp(Body),
  decode(Body, [... {number: Number, title: Title, user: {login: Author}}]).
```

Manifest evidence, `v6/prolog/compile/out/manifest.json`, all `compiled`:
`json_array_spread_fans_out_correlated_siblings`,
`json_array_spread_skips_non_matching_elements`,
`json_typed_capture_folds_into_a_keyed_int_total`,
`json_key_capture_binds_key_and_value`, `json_deep_exact_key_chain_binds`,
`list_column_fans_out_through_spread`, `split_fans_out_through_spread`.
`origin/feature/pr-watch-resident` was checked first, as the brief asked: it has
no such executor, so there was nothing to reuse and nothing was built twice.

## Deviations, each with its throw site

| # | brief asked | what shipped | throw site / reason |
|---:|---|---|---|
| 1 | `worktree` table | not carried | no linked executor answers a filesystem worktree scan. `worktree.rs:105-203` shells `git worktree list --porcelain`, and "Zero shell in the engine" requires a Rust executor. `git_refs.rs` and `repo_at.rs` answer refs, not worktrees. |
| 2 | `pr_comment` filled | rule written | the ORIGINAL never writes this table: `grep -rn pr_comment ~/projects/ghcacher/src` finds two READS and no INSERT. This program adds `comments(last: 50)` to the selection, so it is a superset. `path`/`line`/`in_reply_to_id` stay empty: the issue-comment connection carries none of them. |
| 3 | SSE broadcast | `change_log/4` only | `cmd.rs:177-216` `broadcast_loop` is transport. Subscriptions, heartbeat and pause/resume are daemon lifecycle, which "Infra is bought, never built" puts outside the program. |

Three rows that stood here are CLOSED, and each was a claim about the language
that measurement contradicted.

| was | now |
|---|---|
| "pagination has to live in the executor, a fourth host input cannot be added" | the page walk is `next_page` + `page_queued`, decoded out of the `link` header with `split`/`instr`/`substr`; `page_cap(10)` is a program fact |
| "a rel column cannot spell the alias whose name I computed", so `graphql.rs` had to flatten | `decode(Data, {data: {$RepoAlias: {...}}})` captures the alias INTO a variable; nested spreads then fan out six planes. Runtime receipt in the arc's probe: `{"repo_0": ..., "repo_1": ...}` answers three rows and skips `rateLimit`. |
| "a `json.rows` executor is needed" | `decode` with a spread is that |

## The pull-request state track (folded in from `prwatch.dl6`)

`prwatch.dl6` was a second program watching the same four repositories over its
own `/gh/pulls` executor. Its two rels live here now, over `pull_request`:

| rel | what it answers |
|---|---|
| `pr_transition(repo_ref, number, from_state, to_state, at_tick)` | every state change, read with `pre/1` so the two clock offsets do not conflict |
| `lane_proof(repo_slug, branch, pr, merge_commit_sha)` | a merged pull whose head branch carries a lane prefix: the receipt that a dispatched lane's commits reached main |

The incident its README recorded is closed by construction here. The endpoint
was `state=all` over five pages of a hundred, 6857541 wire bytes on EVERY 60s
tick, because an absent optional column read as the executor's most expensive
default. There is no optional column to omit now: the url and every header are
in the row.

## Executors

| host | executor | file | new? |
|---|---|---|---|
| `http.get` | `HttpGetExecutor` | `executors/http.rs` | NEW |
| `http.post` | `HttpPostExecutor` | `executors/http.rs` | NEW |
| `/env/var` | `EnvExecutor` | `executors/env.rs` | reused |
| `/toml/json` | `TomlJsonExecutor` | `executors/toml.rs` | reused |
| `/soopy/checkout`, `/soopy/mirror_pr_heads` | `SoopyCheckoutExecutor` | `executors/checkout.rs` | reused |
| `/clock/tick` | `ClockExecutor` | `executors/clock.rs` | reused |
| `/dl/tick_cost` | `TickCostExecutor` | `executors/cost.rs` | reused |

Deleted with this arc: `fetch.rs` (368), `graphql.rs` (459), `pulls.rs` (192),
`repos.rs` (150), and their `/http/fetch`, `/gh/rest_cond`, `/gh/repos`,
`/gh/pulls`, `/gh/pr_batch` roster rows.

`hosts.rs` `collect` dispatches one tick's `http.*` demands on a bounded pool
(`std::thread::scope`, width `DL_HTTP_CONCURRENCY` or a quarter of the cores,
floor 2) and joins them in demand order. COUNT receipt in `tests/executors.rs`:
eight endpoints against a listener that holds each request 3s answer in 3.01s,
where serial is 24s.

## Restart

`sql.rs` `run_program_ddl` dropped every table this program declares at every
boot, so each restart re-downloaded roughly a megabyte
(`issues/dl6-run-restart-loses-etags`). A TABLE whose CREATE is the one already
standing in `~/.agent/dl6.db` now keeps its rows; a table whose shape moved is
still dropped, because its rows no longer fit. Live receipt: kill, start, and
the first poll is 8 x 304 with `bytes = 0` out of 8 stored ETags and 8 stored
bodies.
