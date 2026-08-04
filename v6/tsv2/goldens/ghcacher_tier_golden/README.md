# Ghcacher tier golden

`6_gate.sh` compiles the text fixture through the current Prolog-to-TypeScript
compiler, replays one hermetic JSON schedule through both the Prolog oracle and
the emitted SQLite runtime, and byte-diffs both exact tick logs and the final
relation envelope against the checked-in goldens. A COUNT leg then asserts the
cold repo's poll rows land on exactly its due buckets and the hot repo's on
every clock tick.

The schedule uses the existing host-response seam. It does not run `gh`, a
shell host, wall time, or the network. `repo_activity` feeds NON-OVERLAPPING
tier bands: `org/hot` last event at bucket 100 sits in the hot `[0,60)` band
(period 1) and fires every clock tick; `org/cold` last event at bucket 0 sits
in the cold `[60,100000)` band (period 30) and fires only on a multiple of 30.
The `poll` and `fetch` downstream are gated on the `due` row, so a non-due repo
derives no row and costs nothing.

| tick | bucket | `due` add | grades |
|---:|---:|---|---|
| 1 | 100 | `org/hot` only | hot fires, cold silent |
| 2 | 101 | `org/hot` only | hot fires on a CONSECUTIVE tick |
| 3 | 102 | `org/hot` only | still consecutive; cold still silent on a non-multiple |
| 4 | 120 | `org/cold` and `org/hot` | cold fires exactly on a multiple of 30 |
| 5 | 150 | `org/cold` and `org/hot` | and again 30 later |

Measured `batch_query` on bucket 120 is `[120, 0, 'org/cold org/hot']` (the
5.3 value) and `points_budget` is `[120, 1]`; the gate greps the tick log for
both. Slugs are aggregated by a SQL `group_concat`, never by string code, so
the aliased query text is data the way the plan intends.

The COUNT leg of `6_gate.sh` extracts each tick line's `poll` rows and asserts
cold = (0,0,0,1,1,0) and hot = (1,1,1,1,1,0) across ticks 1-6, then prints the
totals. A non-due bucket contributes ZERO `poll` rows naming the cold repo.

Sabotage receipt: change the cold band's `period_ticks` from 30 to 1 and the
cold repo fires every tick, which breaks both the byte diff and the count.

Run from the repository root:

```bash
bash v6/tsv2/goldens/ghcacher_tier_golden/6_gate.sh
```

Success is:

```text
cold poll rows total=2 (due buckets 120,150 only): OK
hot poll rows total=5 (every clock tick): OK
GHCACHER_TIER_GOLDEN_HOLDS ticks=6 final=1
```
