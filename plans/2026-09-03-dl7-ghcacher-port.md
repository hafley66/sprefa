# DL7 ghcacher port

Date: 2026-09-03

Status: initial dependent plan

## Context

The semantic source is `v6/dl/ghcache/ghcache.dl6`. The standalone Rust
application remains implementation evidence for process ownership, SQLite
schema, command serving, and worktree watching:

- `src/main.rs:189-257` starts the command server and worktree watcher before
  entering the GitHub watch loop.
- `src/gh.rs:37-42`, `:143-172`, and `:174-330` combine HTTP transport,
  throttling, SQLite writes, and polling state.
- `src/schema.sql:1-182` declares natural uniqueness for repositories,
  branches, pull requests, children, events, notifications, polling,
  checkouts, and worktrees.
- `v6/dl/ghcache/ghcache.dl6:23-1455` expresses clocked configuration, rate
  budget, cadence, ETag reuse, pagination, GitHub entities, GraphQL batching,
  notifications, checkout, change logs, views, and runtime measurement as
  relations.

The DL6 port already moved ETag state, pagination, GraphQL construction,
throttling, 304 substitution, and polling state into rules over generic host
operations. `v6/dl/ghcache/README.md:23-92` records the contract and live
receipts. Commit `7002c2028` is the transition to rules over one HTTP
transport.

The DL7 dependencies are now concrete:

| Dependency | Current receipt |
| --- | --- |
| complete compiler view for a DL7 emitter | `3b5e019fe` |
| DL7-authored relation/operator/read/projection rows | `1dc9b0696` |
| comptime clock dependency query for level rules | `17a379d05` |
| hosted zero-input source and zero-output sink | `7f3d446df` |
| native Rust/SQLite execution and reload | `plans/2026-09-03-dl7-native-rust-sqlite-runtime.md` |

The port begins after the Rust/SQLite runtime implements the temporal and host
operator rows used by the first delivery slice.

## Decisions

1. Ghcacher is an ordinary DL7 program. GitHub-specific behavior is expressed
   as rules, products, sums, keys, history, and `:/4` edges.
2. Host implementations remain generic. The first port uses clock, HTTP GET,
   HTTP POST, environment, TOML/JSON, checkout, and PR-head mirroring.
3. The complete ordered host input tuple is the initial demand witness. A
   clock bucket changes HTTP identity; a desired SHA changes checkout identity.
4. JSON response bodies use an `any` value whose type node carries the `json`
   capability. Typed projections are ordinary rules over that value.
5. Durable state uses one history/integration constructor with zero or more key
   edge identities and an optional retained-row count.
6. HTTP and Git effects execute after the SQLite tick commits. Responses become
   signed arrivals in a later generation.
7. Reload reaches a completed tick and host-response frontier before migration
   and library swap. A failed compile, load, ABI check, or migration retains the
   active generation and database state.
8. Each new DBSP operator and Rust/SQLite lowering used by ghcacher enters the
   runtime shootout before the port depends on it.

## DL7 relation shapes

The host declarations below use existing `Host`, `Hosted`, and `HostPort`
semantics. Field and port annotations remain edges on their reified identities.

```dl7
(: Repo
   (* (: owner text)
      (: name text)))

(: ClockTick
   (Host Clock
      (* (: every int))
      (* (: bucket int))))

(: TomlJson
   (Host Toml
      (* (: path text)
         (: bucket int))
      (* (: doc any))))

(: EnvVar
   (Host Env
      (* (: name text)
         (: bucket int))
      (* (: value text))))

(: HttpGet
   (Host HttpGetImpl
      (* (: url text)
         (: headers any)
         (: prev_etag text)
         (: bucket int))
      (* (: status int)
         (: response_headers any)
         (: body any)
         (: bytes int))))

(: HttpPost
   (Host HttpPostImpl
      (* (: url text)
         (: headers any)
         (: request_body any)
         (: bucket int))
      (* (: status int)
         (: response_headers any)
         (: body any)
         (: bytes int))))

(: Checkout
   (Host SoopyCheckout
      (* (: repo_slug text)
         (: dest_root text)
         (: want_sha text))
      (* (: checkout_path text)
         (: head_sha text))))

(: MirrorPrHeads
   (Host SoopyMirrorPrHeads
      (* (: repo_slug text)
         (: dest_root text)
         (: want_sha text))
      (* (: checkout_path text)
         (: pr_head_count int))))
```

The lowerer produces ordinary graph and host facts:

```text
:(GhcacheFile, HttpGet, HttpGetRelation, index)
:(HttpGetRelation, url, text, 0)
Hosted(HttpGetRelation, HttpGetImpl)
HostPort(HttpGetRelation, url, Input)
HostPort(HttpGetRelation, body, Output)
```

No GitHub-specific executor is required. Reusable implementations exist in
`v6/sprefa-engine-rs/src/hosts.rs:51-124` for `/clock/tick`, `/http/get`,
`/http/post`, `/env/var`, `/toml/json`, `/soopy/checkout`,
`/soopy/mirror_pr_heads`, and `/dl/tick_cost`. The DL7 resident host must expose
these implementations through its versioned host table.

## Type signatures

```rust
pub struct GhcacheGenerationInput {
    pub generation: GenerationId,
    pub arrivals: SignedBatch,
}

pub struct GhcacheGenerationOutput {
    pub differences: SignedBatch,
    pub demands: SignedBatch,
    pub carry_pending: bool,
}

pub fn run_ghcache_generation(
    module: &Dl7ModuleV1,
    store: &mut SqliteStore,
    input: GhcacheGenerationInput,
) -> Result<GhcacheGenerationOutput, RunError>;

pub async fn execute_ghcache_demands(
    hosts: &HostRegistry,
    claims: &mut ClaimSet,
    demands: SignedBatch,
) -> Vec<SignedArrival>;
```

The DL7 lowering consumes and produces ordinary compiler rows:

```prolog
lower_ghcache_state(
    +ProgramRows,
    +TypeGraphRows,
    -HistoryRows,
    -StorageRows,
    -Diagnostics
).

lower_ghcache_hosts(
    +HostedRows,
    +HostPortRows,
    -DemandRows,
    -ResponseRows,
    -Diagnostics
).
```

## Instance timelines and lifetimes

### Process

The resident process owns the filesystem watcher, compiler process, active and
candidate library handles, arrival queue, host registry, host claims, command
server, and SQLite connection or connection-owning actor.

### Generation

```text
begin SQLite transaction
  -> absorb signed clock, config, HTTP, Git, and command arrivals
  -> close level rules
  -> process temporal edge occurrences
  -> apply keyed replacement, history append, and retention
  -> calculate signed relation differences and host demands
commit
publish differences
execute positive host demands
enqueue host responses for later generations
```

`v6/sprefa-engine-rs/src/driver.rs:197-215` supplies the one-transaction tick
precedent. `run_schedule_live` commits through `drive_tick_transacted` before
collecting host responses at `driver.rs:159-179`.

### Host claim

A claim lives from the first positive demand difference until its response has
entered a committed generation or the demand retracts while pending. Claims are
keyed by hosted relation identity plus the canonical complete input tuple. One
answer row adds its output ordinal to that identity.

### Reload

```text
source difference
  -> compile the closed DL7 project
  -> lower checked rows to DBSP and Rust/SQLite rows
  -> emit changed per-file Rust modules and project root
  -> build a content-addressed candidate dylib
  -> validate ABI, program digest, schema digest, and host contract
  -> finish the active tick
  -> fold responses launched by the active generation
  -> reach empty response and carry queues
  -> migrate keep/refill/recreate/authorized-drop state in one transaction
  -> install the candidate generation
  -> retire the old library after active calls reach zero
```

## Storage, reads, writes, and uniqueness

The first durable inventory preserves the DL6 keys documented at
`v6/dl/ghcache/README.md:317-336` and the state declarations in
`ghcache.dl6`.

| Plane | Relations and uniqueness |
| --- | --- |
| configuration | `chosen_config(config_path)`, `global_setting(first field)`, `org_config(owner)`, `repo_config(owner,name)`, `api_token(first field)` |
| rate and endpoint | `rate_state(api_type)`, `watched_repo(repo_ref)`, `miss_streak(endpoint_path)` |
| polling | ETag, modified, period, polled, changed, and last body by page URL; queued and ordinal by page URL plus bucket |
| GitHub entities | event `(repo_ref,gh_id)`, branch `(repo_ref,name)`, PR `(repo_ref,number)`, children `(repo_ref,number,child identity)`, notification `gh_id` |
| batching | repository ordinal by `repo_ref`; PR-ever-synced by `repo_ref` |
| checkout | repository checkout by `(repo_ref,branch)` |
| logs | call 16,500 rows; change 34,000 rows; tick cost 140,000 rows; PR transition unbounded |

`HistoryV1(SourceRelation, StateOptions, StoredRelation)` receives runtime
meaning from the Rust/SQLite lowering:

- key edge identities absent means append history;
- one or more key edges means replacement by key;
- retained row count absent means unbounded history;
- `pre` reads the evolving stored state during ordered edge processing;
- committed prior state supports transition detection and restart.

Stable identities:

| Item | Identity |
| --- | --- |
| hosted relation | semantic relation identity |
| demand | hosted relation plus complete canonical input tuple |
| response | demand plus output ordinal |
| stored relation | semantic relation digest |
| stored row | declared key tuple or occurrence stamp |
| HTTP page poll | page URL plus bucket |
| checkout | repository, destination root, desired SHA |
| generated module | source module plus relevant lowering digest |
| generation library | ordered generated module digests plus ABI version |

## Required runtime capabilities

The initial port depends on these DBSP and Rust/SQLite rows:

- positive map, join, projection, and retraction;
- antijoin;
- grouped min, max, count, string aggregation, and JSON aggregation;
- recursive fixed point for paging and compiler programs;
- keyed replacement and append history;
- evolving pre-state and prior committed snapshot;
- signed positive and departure triggers;
- row-count retention;
- hosted demand and response boundaries;
- JSON object projection and array fan-out;
- scalar arithmetic, comparison, strings, and bounded sequence generation.

Composite values such as `repo_ref: Repo` need one stable representation across
generated Rust, SQLite, and the library ABI. The first endpoint slice may carry
owner and name as separate columns while that representation is being proved.

## Delivery sequence

1. Port clock, configuration, token loading, one REST endpoint, and `HttpGet`.
2. Add ETag persistence, 304 body reuse, restart, and one wire call per page and
   bucket.
3. Add rate warn/stop policy, cadence, pagination, and 404 cooling.
4. Add repository discovery, events, and branches.
5. Add GraphQL batching, pull requests, reviews, comments, checks, labels, and
   reviewers.
6. Add notifications.
7. Add checkout and pull-request-head mirroring.
8. Add views, transition/change logs, retention, and tick measurements.
9. Exercise dylib reload with an in-flight HTTP request and unchanged SQLite
   state.

The first slice remains one `0_ghcache.dl7` until a receipt proves that a rule in
one source module directly calls a sibling module's declared relation. After that
receipt, dependency and reading order is:

```text
0_types.dl7
1_hosts.dl7
2_config.dl7
3_clock.dl7
4_endpoints.dl7
5_http.dl7
6_events.dl7
7_branches.dl7
8_pulls.dl7
9_notifications.dl7
10_checkout.dl7
11_views.dl7
12_observability.dl7
```

<!-- todo(feature): Port clock, config, token, one REST endpoint, and HttpGet after temporal host execution is available in generated Rust/SQLite. -->

<!-- todo(feature): Preserve ETag and body state across restart, including one-call-per-page-per-bucket and 304 substitution receipts. -->

<!-- todo(feature): Port rate policy, paging, entities, GraphQL batches, notifications, checkout, views, and bounded logs in delivery order. -->

<!-- todo(feature): Prove generation reload with an in-flight HTTP request, quiescent claim frontier, and unchanged compatible SQLite state. -->

<!-- todo(perf): Add every ghcacher-dependent DBSP and Rust/SQLite capability to the relational shootout before the program slice consumes it. -->

## Verification

Reuse the six fixtures under `v6/dl/ghcacher` through its existing gate. Compare
DL6 and DL7 signed differences and final snapshots by semantic relation identity.

Required receipts:

1. The second conditional poll receives 304 and transfers zero body bytes.
2. Process restart reuses stored ETags and response bodies.
3. One page causes one wire call per bucket.
4. `over_budget` produces zero due rows; warning state only increases cadence.
5. N events for one repository produce one checkout demand.
6. Open-to-merged produces one pull-request transition.
7. A code-only reload preserves compatible tables and launches no duplicate
   in-flight effect.
8. Failed compile, load, or migration leaves the previous generation active.
9. The result ledger records compiler time, Rust build time, dylib load and swap
   time, tick latency, SQL time, HTTP calls, rows, bytes, peak RSS, and SQLite
   size.

CI coverage added by this plan consists of deterministic fixture parity,
generated Rust/SQLite integration tests, restart and migration tests, and
shootout gates. Live GitHub and filesystem tests remain separately gated.
