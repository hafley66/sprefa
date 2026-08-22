# ghcache: ghcacher, verbatim, in dl6

`~/projects/ghcacher` (Rust, 3339 lines under `src/`) as one dl6 program.
Every ability is mapped below, one row per ability, `original file:line -> rel
or rule`. Nothing is dropped silently: the things that do not carry over have
their own section with a throw site each.

## Contents

- [Status](#status)
- [How to run it](#how-to-run-it)
- [The rate budget](#the-rate-budget-the-whole-point)
- [Ability map](#ability-map)
- [The 16 tables](#the-16-tables)
- [Storage law](#storage-law)
- [What the language answered that the brief expected to build](#what-the-language-answered-that-the-brief-expected-to-build)
- [Deviations, each with its throw site](#deviations-each-with-its-throw-site)
- [Executors](#executors)

## Status

| leg | state |
|---|---|
| `ghcache.dl6` parses, plans, and types clean | yes |
| `ghcache.dl6` reaches the emitter | **yes** |
| `executors/graphql.rs` + `executors/fetch.rs` | built, 11 unit tests green |
| the six `v6/dl/ghcacher` goldens | unchanged, gate green |

The `3_clock_check.pl` path-walk blowup that used to stop this program at
`compile.pl:239` is pinned off on the compile path
(`clock_path_walk_enabled :- fail.`, ruling `clock_path_check_pinned_off`,
`v6/prolog/conformance/rulings.pl`). Five lowering stops followed and are
fixed in the program: `trigger_arg_not_var` on eleven edge-rule (`<+`)
literals, `edge_into_unkeyed_set(not_an_org/1)`, and
`aggregate_group_not_delta_local` on `rate_pool/4` and `pr_batch/4` (grouped
columns must come from one positive body atom's own delta rows,
`lower.pl:4635`; `pr_batch` now routes through `pr_batch_member_keyed/4` to
materialize `BatchKey` as a stored column first).

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
| `config.rs:25-27` warn/stop thresholds | `api_tier/3` |
| `config.rs:105-108` `fs_owner`/`fs_alias` | `watched_repo.fs_alias`, `checkout_task.dest_root` |

### sync

| ghcacher | dl6 |
|---|---|
| `sync/mod.rs:43-104` `discover_org_repos` | `watched_global(_, _, 'org_repos')` + `discovered_repo/3` |
| `sync/mod.rs:66-90` the `/orgs` 404 -> `/users` fallback | `not_an_org/1`, minted from a 404 response |
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
| 11 | `call_log` | `call_log/8` | `log keep(all)` |
| 12 | `change_log` | `change_log/4` | `log keep(all)` |
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
| 1 | `follow_link_next` as a host INPUT | pagination inside the executor, `pages` as an OUTPUT column | `registry.pl:456-461` fixes `gh_rest_cond`'s inputs at exactly `(endpoint_path, prev_etag, bucket)`. A fourth input fails to unify, and `host_input_roles/3` (`registry.pl:552-557`) then falls through to `identity_roles/2`, giving the host NO freshness column — so it would be demanded once and memoised forever, which is the opposite of polling. Adding the input means editing `v6/prolog/registry.pl`, forbidden to this lane. |
| 2 | `graphql.query(query: key(text)) -> ...` | the registered `gh_pr_batch(batch_key, slug_list, bucket)` | same file, same reason: a new host NAME needs a `host_input_contract/3` row. `gh_pr_batch` was already registered (`registry.pl:467-471`) for exactly this and had no executor; this arc wrote it. |
| 3 | `json.rows` executor | not built | the language has `spread`; see the section above. |
| 4 | `worktree` table | not carried | no registered host answers a filesystem worktree scan. `worktree.rs:105-203` shells `git worktree list --porcelain` and `git status` per worktree, and "Zero shell in the engine" (CLAUDE.md, 2026-08-21) requires a linked Rust executor. `executors/git_refs.rs` and `repo_at.rs` are soopy-backed and answer refs, not worktrees. A `worktree_scan` host is new registry surface. |
| 5 | `pr_comment` filled | rule written, executor answers it | the ORIGINAL never writes this table: `grep -rn pr_comment ~/projects/ghcacher/src` finds only two READS, `query/prs.rs:80` and `:216`, and no INSERT anywhere. `PR_FIELDS` (`sync/prs.rs:7-41`) never selects `comments`. This program adds `comments(last: 50)` to the selection and fills the rel, so it is a superset, not a gap. `path`/`line`/`in_reply_to_id` stay empty because the issue-comment connection carries none of them. |
| 6 | SSE broadcast | `change_log/4` only | `cmd.rs:177-216` `broadcast_loop` is transport. `GET /ticks` (`HOST-CONTRACTS.md:65`) is the runtime's own SSE surface and reads the tick log; `change_log` is the rel it carries. Subscriptions, heartbeat, pause/resume (`cmd.rs:43-128`, `:344-363`) are daemon lifecycle, which CLAUDE.md's "Infra is bought, never built" puts outside the program. |

## Executors

| host | executor | file | new? |
|---|---|---|---|
| `gh_rest_cond`, `fetch` | `http_fetch` | `executors/fetch.rs` | extended |
| `gh_pr_batch` | `gh_pr_batch` | `executors/graphql.rs` | NEW |
| `toml_json` | `toml_json` | `executors/toml.rs` | reused |
| `repo_checkout`, `repo_mirror_pr_heads` | `soopy_checkout` | `executors/checkout.rs` | reused |

`fetch.rs` gained seven output columns (`etag`, `last_modified`,
`poll_interval`, `rate_remaining`, `rate_reset`, `bytes`, `pages`) and
Link-header pagination. Adding columns cannot break an existing program:
`select_columns` keeps only the columns a host DECLARES and drops a row missing
any of them (`hosts.rs`, `carries_every_column`). The six `v6/dl/ghcacher`
goldens are scripted and were re-run: `GHCACHER_RUST_DOOR_HOLDS goldens=6`.

`graphql.rs` flattens the nested per-alias GraphQL answer into six element
arrays (`pulls`, `reviews`, `comments`, `labels`, `requested_reviewers`,
`status_checks`), each element carrying `owner`/`name`/`number`. Flattening
there rather than in dl6 is forced: the answer nests under aliases named
`repo_0..repo_19`, and a rel column cannot spell "the alias whose name I
computed".
