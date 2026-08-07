# rxoracle -- grading sprefa's event sequence against literal rxjs

One contrived scenario, written twice, diffed event by event.

* **leg A** `cases/<name>/leg-a.ts` -- a literal rxjs program. It imports `rxjs`
  and node builtins and **nothing from this repository**. It prints one line per
  event with `console.log`. Every leg-A file is self-contained down to its
  `emit` helper: a shared helper living in this directory would be a repository
  import wearing a different hat, and the whole value of leg A is that a reader
  can check it against the rxjs docs without knowing anything about sprefa.
* **leg B** `cases/<name>/leg-b.dl6` -- the same scenario in DL6, driven
  **only from bash**: `bop serve` on an ephemeral port, `curl` for the program
  load, `curl` for every arrival batch, `curl -N /ticks` for the event capture.
  Nothing in leg B imports this codebase from TypeScript. An e2e that reaches
  into the runtime through an import is a cheating test.

`bash run.sh` is the entry point. Exit 0 means every case matched its declared
expectation; nonzero means at least one did not.

---

## 1. The line format

    <step> <name> <sign> <payload>

| field | meaning |
|---|---|
| `<step>` | two-digit zero-padded step index, `00`-based. The shared clock, defined in section 2. |
| `<name>` | the observable's name on leg A, the relation's name on leg B. **The alignment between the two legs is this name and nothing else.** |
| `<sign>` | `+` a `next` / a delta add. `-` a delta del. `!` an `error`. `.` a `complete`. |
| `<payload>` | the row as a canonical JSON array: `JSON.stringify(row)` on leg A, `json.dumps(row, separators=(",", ":"))` on leg B. Both produce `["s1","r1"]` and `[0]`, which is also exactly what `runtime/ticklog.ts` already writes, so leg B does no re-encoding. |

`-`, `!` and `.` are in the format because sprefa and rxjs each have channels the
other lacks, and a format that could not spell them would hide that fact instead
of measuring it. rxjs has no retraction, so a leg-A file never prints `-`.
sprefa's tick log has no terminal notification, so a leg-B capture never prints
`!` or `.`.

## 2. The shared clock: the STEP, not the tick and not the wall

rxjs measures in scheduler frames and subscription lifetimes. sprefa measures in
ticks. Neither number means anything to the other, so the harness uses a third
clock that both legs can be driven by: the **step**.

* A case declares `stepMs` (default 500) and a list of steps. Step `k` occupies
  `[k*stepMs, (k+1)*stepMs)`.
* An input batch for step `k` is delivered at `k*stepMs + stepMs/2`, the step's
  **midpoint**, on both legs. Leg A schedules it on a `VirtualTimeScheduler` at
  that exact frame. Leg B records `t0 = now - stepMs/2` immediately before its
  first `POST /edb/events` and then sleeps `stepMs` between batches, so its POSTs
  land on the same nominal midpoints.
* An event's step is `floor(time / stepMs)`: virtual frame on leg A, milliseconds
  since `t0` on leg B.

Leg A is deterministic by construction (virtual time, no wall clock anywhere).
Leg B is not, so the harness polices it rather than hoping:

* **The boundary guard.** Any event whose time falls within `guardMs` (default
  100) of a step boundary **fails the run loudly** as a straddle, on either leg.
  A case whose real host durations put an answer near a boundary is a badly
  written case and the harness says so instead of going red at random. Every
  case here is authored so its events land near step midpoints; `run.sh -v`
  prints the measured millisecond offset of every leg-B event so the margin is
  auditable.
* **Intra-step order is discarded** (normalization N2 below), so jitter inside a
  step cannot change the answer at all.

## 3. Normalizations

Every normalization below is a rule with a reason, decided before the cases were
run. None of them was added to make a diff pass. Two are unconditional; two are
per-case opt-ins, and a case that opts into one is reporting a finding by doing
so, not hiding one.

### N1 STEP-FOLD (always on)

Every tick a step produces, including drain ticks, folds into that step. The
tick number never appears in a line.

*Reason.* sprefa's tick number is an engine-internal drain counter with no rxjs
counterpart at all, and it is not even stable within sprefa: `extra_drain_tick`
is a currently unowned defect (ARCH row) where a re-assertion produces one more
tick carrying no rows, and the runtime-bridge arc measured the served engine
producing 4 ticks where the schedule-fed oracle produces 3 for identical deltas.
Comparing tick numbers would therefore fail on an engine-internal number that
both sprefa implementations already disagree about.

### N2 INTRA-STEP-SORT (always on)

Within one step, lines are sorted by `(name, sign, payload)`.

*Reason.* sprefa's tick log is already a *set* per relation per tick --
`runtime/ticklog.ts` sorts `add` and `del` lexicographically and orders
relations alphabetically -- so leg B has no intra-tick order to compare against.
Preserving leg A's order would be comparing a real sequence against an
alphabetized one. Order across steps is preserved, so a per-stream sequence is
still graded; only the interleaving inside one step is not.

### N3 SIGN-PROJECTION (opt in, `"dropDel": true`)

Leg B's `-` lines are dropped.

*Reason.* an rxjs `Observable` has no retraction channel. `next` is its only
value-carrying notification, and there is no combinator that un-emits. A `-`
line therefore has no leg-A counterpart that could ever exist, so grading it is
grading against a channel that is absent by construction. **Opting in is itself
the finding**: every case below that sets `dropDel` records "sprefa retracts
here and rxjs cannot" in its own verdict row, with the dropped lines printed by
`run.sh -v`.

### N4 INTERNAL-REL-HIDE (on by default, `"showInternal": [...]` opts rels back in)

Leg B lines for relations whose name begins `__` are dropped.

*Reason.* `__host_demand_*`, `__host_response_*` and `__host_witness` are
compiler-minted storage-plane relations. They are the same class the
`compound_storage = struct_as_rows` ruling calls boundary-invisible: a program's
author never wrote them and cannot read them by name from the surface. Leg A
names the program's own observables, so grading it against minted plumbing would
be grading a compilation strategy.

`showInternal` is implemented and no case currently opts in, deliberately.
`__host_response_<name>`'s leading columns are a witness digest and an ordinal
minted by the compiler; leg A cannot produce those values, so opting the
relation back in would produce a column of leg-B-only lines that says nothing
except "sprefa mints digests". The question that relation actually answers --
does a dead inner's late answer land anywhere -- is measured instead by the
`receipts` block (section 4), which counts its rows over `GET /idb/:rel` while
the server is up. A count is comparable; a digest is not.

## 3b. The receipts block

Two of the questions this harness exists to answer are not events at all:

* did the superseded effect's subprocess still run to completion
* is the dead inner's answer stored anywhere

`case.json`'s `receipts` asserts both against the live server, in bash, before
teardown: `marksLines` and `marksExact` against `$RXO_MARKS`, the spawn ledger
every host template in this corpus appends to, and `idbRows` against
`GET /idb/:rel`. These are assertions, not normalizations: they add nothing to
either line file and cannot make a diff pass. `marksExact` is a whole-file
equality, which is what makes it a proof of ORDER as well as count:
`host_concurrency` declares `start j1 / done j1 / start j2 / done j2`, and an
interleaved ledger would fail it.

## 4. Case layout

    cases/<name>/case.json    manifest
    cases/<name>/leg-a.ts     literal rxjs
    cases/<name>/leg-b.dl6    the same scenario in DL6

`case.json` fields:

| field | meaning |
|---|---|
| `expect` | `exact` / `modulo` / `diverges` / `inexpressible`. The run FAILS if the measured outcome is not this one, so an expected divergence closing is as loud as a match breaking. |
| `stepMs`, `guardMs` | the clock, section 2. |
| `steps` | one entry per step, `{"label": "...", "batch": [ {rel, sign, row}, ... ]}`. An empty batch is an idle step and issues no request. |
| `env` | environment exported to the served process, for host templates. |
| `dropDel`, `showInternal` | N3 and N4, section 3. |
| `refusal` | `inexpressible` cases only: the named refusal `bop check` must print, and the exit code it must produce. |

## 5. What the harness does not claim

* It does not compare wall-clock durations. The step is the only time unit that
  crosses.
* It does not grade sprefa's tick numbering (N1).
* It grades one served process per case, in memory, on an ephemeral port, with
  `SPREFA_CONFIG` pointed at a path that does not exist and `DL_NO_DAEMON=1`.
  Nothing here reads or writes `~/.local/state` or speaks to a daemon.
