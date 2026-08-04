# Ghcacher clock golden

`6_gate.sh` compiles the text fixture through the current Prolog-to-TypeScript
compiler, replays one hermetic JSON schedule through both the Prolog oracle and
the emitted SQLite runtime, and byte-diffs both exact tick logs and final
relations against the checked-in goldens.

The schedule uses the existing host-response seam. It does not run `gh`, a
shell host, wall time, or the network. `etag_event` is the later listener queue:
the host response commits at tick 2, then its selected tag returns as a new
event at tick 3 rather than participating in tick 2's pure closure.

| tick | committed batch | graded boundary result |
|---:|---|---|
| 1 | `watch`, bootstrap `etag_event`, `interval(300,1)` | keyed clock/etag state and witness-1 demand appear |
| 2 | witness-1 host response | `resp` and `cache_view(tag-v1,17)` appear |
| 3 | later tag feedback plus `interval(300,2)` | keyed clock/etag replacement retracts witness 1's demand and adds witness-2 demand; `fresh_hit` retracts but the keyed `cache_view` latch survives |
| 4 | witness-2 host response | `resp`, `fresh_hit`, and `cache_view(tag-v2,18)` appear; the `cache_view` latch replaces tag-v1 with tag-v2 |
| 5 | late replacement for witness 1, ordinal 0 | raw `__host_response_fetch` replaces its old row; `resp` and `cache_view` emit no delta because witness 1 has no current `poll` membership |

The final raw response table intentionally retains the late old-witness row.
The current public relations contain only witness 2:

```text
poll(repo, tag-v1, 2)
resp(repo, 2, 200, tag-v2, 18, cli/cli)
cache_view(repo, tag-v2, 18, cli/cli)
```

`cache_view` here is the section-1.1 fix: a `key(1)` latch fed by `<+` off the
status-filtering level rel `fresh_hit`, so the clock bucket moving no longer
empties it. The 304-specific grade lives in `ghcacher_304_golden`.

Run from the repository root, or as `just ghcacher-golden` from `v6/`:

```bash
bash v6/tsv2/goldens/ghcacher_tick_golden/6_gate.sh
```

Success is:

```text
GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1
```

No production blocker is required for this schedule-fed standing gate. A live
end-to-end variant would need a deterministic served-world adapter for the
response-to-later-`etag_event` feedback queue. The current served runtime can
commit interval and host rows, while this lab supplies that later listener
event explicitly through the same arrival boundary.
