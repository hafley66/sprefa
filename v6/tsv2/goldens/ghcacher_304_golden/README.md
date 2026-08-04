# Ghcacher 304 golden

`6_gate.sh` compiles the text fixture through the current Prolog-to-TypeScript
compiler, replays one hermetic JSON schedule through both the Prolog oracle and
the emitted SQLite runtime, and byte-diffs both exact tick logs and final
relations against the checked-in goldens.

The fixture is the fixed program from `ghcacher_tick_golden` (section 1.1 of
`plans/2026-08-04-ghcacher-plan.md`): `cache_view` is a `key(1)` latch fed by
`<+` off `fresh_hit`, a level rule that filters `resp` to status 200. Because
the latch has no input edge from the clock, a clock tick moves nothing, and a
304 (which filters out of `fresh_hit`) leaves the last 200 row untouched.

The schedule has a 200 on tick 2, then the clock moves and two consecutive
polls answer 304:

| tick | committed batch | graded boundary result |
|---:|---|---|
| 1 | `watch`, bootstrap `etag_event`, `interval(300,1)` | keyed clock/etag state and witness-1 demand appear |
| 2 | witness-1 host response 200 | `resp`, `fresh_hit`, and `cache_view(tag-v1,17)` appear |
| 3 | later tag feedback plus `interval(300,2)` | clock bucket moves; `fresh_hit` retracts the old row but `cache_view` holds `(tag-v1,17)` with zero delta |
| 4 | witness-2 host response 304 | `resp(304)` appears; `cache_view` emits zero delta; the last 200 row survives |
| 5 | `interval(300,3)` | clock bucket moves again with no response; `cache_view` still holds |
| 6 | witness-3 host response 304 | `resp(304)` appears; `cache_view` emits zero delta |

The three graded expectations:

- (a) `cache_view` keeps its last 200 row `(repo, tag-v1, 17, cli/cli)` through
  every 304 tick. It is never empty: `fresh_hit` retracts on the clock move but
  the `key(1)` latch does not.
- (b) the etag latch still advances: `current_etag` moves `""` to `"tag-v1"` on
  tick 3 and `poll` rides buckets 1, 2, 3. The 304s do not disturb it.
- (c) the tick log is byte-identical between the Prolog oracle and the emitted
  SQLite runtime on both doors.

The final public vanish test:

```text
current_etag(repo, tag-v1)
cache_view(repo, tag-v1, 17, cli/cli)
resp(repo, 3, 304, "", 0, "")
```

Run from the repository root:

```bash
bash v6/tsv2/goldens/ghcacher_304_golden/6_gate.sh
```

Success is:

```text
GHCACHER_304_GOLDEN_HOLDS ticks=6 final=1
```

## Fail-first receipt

The 304 golden is the regression test for the section-1.1 defect. Reverting
`cache_view` to the broken spelling (a plain level rule over `resp` with the
literal 200 and no `key(1)`) makes the golden go red with exit 1. The broken
program is the pre-fix fixture on the branch base.

Reverted fixture:

```dl6
rel cache_view(ep: text, tag: text, stars: int, full_name: text).

cache_view(Ep, Tag, Stars, FullName) <-
  resp(Ep, _, 200, Tag, Stars, FullName).
```

Red output, the gate diff against the checked-in ticks and final (trimmed to
the drifted lines):

```text
@@ -1,7 +1,7 @@
-{"tick":2,...,"cache_view":{"add":[["repo","tag-v1",17,"cli/cli"]],"del":[]},"fresh_hit":{...},"resp":{...}}}
+{"tick":2,...,"cache_view":{"add":[["repo","tag-v1",17,"cli/cli"]],"del":[]},"resp":{...}}}
-{"tick":3,...,"fresh_hit":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]},"interval":...}}
+{"tick":3,...,"cache_view":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]},"interval":...}}
-{"final":{...,"cache_view":[["repo","tag-v1",17,"cli/cli"]],...}}
+{"final":{...,"current_clock":[[300,3]],...}}     (no "cache_view" key at all)
```

The reverted oracle's tick 3 shows `"cache_view":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]}`:
the clock bucket moving destroys the cached row before any 304. Its final state
has no `cache_view` key at all, which is the empty cache the measured defect
predicts. Restoring the `key(1)` latch plus the status-filtered `fresh_hit`
edge restores the row and the gate goes green.
