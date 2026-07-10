# Singleton daemon + registered roots (de-root C)

## Context

Chris, 2026-07-10: "i want it running wherever there is a .dl, idk why cwd matters so
much, its db in daemon mode belongs in a constant position in home." This is the C leg
deferred in the 2026-07-06 de-root session (rootless daemon, add_repo RPC, self.root
demotion). Today cwd is the ADDRESSING key: `daemon_target()` (src/cli/root.rs:26) walks
up from cwd to the nearest `.dl/`-owning ancestor and talks to that root's private socket
at `<root>/.dl/daemon.sock`; spawn-if-missing mints one daemon + one db per root. The XDG
singleton (`daemon_home()`, src/daemon.rs:61, `~/.local/state/sprefa`) exists but its view
comes entirely from config repos (src/daemon.rs:172) — a `.dl`-owning repo you walk into
is invisible to it, and `add_repo` has zero hits in src/.

Morning-of evidence for why per-root daemons are a liability: 12 leaked
`dl daemon start` processes killed (one per test sandbox), one of which
(`dl daemon start /tmp/disc2/p.dl`, cwd = repo root) BOUND the real repo's socket for a
day and served a /tmp program on it, wedging reactive doc regen with binary/program skew.
Per-root spawn-if-missing is the mechanism that leaked; a singleton with registered roots
deletes the mechanism.

## Findings that shape the design

| thing | where | role in this plan |
| --- | --- | --- |
| XDG home + singleton namespace | src/daemon.rs:57-75 (`daemon_home`, `home_dir(None)`) | the constant home position already exists |
| `root_implicit` gen write-back | src/engine/mod.rs:1763-1925, run_gens write_roots (mod.rs:5686) | engine already writes gen targets to each rule's `.git` ancestor when root is a placeholder |
| rule origin stamping | src/ast.rs:381 (`origin`, "stamped by the frontend loader") | per-root program merge without losing write-back targets |
| WatchGate per root | src/watchgate.rs (per-root GitignoreBuilder; :163 notes rootless home) | one watcher instance per registered root, already shaped for it |
| multi-repo coordinate | `_file`/`_prov` keyed (repo, path, rev); `roots.get(repo)` in extract | facts from N roots coexist in ONE engine IF we choose route A |
| `repo` demand sink | engine reserved sink, "repo-sink rules already register dynamically" | precedent: registration = a row, not a config edit |
| deep-root socket relocation | `/tmp/dl-sock/<blake3-16hex>.sock` chokepoint | precedent for hashing a root into a stable short key |
| `dl daemon <verb>` dispatch | src/cli/daemon.rs:15-60, `DL_DAEMON_ROOT` env | the single place addressing changes |
| hot-reload + discovery merge | daemon.rs reload_program (:276), ".dl discovery change — re-merging" | per-root program sets already re-merge on change |

## The fork: one engine vs N engines in one process

**Route A — one engine, roots-as-repos.** Registered root = a repo row in the config-view
engine (the multi-repo coordinate absorbs facts cleanly). FATAL flaw: derived rel names
collide across roots — two repos each declaring `rel todo(...)` in their own `.dl/` land
in one `rel_todo` table. The engine's repo coordinate namespaces FACTS, not user rel
declarations. Solving that means per-root schema prefixes on every table read/write — a
deep rename through lower/query/LSP.

**Route B — one process, one socket, N engines (RECOMMENDED).** The singleton hosts the
existing config-view engine PLUS one `Engine` per registered root. Each root keeps its
program isolation (the 0709.0 "no pushing till program isolation" concern is satisfied by
construction) and gets its own SQLite db at a constant home position. One socket; every
RPC carries a root key; the daemon routes to the right engine. Per-root behavior is
byte-identical to today's per-root daemon — only WHERE the db/socket/process live changes.

Route B is this plan. Route A stays the config-repo org view, unchanged.

## Type signatures

```rust
// src/daemon.rs
/// One registered .dl-owning root served by the singleton.
struct ServedRoot {
    root: PathBuf,            // canonical
    key: String,              // blake3-16hex of canonical root (dir name under home)
    eng: Mutex<Engine>,       // db at home/roots/<key>/db.sqlite
    prog: Mutex<Program>,     // merged <root>/.dl/*.dl, origins stamped
    watch: WatchGate,         // per-root ignore filter, narrow .git watch
}

/// Registry living inside the singleton daemon.
struct RootRegistry {
    roots: Mutex<HashMap<String /*key*/, Arc<ServedRoot>>>,
}

impl RootRegistry {
    /// RPC "add_root". Idempotent; canonicalizes, refuses a root inside another
    /// registered root (nested-repo explosion guard precedent).
    fn add_root(&self, root: &Path) -> Result<AddRootReply>;
    // canon = root.canonicalize()
    // key   = blake3_16hex(canon)
    // if roots.contains(key) -> Ok(already, tick_count)
    // guard: canon under an existing root OR an existing root under canon -> loud refuse
    // ServedRoot::open(canon)  // load .dl/*.dl, cold tick, start watcher
    // persist to home/roots.json; reply (added, diag summary)

    /// RPC "drop_root". Stops the watcher, closes the engine, keeps the db dir
    /// (re-add warms from it); `--purge` deletes home/roots/<key>/.
    fn drop_root(&self, root: &Path, purge: bool) -> Result<()>;
}

// src/cli/root.rs — addressing becomes root-key selection, not socket selection
enum DaemonTarget {
    Singleton { root: Option<PathBuf> },  // root = nearest .dl ancestor of cwd, None = config view
}
fn daemon_target() -> Result<DaemonTarget>;
// DL_DAEMON_ROOT wins as the root key when set (spawned children, tests)
// nearest_dl_ancestor(cwd) -> Some(root): Singleton{root}
// none                     -> Singleton{root: None}  (org/config view)

// src/daemon.rs — every existing RPC gains the root key in its envelope
// {"method":"query_sql","root":"/abs/root",...}  root absent = config view
fn dispatch(reg: &RootRegistry, req: Rpc) -> Result<Value>;
// look up ServedRoot by canonicalized req.root; auto add_root on miss when the
// path owns .dl/ (attach IS registration); else "unknown root" error naming add_root
```

## Instance lifetimes

- **Singleton process**: created on demand by the first attach (ensure_daemon), lives
  until `dl daemon stop` (global form). One per `$XDG_STATE_HOME`. Tests set
  `XDG_STATE_HOME` to a sandbox — hermetic by construction, and a leaked test daemon can
  never bind a developer socket again.
- **ServedRoot / Engine**: created by `add_root` (explicit or first attach), lives until
  `drop_root` or process exit. Survives restart via `home/roots.json` replay (re-adds on
  boot, warm dbs).
- **WatchGate + watcher thread**: one per ServedRoot, same lifetime as the ServedRoot.
- **Config-view engine**: unchanged; the singleton's existing engine, root_implicit=true.
- **Per-root daemons**: RETIRED. `home_dir(Some(root))` and the `<root>/.dl/daemon.*`
  files stop being created; stale ones are reaped on first singleton attach (log line).

## Storage layout

```
~/.local/state/sprefa/            # daemon_home(), overridable via XDG_STATE_HOME
  daemon.sock                     # THE socket (one)
  daemon.pid                      # pid\nstart_secs\n
  daemon.log
  roots.json                      # [{root, key, added_at}] — registration persistence
  db.sqlite                       # config-view engine (existing)
  roots/
    <blake3-16hex>/               # one dir per registered root
      db.sqlite                   # that root's engine db (was <root>/.dl/db)
      daemon.log?                 # no — one log, lines prefixed [<root basename>]
```

`<root>/.dl/` keeps: programs (`*.dl`), caches that are content-addressed and
root-local today (index.scip, perf.jsonl). It LOSES: daemon.sock/daemon.pid/db.

### Sequence of reads and writes (one-shot in a repo)

1. one-shot resolves nearest `.dl` ancestor from cwd (read-only walk).
2. connect `~/.local/state/sprefa/daemon.sock`; ECONNREFUSED -> spawn singleton detached,
   connect-probe loop (existing ensure_daemon shape).
3. RPC carries `root`; daemon canonicalizes, registry hit -> route; miss + `.dl/` exists
   -> add_root (cold tick inside the daemon, one-shot blocks on reply like today's
   spawn-if-missing cold tick); miss + no `.dl/` -> error.
4. engine answers from its own db; gen writes go to the root's tree (origin write_roots).
5. watcher events for that root tick only that engine (no cross-root lock contention;
   drain/debounce stays per ServedRoot).

### Uniqueness conditions

- One singleton per XDG home: bind() on daemon.sock is the lock (existing stale-sock
  reaping on next bind).
- One ServedRoot per canonical root path: registry key = blake3(canonical); symlinked
  aliases collapse to one entry.
- No nested registrations: add_root refuses a root that contains or is contained by an
  existing entry (mirror the SCIP explosion guard message).
- db dir uniqueness: key collision = same canonical path, by construction.

## Phases

- **P0 (ships alone, this week's gripe): `dl daemon start` detaches by default;
  `--foreground` keeps the debug path.** Today start is foreground (src/cli/daemon.rs:21)
  and reads as a hang. One flag + spawn_detached reuse. Also fold in: `start` from a cwd
  with no `.dl` ancestor should SAY it's starting the rootless singleton (today it's
  silent about which kind you got).
- **P1 registry + routing**: RootRegistry, add_root/drop_root RPCs, root key in every RPC
  envelope, `daemon_target()` -> DaemonTarget::Singleton, roots.json persistence,
  `dl daemon status` lists registered roots + per-root tick counts.
- **P2 retire per-root daemons**: spawn-if-missing attaches to the singleton instead;
  `home_dir(Some(root))` callers migrate; stale `<root>/.dl/daemon.*` reaped loudly;
  LSP + vscode extension route through the singleton with the workspace root as key
  (panel contract unchanged — dl/query already carries no socket assumptions).
- **P3 test hermeticity**: e2e daemon tests set `XDG_STATE_HOME=<sandbox>`; kill-on-drop
  guard from the 2026-07-10 leak-hunt arc keeps working (pid file moved to the sandbox
  home). The disc2 class (unsandboxed cwd binding a real socket) becomes structurally
  impossible — add the regression test that proves it.
- **P4 migration + docs**: first singleton attach imports an existing `<root>/.dl/db`
  into `roots/<key>/db.sqlite` (or cold-ticks if schema drifted — digest machinery
  decides); docs/daemon.md rewritten around ONE daemon kind + the config view;
  CHANGELOG + skill regen.

## Open questions

1. **Lock granularity**: one process-wide tick mutex today vs per-ServedRoot engine
   locks. Per-root locks are the point (a kernel-sized root must not block a dotfiles
   root); needs a pass over daemon.rs globals to confirm nothing assumes one engine.
2. **Idle eviction**: N registered roots = N warm engines. Keep the existing idle-timer
   per ServedRoot (drop the Engine, keep the registration; re-open on next RPC)?
3. **`dl daemon stop`**: global stop kills every root's service. Fine (one machine, one
   user), or does stop want a `--root` form that just drop_root's?
4. **roots.json vs rows**: registration could live as rows in the config-view db
   (repo-sink precedent) instead of a JSON file. JSON chosen above because the config
   engine must not be a boot dependency for per-root serving; revisit if the org view
   wants to SEE registered roots as a rel (a `served_root` builtin would be the seam).

## Non-goals

- Route A (roots-as-repos in one engine) and any rel-name namespacing scheme.
- Changing `--no-daemon` one-shot semantics (unchanged, in-process, root = cwd walk).
- Windows/named-pipe transport, multi-user homes.
