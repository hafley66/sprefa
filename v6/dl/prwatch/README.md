# prwatch: pull requests watched live, conditionally

`prwatch.dl6` folds the repository's pull requests, the transitions between
their states, and the cost of watching them, in one resident program.

## The incident this rewrite closes

The first spelling declared `sh pulls(repo_slug, bucket)` with no `state`
column. `executors/pulls.rs:41-44` reads `state` from the demand env and
defaults to **`all`** when the program does not name it, so every 60s tick asked
for `state=all` across `PAGE_CAP = 5` pages of `PER_PAGE = 100` — up to 500
pulls of the repository's entire history. Measured by the coordinator over six
ticks: **6857541 wire bytes on every tick**, RSS 47 -> 104 MB, killed.

Two defects, not one:

| # | defect | fix |
|---:|---|---|
| 1 | an absent optional column read as a default, and that default was the most expensive answer the endpoint has | the endpoint path is spelled IN THE PROGRAM now, pinning `state=all&per_page=100&page=1&sort=updated&direction=desc` — one page, most-recently-updated first |
| 2 | the etag lived only in the executor's process-global map, so nothing durable carried it and a restart re-read everything | `poll_state_etag` is a keyed rel in the one db, and `gh_rest_cond` sends it as `If-None-Match` |

Defect 1 is failure-mode 62's class again: a half-specified call, whose silence
read as a default rather than as a question.

One page sorted by update time is the watch semantic the program actually
wanted. A merged lane pull reaches the top the moment it merges, so `lane_proof`
still sees it without reading the whole history.

## Why the etag rides the call log

The feedback cycle `poll -> rest_response -> etag -> poll` has to WEIGH
something. An edge rule triggered by a level rel grades +0 (TICK-MODEL.md
section 3), so that cycle weighs 0 and the compiler refuses it — measured, the
first attempt died on `unconstructive_clock_cycle` naming exactly
`poll/3, poll_state_etag/2, rest_response/8`. `call_log` is edge-written, so
`poll_state_etag <+ call_log(...)` grades +1, the cycle weighs 1, and the tag
applies from the NEXT tick, which is when the next poll happens anyway.

## Receipts

| leg | command | result |
|---|---|---|
| the program compiles | `swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl -g "compile_dl6('v6/dl/prwatch/prwatch.dl6','/tmp/prwatch.rs',[emitter(emit_rust:emit_program)])" -g halt` | clean, 0.94s |
| tick 2 is a 304 moving zero bytes | `cargo test --test executors pulls_second_pass` | green |
| ten ticks, body once, RSS flat | `cargo test --test executors ten_conditional_ticks` | green: `wire[0] > 0`, `wire[1..] == [0; 9]`, RSS spread under 8 MB |

The ten-tick case is hermetic: a loopback listener serves one ETag, so the
receipt costs zero GitHub points and does not depend on the network being up.

## Running it

```bash
dl6 run v6/dl/prwatch/prwatch.dl6
```

No db flag. The resident runtime folds every program into `~/.agent/dl6.db`
with tables carrying the program's own name (CLAUDE.md, one server one db).
