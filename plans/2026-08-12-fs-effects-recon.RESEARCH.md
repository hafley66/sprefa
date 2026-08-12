# fs-effects-recon: what v3/v4/v5 built, priced as a library

Base verified: `0d218284` (`git log --oneline -1`).

## TOC

1. Findings (corrections to the brief's v6 table)
2. The effect-as-yield answer (deliverable 2)
3. Archaeology: v3, v4, v5 (deliverable 1)
4. Comparison table: v3 vs v4 vs v5 vs v6
5. v6 today: row-by-row verification
6. Build-vs-buy: library research (deliverable 3)
7. Shape recommendation: forks and prices (deliverable 4)
8. Sources and open items

## 1. Findings (these sit at the top per the brief)

| # | finding | evidence |
|---|---|---|
| F1 | The v6 table row "dry-run ABSENT everywhere" is accurate for a **keyword/mode**, but NOT for the concept. `tsv2/labs/staged-writes/1-stage.dl6:1-4` implements dry-run as data: "Read-only; nothing here writes. `edit_add` and `edit_del` ARE the dry run. No mode flag, no --fix, no second code path: the diff is a relation, and applying it is a separate demand." That is the v3 mindset (decision recorded as a row), expressed in the v6 data plane. | staged-writes/1-stage.dl6:1-4, :35-44 |
| F2 | The v6 table row "collect ABSENT in v6" is accurate for a builtin. `collect` appears only as lab research (`tsv2/labs/effect-chain/3_v5_collect.sh`) and an unrelated tree-traversal helper (`tsv2/runtime/structPlane.ts:121`). | as cited |
| F3 | The v6 table row "file read from `.dl6` ABSENT" is accurate for a **direct** read bind, but reads DO reach files as subprocesses: `sprefa-extract` (`v6/sprefa-extract`, `--family`) and `sh` host decls read files. The only runtime-internal read is the watch digest (`tsv2/serve/2_binds.ts:232 digest_of`). Reads are subprocess-scoped, not a builtin relation. | 2_binds.ts:232; sprefa-extract |
| F4 | v6 watch is a **single glob-driven** `watch` bind. There is no file-vs-folder distinction in the surface today; the glob selects. Watch-file and watch-folder must be a designed fork (section 7). | registry.pl:295; types.ts:817-831 |
| F5 | v6 ALREADY batches the `sprefa_extract` host family: "Compatible `sprefa_extract` witnesses in one frontier share one executor invocation" (`types.ts:773`) via `ApplicativeExecutors` fold (`1_hosts.ts:268-274, 477`). This is the Haxl collapse the sagas skills describe, already live for the read/extract side. | types.ts:773, 1_hosts.ts:268 |
| F6 | The write side has NO batching and the lab measured the cost: `tsv2/labs/staged-writes/2-apply.dl6:31-38` "THE HONEST LIMIT... a host's input is ONE ROW, and a write's payload is a RELATION... The only shape that compiles is ONE INVOCATION PER LINE... N spawns, serialized by the runner's `concatMap`". | staged-writes/2-apply.dl6:31-38 |
| F7 | v3 shipped a real bidirectional coroutine primitive (`Yield`/`SubjectRegistry`/`next`) that v4/v5/v6 did not carry. The brief's premise ("the batcher already existed, approval policy existed, dry-run was a decision recorded per row") is confirmed and is the strongest part of v3 to reuse. | `effect_runtime/src/subjects.rs` (full) |

## 2. The effect-as-yield answer (deliverable 2)

### What redux-saga is (from `~/projects/claude-research/skills_archive/commands/sagas/redux-saga-essence.md:9`)
Ops are pure generator functions that yield plain Effect descriptions; a central interpreter reads each yielded effect, performs it, resumes the generator with `gen.next(value)`. Invariant 1: op = pure description producer, no IO. Invariant 2: interpreter owns IO, one chokepoint. Invariant 3: resumption is bidirectional. Invariant 5: batching is interpreter-local (Haxl makes it first-class). Invariant 6: determinism for test = assert the yielded Effects.

### Did v3 implement it, or merely resemble it? It did BOTH, in two layers.

**Layer A: the EffectKind/Batcher split is the interpreter shape (resembles redux-saga).**
- `EffectKind` = request struct + `type Response` (effect_runtime/src/lib.rs:32). `Batcher<E>` = does the IO (lib.rs:64). `RtCtx::put(e).await` dispatches (lib.rs:202). The v3 doc names the lineage explicitly: "the convergent surface four ecosystems found": Haxl `fetchBatch`, redux-saga `yield put(action)`, tower `Service`, sprefa `EffectKind`. (docs/convergent-evolution-effect-dispatcher.md, opening table + section).
- So the **description** (`WriteRangeEffect`) and the **interpreter** (`WriteRangeSink`) are separate layers, exactly as the saga wants.

**Layer B: the yield/resume primitive is the coroutine shape (real saga).**
- `Yield<S>` is an EffectKind whose Response is `Result<Arc<S::Payload>, Unsubscribed>` (subjects.rs:106-113). `SubjectRegistry<S>` is a keyed store of pending yields; outside code calls `registry.next(key, value)` to resume (subjects.rs:163), `unsubscribe` to cancel (:175). `subscribe` pre-inserts the entry before returning to close the race (subjects.rs:249). This is `gen.next(value)` in Rust. It is bidirectional.
- The production wire: `build_seed_ctx` registers `Yield<WriteApproval>` and a `SubjectRegistry<WriteApproval>`, and wires both write sinks to `stage_and_approve` (server/src/run.rs:499, :529-534, :558).

**But the write op itself is NOT a pure description producer.**
- `WriteCursorOp::pipe` (pipeline/src/ops/write_cursor.rs:48-91) computes `new_bytes = c.active()` itself (line 53), then `ctx.put(WriteRangeEffect{...})` and (staging path) awaits a `registry.subscribe(key)` resume (:66-78). It does not yield an inert description and stop; it constructs the effect AND awaits its result inline in one `pipe`. The op is a contributor to the interpreter loop, not a pure generator that a separate step fun drives.

**The decisive line (the brief's hint, confirmed):** the dry-run decision is recorded by the SINK, the interpreter, not a skipped code path. `WriteRangeSink::StageAndApprove::write` (effects.rs:814-845) calls `policy.decide(&id)` → `decision`, then matches: `DryRun => Err(Arc::<str>::from("dry-run"))` (effects.rs:820), records the `StagedWriteRangeRow { id, effect, decision, result }` (:835-840), and only splices on `Approved`. The op sees the same `Result` shape either way.

### What that difference buys and costs

| concern | yield-inert-description (pure saga) | v3 as-built (op + sink decision) |
|---|---|---|
| dry-run | dry-run is a property of the interpreter; the generator is unchanged | dry-run is a per-row `WriteDecision` recorded in the sink buffer; surfacable to a cockpit/LSP | buy: decision is data, reviewable across runs (id is content-stable, effects.rs:525-531) |
| batching | interpreter can coalesce same-kind descriptions (Haxl) | v3 batches READS (`ReadBytesBatchEffect`, effects.rs:302) but writes are one put each | cost: write fan-in had no collapse in v3 |
| testing | assert the yielded Effects, no mocks (essence.md:18) | tests assert the recorded staged rows (write_cursor.rs:316-324, :360-394) | both testable; v3 tests the interpreter output, not the op purity |

### Answer to the deliverable's question
Does `WriteCursorOp` yield an inert description, or perform IO and record a decision afterward? It does the **second**: the op performs its rendering inline and awaits the batcher, and the Interpreter (`WriteRangeSink`) records the decision. The saga shape exists as a framework primitive (`Yield`/`SubjectRegistry`) and as the EffectKind/Batcher split, but the write op itself is not a suspended pure generator. v3 "implemented the interpreter half of redux-saga and shipped the yield primitive it needs, but stopped short of making ops pure description producers."

### The canonical shape for the recommendation

```mermaid
sequenceDiagram
    participant Op
    participant RtCtx
    participant BM as Batcher (interpreter)
    participant Sink as StageAndApprove sink
    participant Reg as SubjectRegistry&lt;WriteApproval&gt;
    Op->>Reg: subscribe(key)  [pre-insert: race closed]
    Op->>RtCtx: yield put(WriteRangeEffect{key})
    RtCtx->>BM: submit (erase, downcast)
    BM->>Sink: run
    Note over Sink: policy.decide(id) -> decision
    alt Approved
        Sink->>Sink: splice to disk + invalidate_fs
    else DryRun
        Sink->>Sink: decision=DryRun; result=Err("dry-run")
    else Rejected
        Sink->>Sink: decision=Rejected; result=Err("rejected by policy")
    end
    Sink->>Sink: record StagedWriteRangeRow{id,decision,result}
    Sink->>Reg: next(key, result)
    Reg-->>Op: resume (approval.await)
```

## 3. Archaeology: v3, v4, v5 (deliverable 1)

### 3.1 v3 (`~/projects/sprefa-archive-20260701/v3`) - the effect dispatcher, most complete

**Effect kinds (all in `crates/pipeline/src/effects.rs`):**

| kind | line | shape |
|---|---|---|
| `FsListFilesEffect` / `FsListFilesBatcher` | :116 / :152 | list files under `(repo, rev)`; PureEffect, cached in domain `"fs"` |
| `ReadBytesEffect` / `ReadBytesBatcher` | :189 / :263 | read `(repo, rev, path)`; PureEffect, cached |
| `ReadBytesBatchEffect` / `read_bytes_batch` | :302 / :370 / :381 | bulk read; rayon `par_iter` over the slice; collapses N puts |
| `PrintEffect` / `PrintSink` | :413 / :432 | one line to stdout/buffer |
| `WriteFileEffect` / `WriteFileSink` | :491 / :536 | write `(path, bytes)`; sinks Disk/Buffer/StageAndApprove |
| `WriteRangeEffect` / `WriteRangeSink` | :708 / :757 | splice `(file, byte_range, new_bytes, mode)`; modes Replace/Append/Prepend/Wrap (:678) |
| `LspDiagEffect` / `LspDiagBatcher` | :969 / :1006 | LSP diagnostic per cursor |
| `AstParseEffect` | :1048 | offload tree-sitter parse to a worker pool |
| `ShEffect` / `ShBatcher` | :1147 / :1175 | `bash -c`; ShPolicy Auto/Cache/Approve/DryRun (:1106) |
| `Yield<S>` / `YieldBatcher<S>` | effect_runtime/src/subjects.rs:106 / :283 | bidirectional resume primitive |
| `WriteEffect`/`WriteBatcher` | registered in server run.rs:559 | write rows into the RelationStore |

A second effect model coexisted: the spine `MutationEffect` (crates/spine/src/mutations.rs:181) with an **effect cache** `EffectStatus Skip|Emit|Stale` (:56) and persisted `EffectOutcome` (:69), approved via a `MutationHandler` (:210). This is the codemod/approval-with-history surface.

**How a write was expressed:** an effect value (`WriteRangeEffect`/`WriteFileEffect`) yielded via `ctx.put(e).await`. Op = `write_cursor`/`write_file`. The op renders bytes itself, then puts the effect; staging path pre-subscribes a `SubjectKey` and awaits the resume (write_cursor.rs:66-78).

**Who decided / where the decision was recorded:** the batcher's sink. `WriteRangeSink::StageAndApprove::write` (effects.rs:814-845) consults `policy.decide(&id)` and records the row in the staged buffer. Policy arrives per-run: `transport_http.rs:93-103` maps an HTTP request's `dry_run`/`approve_only` fields to `WritePolicy::DryRun`/`ApproveOnly`/`ApproveAll`; `build_seed_ctx` threads it into the sinks (run.rs:529-534).

**What dry-run meant:** a recorded decision, not a skipped path. `DryRun => Err("dry-run")`, row recorded with `decision=DryRun`, file untouched; assertion at write_cursor.rs:360-368. `ApproveOnly` rejects the rest (`"rejected by policy"`, effects.rs:821). The `sh` side had its own `ShPolicy::DryRun` (effects.rs:1241-1253): never runs, returns empty stdout, records a `StagedShRow`.

**What batching existed:** reads only. `ReadBytesBatchEffect` collapses N `ensure_content_loaded` puts into one rayon call (effects.rs:302-402); ops call `ensure_content_loaded_batch` (effects.rs:324). Writes were one put each.

**What the codemod/refactoring system was:** the pipeline wrote via `write_cursor`/`write_file`/`sh` terminal ops (the "write side" of the cursor pipe). The spine crate supplied the `MutationEffect` cache-and-approve layer (mutations.rs). `rewrite_files(&[FileEdit])` and `shell_batch(&[ShellCall])` on the `Writer` trait (writers/mem.rs:111,:113) are batch-shaped write surfaces, `unimplemented!()` in the mem writer.

**What watch existed:** `spawn_fs_watcher(repo, root, sender, debounce)` (fs_watcher.rs:92) using `notify` + `notify_debouncer_mini`, recursive watch on `root`, gitignore-filtered via `ignore`, emitting `Change::FileBatch { repo, rev: "wt", paths }` per quiesce window (:153-157). The downstream used it to invalidate the fs-domain cache. Documented macOS granularity caveat: FSEvents reports parent dirs, under-evicting (:85-91).

### 3.2 v4 (`~/projects/sprefa-archive-20260701/v4`) - one crate, DRed reactive runtime, writes became direct IO

**Effect kinds / surface.** v4 pulled `effect_runtime`'s v2 Component graph (Node/Wake/BarrierScope/FactStore, see src/v2_ops.rs imports) into one crate. There is NO `EffectKind`/`Batcher`/`WritePolicy`/`WriteDecision`/`StagedWrite`/`SubjectRegistry<WriteApproval>`/`ShPolicy` anywhere in v4/src (grep over those names returns empty). The `v3_effect_runtime_v2_fact_store` crate carries only the fact-store layers, not the write-policy fls.

**How a write was expressed:** `write_cursor` and `write_file` remained as ops, but they perform **direct IO inside the component**: `WriteCursorComponent::render_batch` reads the file with `std::fs::read` and writes with `std::fs::write` (v2_ops.rs:3450, :3459); `WriteFileComponent::render` writes immediately (v2_ops.rs:3520-3529). No interpreter, no recorded decision in the v3 sense.

**Who decided / where recorded:** v4 replaced the policy with two inline guards: drift detection (`write/file-drift`: re-hash on-disk bytes, compare to the interned `content_hash`, skip the write if changed, v2_ops.rs:3453-3463) and dirty dispatch (`dispatch_file_dirty` → `SourceWake::dirty`, v2_ops.rs:87-97) so downstream recomputes after a write. Decisions became diagnostics, not a per-row decision enum.

**What dry-run meant:** absent as a term; grep finds no `dry_run`/`DryRun` in v4/src. v4's own design doc says the write op stays "a per-cursor effect" (docs/v4-runtime-batching.md:153).

**What batching existed:** write batching per file. `WriteCursorComponent::render_batch` groups hits by FS path, sorts right-to-left so earlier offsets stay valid, one read + one write per file (v2_ops.rs:3277-3459). Plus `CollectComponent`/`collect()` fan-in (v2_ops.rs:3593+, docs/v4-runtime-batching.md:120-158).

**What the codemod/refactoring was:** same sprefa pipeline form (`write_cursor`/`write_file`), plus `next`/`next?` event/yield primitives for imperative workflow control (docs/v4-runtime-batching.md "Next / Next?").

**What watch existed:** no OS-notify fs watcher in the runtime. Watch is a git/gh event poller (`git_watch.rs`, `spawn_ghcache_watcher` in bin/sprefa-daemon.rs:201) feeding `DirtyNotice`/`SourceWake` into a DRed dirty-source sweep (dirty_source.rs:1-50). External file changes are not part of this; the model is re-derive-on-demand.

### 3.3 v5 (`/Users/chrishafley/projects/sprefa/src`) - the @async effect runtime

**Effect kinds.** v5's only effect system is `@async`/`@stream`/`sh` rule bodies (src/effect.rs). A rule body solution queues a `pending_effect` row into SQLite; the daemon runs `drain_effects`/`drain_streams` BETWEEN ticks (never inside `tick`) through an `EffectExec`; the real-IO implementation is `ShellEffectExec` (effect.rs:148, :186, :309). No write-file/codemod effect: `write_file`/`write_cursor` appear in v5 only as parser keywords and a `--move` AST analysis tool (ast.rs:919, lib.rs:1206), not as effect kinds.

**How a write/effect was expressed:** `sh fn(...)` decls. The `collect(x[, N])` wrapper (effect.rs:558-577) gathers variable `x` across ALL body solutions so the effect fires once with the whole set ("the provider-native batch-by-id", examples/gh-cache-batch.dl:12-18). `collect(x, N)` caps each batched request at N.

**Who decided / where recorded:** `pending_effect` rows carry states queued/running/done/failed/orphaned (effect.rs STATE_*); each request has a blake3 id over `head_rel|kind|args|clock_salt` (effect.rs:786-793). No WritePolicy analog.

**What dry-run meant:** two unrelated dry-runs exist: the `--move` refactor prints the planned edits by default and writes only with `--fix` (lib.rs:1206, :1628, :1637), and a `checkout` demand sink honors `DL_CHECKOUT_DRY_RUN=1` writing a `checkout_plan` twin instead of mutating (engine/decls.rs:370-405). Neither is a general write-effect dry-run.

**What batching existed:** `collect(x[,N])` over shell effects (effect.rs:691-737). This is the "collect from dl5" the user named.

**What watch existed:** `@stream`/`sh*` subscriptions are the streaming watch surface (`drain_streams`, effect.rs "subscriptions... stay 'running' forever"); plus git checkout sinks in `--watch` daemon mode (decls.rs:370).

### 3.4 v5cozokuzu (archive) - not an effect system
A throwaway Cozo-vs-Kuzu storage experiment (src/lib.rs shared fixture + src/bin/*_demo.rs; 108 total lines). No write/watch/effect code. Row: absent.

### 3.5 Why each did not survive

| transition | observed reason (measure not assumed) |
|---|---|
| v3 → v4 | v4 built on the `effect_runtime` v2 **Component queue + SQLite FactStore** runtime; the v3 `EffectKind`/`Batcher`/`WritePolicy`/staging registry and `SubjectRegistry<WriteApproval>` did not carry into v4/src (grep empty). Writes became direct-IO components because the DRed runtime already owned re-derive via dirty dispatch; the v3 sink's `invalidate_fs` was replaced by `dispatch_file_dirty`, and drift detection replaced the approval policy. Direct evidence of the stated intent is thin; this is an inference from what shipped. |
| v4 → v5 | v5 adopted SQLite-backed `@async`/`sh` effects with `collect` for batch, dropping v4's Component render_batch writes and the `next`/`next?` channel surface. The approval/policy machinery stayed out. |
| v5 → v6 | v6 replaced the Rust engine with a TS rx runtime + Prolog compiler + host decls (`sh`), keeping subprocess effects and adding `watch`/`interval` binds and witness caching; the `collect` aggregate did not land (F2), and the write side was deferred to the `staged-writes` lab (F6). |

## 4. Comparison table: v3 vs v4 vs v5 vs v6 (centerpiece)

| question | v3 | v4 | v5 | v6 (current) |
|---|---|---|---|---|
| effect kinds (single source) | 10+ in effects.rs (list/read/read-batch/print/write-file/write-range/sh/lspdiag/astparse/yield) | Component ops: write_cursor/write_file + collect | `@async`/`@stream`/`sh` only | `sh`, `watch`, `interval`, `extract`/`extract_repo` host decls |
| how a write is expressed | effect value via `ctx.put(WriteRangeEffect).await` | direct `std::fs` in component | absent (no write effect) | `sh`/extract subprocess; lab uses `zone.py`/splice shelling |
| who decides / where recorded | `WriteRangeSink::StageAndApprove` consults `policy.decide(id)`, records `StagedWriteRangeRow` (effects.rs:814-845) | drift detection `write/file-drift` + `dispatch_file_dirty` (v2_ops.rs:3453, :87) | `pending_effect` state machine + blake3 id (effect.rs) | witness cache states done/error + response rel (types.ts:747-775) |
| dry-run meaning | decision per row: `DryRun => Err("dry-run")`, buffered+reported, no splice (effects.rs:820) | absent | `--move` prints plan, writes only on `--fix` (lib.rs:1628); `checkout`/`checkout_plan` twin | no keyword; staged-diff-as-relation lab (F1) |
| batching over | reads: `ReadBytesBatchEffect` rayon (effects.rs:302); writes: none | writes per file right-to-left (v2_ops.rs:3277); `collect` fan-in | `collect(x[,N])` shell (effect.rs:558) | extract host applicative fold (F5); writes: none (F6) |
| codemod/refactor system | pipeline write ops + spine `MutationEffect` cache+approve (mutations.rs) | pipeline write ops + `next`/`next?` | `--move` use-path rewriter (lib.rs:1206) | staged-diff lab (1-stage.dl6) |
| watch: file/folder/glob | notify recursive root, gitignore-filtered, emits FileBatch (fs_watcher.rs:92) | git/gh event poll + DRed dirty sweep (no OS notify) | `@stream` subscriptions + checkout sink | `watch(glob,path,digest)` bind, coalesced, node fs.watch default, @parcel/watcher swappable (types.ts:807-831) |
| why it did not survive | (start) | policy/staging registry dropped; IO inlined into components | v5 dropped component writes | v6 deferred writes to lab |

## 5. v6 today: row-by-row verification (deliverable 1, second part)

| v6 capability (from brief) | status | verified evidence |
|---|---|---|
| `watch` as a declared bind | EXISTS | registry.pl:295 `bind_definition(watch, [col(glob, text), col(path, text), col(digest, text)])`; executor `live_watch` at :298; `IWatchBindRunner` types.ts:817 |
| host effects with a witness cache | EXISTS | `IWitnessCache` types.ts:747-759, `IHostRunner` :761-775; cache rows serve as in-flight lock |
| `sh` shell host decl | EXISTS | grammar dl.langium ShDecl :54-57 (comment block :52); example prolog/compile/dl_view/duplicate_host_name_is_refused.dl6:1 |
| file read from `.dl6` | ABSENT as builtin (subprocess only) | only runtime-internal digest read at 2_binds.ts:232; reads reach files via `sh`/`sprefa-extract` subprocess (F3) |
| file write from `.dl6` | ABSENT as builtin | no write bind; `sh` one fork per call (F6, 2-apply.dl6) |
| dry-run | ABSENT as keyword; present as staged-diff data in the lab | F1; staged-writes/1-stage.dl6:1-4 |
| `collect` batching | ABSENT in v6; v5-only | F2; labs reference v5's collect, not a v6 builtin |

## 6. Build-vs-buy (deliverable 3) - written candidates per problem

Research agent fetched crates.io versions/dates 2026-08. Recommendation per area follows each table.

### 6.1 Apply a set of file edits atomically with dry-run

| candidate | version | maintenance | API shape | does NOT cover | dep weight | dry-run? |
|---|---|---|---|---|---|---|
| `codemod` | 0.0.0 | yanked/archived 2022 | placeholder, never implemented | nothing usable | negligible | no |
| `codemod-tokens` | not published | n/a | n/a | n/a | n/a | n/a |
| rust-analyzer `TextEdit`/`SourceChange` | internal only | rust-analyzer repo | byte-range→text list + bundled file edits | NOT a standalone crate; only `text-size`/`rowan`/`salsa` publish the model's parts | n/a | no |
| `ropey` | 1.6.1 (2.0.0-beta.1) | active | `Rope` buffer, byte/char/line indexing | diff, edit application, persistence (you build the model) | light | no |
| `similar` | 3.1.2 | active | `TextDiff::from_lines/chars`, `DiffOp` | applying is not diffing; no transaction | light (bstr, opt unicode) | no (diff only) |
| `diffy` | 0.5.1 | active | `create_patch`/`apply`, `Patch` | no multi-file transaction, no validate-before-write | light | apply, not rollback |
| `imara-diff` | 0.2.0 | slow | raw Myers/patience hunk singletons | no formats, no apply | light | no |

Cost: `ropey` (buffer) + `similar` (diffs) + `diffy` (patch apply) are each useful parts; none applies a multi-file edit set atomically with dry-run. The transaction (validate-all, then write, then report, rollback on error) is ours to write over per-file `tempfile::persist` (6.6) and the v3 `StagedWriteRow` record shape.

### 6.2 Codemod / refactor driver

| candidate | version | maintenance | API shape | does NOT cover | dep weight | dry-run? |
|---|---|---|---|---|---|---|
| `ast-grep-core` / CLI | 0.45.1 | active | pattern over tree-sitter CST; `$A`/`$$$` holes | single-language per CST; you write the driver | heavy (grammars) | CLI dry-run; lib leaves diff to you |
| `comby` | Go/Python | active (Go) | structural holes `:[x]` | **no Rust binding ships**; FFI to Go binary needed | n/a | Python CLI -diff |
| `jscodeshift` model | JS | active | `Collection` + transform closures | **no Rust equivalent**; closest is ast-grep-core or `syn` `VisitMut` | n/a | yes (--dry) |
| `ide-assists` (rust-analyzer) | internal | rust-analyzer repo | `Assist` registry + edit builder over rowan/ide-db | not published; coupled to its stack | n/a | no |
| `syn` + `prettyplease` | syn 3.0.3 / pp 0.3.0 | active | parse `syn::File`, `VisitMut`, reprint | Rust-only; no query language; no parallel walk | syn med, pp light | no |

Cost: `ast-grep-core` already a repo dependency (v3 ast_grep op, v6 sprefa-extract) and is the best generic driver (pattern language + CLI). `syn`+`prettyplease` for typed Rust-only rewrites. Neither is an effect-dispatch layer; that is section 2's interpreter, ours.

### 6.3 Batched file reads + directory walking

| candidate | version | maintenance | API shape | does NOT cover | dep weight |
|---|---|---|---|---|---|
| `ignore` | 0.4.33 | active | `WalkBuilder`/`WalkParallel`, gitignore+ignore filters | listing only; you do the reads/batching | medium |
| `walkdir` | 2.5.0 | slow/stable | sequential `WalkDir` iterator | no ignore parsing, no parallel | light |
| `jwalk` | 0.9.0 | **deprecated** ("Use dua-core") | parallel rayon walk | no ignore matching; superseded | light |
| `globset` | 0.4.20 | active | `GlobSetBuilder`, path matching | matching only, no walk | light |

Cost: `ignore` WalkParallel + `globset` filters. This is what v3 already used for the watcher (fs_watcher.rs imports ignore gitignore) and what v4 used for walking. `jwalk` is a non-candidate (deprecated).

### 6.4 File and folder watching

| candidate | version | maintenance | API shape | does NOT cover | dep weight | dry-run? |
|---|---|---|---|---|---|---|
| `notify` | 8.2.0 (9.0 rc) | active | `RecommendedWatcher` → raw Event stream | no debounce, no filtering, no command-run | light | no |
| `notify-debouncer-full` | 0.7.0 | active | coalesces raw events across a timeout | no command-run, no filter DSL | medium | no |
| `watchexec` | 8.2.0 (lib+CLI) | active | watch + debounce + command-run + filtering + signal | you wire the command loop; CLI ships separately | heavy (tokio) | no |
| `@parcel/watcher` / `fs.watch` (Node) | Node | active/stable | native subscribe / EventEmitter | Node-only, not Rust | n/a | no |

**watchexec verdict (the brief demands it be stated):** watchexec is a maintained lib AND CLI doing watch+debounce+run, which is most of the ask. It beats extending the v6 `watch` bind IF the runtime wants debounce+run+signal in one dependency. But v6 deliberately keeps the watcher behind an adapter (`IWatchSource`, types.ts:824-831; "Swapping in `@parcel/watcher` later replaces `IWatchSource` alone") and the first impl is zero-dependency `fsPromises.watch`. watchexec would replace `IWatchSource` only, costing a heavy tokio dep for a seam already collapsed to `(glob,path,digest)` rows. Recommendation: keep the seam; adopt `watchexec` only when the run/command/signal behavior is wanted in-process on the file side, or keep Node's watcher for the TS runtime and put Rust-side `notify`/`watchexec` behind the `sprefa-write`/`sprefa-watch` sibling CLI (section 7).

### 6.5 Effect description + interpreter in Rust

| candidate | status | API shape | fit |
|---|---|---|---|
| `futures` Stream/StreamExt | stable | pull streams, hand state machine | fine, no generator sugar |
| `genawaiter` | dormant (last release 2020) | `gen!{}` yield iterator, proc-macro-hack | not for real use |
| `async-stream` | slow/stable | `stream!` proc macro, one-way yield | no two-way resume |
| std Coroutine/gen | **nightly only in 2026** | native `yield` with resume arg | not stable |
| enum + match interpreter | n/a | Effect enum + a step match | zero deps, dry-run by construction |

Cost: enum + match interpreter is the norm and the dependable shape; dry-run falls out for free (a non-approved decision just isn't performed). This is exactly the v3 `EffectKind`/`WriteDecision` shape. Rust-native generators are not stable in 2026 (tracking issue still nightly), so this is not a near-term sugar.

### 6.6 Atomic file replace

| candidate | version | maintenance | API shape | does NOT cover | dep weight |
|---|---|---|---|---|---|
| `tempfile` (+persist) | 3.27.0 | active | `NamedTempFile::persist` same-dir rename | single-file; no dir/rollback transaction | light |
| `atomicwrites` | 0.4.4 | slow | `AtomicFile` temp+rename | single-file; quirks | light |
| `cap-std` | 4.0.2 | active | capability `Dir`/`File` WASI-style | sandboxing framing, not replacement; no persist | medium |

Cost: `tempfile::persist` is the per-file atomicity primitive (same-dir rename). `atomicwrites` adds little; `cap-std` only if capability sandboxing is wanted. Multi-file transaction/rollback is ours over per-file persist.

### 6.7 Cross-cutting gap (everything must write this)
Multi-file atomic transaction with dry-run + rollback (areas 1/6) is not covered by any crate: dry-run, validate-before-write, and rollback are the bespoke layer, standing on `tempfile::persist`, an effect enum, and a `StagedWriteRow` record. No crate provides "apply this set of edits, dry-run first, report decisions as rows, roll back on error". Confirmed by the research agent across all six tables.

## 7. Shape recommendation (deliverable 4) - the `sprefa-extract` model, as forks with prices

### 7.0 The model: what `sprefa-extract` actually is
- A standalone Rust crate outside the v6 workspace (`[workspace]` table in its own Cargo.toml, "the whole point: prove the v6 extraction leaf with no v5 tree in the build graph", Cargo.toml:1-6).
- Library + CLI: `extract` bin (src/bin/extract.rs) is "THE BIN OWNS NO EXTRACTION LOGIC. Argument parsing, one library call, print" (:10). Clap-only (`cli` feature gate, Cargo.toml), no tokio, sync, rayon-parallel, arena-mastered ("No DB, no async", Cargo.toml description).
- Wire: **flat JSONL to stdout**, `--schema` prints the contract (bin extract.rs:16). The runtime shells out via `host_executor(extract, sprefa_extract)` (registry.pl:303) with a `"$DL_EXTRACT_BIN" ... {path}` command template (registry.pl:344); `runSprefaExtract` = `runShellLine` (1_hosts.ts:252-254); stdout decoded as JSON-array | JSONL | whitespace columns (1_hosts.ts coerce).
- Who invokes: `HostExecutors` registry map (1_hosts.ts:261-265), driven by `groupInvocations` folding compatible witnesses into one frontier run (1_hosts.ts:477, types.ts:773).

### 7.1 Should a sibling exist? Fork with prices.

**Fork A - `sprefa-write` sibling crate (named by the user), same pattern as sprefa-extract.**
- Wire: same JSONL-to-stdout, `--schema` contract, clap bin owning no logic; runtime shells out per demand. `host_executor(write, sprefa_write)` with a command template.
- Plus: a `--apply` vs `--dry-run` (or `--plan`) flag; `--plan` prints the edit set as JSONL rows, `--apply` performs under a single transaction (tempfile persist per file + rollback). The decision set is the JSONL that comes back, matching v3's `StagedWriteRow`.
- Batching: shipping a **batch-shaped host decl** (`write_file(path, edits...)` or `apply(zone)`) whose input is a path + a JSON array of edits, so N edits collapse into one invocation; the binary folds by path exactly like `groupInvocations` folds by witness.
- Price: one more standalone crate to test and release; needs a defined wire for multi-edit payloads (a column cannot carry an arbitrary array without a schema agreement, the 5-span.dl6 lesson).
- What it buys: the "tired of welding the chassis" ask is served directly, decoupled from the TS runtime, testable in isolation like sprefa-extract.

**Fork B - inside the existing host runner (extend `IHostRunner`).**
- Write as an in-process host that takes a path + edits, applies transactionally, returns rows.
- Lower ceremony (no new crate/release); but re-ties write logic to the TS runtime, the thing the user wants to stop welding each time. Gains none of the cross-version reuse the archaeology showed keeps being rebuilt.

**Fork C - a shared `sprefa-write-core` lib crate + thin CLI + thin runtime adapter both consume.** Highest reuse; the write engine (edit-set, dry-run decision, transaction, atomic replace) lives once in Rust, the v6 runtime and any future v7 both shell out to one CLI.

Price table:

| fork | reuse across versions | ceremony | decoupling from TS runtime | matches user's ask |
|---|---|---|---|---|
| A: sibling crate | good (lib+CLI reusable) | medium (new crate+release) | yes | yes |
| B: in host runner | low (rewrites each time) | low | no | partial |
| C: core lib + thin CLI + adapter | best | high (3 surfaces) | yes | yes |

### 7.2 The wire between runtime and this thing, and why
A batch-shaped host decl, same `"$DL_EXTRACT_BIN"`-style template (`"$DL_WRITE_BIN" ... {path}`), stdout JSONL of one decision row per edit `{id, path, byte_range, decision, result}`, echoed by the runtime onto a response rel (as the current host response decode already does). Why: it reuses the existing executor registry, the `--schema` contract, the existing output decode, and it keeps the write side a pure subprocess read of a file the demand row already names (the property that earns the `ApplicativeExecutors` fold for extract, 1_hosts.ts:268-274). JSONL is chosen because a column cannot carry an arbitrary edit array (5-span.dl6 records the span-wrapping rejection of span columns), so the list must ride stdout.

### 7.3 How a `.dl6` program names a write, given `sh` already forks per call
Two surfaces:
- **Reuse `sh`** for the whole-file case (filename is a column): one `sh` decl with a template that invokes `"$DL_WRITE_BIN" ... ` per file. Cost: one fork per call unless batched.
- **New batch-shaped host decl**: `write_file(path: text, digest: text) -> (...) = \`"$DL_WRITE_BIN"{digest} {path}\`.` where the binary reads the edit set from a digest-addressed cache or from a schema column. The `write_file` name is already a reserved op across v3/v4 (parser keyword), so the naming carries.

### 7.4 How batching arrives - three options priced
1. **Port `collect`** (v5): a `collect(x, N)` aggregate wrapper so one write host fires once with the whole set. Matches the user's "yes to batching, its purpose of collect from dl5". Cost: the aggregate is a language feature (registry + lowering), v5's was shell-only.
2. **Batch-shaped host decl**: the write decl takes a path + an edits list; the runtime folds same-path demands into one invocation (mirror `groupInvocations`). Cost: needs the JSONL arrays-as-payload wire (7.2).
3. **Library batches internally**: the CLI buffers edits and applies per-file transactionally regardless of caller. Cost: internal, invisible to the runtime; best for dry-run/rollback atomicity, no language change.

Recommendation: 3 for atomicity + 2 for transport (so the count of forks drops to the count of files, not edits), and 1 only if the user wants the dl-level aggregate syntax. These are priced as options; the user rules.

### 7.5 Where the dry-run decision is recorded so a user sees it
In the returned rows, per edit: `{id, path, byte_range, decision, result}`, exactly v3's `StagedWriteRangeRow` (effects.rs:735-740) and the staged-diff relations (1-stage.dl6 edit_add/edit_del). The `id` is content-stable across runs (v3 write_range_id, effects.rs:742) so a cockpit can reference a prior run's id when approving, which is the v3 `ApproveOnly(set)` mechanism made data-level. The runtime lands these on a response rel; a `.dl6` program can query them, so dry-run is inspectable by the language, not a mode flag.

### 7.6 How watch-file and watch-folder differ in the surface
v6 today has one glob `watch` bind; no file/folder split (F4). Options priced:
1. **Keep one `watch` bind, glob decides.** A folder is `watch("**")`/`watch("dir/**")`; a file is `watch("dir/x.rs")`. Cost: zero design; but a file watch should emit the file's digest changes and a folder watch adds/removes over matches, which the single `(glob,path,digest)` row shape already distinguishes by path shape. The `added`/`removed` counts are rows, not FS events (types.ts:797-805).
2. **Two binds, `watch_file(glob)` and `watch_dir(glob)`**, differing only in what they emit: file watch emits per-file digest rows; dir watch emits an aggregate of matched files per window. Cost: two bind_definition rows + two executors, mirrors the runtime's own `interval`/`watch` split.
3. **One bind, a `depth`/`kind` column** on the watch decl. Cost: widens the schema.

Price: option 1 is already the behavior; option 2 is the accurate surface if the user wants the file-editing case (digest per file) distinct from the watching-many case (matched set). The fork is cheap either way because the arrival rows already collapse to `(path, digest)` with a sign.

## 8. Sources and open items

Primary code, all read directly with cited lines:
- v3 effect_runtime: `~/projects/sprefa-archive-20260701/v3/crates/effect_runtime/src/lib.rs`, `subjects.rs`
- v3 pipeline effects: `~/projects/sprefa-archive-20260701/v3/crates/pipeline/src/effects.rs`, `ops/write_cursor.rs`, `ops/write_file.rs`, `fs_watcher.rs`
- v3 server wiring: `.../v3/crates/server/src/run.rs` (:480-599), `transport_http.rs` (:93-103)
- v3 spine mutations: `.../v3/crates/spine/src/mutations.rs`, `writers/mem.rs`
- v3 doc: `.../v3/docs/convergent-evolution-effect-dispatcher.md`
- v4: `.../v4/src/v2_ops.rs` (:3217-3560), `.../v4/src/git_watch.rs`, `.../v4/src/dirty_source.rs`, `.../v4/docs/v4-runtime-batching.md`
- v5: `/Users/chrishafley/projects/sprefa/src/effect.rs`, `lib.rs` (:1206-1290), `engine/decls.rs` (:323-405), `examples/gh-cache-batch.dl`
- v6: `v6/prolog/compile/registry.pl` (:295-304, :336-360), `v6/tsv2/runtime/types.ts` (:747-831), `v6/tsv2/serve/1_hosts.ts` (:242-300, :477), `v6/tsv2/serve/2_binds.ts` (:226-232), `v6/dl/grammar/dl.langium` (:52-57), `v6/tsv2/labs/staged-writes/*.dl6`
- skills: `~/projects/claude-research/skills_archive/commands/sagas/redux-saga-essence.md`, `sprf-effect-runtime.md`, `applicative-batching.md`

Library versions/dates: research agent fetch of crates.io/docs.rs, 2026-08.

<!-- todo(decision): user to pick Fork A vs B vs C for sprefa-write (section 7.1). -->
<!-- todo(decision): user to pick batching path 1/2/3 (section 7.4). -->
<!-- todo(decision): user to pick watch surface 1/2/3 (section 7.6). -->
<!-- todo(feature): a write host's answer is the only evidence a write happened; 6-ordinal.dl6 shows a response column collision shadows it (F-findings note in 7.2/7.5). -->
