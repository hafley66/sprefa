# Effect chain-and-batch lab

Base sha `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd`, branch `lane/effect-chain-batch`.
Zero production edits: everything below runs against the shipped compiler
(`v6/prolog/`) and the shipped served runtime (`v6/tsv2/serve/`), plus the shipped
v5 binary for the `collect` half.

Re-run everything:

```
bash v6/tsv2/labs/effect-chain/run-all.sh
```

Individually (from `v6/tsv2`, with `SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`):

| receipt | command | answers |
| --- | --- | --- |
| 1 | `node --experimental-transform-types labs/effect-chain/1_chain.ts` | ticks per chain stage, same-tick verdict |
| 2 | `node --experimental-transform-types labs/effect-chain/2_batch.ts` | spawn counts at 1 / 10 / 100 demands |
| 3 | `bash labs/effect-chain/3_v5_collect.sh` | what v5 `collect` did, run |
| 4 | `node --experimental-transform-types labs/effect-chain/4_fanin_gap.ts` | whether fan-in is writable in v6 today |

Hermetic throughout: ephemeral ports read back off `server.address()`, scratch
`:memory:` or `mktemp` dbs, `SPREFA_CONFIG` naming a file that does not exist,
`DL_NO_DAEMON=1`, and for the v5 leg `DL_STATE_DIR` into the scratch tree with
the isolation ASSERTED (`state/invocations.db` must exist there) rather than
claimed. Nothing touches `~/.local/state` and no daemon is started.

Standing facts this lab builds on rather than re-deriving, from
`plans/2026-07-30-teardown-and-flatten-lab.md`: host invocations run under
`concatMap` so they serialize and a superseded inner blocks its successor for
its full duration; nothing reads `delta.del` on the effect side, so there is no
teardown; effect identity is content-addressed, so an identical demand is a
cache hit and never a second spawn; of the four flatteners, `concat` is what
ships.

---

## 1. Chaining

### 1.1 The mechanism, traced

One `sh` declaration compiles to two relations and one rewritten rule
(`v6/prolog/1_host_expand.pl`, `expand_probe_rule/5` and `expand_probe/7`):

```
__host_demand_<name>(identity_digest, witness_digest, inputs..., salts...)   derived, level rule
__host_response_<name>(witness_digest, ordinal, inputs..., outputs...)        EDB, arrival target
```

The rule `s1(out) <- seed(input), stage_1(input, out).` becomes two rules: a
demand rule whose body is everything BEFORE the host atom, and the original rule
with the host atom replaced by a witness bind plus a read of the response rel.
The chain is therefore not a special construct at all: stage two's demand rule
reads stage one's ordinary output rel, so the chain is the rel graph.

The runtime half (`v6/tsv2/serve/1_hosts.ts`) reads the demand rel's `+deltas`
off `engine.ticks$`, dedupes the witness in process and against the durable
`__host_witness` table, spawns, decodes stdout into the response rel's declared
columns, and calls `engine.submit(arrivals)`. That submit enqueues a NEW batch on
`LiveEngine.arrivals`, which `concatMap` runs as its own tick.

### 1.2 Ticks per stage, measured

`labs/effect-chain/1_chain.ts`, real `/bin/sh` spawns, one seed row:

| stages | total ticks | spawns | wall ms |
| --- | --- | --- | --- |
| 1 | 2 | 1 | 23 |
| 2 | 3 | 2 | 21 |
| 3 | 4 | 3 | 20 |
| 4 | 5 | 4 | 73 |

**N stages cost exactly N+1 ticks. One hop per stage, no drain ticks, no waste.**

The tick log says where each hop happens. Three stages, verbatim:

```
{"tick":1,"deltas":{"__host_demand_stage_1":{"add":[["identity|stage_1|input:text=alpha","witness|stage_1|input:text=alpha","alpha"]],"del":[]},"seed":{"add":[["alpha"]],"del":[]}}}
{"tick":2,"deltas":{"__host_demand_stage_2":{"add":[["identity|stage_2|input:text=alpha-1","witness|stage_2|input:text=alpha-1","alpha-1"]],"del":[]},"__host_response_stage_1":{"add":[["witness|stage_1|input:text=alpha",0,"alpha","alpha-1"]],"del":[]},"s1":{"add":[["alpha-1"]],"del":[]}}}
{"tick":3,"deltas":{"__host_demand_stage_3":{"add":[["identity|stage_3|input:text=alpha-1-2","witness|stage_3|input:text=alpha-1-2","alpha-1-2"]],"del":[]},"__host_response_stage_2":{"add":[["witness|stage_2|input:text=alpha-1",0,"alpha-1","alpha-1-2"]],"del":[]},"s2":{"add":[["alpha-1-2"]],"del":[]}}}
{"tick":4,"deltas":{"__host_response_stage_3":{"add":[["witness|stage_3|input:text=alpha-1-2",0,"alpha-1-2","alpha-1-2-3"]],"del":[]},"s3":{"add":[["alpha-1-2-3"]],"del":[]}}}
```

Each interior tick carries three things at once: the previous stage's ANSWER
(`__host_response_stage_k`), the rel it derives (`s_k`), and the NEXT stage's
DEMAND (`__host_demand_stage_k+1`). The demand for stage k+1 is derived in the
same SQL fixpoint that landed stage k's answer. That is why the chain costs one
tick per stage and not two.

### 1.3 Same-tick chaining: no, and it is forbidden twice

**Blocker 1, the compiler, by name.** Two host atoms in one rule body is a named
refusal, not a silence (`1_host_expand.pl:355`, `probe_mismatch(multiple_probes)`).
Posting

```
s2(out) <- seed(name), stage_1(name, mid), stage_2(mid, out).
```

returns 400 with

```
probe_mismatch(multiple_probes((seed(_56),probe(stage_1,[_56],[_82],[]),probe(stage_2,[_82],[_104],[]))))
```

So a two-shell chain cannot even be SPELLED inside one rule. Chaining must route
through a rel, and a rel crossing is a tick crossing when the producer is a host.

**Blocker 2, the runtime, structurally.** `LiveEngine.tickOnce`
(`serve/3_engine.ts:183`) runs `program.tick(seam, arrivals)`, one Observable that
completes when the emitted SQL fixpoint is done. `ticks$` emits AFTER that. The
host runner is downstream of `ticks$`, spawns with `node:child_process`, and its
stdout arrives on a later event-loop turn. Its only way back in is
`engine.submit()`, which pushes a fresh `QueuedBatch` through the same `concatMap`
that runs ticks one at a time. There is no re-entrant path from inside a tick out
to a subprocess and back into the same fixpoint, and the emitted program is not
resumable mid-fixpoint.

**The general reason, which is why this is a law and not an artifact.** A tick is
a closed derivation over facts already known. Stage k+1's demand is a function of
stage k's ANSWER, and the compiler cannot know an answer. Any construct that put
two shells in one tick would have to suspend the fixpoint on I/O, which is the
same thing as saying the tick is no longer a fixpoint. The tick boundary IS the
effect boundary.

### 1.4 The scaling shape, which is the real cost

Same 3-stage chain, N seed rows posted in ONE batch
(`labs/effect-chain/1_chain.ts` receipt 1c):

| seeds | total ticks | spawns | wall ms |
| --- | --- | --- | --- |
| 1 | 4 | 3 | 44 |
| 10 | 31 | 30 | 274 |
| 50 | 151 | 150 | 603 |

Ticks = `seeds * stages + 1`, spawns = `seeds * stages`. **Every single host
answer gets its own tick.** Ten items through three stages is thirty ticks, not
four. The engine is not batching answers back in, and because host invocations
are serialized by `concatMap` the wall time is the sum of every subprocess.

This is the finding that matters for a real workflow: the per-stage tick cost is
already minimal, and the per-ITEM cost is not amortized at all.

---

## 2. Batching, as it exists

### 2.1 The one mechanism

`serve/1_hosts.ts:393 groupInvocations` is the whole of it, 20 lines:

```ts
function invocationKey(demand: HostDemand): string {
  const orderedInputs = demand.plan.inputs.map((input) => [
    input.name, input.type, demand.inputs.get(input.name) ?? "",
  ]);
  return JSON.stringify([demand.plan.execution, demand.plan.template, orderedInputs]);
}
```

A demand whose `plan.execution` is not `sprefa_extract` is pushed as its own
singleton group and never considered again. Everything else groups by the key
above.

**The compatibility rule, stated exactly.** Two demands share one subprocess iff
all four hold:

1. both are in the same tick's frontier (the runner groups one delta batch, and
   boot replay groups one scan);
2. `plan.execution == "sprefa_extract"` for both;
3. byte-identical `plan.template`;
4. identical values for every declared input column, in declared order.

Different host NAMES are fine, and that is the point: (2)+(3)+(4) let N different
declarations that are N different named PROJECTIONS of one command's stdout share
one run. `execution` is itself decided at compile time by a substring test on the
template text (`registry.pl:222 host_execution/3`: the template must start
`"$DL_EXTRACT_BIN" ` and end `{path}`).

### 2.2 Measured spawn counts

`labs/effect-chain/2_batch.ts`. Every count is a byte a spawned shell appended to
a ledger file, so it is a process count, not a proxy.

Plain `sh` host, one declaration, N distinct input values:

| demands | spawns | ticks | wall ms |
| --- | --- | --- | --- |
| 1 | 1 | 2 | 23 |
| 10 | 10 | 11 | 36 |
| 100 | 100 | 101 | 291 |

The extractor grouping path, 7 registered projection declarations over N paths,
one shared template:

| paths | demands | spawns | ticks |
| --- | --- | --- | --- |
| 1 | 7 | 1 | 2 |
| 10 | 70 | 10 | 11 |
| 100 | 700 | 100 | 101 |

**The "7 subprocesses per path to 1" claim is verified: 700 demand rows produce
100 subprocesses.** Grouping also collapses TICKS, which the earlier arc did not
claim: the 7 projections' response rows are submitted in one `engine.submit`
call, so 700 answers cost 100 ticks and not 700.

Projection-count sweep at 10 paths, and the control that isolates rule (3):

| shape | demands | spawns | ticks |
| --- | --- | --- | --- |
| 1 projection | 10 | 10 | 11 |
| 2 projections | 20 | 10 | 11 |
| 7 projections | 70 | 10 | 11 |
| 7 projections, one flag of template text differing | 70 | **70** | 71 |

The last row is the same seven declarations with `--slot0` … `--slot6` spliced
into otherwise identical templates. Still `sprefa_extract` execution (the prefix
and suffix tests still pass), still one path each; grouping collapses entirely
because the key compares template TEXT.

Rule (2), the executor gate:

| shape | demands | spawns |
| --- | --- | --- |
| two `sh` decls, byte-identical template AND input values, `shell` execution | 2 | **2** |

Rule (4) is what the content-addressed cache already gives, restated:

| shape | spawns |
| --- | --- |
| `item("repeat")` added | 1 |
| the same row retracted and re-added | 1 |

### 2.3 The limits, plainly

- **A plain `sh` host cannot batch at all.** Not "does not by default": the
  `execution !== "sprefa_extract"` branch is an unconditional `continue`. The
  reason given in the code is honest (a shell command may carry effects even when
  its text and inputs match) and it is also the whole of the policy. There is no
  declaration-level way to say otherwise.
- **Grouping is fan-out dedupe, never fan-in.** Compatible demands must have the
  SAME input values. Two demands with different values are never merged, at any
  execution. So the subprocess count is bounded below by the number of distinct
  (template, input-tuple) pairs, which for the ingest workload is the number of
  files. The 7-to-1 win is real and it is orthogonal to the N-files cost.
- **Grouping is frontier-local.** Demands from two different ticks never group,
  even when identical, because each tick's `+delta` is its own batch. That is
  correct given the cache (the second one is a cache hit anyway), and it means
  the batch window is exactly one tick and is not tunable.
- **The batching decision rides a substring match on template text.** Adding a
  flag can silently take a program from 10 spawns to 70. The compiler holds the
  template at compile time and says nothing.

---

## 3. v5's `collect`

### 3.1 What it was

`src/effect.rs:564 collect_inner_var` / `:574 collect_chunk`, consumed by
`rebuild_async` (`src/effect.rs:610`). Surface: `examples/gh-cache-batch.dl`, one
line:

```
pr_resp(body) <- @async watch_pr(slug), pr_batch(collect(slug, 20)) -> (body).
```

`rebuild_async` evaluates the rule body to a solution set, and when one effect arg
is `collect(x[, N])` it takes a different path entirely: gather `x` across ALL
solutions, sort, dedupe, chunk by N, and emit ONE `pending_effect` row per chunk
whose hole carries the comma-joined values, marked `batch=1`. The `batch` flag
tells the drain that the head rebuilds purely from the response (one head row per
output LINE), because no single body solution keys the request.

### 3.2 Run, not read

`labs/effect-chain/3_v5_collect.sh`, five body solutions, one hermetic `sh` decl,
`dl ... --settle` as the one-shot effect runtime:

| spelling | solutions | spawns | requests emitted |
| --- | --- | --- | --- |
| `gather(name)` | 5 | **5** | `{"items":"delta"}` `{"items":"bravo"}` `{"items":"charlie"}` `{"items":"echo"}` `{"items":"alpha"}` |
| `gather(collect(name, 2))` | 5 | **3** | `{"items":"alpha,bravo"}` `{"items":"charlie,delta"}` `{"items":"echo"}` |
| `gather(collect(name))` | 5 | **1** | `{"items":"alpha,bravo,charlie,delta,echo"}` |

The non-collected-arg rule is a loud failure, exit 1, zero answer rows:

```
collect effect `gather`: the non-collected args vary across body solutions;
collect batches ONE request — give the varying arg its own rule
```

That message is v5's version of the same compatibility question v6 answers in
`invocationKey`: v5 requires the non-batched args to AGREE and says so; v6
requires ALL args to agree and says nothing (it just does not group).

### 3.3 What collect gave that today's grouping does not

**Fan-in over distinct values. Grouping does not cover it, not partially.**

| | v5 `collect` | v6 `groupInvocations` |
| --- | --- | --- |
| direction | N distinct input values -> 1 command | N declarations sharing 1 command |
| input values | must DIFFER (that is the payload) | must be IDENTICAL |
| non-batched args | must agree, loud error otherwise | must agree, silently no group otherwise |
| chunking | `collect(x, N)` -> ceil(\|x\|/N) requests | none |
| response fan-out | one head row per stdout LINE (`batch=1`) | one row per stdout line per PROJECTION |
| applies to | any `sh` decl | `sprefa_extract` templates only |
| declared where | at the call site, in the program | in `registry.pl`, by substring match |

The two mechanisms are orthogonal and both are worth having. v6's grouping
removes redundant runs of the same command; v5's collect removes runs entirely by
making one command do N units of work. On the ingest workload v6's win is
7x and v5's would be Nx.

One more difference worth recording: `collect` has **no `op_catalog` row**
(receipt 3c: 28 rows, zero of them `collect`/`async`/`effect`/`sh`). v5's
self-describing catalog does not describe its own effect surface.

---

## 4. Can fan-in be written in v6 today without a new construct?

No, and the gap is one aggregate wide.

A host input is an ordinary column. So the v5 idiom has an obvious v6 spelling
using only shipped constructs, IF a list-shaped aggregate existed:

```
rel batch(items: text).
batch(json_array(slug)) <- watch_pr(slug).            # the fold
rel resp(items: text, body: text).
resp(items, body) <- batch(items), gather(items, body).   # one demand row, one spawn
```

`labs/effect-chain/4_fanin_gap.ts` grades exactly that.

**4a. The fold is refused.** `joined(json_array(id)) <- item(id).` returns 400:

```
unsupported_construct(aggregate_head(json_array(_))); reason=aggregate_head
```

**4b. The rest of the shape works.** Same program with `highest(max(id)) <- item(id).`
loads (200), and five `item` rows produce **one demand row and one spawn**
(`gathered = [[4,"4-gathered"]]`). Nothing about the demand plane objects to an
aggregated head; the reduction from N rows to one host call is already legal. The
only missing piece is an aggregate whose result is the set.

**4c. The shipped inventory, as the compiler answers it:**

| head | status |
| --- | --- |
| `count` `sum` `min` `max` `avg` | 200 |
| `json_array` `json_object` | 400, `refuse(aggregate)` |
| `group_concat` | **200** |

**4d. `group_concat` is a trap, and this is a new finding.** It is not in
`registry.pl` at all, so it is not read as an aggregate. It compiles as a compound
VALUE term and stores its own text, one row per input:

```
{"tick":1,"deltas":{"item":{"add":[[1],[2],[3]]},"tally":{"add":[["group_concat(1)"],["group_concat(2)"],["group_concat(3)"]]}}}
```

Three rows reading `group_concat(1)`, `group_concat(2)`, `group_concat(3)`. No
error, no warning. A cold author reaching for the SQL-flavoured name for the
missing aggregate gets silence and wrong data. This is review finding A5
(undeclared names compile clean) landing precisely on the path someone would take
to hand-roll fan-in.

Two smaller walls found on the way:

- `min`/`max` over a `text` column is refused,
  `aggregate_operand_not_number(max,_,text)`. v5's `op_catalog` states
  "min/max carry the arg type"; v6's are number-only.
- `concat(...)` exists as an n-ary expression lowering to `||`
  (`lower.pl:439`), so a FIXED number of columns can be joined. There is no
  operator that folds a variable number of ROWS. `+` is `both_number`.

So today, fan-in is unreachable from the surface by any route: no list aggregate,
no row-folding concat, no window function to bucket by, and no declaration-level
batch policy. The chunking half of `collect(x, N)` needs a stable per-row ordinal
on top of that (`mod` exists, an ordinal does not).

---

## 5. The line: what comptime prolog can own, and where it stops

### 5.1 What the compiler already decides

Reading `1_host_expand.pl compile_host_decl/2` and the emitted `hostPlans` array,
comptime owns more than it looks like:

| decided at compile time | where |
| --- | --- |
| which effects exist, and their exact command text | `sh_decl/4` -> `host_plan/7` |
| every input and output column NAME and TYPE | `validate_columns/2` |
| that every identity input is referenced by the template, no output is, and no unknown hole is | `validate_template/4` |
| which columns are IDENTITY (key the cache, return on responses) vs FRESHNESS (key the witness only) | `host_input_roles/3` |
| the content-address expressions themselves, as SQL | `digest_expr/6` |
| the executor, hence today's whole batching policy | `host_execution/3` |
| the ORDER of stages, because the rel graph is the order | the rule graph |
| the refusals: two probes in one body, output-as-input, unknown hole, unknown executor | throws, all named |

The emitted module carries all of it as plain data:

```ts
export const hostPlans = [{ name: "stage_1", inputs: [{name:"input",type:"text"}],
  outputs: [{name:"out",type:"text"}], template: "...", demandRel: "__host_demand_stage_1",
  responseRel: "__host_response_stage_1", execution: "shell" }, ...];
```

### 5.2 What is irreducibly runtime

| decided at run time | why it cannot move |
| --- | --- |
| which demand ROWS exist | the extension of a rel is data |
| whether a witness was already answered | durable state across restarts |
| which demands land in the same frontier | arrival timing |
| the answer, and therefore every stage after the first | the whole point of an effect |
| invocation grouping, today | it compares input VALUES |
| invocation order | `concatMap` over a live stream |

### 5.3 The boundary, in one sentence

**Comptime owns an effect's SHAPE and its IDENTITY FUNCTION; runtime owns its
EXTENSION and its ANSWER; and a chain is exactly a place where one stage's shape
is comptime while its extension is a function of the previous stage's answer,
which is why the tick boundary and the effect boundary are the same boundary.**

Two examples on each side of it, from this lab:

*Comptime side.* Whether `call_node` and `call_ref` CAN share a subprocess is
fully decided by their declarations: same executor, same template text, same
declared input list. The compiler holds all three. Today it decides one of them
(`host_execution/3`) and leaves the other two to a runtime string compare of the
same compile-time data. The 70-vs-10 spawn control in receipt 2 is a compile-time
property measured at run time.

*Runtime side.* Whether `stage_2` runs once or fifty times is decided by
`stage_1`'s stdout. No amount of comptime analysis reaches it. The compiler can
know that `stage_2` runs AFTER `stage_1` and with WHICH command text, and that is
already the plan; the cardinality is not knowable.

### 5.4 Consequence, and the smallest correct move

The user's question was "how much can be pushed into comptime prolog". The
measured answer is: the planning is ALREADY there and it is not the bottleneck.
Every gap this lab found is a missing FOLD, not a missing plan.

- Chaining costs the theoretical minimum in ticks per stage (N+1) and is already
  fully comptime-planned. Nothing to move.
- Batching per item is where the cost is (receipt 1c: 50 items through 3 stages
  is 150 serialized subprocesses and 151 ticks), and the one thing that would
  reduce it is the fold v5 had and v6 does not.
- The smallest correct move is therefore ONE list-valued aggregate head. With
  it, v5's `collect(x)` is expressible in shipped constructs with zero new effect
  machinery, because a host input is already just a column and receipt 4b proves
  an aggregated head already collapses N rows into one demand and one spawn. The
  chunked form `collect(x, N)` needs a second thing (a stable per-row ordinal) and
  should be priced separately.

Not proposed here, and deliberately: whether the batch policy should be spelled
in the declaration instead of matched out of template text. That is a spelling
question with an existing registry-row answer, and it is the user's call.

---

## 6. Findings

| # | finding | status |
| --- | --- | --- |
| F1 | N-stage chain = N+1 ticks, one hop per stage, no drain waste | measured, no defect |
| F2 | Same-tick chaining is refused by name (`probe_mismatch(multiple_probes)`) AND structurally impossible (a tick is a closed fixpoint; effects are observed only at its boundary) | correct as-is |
| F3 | Every host answer gets its own tick: `seeds * stages + 1` ticks, and invocations are serialized by `concatMap`, so wall time is the sum of every subprocess | real cost, unowned |
| F4 | `groupInvocations` verified: 700 demands -> 100 spawns at 100 paths; it also collapses ticks 700 -> 100, which the earlier arc did not claim | verified |
| F5 | Grouping compatibility is (same frontier) AND (`sprefa_extract`) AND (byte-identical template) AND (identical input values). A plain `sh` host cannot batch at all, at any input | measured, four-way control |
| F6 | The batching decision rides a substring match on template text; one added flag silently took the same 7 declarations from 10 spawns to 70, and the compiler holds the template and says nothing | named, diagnostic gap |
| F7 | v5 `collect` is fan-in over DISTINCT values (5 -> 3 at chunk 2, 5 -> 1 unchunked, measured); v6 grouping is fan-out dedupe over IDENTICAL values. Orthogonal; grouping does not cover collect even partially | measured both sides |
| F8 | Fan-in is unreachable from the v6 surface today: no list aggregate (`json_array` refused), no row-folding concat, no ordinal to chunk on | measured |
| F9 | **`group_concat(x)` in a head compiles clean (200) and is not an aggregate**: it stores the literal text `group_concat(1)`, one row per input, no error. The exact spelling a cold author reaches for when hand-rolling fan-in | NEW DEFECT, unowned |
| F10 | `min`/`max` are number-only in v6 (`aggregate_operand_not_number`); v5's catalog says min/max carry the arg type | divergence, named |
| F11 | v5's `collect` has no `op_catalog` row; the self-describing catalog does not describe the effect surface at all (0 of 28 rows) | v5-side gap |
| F12 | Compiler refusals still print as swipl `Unknown message: probe_mismatch(...)` with no file or line (receipt 1b), which is review finding B4 reproduced | known, unowned |

## 7. Named slots

- `slot_batch_policy_residency` — is a host's batchability a registry row keyed
  off template text (today), or a word in the `sh` declaration? F6 is the cost of
  the current answer.
- `slot_list_aggregate_spelling` — the one construct F8 needs. `json_array` is
  already the refused registry row; whether the un-refusal is `json_array` or a
  differently-named text fold is a spelling call, and F9 says whatever it is
  called, `group_concat` must stop being silently accepted.
- `slot_row_ordinal` — the chunked `collect(x, N)` half. Needs a stable per-row
  ordinal; `mod` already exists for the bucket arithmetic.
- `slot_answer_coalescing` — F3. Host answers could be buffered into one arrival
  batch the way the watch bind already buffers file events
  (`bufferTime(100ms)`, `serve/2_binds.ts`). That is a runtime change and this
  lane made none.

## 8. What a receipt would need that this lane did not make

Per the lab's own terms, the one measurement blocked on a runtime change:

**The cost of answer coalescing (slot F3 / `slot_answer_coalescing`) cannot be
measured without editing `serve/1_hosts.ts`.** Today `runInvocation` calls
`engine.submit` per invocation, so answers are 1:1 with ticks by construction and
there is no knob to turn off. Measuring the alternative means adding a
`bufferTime`-shaped merge between the host runner's completions and
`engine.submit`, which is a production edit this lane is not making. The change
is small and localized (one operator between `concatMap((invocation) => ...)` and
the submit, plus a decision about whether the drain-boundary tick numbering the
runtime bridge already documents can absorb it). Stated, not made.
