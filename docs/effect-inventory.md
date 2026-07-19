# Effect / Lock / Channel / Crate-Usage Inventory

Empirical seed data for a future effect-classification analysis of the `src/`
tree in the `kimi-effect-inventory` worktree.  All counts come from `rg`
searches of `src/` only; no build or compile was run.

## 1. EFFECT ROOTS by class

### 1.1 process / filesystem

Command:
```bash
rg -c -n '\\b(std::process::(Command|Child|Stdio|Output|ExitStatus|id)|Command::(new|spawn|output|status|arg|args|env|current_dir)|std::fs::|fs::(read|write|remove|rename|copy|metadata|canonicalize|create_dir|read_to_string|OpenOptions|File::create|File::open)|tempfile::)\\b' src/
```

Total sites: 422

| file | line-matches |
| --- | --- |
| src/setup/hooks_tests.rs | 43 |
| src/scip_setup.rs | 39 |
| src/daemon.rs | 36 |
| src/why.rs | 20 |
| src/watchgate.rs | 19 |
| src/lib.rs | 16 |
| src/engine/repo.rs | 15 |
| src/engine/extract/call_render_tests.rs | 13 |
| src/setup/hooks_idempotency_tests.rs | 13 |
| src/config.rs | 12 |
| src/setup/manifest/actions.rs | 12 |
| src/lsp.rs | 11 |
| src/perflog.rs | 10 |
| src/rels/git.rs | 10 |
| src/scip_import.rs | 9 |
| src/setup/manifest.rs | 9 |
| src/setup/manifest/write.rs | 9 |
| src/repo.rs | 8 |
| src/engine/gen.rs | 7 |
| src/engine/family/mod.rs | 6 |
| src/graph/modgraph.rs | 6 |
| src/setup/vscode.rs | 6 |
| src/cli/mod.rs | 5 |
| src/engine/derive.rs | 5 |
| src/engine/query.rs | 5 |
| src/agent.rs | 4 |
| src/daemon/read.rs | 4 |
| src/daemon/shell/http.rs | 4 |
| src/engine/generation.rs | 4 |
| src/engine/mod.rs | 4 |
| src/engine/pipeline/source_stage_tests.rs | 4 |
| src/engine/staged_delta/tests.rs | 4 |
| src/jobq/tests.rs | 4 |
| src/setup.rs | 4 |
| src/cli/inputs.rs | 3 |
| src/engine/scan.rs | 3 |
| src/propose/mod.rs | 3 |
| src/rels/mod.rs | 3 |
| src/setup/hooks.rs | 3 |
| src/setup/wire.rs | 3 |
| src/update.rs | 3 |
| src/effect.rs | 2 |
| src/engine/eval.rs | 2 |
| src/engine/rpc.rs | 2 |
| src/engine/source_prepare.rs | 2 |
| src/frontend.rs | 2 |
| src/rels/analysis.rs | 2 |
| src/activity.rs | 1 |
| src/daemon/http_discovery.rs | 1 |
| src/daemon/shell/watch.rs | 1 |
| src/db.rs | 1 |
| src/engine/extract/mod.rs | 1 |
| src/engine/pipeline/full_sources.rs | 1 |
| src/engine/pipeline/mod.rs | 1 |
| src/hook.rs | 1 |
| src/rels/scip.rs | 1 |

### 1.2 network / sockets

Command:
```bash
rg -c -n '\\b(std::net::|std::os::unix::net::(UnixListener|UnixStream)|UnixListener|UnixStream|TcpListener|TcpStream|axum::|hyper::|reqwest::|http::|tokio::net::|tokio_util::)\\b' src/
```

Total sites: 35

| file | line-matches |
| --- | --- |
| src/daemon.rs | 12 |
| src/daemon/shell/http.rs | 10 |
| src/daemon/shell/uds.rs | 4 |
| src/hook.rs | 4 |
| src/daemon/shell/mod.rs | 2 |
| src/mcp.rs | 2 |
| src/daemon/http_discovery.rs | 1 |

### 1.3 database (rusqlite-style method calls)

Command:
```bash
rg -c -n '\\.(execute|execute_batch|prepare|query|query_row|query_map|query_and_then)\\b' src/
```

Total sites (all files): 725; top 10 files below.

| file | line-matches |
| --- | --- |
| src/storage/call.rs | 88 |
| src/engine/meta.rs | 67 |
| src/db.rs | 48 |
| src/engine/extract/mod.rs | 39 |
| src/engine/derive.rs | 36 |
| src/engine/rpc.rs | 26 |
| src/engine/extract/call_render_tests.rs | 25 |
| src/effect.rs | 24 |
| src/engine/declare.rs | 24 |
| src/engine/deltaflow.rs | 23 |

### 1.4 console output

`println!`/`eprintln!`/`print!`/`eprint!` command:
```bash
rg -c -n '\\b(println!|eprintln!|print!|eprint!)\\b' src/
```
Result: 0 bare console-print macro sites.

`tracing::` macro command:
```bash
rg -c -n '\\btracing::(trace!|debug!|info!|warn!|error!|span!|instrument)\\b' src/
```
Result: 10 `tracing::...` macro sites (repo is migrating to tracing).

| file | tracing:: macro sites |
| --- | --- |
| src/engine/tick.rs | 3 |
| src/engine/repo.rs | 2 |
| src/engine/declare.rs | 1 |
| src/engine/derive.rs | 1 |
| src/engine/eval.rs | 1 |
| src/engine/gen.rs | 1 |
| src/engine/reconcile.rs | 1 |

### 1.5 time / sleep / blocking

Command:
```bash
rg -c -n '\\b(std::thread::sleep|Instant::now|SystemTime::now|recv_timeout|tokio::time::sleep|std::time::Instant::now|std::time::SystemTime::now)\\b' src/
```

Total sites: 86

| file | line-matches |
| --- | --- |
| src/engine/derive.rs | 15 |
| src/daemon.rs | 14 |
| src/engine/tick.rs | 9 |
| src/activity.rs | 4 |
| src/engine/mod.rs | 4 |
| src/jobq/tests.rs | 4 |
| src/daemon/read.rs | 3 |
| src/daemon/shell/jobs.rs | 3 |
| src/daemon/shell/watch.rs | 3 |
| src/db.rs | 3 |
| src/engine/extract/mod.rs | 2 |
| src/engine/family/mod.rs | 2 |
| src/engine/reconcile.rs | 2 |
| src/lib.rs | 2 |
| src/watchdog.rs | 2 |
| src/why.rs | 2 |
| src/cli/check_deadline.rs | 1 |
| src/engine/eval.rs | 1 |
| src/engine/extract/call_render_tests.rs | 1 |
| src/engine/pipeline/source_stage_tests.rs | 1 |
| src/engine/repo.rs | 1 |
| src/engine/source_prepare.rs | 1 |
| src/engine/staged_delta/tests.rs | 1 |
| src/hook.rs | 1 |
| src/jobq/dispatch.rs | 1 |
| src/jobq/mod.rs | 1 |
| src/perflog.rs | 1 |
| src/setup/manifest.rs | 1 |

### 1.6 process control (`std::process::exit`)

`panic!`/`unwrap`/`expect` were intentionally skipped as too noisy.

Command:
```bash
rg -c -n '\\b(std::process::exit|process::exit)\\b' src/
```

Total sites: 16

| file | line-matches |
| --- | --- |
| src/daemon.rs | 6 |
| src/cli/mod.rs | 5 |
| src/watchdog.rs | 2 |
| src/cli/daemon.rs | 1 |
| src/daemon/shell/timers.rs | 1 |
| src/tray.rs | 1 |

## 2. LOCK TOPOLOGY

### 2a. Lock field / static declarations

Search command:
```bash
rg -n 'Mutex<|RwLock<|parking_lot::Mutex|parking_lot::RwLock' src/
```

Found 19 declarations.

| location | kind / name | type | owner |
| --- | --- | --- | --- |
| src/activity.rs:120 | static SLOT | OnceLock<Mutex<Activity>> | global activity slot |
| src/perflog.rs:51 | static CURRENT_ROOT | Mutex<Option<PathBuf>> | global perf-log root |
| src/perflog.rs:55 | static FILES | OnceLock<Mutex<HashMap<PathBuf, Option<std::fs::File>>>> | global perf-log file-handle cache |
| src/jobq/mod.rs:247 | field db | Mutex<Db> | JobQueue |
| src/jobq/mod.rs:250 | field wake_gen | Mutex<u64> | JobQueue |
| src/jobq/tests.rs:30 | field seen | Arc<Mutex<Vec<JobRow>>> | RecordingRunner (test) |
| src/jobq/tests.rs:274 | field seen | Arc<Mutex<Vec<String>>> | PanicOnRootRunner (test) |
| src/db.rs:78 | field pending_syms | Arc<Mutex<Vec<String>>> | Db |
| src/db.rs:351 | static CACHE | OnceLock<Mutex<HashMap<String, regex::Regex>>> | compiled-regex cache |
| src/engine/repo.rs:566/567 | static LAST | OnceLock<Mutex<HashMap<String, std::time::Instant>>> | per-repo last-fetch cache |
| src/engine/repo.rs:1127 | static BATCHES | OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<GitBatch>>>>> | git-batch cache |
| src/daemon.rs:93 | static CURRENT_OP | OnceLock<Mutex<String>> | global current-op label |
| src/daemon.rs:278 | field program_files | Mutex<Vec<PathBuf>> | ServedRoot |
| src/daemon.rs:282 | field prog | Mutex<Program> | ServedRoot |
| src/daemon.rs:283 | field eng | Mutex<Engine> | ServedRoot |
| src/daemon.rs:284 | field last_activity | Mutex<Instant> | ServedRoot |
| src/daemon.rs:296 | field last_changed_paths | Mutex<Vec<PathBuf>> | ServedRoot |
| src/daemon.rs:318 | field read_view | RwLock<Arc<crate::daemon_read::ReadView>> | ServedRoot |
| src/daemon.rs:909 | field roots | Mutex<HashMap<String, Arc<ServedRoot>>> | Daemon |

### 2b. Direct `.lock()` / `.read()` / `.write()` method calls

Raw search command:
```bash
rg -c -n '\.(lock|try_lock|read|try_read|write|try_write)\(' src/
```

After excluding non-lock `.read()`/`.write()` (e.g. `std::io::Read`, `OpenOptions::write`, `GitBatch::read`) and `stdin.lock()`: 24 direct lock-method sites.

| file | lock-method sites |
| --- | --- |
| src/activity.rs | 8 |
| src/daemon.rs | 3 |
| src/perflog.rs | 3 |
| src/db.rs | 3 |
| src/engine/repo.rs | 3 |
| src/daemon/read.rs | 2 |
| src/tray.rs | 1 |
| src/jobq/mod.rs | 1 |

### 2b-helper. Lock acquisitions via project helpers `lock` / `rlock` / `plock`

Command:
```bash
rg -c -n '\\b(lock|rlock|plock)\\(&' src/
```

Total helper-based acquisitions: 83

| file | helper acquisitions |
| --- | --- |
| src/daemon.rs | 60 |
| src/jobq/tests.rs | 10 |
| src/jobq/mod.rs | 9 |
| src/daemon/shell/watch.rs | 3 |
| src/daemon/shell/timers.rs | 1 |

### 2c. Named locks appearing most often at acquisition sites

Extracted by grepping the receiver passed to `lock`/`rlock`/`plock` and stripping the prefix:
```bash
rg -o '\b(lock|rlock|plock)\(&([^)]+)\)' src/ | sed -E 's/.*\(&([^)]+)\)/\1/' | sed -E 's/^(self|sr|q|d|daemon)\.//' | sort | uniq -c | sort -nr
```

| lock name | acquisition count |
| --- | --- |
| prog | 18 |
| eng | 18 |
| db | 12 |
| roots | 11 |
| program_files | 11 |
| seen | 5 |
| last_changed_paths | 3 |
| wake_gen | 2 |
| last_activity | 2 |
| read_view | 1 |

### 2d. Candidate lock-ordering edges

Obvious sites where one lock is held while another is taken (spotted by reading; not exhaustive):

| edge | citations |
| --- | --- |
| ServedRoot.prog → ServedRoot.eng | src/daemon.rs:365-366, 407-408, 434-435, 595-596, 626-627 |
| ServedRoot.program_files → ServedRoot.eng | src/daemon.rs:522-523 (`pf` clone held across `eng` lock) |

## 3. CHANNEL / CONDVAR SURFACE

Channel creation command:
```bash
rg -n '\\b(channel|sync_channel|unbounded_channel|bounded_channel|broadcast::channel|watch::channel)\\s*\\(' src/ | grep -v '^\s*///'
```

Total channel creation sites: 4; unbounded: 3.

| file | line | site |
| --- | --- | --- |
| src/engine/derive.rs | 40 | let (tx, rx) = mpsc::channel(); |
| src/cli/check_deadline.rs | 18 | let (tx, rx) = mpsc::sync_channel(1); |
| src/daemon.rs | 1235 | let (broadcast_tx, broadcast_rx) = tokio::sync::mpsc::unbounded_channel(); |
| src/lib.rs | 633 | let (tx, rx) = std::sync::mpsc::channel(); |

### Condvar / park / Notify

Command:
```bash
rg -n '\\b(Condvar::new|\\.wait\\(|wait_timeout|wait_while|wait_until|thread::park|park_timeout|Notify::new|\\.notified\\(|\\.notify_waiters)\\b' src/
```

| file | line | site |
| --- | --- | --- |
| src/daemon.rs | 1234 | let job_notify = Arc::new(Notify::new()); |
| src/jobq/mod.rs | 264 | wake_cv: Condvar::new(), |
| src/jobq/mod.rs | 548 | .wait_timeout(g, timeout) |

### Crossbeam channel usage in src/

Command: `rg -n '\bcrossbeam(_channel)?::' src/`

| file | line | site |
| --- | --- | --- |
| src/lsp.rs | 326 | fn spawn_daemon_subscriber(root: PathBuf, sender: crossbeam_channel::Sender<lsp_server::Message>) { |

## 4. EXTERNAL CRATE USAGE MAP

Parsed from `use` lines in `src/`.

Command:
```bash
rg -n '^use [a-z_0-9]+::' src/
```

Then filtered with `awk` to drop `crate`/`std`/`self`/`super` and every internal module name found under `src/`.

Distinct external crates imported in `src/`: 22.

| crate | files | use-line count | using files |
| --- | --- | --- | --- |
| anyhow | 66 | 66 | src/anchor.rs, src/channel.rs, src/cli/check_deadline.rs, src/cli/daemon.rs, src/cli/inputs.rs, src/cli/mod.rs, src/cli/query.rs, src/cli/root.rs, src/config.rs, src/corpus.rs, src/daemon.rs, src/daemon/http_discovery.rs, src/daemon/read.rs, src/daemon/shell/http.rs, src/daemon/shell/watch.rs, src/db.rs, src/docs_cmd.rs, src/effect.rs, src/embed/candle_be.rs, src/embed/fastembed_be.rs, src/embed/mod.rs, src/embed/stub.rs, src/engine/cold_stage.rs, src/engine/desugar.rs, src/engine/extract/mod.rs, src/engine/family/mod.rs, src/engine/family/router.rs, src/engine/mod.rs, src/engine/pipeline/apply.rs, src/engine/pipeline/full_sources.rs, src/frontend.rs, src/hook.rs, src/jobq/dispatch.rs, src/jobq/mod.rs, src/ktpath.rs, src/lex.rs, src/lib.rs, src/lower.rs, src/lsp.rs, src/mcp.rs, src/parse/mod.rs, src/refactor.rs, src/rels/analysis.rs, src/rels/catalog.rs, src/rels/embed.rs, src/rels/env.rs, src/rels/extract_family.rs, src/rels/filelines.rs, src/rels/git.rs, src/rels/mod.rs, src/rels/perf.rs, src/rels/propose.rs, src/rels/querylog.rs, src/rels/scip.rs, src/rels/write_ledger.rs, src/rpc.rs, src/scip_import.rs, src/scip_setup.rs, src/setup.rs, src/setup/manifest.rs, src/sg.rs, src/storage.rs, src/storage/call.rs, src/tray.rs, src/update.rs, src/verbs.rs |
| serde_json | 11 | 11 | src/cli/daemon.rs, src/daemon.rs, src/daemon/http_discovery.rs, src/daemon/read.rs, src/daemon/shell/http.rs, src/daemon/shell/uds.rs, src/jobq/mod.rs, src/setup/hooks_idempotency_tests.rs, src/setup/manifest.rs, src/setup/manifest/json_edit.rs, src/why.rs |
| rusqlite | 9 | 9 | src/agent.rs, src/daemon/read.rs, src/db.rs, src/engine/deltaflow.rs, src/engine/pipeline/full_sources.rs, src/engine/pipeline/source_stage.rs, src/engine/staged_delta/mod.rs, src/engine/staged_delta/sql.rs, src/jobq/mod.rs |
| tokio | 3 | 7 | src/daemon.rs, src/daemon/shell/mod.rs, src/daemon/shell/uds.rs |
| axum | 1 | 5 | src/daemon/shell/http.rs |
| tree_sitter | 3 | 3 | src/ingest/mod.rs, src/lsp.rs, src/propose/mod.rs |
| regex | 3 | 3 | src/comment.rs, src/engine/mod.rs, src/graph/modgraph.rs |
| rayon | 3 | 3 | src/effect.rs, src/engine/extract/mod.rs, src/engine/mod.rs |
| serde | 2 | 2 | src/config.rs, src/setup/manifest.rs |
| syn | 1 | 2 | src/graph/typegraph.rs |
| tracing_subscriber | 1 | 1 | src/trace.rs |
| tokio_util | 1 | 1 | src/daemon/shell/mod.rs |
| protobuf | 1 | 1 | src/scip_import.rs |
| oxc_ast_visit | 1 | 1 | src/graph/typegraph.rs |
| oxc_ast | 1 | 1 | src/graph/typegraph.rs |
| lsp_types | 1 | 1 | src/lsp.rs |
| lsp_server | 1 | 1 | src/lsp.rs |
| ignore | 1 | 1 | src/watchgate.rs |
| clap | 1 | 1 | src/cli/mod.rs |
| ast_grep_language | 1 | 1 | src/sg.rs |
| ast_grep_core | 1 | 1 | src/sg.rs |
| ast_grep_config | 1 | 1 | src/sg.rs |

---

## Summary Headline Numbers

- **1. EFFECT ROOTS:** 1294 observable effect-root sites
- **2. LOCK TOPOLOGY:** 107 observable lock-acquisition sites (direct method calls + helper wrappers)
- **3. CHANNEL / CONDVAR SURFACE:** 4 channel creation sites, 3 of them unbounded
- **4. EXTERNAL CRATE USAGE MAP:** 22 distinct external crates imported in `src/`
