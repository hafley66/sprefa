# RCA: the phantom extract diff (2026-07-18, the day-long haunting)

One-line: a broken change-comparison in the term-extract path made the engine
believe extract output changed on every tick, escalating every tick into an
unconditional 271-rel derived rebuild; the write load queued all RPCs behind
the engine lock, and instant — polling that lock from its own main thread —
froze solid within 4 seconds of every daemon start, all day, every time.

Two systems, five defects, in firing order. Each fix's receipt exposed the
next layer, per the ledger standard.

## D1 — the generator (dl): `extract_changed` could never be false

`eval_extract_rules` (src/engine/reconcile.rs:449-525) decides whether a
term-extract head's rows moved by comparing a `before` snapshot against the
freshly built `after` rows. Two independent breaks made `before != after`
a tautology:

1. **Arity**: `before` read `SELECT * FROM rel_x`, which includes the
   `__src TEXT DEFAULT ''` bookkeeping column every rel table carries
   (src/engine/declare.rs:290). `after` is built from declared columns only.
   Any non-empty extract head ⇒ row arity differs ⇒ "changed".
2. **NULL encoding**: `before` encoded SQL NULL as `"t"`
   (reconcile.rs:501), `after` encoded `Value::Null` as `"n"`
   (reconcile.rs:487). Even at equal arity, any NULL ⇒ "changed".

The sprefa root arms this permanently: `.dl/git-graph.dl:55-57,120-126`
(json term-extract heads `candidate`, `pr_row`) are non-empty once the gh
drain lands. From that moment, every full tick on this root reported
extract churn that did not exist.

## D2 — the amplifier (dl): phantom churn escalates to whole-program rebuild

Per-tick derived scoping is fully implemented — `affected_derived` walks
reachability from `changed_source_rels` (src/engine/tick.rs:859-873,
958-976, 1410-1422; src/engine/strata.rs:332-361), seeded by digest gates
(tick.rs:498, 679-764). None of it mattered: `extract_changed == true`
hits the escalation arm at tick.rs:879-890, which calls
`rebuild_derived(&strata.pre_rules, &strata.pre_rels)` — all 271 derived
rels, full wipe + refill (src/engine/derive.rs:422), every full tick.
Receipt from the 2026-07-18 18:48 run: 271 `[derived] full wipe` lines per
tick, 5,521 in one short session, ~7MB of daemon.log in 2 minutes.

The scoping machinery was a working engine bolted to a bypass that was
always taken.

## D3 — the treadmill (dl): ticks never stopped coming

Class 19 (docs/failure-modes.md) kept the generator fed: the daemon's own
perf.jsonl appends inside the watched root scheduled a tick every ~2s until
the watch filter landed; every path tick marks the poll dirty, so
`poll_tick` → `tick_full` (src/daemon.rs:607-609) re-entered D1+D2
continuously. Deploys made it worse: the `extract:{exe_stamp}:…` digest key
turns every binary swap into a full re-extract (1.1GB written in the first
58s; 200-336s ticks).

## D4 — the queue (dl): every API call waits behind the storm

Every RPC that touches the engine blocks on that root's single engine
mutex (`lock_eng`, src/daemon.rs:104). With D2 holding the lock for whole
rebuild passes, client calls went from ms to seconds-minutes. This was
"the server is clearly queuing": head-of-line blocking, not overload.
The receipts were already being written — waits ≥250ms emit `lock-wait`
verdicts naming the blocking op (src/verdict.rs:19), visible in
`dl daemon why`.

## D5 — the victim (instant): the freeze was self-inflicted

instant polls `sprefa_ping` every 4s. The command was a **sync**
`#[tauri::command]` — Tauri 2 runs those **on the main thread** — doing
blocking Unix-socket I/O with a 10s read timeout, reading the response one
byte per syscall (src-tauri/src/sprefa_plugin/commands.rs:104,60).

- No daemon: `connect()` fails in µs. Harmless. instant feels fine.
- Daemon up but slow (D4): connect succeeds, read blocks up to 10s **on the
  main thread**. Block time (10s) > poll period (4s) ⇒ the main thread is
  re-frozen continuously. Freeze begins within one poll of `d start`,
  deterministic, forever.

So dl starting didn't overload instant; it gave instant's own blocking
poll something to block on. The same sync-command class ran the whole
status loop: `list_sessions` (tmux spawns), `rogue_agent_sessions`
(ps+lsof+tmux), `cdp_status` (blocking HTTP GET) every 4-8s on the main
thread — the baseline "chokes while merely open" under any system load.
Adjacent: the CGEventTap ran Active on an unprioritized thread with no
TapDisabled recovery (systemwide input lag aggravator, dead-hotkey cause).

## Why it read as a haunting

Each layer had an innocent explanation that survived until the next
receipt: "deploys are just expensive" (D3), "the machine is swapping"
(D4's load), "instant is heavy" (D5). The generator (D1) was silent — its
only symptom was that ticks were always expensive, which everything else
plausibly explained. The unmasking order ran backwards: instant's freeze →
the lock queue → the 271-wipe log pattern → the N+1 false-scream keying →
the phantom diff.

## Fixes landed (each with its receipt)

| defect | fix | receipt |
|---|---|---|
| D1 | explicit declared-column projection in `before`; NULL encodes `"n"` both sides (commit f9414e3c) | fail-pre-fix it-test `f_term_extract_steady_state_does_not_force_full_rebuild` (tests/it/tick_digest.rs) — failed with `got ["payload","downstream"]` on an unchanged tick pre-fix |
| D2 | no code change needed — scoping works once D1 stops lying | same test pins the steady state |
| D3 | watch filter + no-op broadcast gate (landed 2026-07-18 AM, `watch_filter_tests` + broadcast witness); watched roots now named at boot (a31eaa90) | tick-every-2s loop gone from trail |
| D4 | unchanged by design this pass; `lock-wait` verdicts are the receipt surface; scheduler arc owns the write budget | `dl daemon why` names blockers |
| D5 | all 5 `sprefa_*` + 3 poll commands async + `spawn_blocking`; ping timeout 10s→1s; tap QoS + respawn-on-disable (instant repo, uncommitted) | cargo check clean; A/B: rebuilt instant stays interactive during a cold rebuild |
| noise | `_derived_complete` counter keyed per rel-set (c3148d90) so the crash-rail marks stop false-screaming | witness test failed pre-fix with `("INSERT _derived_complete", 71)` |

## Standing lessons

- A change-detector that compares differently-shaped encodings is not a
  detector; it is a constant. Diff inputs must share one projection and one
  encoding, pinned by a steady-state test (nothing changed ⇒ nothing
  rebuilt).
- An escalation arm (`extract_changed` ⇒ rebuild everything) is a bypass
  around every optimization downstream of it; bypasses need their own
  fail-pre-fix coverage, not just the machinery they bypass.
- A UI-process poll must never share a thread with the UI, and a liveness
  probe's timeout must be shorter than its period, or the probe becomes the
  outage.
