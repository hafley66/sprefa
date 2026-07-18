# Resource-aware scheduler: schedule from declared read/write sets, not kind priorities

Status: PLAN (research + design, no implementation). Branch `sched-plan`, based on
`next` @ e06b14f7.

User directive: ordering jobs by type in a hand-ranked order is not good enough. In a
frontier tick we KNOW which tasks touch which files/repos/rels. The schedule must
derive from declared resource dependencies (read/write sets) and machine physics
(CPU/IO/memory budgets). "Physically correct" means: two jobs run concurrently
exactly when their declared resources permit it, ordering falls out of
who-produces-what, and `dl daemon why` can name the resources a running job holds
from the on-disk trail alone.

## Baseline (what this plan replaces piecewise)

- `src/jobq/mod.rs`: durable single-writer SQLite queue (`<home>/jobs.sqlite`).
  `_job(key PK, kind, root, arg, priority, state, dirty, cancelled, attempts,
  run_at, enqueued_at, started_at, finished_at, last_error)`, index
  `_job_ready(state, run_at, priority)`. Claim = `priority DESC, run_at ASC,
  enqueued_at ASC LIMIT 1` (mod.rs:339). Coalescing key-UPSERT (mod.rs:275),
  jittered backoff, 900s lease sweep, `reset_running_on_boot`.
- Key vocabulary today: `tick:{root}`, `sink:{root}`, `cold:{root}:{family}:{shard}`.
- Workers (`src/daemon_shell/jobs.rs:21`): `n_workers = max(2, cores/4)` tokio tasks
  for Tick+SinkDrain, exactly ONE ColdExtract worker (daemon.rs:1297, 1308). Claim
  loop awaits `job_notify` (tokio Notify, daemon_shell/mod.rs:62) with a 500ms
  backstop (jobs.rs:71-77). Per-root serialization is by key dedup, not pinning.
- Budgets: `apply_process_budget` (daemon.rs:1096) = QoS UTILITY + nice +10 + IOPOL
  THROTTLE; `apply_daemon_budget` (daemon.rs:1131) adds PRIO_DARWIN_BG + rayon width
  `daemon_thread_count` (daemon.rs:1075). Env: DL_NO_BUDGET, DL_DAEMON_THREADS.
- Cold staging (`src/engine/cold_stage.rs`, plan 2026-07-17, IMPLEMENTED):
  `_cold_node(family, shard)` in the corpus db; dataflow drains in 512KiB
  contiguous byte-sorted chunks (`COLD_CHUNK_TARGET_BYTES`, 64-file cap,
  cold_stage.rs:58); `scip-index` node at priority `count+1` (cold_stage.rs:246);
  family priority is the hand rank `count - idx` (cold_stage.rs:218). Measured
  (release, 3.37MB corpus): longest chunk 766ms, call resolution barrier 591ms
  (irreducible corpus-global name→def), completion tick 119ms.
- Attribution today: `why.jsonl` samples every ~2s (src/why.rs) joining
  `activity::Phase` + `tick_root` + job key + cpu/io/rss; `perf.jsonl` per phase.
  No persisted read/write-set or file-set attribution.
- No scope columns exist in any schema. The nearest partition axes:
  `_cold_node(family, shard)` and the in-memory `program_scope: HashSet<String>`
  of rel names (tick.rs:810).

The hand-ranked priority integer is doing three jobs at once: (a) encoding the
producer→consumer DAG (scip before call, module before type), (b) encoding
importance, (c) tiebreaking. This plan splits them: (a) becomes derived readiness
from read/write sets, (b) becomes a computed utility (consumer count / critical
path over the same sets), (c) stays a tiebreak.

Contents:
1. Theory synthesis: scheduling as selection over the ready frontier
2. Build-vs-buy survey (the law: candidate-by-candidate, before any bespoke line)
3. The design (four layers: signatures, pseudo-code, lifetimes, storage)
4. Migration path (each step shippable)
5. Receipts: what proves "physically correct"
6. Open user decisions

---

## 1. Theory synthesis

### 1a. The formal frame

Every scheduler surveyed below is one machine:

```
State:      a set of TASKS, each with
              reads(t), writes(t)   declared or implicit resource sets
Derived:    deps(t)  = { pending/running j : writes(j) ∩ reads(t) ≠ ∅ }
            READY(t) iff deps(t) = ∅            (Kahn firing rule: a node fires
                                                 when all input tokens are present)
Conflict:   CONFLICT(a, b) iff writes(a) ∩ (reads(b) ∪ writes(b)) ≠ ∅
                            or writes(b) ∩ reads(a) ≠ ∅
            (database conflict serializability, Eswaran/Gray 1976)
Selection:  among READY tasks not conflicting with any RUNNING task,
            pick argmax utility(t), subject to budget (width, memory, IO tier)
Advance:    completing t empties other tasks' deps: the frontier moves
            (timely dataflow progress tracking, restated)
```

Everything else is policy inside the frame: the resource vocabulary, the utility
function, the width, and what is durable. Four theory results anchor the policy:

- **Graham list scheduling (1966)**: greedy selection from the ready set onto m
  machines is a (2 - 1/m)-approximation of optimal makespan REGARDLESS of the
  priority order. Consequence: readiness + conflict admission carry the
  correctness; utility only trims the constant. A mediocre utility function
  cannot break the schedule.
- **Critical path / HEFT upward rank**: utility(t) = cost(t) + max over consumers
  of utility(consumer). "Longest chain first" is the standard antidote to
  head-of-line blocking; it derives scip-first (scip feeds the whole resolution
  ladder) instead of hand-coding it.
- **Blumofe-Leiserson**: T_p <= T_1/p + O(T_inf). With width capped at 2, span
  dominates. The measured 591ms call barrier IS T_inf; no selection policy beats
  shortening the critical path. Sets expectations honestly.
- **Multi-granularity locking (Gray/Lorie/Putzolu/Traiger 1976)**: hierarchical
  resources (db > file > record; here root > rel > shard) take intention modes
  (IS/IX) at ancestors so a coarse writer (X on the rel) collides with a fine one
  (IX left by a shard writer) in O(depth) prefix tests, never by enumerating
  leaves. This is the exact conflict test for our scoping; the compatibility
  matrix is standard (IX~IX compatible, X~anything incompatible).

Bin packing already lives in the codebase: the 512KiB chunker is first-fit on the
byte-sorted file stream (bin = target bytes, cap = 64 files). Its role in the
frame: bound per-task cost so greedy selection is never stuck behind one huge task,
the same reason morsel-driven engines pick ~100K-row morsels and tokio caps a task
at ~128 polls between yields.

Determinacy license: Kahn (1974) proves a network of processes that are pure
functions of their declared inputs has one unique result independent of execution
order and speed. Our jobs are digest-guarded idempotent refreshes over declared
inputs; any conflict-respecting schedule yields identical relation contents.
Scheduling is thereby a pure performance decision, which is what makes this
redesign safe to do incrementally.

### 1b. The surveyed schedulers, mapped onto the frame

Event-loop claims verified against current spec/source (2026-07); Go per the
Vyukov scalable-scheduler design doc + runtime/proc.go; k8s per the
kube-scheduler framework docs (detail in section 2m).

| | Task sources | Queue structure | Selection policy | Starvation control | Cooperation / preemption |
|---|---|---|---|---|---|
| HTML / Blink | spec task sources (input, timers, networking, rendering, postTask) mapped via TaskType to queues | many per-source per-frame queues + microtask queue + idle queue | spec: "choose one task queue in an implementation-defined manner"; Blink: fixed priority (input > gesture-compositor > normal > best-effort) | explicit anti-starvation: scheduler occasionally runs lower-priority tasks; requestIdleCallback deadline capped at 50ms; idle periods end early when urgent work arrives | run-to-completion; microtask checkpoint drains to empty after each task; rendering interleaved at vsync opportunities |
| asyncio (CPython main) | call_soon, call_at, selector IO events | ONE FIFO deque `_ready` + min-heap `_scheduled` on monotonic time | strict FIFO over a snapshot of `_ready`; expired timers admitted per iteration | none needed (single queue); heap rebuilt at >50% cancelled timers | run-to-completion callbacks; 3.12 eager tasks skip the loop if they never block |
| tokio multi-thread | spawns, wakes, IO/timer driver | per-worker 256-slot ring + LIFO slot + mutex global injection list | LIFO slot > local FIFO > global queue every ~10ms (dynamic interval); steal half when empty | global-queue interval bounds injection latency; LIFO slot capped at one; searching-worker cap | cooperative budget ~128 polls, resources return Pending at zero; driver polled when idle or every 61 tasks |
| Go runtime (GMP) | go statements, timer wheel, netpoller readiness | per-P local ring (256) + runnext slot + global queue; GOMAXPROCS Ps multiplexed onto Ms | runnext > local FIFO > global (checked 1/61 schedticks) > netpoll > steal half from a random P | the 1/61 global-queue check; sysmon retakes Ps running one G > 10ms; P handoff when an M blocks in a syscall | preemptive since 1.14 (async signal-based); before that cooperative at prologue checks; netpoller feeds IO-ready Gs back into run queues |
| kube-scheduler | pending Pods (declared resource requests, affinities) | one scheduling queue over the cluster state | filter (feasibility predicates) then score (weighted priorities, bin-packing spread/pack) then bind, one Pod at a time | priority + preemption: a high-priority unschedulable Pod may evict lower-priority ones | not applicable (placement, not execution); declarative: schedule derives from declared requests vs node capacity |

Readings for this repo:

- **Chromium is the cautionary tale the user's directive predicts.** It started
  from per-source queues with fixed priorities (our per-kind claim) and grew
  anti-starvation, freezing/throttling policies, idle deadlines, and dynamic
  reprioritization (postTask TaskController) because queue identity is too coarse
  a scheduling signal. We skip that arc by scheduling on the resources directly.
- **asyncio is the honest floor**: one worker + run-to-completion needs only
  FIFO + a time heap. Our daemon is asyncio-shaped per kind today. This plan is
  the step to conflict-admitted width where declared sets prove disjointness.
- **tokio's cooperative budget maps to chunk sizing**: a task must reach a yield
  point (chunk boundary) in bounded cost so selection gets a say again. The LIFO
  slot (run the task you just unblocked) is the locality argument for preferring
  a just-unblocked consumer at equal utility.
- **Idle-until-urgent** (Blink idle tasks + eager evaluation on first demand) is
  cold extraction's exact posture: background-tier work that interactive demand
  preempts at the next node boundary.
- **Microtask checkpoint** = a nested drain-to-empty at a boundary: the
  sink-drain after tick and the coalescing UPSERT (rapid saves collapse before
  the next claim) are both this shape.
- **Go's sysmon is the lease sweep**: an external monitor reclaiming a
  scheduling slot from a worker that ran too long, exactly `sweep`'s 900s lease
  reclaim at a different timescale. **P handoff on blocking syscall** is the
  dbw phase-split argument (design 3.4): a worker blocked on IO should not hold
  a scheduling slot; declaring the flush phase separately is our handoff. **The
  netpoller** is the doorbell: external readiness (IO there, enqueue/finish
  here via job_notify) feeds the ready set instead of being polled for. **The
  1/61 global check** is starvation control by counter, the same role our
  barrier rule plays by scope.
- **kube-scheduler is the declarative-placement member of the family**: its
  filter / score / bind decomposition is exactly readiness+conflict / utility /
  claim in the frame. It proves the user's target shape at datacenter scale:
  the schedule derives from declared requests against capacity, and priority is
  a scoring input, never the mechanism. Section 2m covers what does and does
  not transfer.

RxJS mapping (the working vocabulary for discussing this design): an RxJS
scheduler is a process manager deciding when subscribed work runs (queue /
asap / async map onto urgency classes), and the flattening operators are
admission policies for a new task arriving while a conflicting one runs:

- `concatMap` = queue it behind the running one: our per-key serialization by
  construction (one row per key, next run after finish).
- `exhaustMap` = drop the new arrival while busy: our coalescing UPSERT is
  exhaustMap with a trailing rerun (the dirty flag replays the latest
  coalesced arg once, rather than dropping it entirely).
- `switchMap` = cancel the running one, start the new: the reserved
  `cancelled` column + J3 mid-run cancellation checks.
- `mergeMap(n)` = admit up to n concurrently: conflict-gated width admission
  (step 4), where n is the budget width and "may these overlap" is the scope
  conflict test instead of unconditional.

The design in section 3 is therefore: exhaustMap-with-trailing per key
(unchanged), mergeMap(width) across keys gated by declared scopes (new), with
switchMap reserved for J3 cancellation.

### 1c. The frontier already exists on disk

Timely dataflow's progress tracking ("no more work at times < F can arrive")
restates what `_cold_node.state`, the `extract:<family>` digests, and
`_derived_complete` already are: a durable record of which productions are
complete. The scheduler needs no new frontier mechanism; it needs to READ the one
on disk. A job whose read set names a resource whose producer has not completed is
not READY. That single rule replaces the scip-before-call hand ranking, because
`ExtractFamily::rels()` (extract_family.rs:105) already declares every family's
write set and the resolver inputs (scip_ref, module_edge_rev, ...) are its read
set.

---

## 2. Build-vs-buy survey

Standing law: candidate-by-candidate written analysis before any bespoke proposal;
no one-line dismissals. Requirements matrix:

- **R1 durable**: survives kill -9; fits the SQLite single-writer posture.
- **R2 resource-declaring tasks**: read/write sets over files/repos/rels.
- **R3 dependency-DAG ordering**: readiness derived from produced-by, not rank.
- **R4 budgeted execution**: honors the existing caps; nothing seizes the machine.
- **R5 self-diagnosis**: `dl daemon why` attributes from the on-disk trail.
- **R6 no new async runtime unless justified.**
- **R7 effect purity** (user criterion, 2026-07-18): does the crate itself do
  IO/syscalls/spawns/network, or is it pure computation? The repo marks side
  effects explicitly; an effectful dep is opaque to that discipline. Pure
  computation (verifiable by absence of std::net/std::process/std::fs usage) gets
  strong preference over runtime/broker-shaped candidates. Cargo.lock holds 467
  packages and zero graph libs today; the user is open to adopting a pure
  graph-algorithms crate ("it probably does its job really well").

Policy line (user, 2026-07-18): the SQL fixpoint stays canonical for REACTIVE
rels (closure/scc ops, digest-guarded). An adopted graph library serves
IN-PROCESS scheduler/analysis algorithms (toposort, critical path, transitive
reduction) on small materialized graphs. Two graph machineries, one boundary:
data-plane graphs live in SQL, control-plane graphs live in the library.

### 2a. bevy_ecs schedules (declared-access auto-parallelism)

The closest existing "physically correct by declared access" system in Rust.

- Access is derived from parameter types, never hand-declared: `Query<&T>` = read
  on component T, `Query<&mut T>` = write, `Res/ResMut` likewise; `&mut World`
  (exclusive) = write-everything. Merged per system into a
  `FilteredAccessSet<ComponentId>` (read bitset + write bitset + filter terms).
- Conflict test: writes_A ∩ reads_or_writes_B or writes_B ∩ reads_or_writes_A,
  as fixed-bitset intersections, with With/Without filters able to prove
  disjointness. Same-system aliasing panics at init; cross-system conflicts
  merely serialize.
- Since 0.16 (PR #16885) all pairwise conflicts are computed ONCE at schedule
  build; each system carries a precomputed conflict bitmask and runtime dispatch
  is "bit tests against the running set". Deliberately conservative (archetype-
  level runtime checking was removed as not worth its cost).
- Dispatch rule: run when (a) all DAG predecessors (.before/.after/.chain edges)
  finished and (b) conflict mask ∩ running set = ∅. Plus ambiguity detection at
  build: any conflicting pair with no ordering path is reported.
- Usable standalone (bevy_ecs is a separate crate, headless World+Schedule), but
  jobs would have to become statically registered systems over ECS data; our
  tasks are dynamically generated per corpus and our state is SQLite rows.
- Purity: the core is computation, but the multi_threaded executor spawns its own
  task pool (bevy_tasks), and adopting the crate means hosting our state inside
  its World. Dependency weight is real.
- What it lacks for us: hierarchical resources (flat ComponentId space; no
  root > rel > shard lattice), durability (R1 fail), priorities/cost model,
  dynamic task sets.

Verdict: **borrow-the-pattern, reject adoption.** The pattern: access sets
derived from the job's type/constructor, conflicts precomputed where possible,
runtime admission = cheap set tests against the running set, plus ambiguity
detection as a lint. The rejection: no durability, flat resource space, static
system registration, and an executor with its own thread pool (R1, R7).

### 2b. petgraph (+ daggy)

- Pure graph algebra: `toposort` (O(V+E), Err(Cycle)), `is_cyclic_directed`,
  `tarjan_scc`, `has_path_connecting`, transitive reduction + closure
  (`algo::tred::dag_transitive_reduction_closure`). `Graph`/`StableGraph`/
  `GraphMap` storage. daggy wraps StableGraph with cycle-rejecting `add_edge`;
  low activity, petgraph alone suffices.
- Purity: pure computation, no std::net/std::process/std::fs in the library, no
  runtime, no spawns. Default features are lightweight (fixedbitset, indexmap).
  The strongest R7 score in the survey.
- What it gives the scheduler: cycle rejection at seed time (a cyclic dependency
  declaration is a bug we want loud), transitive reduction before rendering the
  job graph in `why`/vscode, closure for fast "does A precede B" queries
  (distinguishing ordered-by-dependency from conflicting-and-unordered, bevy's
  ambiguity lint), longest-path for HEFT upward rank.
- What it lacks: everything scheduling-specific (running state, weights,
  admission), which is exactly the part that must be ours because it is welded
  to `_job`/SQLite. That split is clean: petgraph owns graph math on the small
  materialized job DAG (~tens of nodes); jobq owns states and durability.

Verdict: **adopt** for control-plane graph algorithms, under the policy line
above (SQL fixpoint stays canonical for reactive rels). First uses: cycle check +
toposort + longest-path at cold-seed time; transitive reduction for `why` output.

### 2c. pathfinding (implicit-graph algorithms)

- Algorithm inventory: dijkstra/astar/bfs/dfs/idastar (shortest paths),
  `topological_sort`, `topological_sort_into_groups`,
  `strongly_connected_components`, kuhn_munkres (assignment), connected
  components. Actively maintained, wide use.
- API shape: implicit graph. Every algorithm takes a successors closure
  (`FnMut(&N) -> impl IntoIterator<Item = N>` or `(N, C)` with costs) instead
  of a materialized structure. The user's flagged fit: when edges live in
  SQLite, the closure can query them lazily and no graph object is built.
- `topological_sort_into_groups` returns Kahn frontier LAYERS: group 0 = nodes
  with no predecessors, group k = nodes whose predecessors are all in groups
  < k. That is literally the ready-frontier staging of section 3 (group 0 = the
  initially READY set), named as a library function.
- Purity: pure computation. No IO, no spawns, no runtime; deps are itertools /
  indexmap / num-traits class.
- vs petgraph for our use: pathfinding avoids materialization; petgraph has the
  richer DAG toolkit (transitive reduction + closure, stable indices, longest
  path via toposort walk). The control-plane job DAG is tens of nodes, so
  materialization costs nothing, and the policy line already keeps big
  data-plane graphs in SQL where the fixpoint is canonical.

Verdict: **strong alternative to petgraph; adopt exactly one.** Recommendation
stays petgraph (tred/closure for `why` rendering, longest-path walk); if the
closure-over-SQL style is preferred, pathfinding covers toposort + SCC + the
frontier-layering primitive with equal purity. Decision 1 in section 6.

### 2d. rayon (in use)

- Gives: fixed-width pool, per-thread deques, work stealing, structured
  scope/join, panic propagation. Already the compute layer under
  `daemon_thread_count` width.
- Does not give: priorities (deque order only), fairness, preemption, resource
  awareness, per-task cancellation. A width-2 pool with two long tasks is
  head-of-line blocked regardless of importance; stealing only helps splittable
  work.
- Purity: pure computation API but owns a global thread pool (spawns OS threads).
  Already in-tree and budget-capped; no new exposure.

Verdict: **keep as the execute layer, never the planner.** The planner decides
WHICH jobs are admitted; rayon parallelizes WITHIN a job's compute phase. The
width cap stays the daemon's, not rayon's default.

### 2e. tokio as scheduler parts (already the daemon shell)

- The daemon already runs workers as tokio tasks over spawn_blocking
  (daemon_shell/jobs.rs:39) with a Notify doorbell. No NEW runtime question
  arises (R6 satisfied by status quo).
- Borrowable primitives: `Semaphore::acquire_many` (weighted permits =
  byte-budget admission), `OwnedSemaphorePermit` (droppable resource lease),
  `JoinSet` (running-set bookkeeping). Borrow the SHAPES; the conflict test
  itself must live at claim time in jobq where it is durable and attributable.
- Purity: a runtime by definition; but already a dependency, so R7 cost is sunk.

Verdict: **borrow-the-pattern** (owned droppable permits as the in-memory mirror
of held scopes); no new adoption needed.

### 2f. timely-dataflow / differential-dataflow

- Progress tracking is the cleanest formal answer to "when may downstream run
  under concurrent upstream production": per-edge counts of outstanding
  capabilities, frontier = antichain of times that may still arrive.
  `Antichain`/`MutableAntichain` are plain data types usable standalone.
- Adopting the runtime: dedicated worker threads for the process lifetime
  (`timely::execute`), graph fixed at construction, state in memory (restart =
  replay), its own comm stack. R1 fail, R4 awkward, R7 fail (spawns threads).
- Our timestamps are trivial (one generation per resource, total order per
  root), so the antichain machinery is overkill: the frontier collapses to "is
  the producer's digest/state row present", already durable on disk (1c).

Verdict: **reject adoption; the borrowed idea (frontier = durable completion
records) is already implemented** as `_cold_node.state` + digests. Cite it as
the semantics, not a dependency.

### 2g. salsa

- Demand-driven memoized query graph: inputs carry revisions, derived queries
  record dependencies at runtime, red-green revalidation with early cutoff,
  durability tiers skip whole revalidation classes, fixpoint cycle handling,
  parallel queries with cancellation on new revisions.
- Pull-only: nothing computes until demanded. A daemon that must eagerly fill a
  corpus after a file event is push-shaped; driving salsa would mean
  synthesizing demand for every derived output every tick, inverting its design.
- No durability: the memo graph is in-RAM; kill -9 loses it (rust-analyzer's
  persistent-cache issue is still open). R1 fail.
- Purity: the best of the queue-survey set. Pure computation over its own
  storage structs; no IO, no thread spawns of its own (parallelism is
  caller-provided).
- Overlap with in-house machinery: the `src:`/`drv:`/`extract:` digests and
  `_derived_complete` ARE a durable red-green system already; adopting salsa
  would duplicate, on weaker (in-RAM) footing, what the engine has on disk.

Verdict: **reject adoption; the borrowed ideas (revision cutoff, durability
tiers as stage order) are already implemented in the digest layer.** Cite as
semantics.

### 2h. apalis

- tokio + tower job framework; backends via sqlx (Postgres/MySQL/SQLite),
  Redis, cron. Middleware layers give retry/timeout/rate-limit/concurrency
  per worker, tower-style.
- Scheduling semantics: per-backend FIFO with run_at; no job-to-job
  dependencies, no resource declarations, no conflict admission. Priorities are
  not a first-class cross-backend concept.
- Orphan recovery: `reenqueue_orphaned` on a live interval (already cited and
  borrowed in jobq's lease sweep, mod.rs:49).
- Dependency weight: drags tokio + sqlx + tower; the SQLite backend brings its
  own pool and migrations, displacing the in-house `db::open` posture.
- Purity: runtime/broker-shaped, effectful throughout (IO, spawns, timers).

Verdict: **reject adoption (re-confirmed); the recovery pattern is already
borrowed.** Nothing in it addresses R2/R3, which are the point of this plan.

### 2i. effectum

- SQLite-backed durable queue (the closest external analog to jobq): jobs with
  priority, run_at, `weight`, retries with randomized backoff, recurring jobs,
  job-type routing to workers, `expires_at` heartbeat extension.
- The `weight` concept is the survey's one genuine budget prior: a worker has a
  max concurrency measured in weight units and a claimed job consumes its
  declared weight, so one heavy job can occupy what would otherwise be several
  light slots. That is scalar bin-packing admission at claim time.
- Still no read/write sets, no dependencies between jobs, no conflict test:
  weight is a magnitude, not an identity. Two weight-1 jobs that write the same
  table are happily run together.
- Owns its database file, connection handling, and worker tasks (tokio);
  adopting it means a second SQLite management layer beside `db::open`, and its
  startup-only `expires_at` recovery is weaker than jobq's live sweep (already
  noted at mod.rs:47).
- Purity: effectful (owns DB IO, spawns tasks, timers).

Verdict: **reject adoption; borrow `weight` as the shape of scalar budget
admission** (our estimated-bytes cap in 3.4 is effectum weight with bytes as
the unit).

### 2j. underway / aide-de-camp / other durable-queue crates

- **underway** (Postgres/sqlx): jobs as multi-STEP functions, each step a
  durable checkpoint with its own retry policy; steps run strictly in
  sequence. Dependency composition exists but only as a linear chain within
  one job, not a DAG across jobs. Postgres-only (R1 fail for us). Effectful.
- **aide-de-camp**: SQLite backend exists but the project is dormant and its
  own docs warn SQLite lacks the row locking its multi-worker design assumes,
  the exact mismatch jobq's single-writer claim already sidesteps. Effectful.
- **sqlxmq** (Postgres-only, message-queue-shaped, low maintenance), **fang**
  (Postgres, cron-flavored, FIFO per queue), **hatchet** (a separate server
  platform with a Rust client; broker-shaped, the heaviest possible
  dependency for an in-process daemon). All effectful; none declare resources
  or dependencies beyond queue identity.
- Cross-cutting finding of the survey: **no crate in the Rust durable-queue
  ecosystem ships read/write-set conflict scheduling.** The closest artifacts
  are effectum's scalar weight and underway's linear steps. The capability this
  plan needs does not exist off the shelf; what exists off the shelf (durable
  claims, leases, backoff) jobq already implements against the same prior art.

Verdict: **reject all as dependencies; the survey's value is negative
confirmation** that R2/R3 must be built, and positive confirmation that it
should be built ON jobq rather than beside a crate that would have to be
forked to learn about scopes.

### 2k. buck2 / DICE; Bazel action scheduling

- DICE (buck2's incremental computation engine) is published in the buck2
  workspace and on crates.io but self-described as experimental and built for
  buck2's internal use: async key/computation graph, versioned values,
  cancellation, projections. Adopting it means adopting buck2's async runtime
  posture and an API explicitly not stabilized for outsiders.
- Bazel's local action scheduling is the useful prior: each action (spawn)
  declares resource ESTIMATES (cpu, ram; `--local_resources` sets machine
  capacity) and a ResourceManager admits spawns only while declared totals fit
  capacity, blocking the rest. Declared-request admission against a machine
  budget, shipped at scale for years.
- Skyframe/DICE ordering is demand-driven over the key graph (dependencies
  discovered during evaluation), with the action cache as the durable resume
  point; the RAM node graph itself dies with the server (the cold-start plan
  already borrowed the node-graph-as-cursor and re-hosted it in SQLite).
- Neither system has write-write conflict admission (build actions are
  write-disjoint by construction: each action owns its declared outputs). The
  lesson transfers inverted: MAKE jobs write-disjoint where possible (chunk
  shard atoms) so admission degenerates to the cheap case.
- Purity: DICE is runtime-entangled (async, spawning); Bazel is a JVM server.

Verdict: **reject DICE as a dependency; borrow Bazel's declared-estimates
admission** (it is decision 6's perf-fed cost model with capacity = budget
width) **and the outputs-disjoint-by-construction discipline.**

### 2l. SQLite single-writer vs SKIP LOCKED patterns

- The SKIP LOCKED literature (que, graphile-worker, the 2ndQuadrant/EDB
  writeups) solves ONE problem: N concurrent claimers hammering one table
  without serializing on row locks (`FOR UPDATE SKIP LOCKED LIMIT 1`).
- We have exactly one claimer process; `BEGIN IMMEDIATE` on the single writer
  IS the arbitration (jobq already states this at mod.rs:321). The machinery is
  vacuous here; what transfers is everything around it, already borrowed:
  covering index on (state, run_at, priority), lease/heartbeat columns, batch
  claims, jittered backoff.
- What the literature adds that jobq has NOT borrowed yet: claim windows larger
  than 1 (claim a batch of admissible jobs in one tx when width > 1), which
  section 3's candidate window anticipates.

Verdict: **pattern already absorbed; nothing further to adopt.**

### 2m. The jokes, taken seriously

**Kubernetes.** The kube-scheduler's decomposition (filter feasibility by
declared requests vs node allocatable, score survivors by weighted strategies
including bin-packing, bind the winner) is precisely claim_admissible's
readiness/conflict filter, utility score, and claim: the design in section 3 IS
a one-node kube-scheduler over rels instead of nodes. What does not transfer:
everything that makes k8s k8s: the API-server/etcd control plane, informers,
eviction/preemption of RUNNING pods (we never kill a running job for placement;
cancellation is J3's cooperative flag), node heterogeneity, and pod lifetimes
measured in days (our jobs live milliseconds to seconds, so placement mistakes
self-correct at the next claim rather than compounding). The concept transfers;
the machinery would be a datacenter scheduler bolted to a laptop daemon.

**Celery.** The broker/worker split would buy process isolation, a mature
routing vocabulary, and canvas composition, whose chord (run a group, then a
barrier callback when all land) is exactly the cold completion gate. It would
cost: a broker process (Redis/RabbitMQ) under a turnkey CLI, serializing jobs
that are currently in-process closures over the shared engine mutex, at-least-
once semantics (task_acks_late + prefetch tuning) replacing exactly-once-per-
key coalescing, and chord synchronization that celery itself implements by
polling. Its scheduler also declares nothing about resources: routing keys are
queue identity, the exact hand-ranked-kind shape this plan retires. The
composition vocabulary is worth stealing as NAMES (our completion gate is a
chord; per-key serialization is a chain); the architecture is a mismatch.

**Postgres.** Three genuine offers: advisory locks (`pg_try_advisory_xact_lock`
on int pairs: a scope atom hashed to a lock id is a DB-arbitrated conflict
test), SKIP LOCKED multi-claimer claims, LISTEN/NOTIFY as a cross-process
doorbell. All three matter exactly when multiple PROCESSES contend for the
queue. We have one daemon process, an in-process doorbell (tokio Notify), and a
single-writer claim; at this scale Postgres buys operational weight (a server
dependency for a CLI tool) and no capability. The advisory-lock idea survives
as pattern: our `_job_scope` rows + prefix conflict test are advisory locks
with hierarchy, which plain advisory lock ids (flat ints) cannot express.

### 2n. Verdict table

| Candidate | R1 durable/SQLite | R2 rw-sets | R3 DAG order | R7 purity | Verdict |
|---|---|---|---|---|---|
| bevy_ecs | no | yes (flat, in-RAM) | yes (static) | impure (task pool) | borrow pattern: derived access sets, precomputed conflicts, ambiguity lint |
| petgraph | n/a (lib) | n/a | toposort/tred/longest-path | PURE | **adopt** (control-plane DAG math) |
| pathfinding | n/a (lib) | n/a | toposort + frontier groups | PURE | alternative to petgraph; adopt exactly one |
| daggy | n/a (lib) | n/a | cycle-rejecting DAG | pure, dormant | reject (petgraph suffices) |
| rayon (in-tree) | n/a | no | no | owns thread pool | keep as execute layer only |
| tokio (in-tree) | n/a | no | no | runtime | borrow shapes (owned permits); no new adoption |
| timely/differential | no (RAM, replay) | no | frontiers | runtime-shaped (spawns workers) | reject; frontier idea already on disk |
| salsa | no (RAM) | derived deps | pull-only | pure computation | reject; digest layer already implements the ideas |
| apalis | partial (sqlx SQLite) | no | no | effectful (tokio+sqlx+tower) | reject; recovery pattern already borrowed |
| effectum | yes (own SQLite) | no (scalar weight) | no | effectful (owns DB, spawns) | reject; borrow weight-as-budget-admission |
| underway | no (Postgres) | no | linear steps only | effectful | reject |
| aide-de-camp | weak (dormant) | no | no | effectful | reject |
| sqlxmq / fang / hatchet | no / no / server | no | no | effectful | reject |
| buck2 DICE | no (RAM+cache) | outputs only | demand-driven | runtime-entangled | reject; borrow Bazel declared-estimates admission |
| k8s / celery / postgres | systems | k8s: yes | celery canvas | n/a | borrow shapes only (filter/score/bind; chord; advisory-lock hierarchy) |

Bottom line: **no existing crate ships read/write-set conflict scheduling; the
two pure-computation adoptable candidates are graph-algorithm libraries
(petgraph or pathfinding), and everything scheduling-specific is a bounded
extension of jobq** (one table, one column, one claim function), built from
borrowed shapes: bevy's derived access sets, Gray's hierarchy test, HEFT's
rank, effectum's weight, Bazel's declared estimates, k8s's filter/score/bind.

---

## 3. The design

Planning protocol: four layers, allowed to disagree.

### 3.1 Type signatures

```rust
// src/jobq/scope.rs (NEW): the resource vocabulary. Minimal by construction:
// only what the three JobKinds actually touch. Hierarchical so the conflict
// test is a prefix walk (Gray intention-lock shape) not a leaf enumeration.
//
// Grammar (stored as canonical strings, one row per atom):
//   root=<root_id>                      the whole corpus engine of one root
//   root=<root_id>/rel=<rel_name>       one relation's rows
//   root=<root_id>/rel=<rel_name>/shard=<k>   one chunk's row range
//   root=<root_id>/dbw                  the corpus db WRITE handle (SQLite
//                                       single-writer is itself a resource)
//   net                                 the network (SinkDrain; budget-shaped,
//                                       participates in width caps not conflicts)
// Files are NOT scope atoms in v1: no current job pair conflicts on files but
// not on rels (Tick reads changed files then writes rels; ColdExtract reads a
// chunk of files then writes rels). rel+shard is the minimal vocabulary that
// separates every live pair. file=<path> can be added later without schema
// change (it is just a longer prefix under root=).

pub(crate) struct ScopeAtom(String);          // canonical, validated on parse

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeMode { Read, Write }     // 'r' | 'w' in storage

pub(crate) struct ScopeSet {
    pub reads: Vec<ScopeAtom>,
    pub writes: Vec<ScopeAtom>,
}

impl ScopeSet {
    // Gray-style hierarchical conflict: two atoms collide iff one is a prefix
    // of the other (equal counts as prefix). A write collides with any
    // overlapping read or write; reads never collide with reads.
    pub fn conflicts_with(&self, other: &ScopeSet) -> Option<(ScopeAtom, ScopeAtom)>;
}

// src/jobq/mod.rs: JobRow gains a declared scope, derived (bevy-style) from the
// constructor, never hand-written at call sites.
impl JobRow {
    // Tick: reads  root=<r>            (whole-engine read: reconcile walks it)
    //       writes root=<r>            (derived cascade may touch any rel)
    //       writes root=<r>/dbw
    // Narrowing Tick's write set to program_scope rel names is migration step 5.
    // SinkDrain: reads root=<r>; writes root=<r>/dbw; reads net (budget atom).
    // ColdExtract(family, shard):
    //       reads  root=<r>/rel=<dep>       for each declared family input
    //              (scip: none; module: none; type/call: rel=module_edge_rev,
    //               rel=scip_ref, ...; dataflow chunk: none)
    //       writes root=<r>/rel=<out>/shard=<k>  for each fam.rels() output
    //              (wholesale family: root=<r>/rel=<out>, no shard suffix)
    //       writes root=<r>/dbw            only during its flush phase; v1
    //              declares it whole-job (honest, coarse), see layer 3.
    pub fn scope(&self) -> ScopeSet;
}

// The read-set half of ColdExtract comes from a new ExtractFamily method,
// making the family declare its inputs the way rels() declares outputs:
pub trait ExtractFamily {
    fn rels(&self) -> &'static [&'static str];          // exists: write set
    fn input_rels(&self) -> &'static [&'static str];    // NEW: read set
    // scip-index node: input_rels of every family that reads the scip ladder
    // name "scip_ref" etc.; the resolver inputs are today implicit in code
    // (call.rs building by_name/scip_name_defs/module_import_map). Writing
    // input_rels makes the implicit explicit; the it-test in section 5 pins
    // that the derived order equals the hand-ranked order it replaces.
}

// Selection: replaces the bare SQL ORDER BY with a two-phase pick.
pub(crate) struct RunningScopes {
    // in-memory mirror of _job_scope rows for state='running' jobs;
    // rebuilt from disk on boot (crash-consistent because
    // reset_running_on_boot empties the running set first).
    held: Vec<(JobKey, ScopeSet)>,
}

impl JobQueue {
    // NEW claim: pick max-utility READY candidate whose scope conflicts with
    // no RUNNING job's scope. O(candidates * running); running <= width (2-3).
    pub(crate) fn claim_admissible(
        &self,
        kinds: &[JobKind],
        running: &RunningScopes,
    ) -> Result<Option<JobRow>>;
}

// Utility: two components, deliberately separated (the postTask lesson:
// importance class and ordering are different signals).
//   utility(t) = (urgency_class(t) << 32) | upward_rank(t)
//   urgency_class: INTERACTIVE (Tick, SinkDrain: demand-driven, a human or a
//     sink cadence is waiting) > BACKGROUND (ColdExtract: fill work).
//     This is a source classification (Blink queue classes, idle-until-
//     urgent), not a producer ranking; it never orders jobs WITHIN a class.
//   upward_rank(t) = cost_estimate(t) + max over consumers c of upward_rank(c)
//     (HEFT). Consumers = jobs whose reads intersect t's writes.
//     cost_estimate: chunk jobs = chunk byte size (bin-packed, known);
//     wholesale families = last observed duration from perf.jsonl, default 1.
// Computed with petgraph: build DiGraph over pending jobs, assert acyclic
// (Err(Cycle) = seed bug, loud), longest-path by toposort walk. The same
// walk's Kahn layering (pathfinding names it topological_sort_into_groups:
// group 0 = no predecessors, group k = all predecessors in groups < k) is
// the ready-frontier staging: group 0 is the initially READY set, and each
// completion can only promote jobs from the next group. `why` renders these
// groups as the cold-start stage display.
pub(crate) fn compute_utilities(jobs: &[JobRow]) -> Vec<(JobKey, i64)>;
```

### 3.2 Pseudo-code

```rust
// claim_admissible (jobq): one BEGIN IMMEDIATE, same shape as today's claim.
// fn claim_admissible(kinds, running) -> Option<JobRow> {
//   candidates = SELECT ... FROM _job
//     WHERE state='pending' AND cancelled=0 AND run_at<=now AND kind IN kinds
//     ORDER BY utility DESC, priority DESC, run_at ASC, enqueued_at ASC
//     LIMIT 16;                       // bounded probe window, not O(pending)
//   let mut barrier: Option<ScopeSet> = None;   // anti-starvation, see below
//   for cand in candidates {
//     scope = load_scope(cand.key);   // _job_scope rows, one query per cand
//     // READINESS (Kahn firing): no pending/running job writes what cand reads.
//     //   SELECT 1 FROM _job_scope w JOIN _job j ON j.key=w.job_key
//     //   WHERE j.state IN ('pending','running') AND j.key != cand.key
//     //     AND w.mode='w' AND prefix_overlap(w.scope, cand.read_atoms)
//     // prefix_overlap in SQL: w.scope = a OR w.scope LIKE a||'/%' OR
//     //                        a LIKE w.scope||'/%'  (per read atom).
//     // 'net' and 'dbw' atoms are EXEMPT from readiness (they are leases,
//     //  not produced data): readiness considers rel/root atoms only.
//     if !ready(cand) { continue; }
//     // ADMISSION (conflict serializability against the running set):
//     if running.iter().any(|(_, held)| scope.conflicts_with(held))
//         || barrier.as_ref().is_some_and(|b| scope.conflicts_with(b)) {
//       // ANTI-STARVATION (no queue jumping): cand was READY but blocked.
//       // Remember the FIRST (= highest-utility) blocked candidate as a
//       // barrier; later candidates that conflict with IT are not admitted
//       // either, so a stream of narrow jobs cannot starve a broad-write
//       // job (Tick, X on root) indefinitely. Chromium ships the same idea
//       // as an explicit anti-starvation policy over fixed priorities.
//       if barrier.is_none() { barrier = Some(scope); }
//       continue;
//     }
//     UPDATE _job SET state='running', started_at=now WHERE key=cand.key;
//     running.push((cand.key, scope));
//     return Some(cand);
//   }
//   None
// }
//
// Seeding (cold_stage): scopes computed from what the tick already knows.
// fn seed_cold_nodes(prog) -> Vec<ColdJob> {
//   // as today: scip node + per-family nodes + per-chunk nodes, one batched
//   // insert (N+1 law). NEW: for each job, emit scope rows derived from
//   // fam.rels() (writes) and fam.input_rels() (reads); chunk k gets
//   // /shard=k write suffixes. NO hand priorities: enqueue with priority=0,
//   // then utilities = compute_utilities(all seeded jobs) in one pass,
//   // written back in the same batched insert.
//   // The hand rank count-idx DELETES; the it-test pins order equivalence.
// }
//
// Coalescing x scopes: the enqueue UPSERT merges args (path union). Scope
// rows for a key are REPLACED on every enqueue (DELETE+INSERT in the same
// tx): a Tick re-request carries the same root-level scope; a future
// narrowed Tick recomputes its rel scope from the new merged paths. A dirty
// re-run reuses the row's current scope: scope is a function of (kind, arg),
// and arg union is already the coalesce contract.
//
// Worker loop (daemon_shell/jobs.rs): unchanged shape. claim ->
// spawn_blocking(run) -> finish. finish() removes the key from
// RunningScopes and rings job_notify (a completion may make successors
// READY: the frontier advanced).
//
// dl daemon why: the sample line gains "holds": the running jobs' write
// atoms, read straight from _job_scope (on-disk, so post-mortem too);
// "waiting": for each pending job, the first blocking (writer_key, atom)
// pair from the readiness probe. Both are cheap: running <= width,
// pending is small.
```

### 3.3 Instance lifetimes

- **Scope rows (`_job_scope`)**: durable, born in the enqueue transaction, live
  as long as their `_job` row (sweep deletes them with done-row GC, same tx).
  They are the attribution record: after kill -9, `started_at` + scope rows
  state what the dead process was holding, satisfying the self-diagnosis law
  with zero new trail machinery.
- **`RunningScopes`**: in-memory, one per daemon process, rebuilt empty at boot
  (correct: `reset_running_on_boot` re-pends every running row first). Updated
  at claim/finish under the queue mutex. The conflict test always runs against
  this mirror; the durable rows exist for crash forensics and `why`, never for
  hot-path reads.
- **Utility column**: durable on `_job`, computed at seed (cold graph) or
  enqueue (default 0 for Tick/SinkDrain: they have no pending consumers).
  Stale-ness is bounded: utilities are recomputed only when the pending set's
  DAG changes shape (a seed, or a program edit changing input_rels), not per
  claim.
- **The petgraph `DiGraph`**: transient, built inside `compute_utilities` and
  `seed`-time cycle check, dropped after. Never held across ticks: the durable
  form of the DAG is the scope rows it was derived from.
- **Crash semantics**: unchanged from J1. A running job dies -> boot reset
  re-pends it; its scope rows persist untouched (they describe the job, not the
  execution); `_cold_node` remains the completion frontier. No new crash state
  was introduced because the only new durable data (scopes, utility) is a pure
  function of (kind, arg) and re-derivable.

### 3.4 Storage layout, read/write sequence, uniqueness

```sql
-- jobs.sqlite (control state, next to _job)
CREATE TABLE IF NOT EXISTS _job_scope(
  job_key TEXT NOT NULL,
  mode    TEXT NOT NULL,             -- 'r' | 'w'
  scope   TEXT NOT NULL,             -- canonical atom string
  PRIMARY KEY (job_key, mode, scope)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS _job_scope_by_scope ON _job_scope(scope, mode);

ALTER TABLE _job ADD COLUMN utility INTEGER NOT NULL DEFAULT 0;
-- _job_ready index gains utility:
CREATE INDEX IF NOT EXISTS _job_ready2 ON _job(state, run_at, utility, priority);
```

Read/write sequence (one claim):

1. `BEGIN IMMEDIATE` (single-writer SQLite is the coordinator, as today).
2. Read <= 16 candidates by `(utility DESC, priority DESC, run_at, enqueued_at)`
   via `_job_ready2`.
3. Per candidate: read its scope rows (PK-prefix scan), run the readiness probe
   (one query over `_job_scope` joined to live `_job` rows), then the conflict
   test against `RunningScopes` in Rust.
4. First admissible candidate: `UPDATE _job SET state='running'`. Commit.

Why the probe is O(running), not O(pending): the CONFLICT half tests only
against `RunningScopes` (<= width entries, in memory, zero SQL). The READINESS
half is a SQL semi-join, but its cost is bounded by the candidate window (16)
times the scope index lookup, and short-circuits on first blocker; it never
scans the pending set as a whole. The pathological case (every candidate
blocked) costs 16 indexed probes and returns None, at which point the worker
parks on the doorbell; a finish() advances the frontier and rings it.

Uniqueness:

- `_job_scope` PK `(job_key, mode, scope)`: re-declaring an atom is a no-op;
  scope replacement is DELETE-then-INSERT inside the enqueue tx, so a key's
  scope set is always exactly the last enqueue's derivation.
- Scope atoms are canonical strings (single builder function, no free-form call
  sites), so prefix logic never sees two spellings of one resource.
- One `utility` per row, recomputed only by the seed/enqueue writer; claim never
  writes it. `priority` remains as tiebreak input only; no new writer of it.

Throughput/space guardrails:

- At most width jobs run, so at most width jobs' row sets are in memory
  (structural, unchanged). Chunk sizing stays the 512KiB bin-packer; the
  scheduler never merges chunks.
- The dbw atom keeps write-phase overlap honest: two admitted jobs on one root
  serialize their flush on the SQLite write lock anyway; declaring dbw makes
  that visible instead of implicit. v1 declares dbw for the whole job (coarse:
  it forbids intra-root co-running). Splitting jobs into a compute phase
  (no dbw) and flush phase (dbw) is the step that unlocks intra-root overlap of
  chunk COMPUTATION with another job's flush, and it is deferred to step 6:
  collect-then-flush already structures the code that way, the scheduler just
  cannot see it yet.
- IO/CPU tiers: unchanged, `apply_daemon_budget` caps stand. The scheduler
  admits; the budget throttles. Nothing in this design can exceed the caps
  because execution still happens on the same budget-capped workers.

### 3.5 What the utility function derives (worked example)

Cold seed on this repo (module, type, call, dataflow x 7 chunks, doc, scip):

- scip writes rel=scip_ref; type+call read it. call writes rel=call_edge...;
  the completion Tick reads everything. module writes rel=module_edge_rev; type,
  call, doc read it.
- Readiness alone forces: scip and module before type/call/doc; chunks anytime;
  completion Tick last. The hand-ranked `count - idx` DELETES.
- Upward rank orders the ready set: scip and module first (they unblock the
  most downstream cost), then call (591ms, longest single node), then type,
  then chunks by size. This reproduces today's order where it was right and
  improves it where it was arbitrary (chunks after doc had no reason; under
  utility, a 500KiB chunk outranks 0.3ms doc).

---

## 4. Migration path (each step shippable)

1. **Scope rows, write-only.** `_job_scope` + `JobRow::scope()` +
   `ExtractFamily::input_rels()`. No claim change. `dl daemon why` gains
   "holds"/"waiting" from the rows. Receipt: why names the scope of a running
   cold job; a kill -9 post-mortem shows what the dead process held.
2. **Readiness from scopes.** Claim skips candidates whose read atoms have a
   live writer. Hand ranks stay but become redundant for ordering. Receipt: the
   it-test asserting derived cold order == hand-ranked order (order-equivalence
   pin), then the scip/module-first behavior with priorities zeroed.
3. **Utility replaces rank.** `compute_utilities` (petgraph) at seed; cold
   seeding stops writing hand priorities; `priority` demotes to tiebreak.
   Receipt: same it-test still green with rank code deleted.
4. **Conflict-gated width.** ColdExtract workers 1 -> 2; admission by
   `conflicts_with` against RunningScopes. Receipt: two dataflow chunks
   (disjoint shard atoms) observed overlapping under width 2; call (reads
   scip_ref) never overlaps scip; wall-clock cold start drops measurably below
   the single-flight baseline on the 3.37MB corpus.
5. **Narrow Tick's write set.** program_scope (tick.rs:810) already computes
   the affected rel names; declare them instead of root=<r> when available.
   Unlocks tick-vs-chunk co-running on disjoint rels.
6. **Phase-split dbw.** Jobs declare dbw only for their flush phase
   (collect-then-flush seam); unlocks intra-root compute/flush overlap.

Steps 1-3 change zero runtime behavior until 2; every step keeps the old path
one revert away; the schema is additive throughout (no _job rewrite).

---

## 5. Receipts: what proves "physically correct"

- **R-order**: it-test builds the cold job set, zeroes priorities, and asserts
  claim order satisfies every declared read-before-write edge (scip before
  type/call, module before type/call/doc) with NO rank input. The order is
  derived, therefore a new family with declared input_rels slots itself.
- **R-overlap**: under width 2, two chunk jobs with disjoint shard atoms are
  observed running concurrently (started_at windows intersect in _job).
- **R-exclusion**: a chunk job and a wholesale rebuild of the same rel never
  overlap (IX-vs-X prefix conflict); asserted by an it-test that seeds both.
- **R-why**: `dl daemon why` names, from disk alone, the atoms every running
  job holds and the (writer, atom) blocking each waiting job, including after
  kill -9 (the self-diagnosis law, extended to scopes).
- **R-budget**: cold start under the new scheduler stays within
  `daemon_thread_count` width and the existing QoS/IOPOL tiers (no new
  spawns); wall-clock improves only through admitted overlap, never through
  cap violations.
- **R-determinism**: relation contents byte-identical between single-flight and
  conflict-admitted runs (the Kahn determinacy claim, pinned by the existing
  determinism it-test extended to a width-2 cold start).

---

- **R-starve**: with a Tick pending (write root=X) and a stream of re-enqueued
  chunk jobs, the Tick starts within one running-job completion of becoming
  ready (the barrier rule); asserted by an it-test that floods narrow jobs.

---

## 6. Open user decisions

1. **Adopt petgraph now?** It enters Cargo.lock for cycle check, toposort,
   longest path, transitive reduction on the control-plane job DAG; pure
   computation, no IO/spawns. Alternative: a ~40-line in-degree walk in-tree.
   Recommendation: adopt; the tred/closure algorithms for `why` output and the
   cycle-rejection ergonomics are worth one pure dep, and the user has okayed a
   graph lib. The policy line (SQL fixpoint canonical for reactive rels) goes
   into CLAUDE.md with the arc.
2. **dbw scoping in v1: whole-job or phase-split?** Whole-job is honest but
   forbids intra-root co-running (only cross-root and compute-vs-compute
   overlap... which whole-job dbw also blocks). Phase-split (dbw only during
   flush) unlocks intra-root overlap but needs the collect/flush seam surfaced
   to the queue. Recommendation: whole-job in v1 (migration step 4 still wins
   across roots and proves the machinery); phase-split as step 6.
3. **Files as scope atoms in v1?** rel+shard separates every live job pair
   today; file atoms add vocabulary with no current discriminating pair.
   Recommendation: no; the grammar reserves the extension.
4. **ColdExtract width after step 4: 2, or daemon_thread_count?**
   Recommendation: 2 (the budget floor); chunks are rayon-parallel inside, so
   more admitted jobs oversubscribe the same width-2 pool.
5. **Urgency classes: is INTERACTIVE > BACKGROUND the full set?** SinkDrain is
   cadence-driven, arguably a third MAINTENANCE class between them.
   Recommendation: two classes until a receipt shows sink latency mattering;
   classes are data (a column), adding one is not a schema change.
6. **Cost estimates from perf.jsonl: wire now or constant-1 first?** Upward
   rank degrades gracefully with cost=1 (pure consumer-count ordering).
   Recommendation: constant-1 in step 3; perf-fed costs as a follow-up with a
   receipt comparing schedules.
   USER AMENDMENT (2026-07-18, voice): overridden in two parts. (a) Family
   weight is unknowable a priori and the one measurement taken (cold-chunk arc)
   inverted the assumed ranking (dataflow 4.4s was the hog, parse 4%), so
   constant-1 over family-sized lumps re-encodes exactly that blindness.
   Perf-fed observed cost per (family, shard-bytes) rides from the start; the
   trail already records it (extract-rebuild verdicts: family, files, ms).
   (b) The schedulable unit is the SHARD (byte-bounded chunk) for EVERY
   family, not only dataflow; family survives as a locality grouping and the
   code that runs, never as the admission-sized lump. Corollary: the demand
   join (program -> rel -> family -> shard) must exist as rows so the
   scheduler can order shards by which served programs are blocked on them;
   both inputs already exist in code (served-program body atoms walked at
   strata.rs auto_indexes; supply side named by ExtractFamily::input_rels()).
7. **Keep the `priority` column?** After step 3 it is tiebreak-only.
   Recommendation: keep through step 4, then fold into utility's low bits and
   drop the column in a schema-cleanup arc once receipts hold.
