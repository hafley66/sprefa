# Ambient-config hermeticity design

## Context

Item 12 in `plans/2026-07-11-codex-feedback-queue.md` records the same failure
mode from at least four agents: a local-looking `dl` command can read repositories
from a user-global config, attach to a long-lived process, and persist results
somewhere other than the command's apparent working tree. The current safe recipe
is three independent controls:

```sh
SPREFA_CONFIG=/nonexistent/config.toml DL_NO_DAEMON=1 dl program.dl --db "$scratch/db.sqlite"
```

That recipe is easy to omit and the consequences are not visible in one place.
The existing `[config] N repo(s) registered` line does not name the config file,
and a successful daemon attach emits no client-side line naming the served root or
the database that receives derived-state writes.

### Current resolution order at base `07030b1`

Config resolution is first-existing-wins (`src/config.rs:162-195`):

1. If `SPREFA_CONFIG` is set, its value is the *only* candidate. A missing file
   produces an empty config; there is no fallback.
2. Otherwise, if `XDG_CONFIG_HOME` is set, try
   `$XDG_CONFIG_HOME/sprefa/config.toml`.
3. Then try `$HOME/.config/sprefa/config.toml`.
4. If no candidate exists, `load_default()` returns an empty config.

`config_path()` is subtly different: it returns the first existing candidate or,
if none exists, the first candidate anyway so a watcher can observe later file
creation. In-process engines call `load_default()` through `load_repos()`
(`src/lib.rs:258-268`). Every daemon `ServedRoot`, including the config view and
each registered workspace root, calls `load_repos_eager()` when opened
(`src/daemon.rs:570-612`). A daemon therefore uses the config environment it was
started with, not a later client's environment.

Normal CLI dispatch applies `--no-daemon` by setting `DL_NO_DAEMON=1` before mode
selection (`src/cli/mod.rs:248-269`). Inline and folder inputs force that opt-out,
but a zero- or one-file run remains daemon-eligible when all of these hold:

- daemon use is enabled;
- the resolved root contains `.dl/`;
- the database is absent or was implicitly defaulted; and
- no more than one positional program was supplied.

The gate is split between `src/cli/mod.rs:273-380`, `daemon::enabled_for()` at
`src/daemon.rs:1878-1886`, and `run_file()` at `src/lib.rs:144-161`. A successful
attach may spawn the singleton, lazily register the root, use the daemon's
`.dl/*.dl` discovery set, and write derived state to
`$XDG_STATE_HOME/sprefa/roots/<root-key>/db.sqlite`. A daemon-free discovery run
instead implicitly writes `<root>/.dl/cache.db` and may create
`<root>/.dl/.gitignore` (`src/cli/mod.rs:292-315`). The singleton's config-view
database is `$XDG_STATE_HOME/sprefa/db.sqlite`; daemon path derivation otherwise
falls back through `HOME` (`src/daemon.rs:111-121, 175-183`).

The safety property needed here is therefore not “same output every time.” It is:
the process reads no user-global repository registry, communicates with no
pre-existing background engine, and creates no implicit persistent database.
The selected root and program still read source files, and explicitly requested
effects or output paths remain explicit side effects.

## Goals and non-goals

The design must:

- make every ambient config or daemon routing decision visible on stderr before
  query/diagnostic output;
- make one flag replace the three-part agent incantation;
- report the daemon's real config source and served-root database, not paths
  reconstructed from the client environment;
- keep stdout byte-compatible for JSON-lines, hooks, MCP, LSP, and human queries;
  and
- distinguish implicit persistence from an explicit `--db` chosen by the caller.

It does not promise a read-only program, disable `--fix`, `--verify`, `--apply`,
`gen`, shell effects, network access, or source-tree reads. Those are program/run
semantics rather than ambient-context selection. `--hermetic --apply` is allowed
and means “apply using only the explicitly selected root/program/config/db.”

## Decisions

### Ranked policy options

1. **Recommended — strict ambient hermeticity.** `--hermetic` selects no config,
   no daemon, and no implicit persistent database. With no `--db`, use SQLite
   memory storage even in `.dl` discovery mode; an explicit `--db PATH` is
   honored. This exactly replaces all three ledgered controls while retaining an
   inspectable scratch database when the caller asks for one.
2. **Environment-alias compatibility.** Implement `--hermetic` as if
   `SPREFA_CONFIG` named a missing file and `DL_NO_DAEMON=1`, but preserve the
   root-local `.dl/cache.db` default. This is smaller, but it does not replace the
   ledgered scratch-`--db` step and still creates `.gitignore`/cache files.
3. **Observability only.** Add the banner and keep `--no-daemon` plus environment
   variables as the control surface. This fixes surprise after the fact, not
   reproducibility, and leaves the four-agent usability failure intact.

Choose option 1. “Hermetic” should describe the default behavior of the flag,
not require a footnote saying that a persistent cache remains implicit.

### `--hermetic` semantics

| Surface | Normal default | With `--hermetic` | Notes |
|---|---|---|---|
| Config candidates | `SPREFA_CONFIG`, then XDG, then HOME | disabled; zero configured repos | Do not emulate this with a magic nonexistent pathname internally. |
| Daemon socket | may attach or spawn for a `.dl` root | never connect, probe, attach, register, restart, or spawn | Stronger and clearer than merely making attach fail. |
| Implicit database | discovery/check/LSP may choose `.dl/cache.db`; daemon chooses XDG state | SQLite memory database | No `.dl/.gitignore` creation. |
| Explicit `--db PATH` | honored and daemon-ineligible | honored, in-process | The path is explicit caller state, commonly a scratch DB. |
| Root/program discovery | cwd/nearest `.dl`, then `.dl/*.dl` as today | unchanged | Root-local inputs are in scope, not ambient user config. |
| `--parse-only` | no scan/database | unchanged | Config and daemon remain untouched; no banner is needed. |
| Source reads and git reads | enabled | enabled | Hermetic means scoped inputs, not no inputs. |
| `gen`, `--fix`, `--verify`, `--apply`, effects | governed by their existing flags/mode | unchanged | The flag must not masquerade as a read-only sandbox. |
| `SPREFA_CONFIG` plus `--hermetic` | env selects config | flag wins; print `config=off(--hermetic)` | A command-line safety request outranks ambient env. |
| `DL_NO_DAEMON=0` plus `--hermetic` | daemon remains enabled | flag wins | Only exact `DL_NO_DAEMON=1` disables today; hermetic is explicit policy. |
| `--no-daemon` plus `--hermetic` | in-process | valid and redundant | No conflict error; scripts may migrate incrementally. |

Initial scope is engine-backed top-level runs parsed by `Cli`: file/discovery,
`--check`, `--diag-json`, `--lsp`, `--mcp`, `--hook`, `--watch`, `--settle`,
`--changed`, `--move`, and `--verify`. Pre-clap query/control subcommands need an
explicit policy decision: the recommendation is to teach `what`, `summary`, and
`q` the same global flag in the same implementation arc, while rejecting
`--hermetic` on `daemon start/serve/load` because requesting daemon control and
forbidding daemon contact are contradictory. Pure documentation/setup/index
subcommands neither load the repo config nor use the execution database and may
reject the irrelevant flag rather than pretending it did work.

### Loud context banner

The exact one-line schema is:

```text
[dl context] config=<source>; execution=<route>; state-writes=<destination>; use --hermetic for config=off, daemon=off, implicit-db=memory
```

Concrete examples:

```text
[dl context] config=/Users/chris/.config/sprefa/config.toml (3 repos); execution=in-process root=/work/sprefa; state-writes=/work/sprefa/.dl/cache.db; use --hermetic for config=off, daemon=off, implicit-db=memory
[dl context] config=/Users/chris/.config/sprefa/config.toml (3 repos; daemon environment); execution=daemon pid=4812 root=/work/sprefa (attached); state-writes=/Users/chris/.local/state/sprefa/roots/91b6c3a34a5fd120/db.sqlite; use --hermetic for config=off, daemon=off, implicit-db=memory
[dl context] config=off(--hermetic); execution=in-process root=/tmp/case; state-writes=memory
[dl context] config=off(--hermetic); execution=in-process root=/tmp/case; state-writes=/tmp/case/result.sqlite (explicit --db)
```

Rules for emission:

- Write exactly one line to **stderr**, never stdout.
- Emit after final routing/config/database resolution but before the first engine
  tick, query RPC, diagnostic payload, LSP handshake, MCP loop, or hook response.
- Emit for an engine-backed invocation when at least one risk is present: a real
  config file was loaded, a daemon route was selected, or an implicit persistent
  database was selected. Also emit in `--hermetic` mode as positive confirmation.
- Do not emit for `--parse-only` or non-engine informational subcommands.
- `DL_LOG=quiet` may suppress performance/debug verdicts but must **not** suppress
  this safety line. Machine-readable stdout remains clean; callers that discard
  stderr have explicitly declined diagnostics.
- Render absolute, lexically normalized paths where possible. Do not use `~`,
  because copied logs must identify the actual destination.
- `state-writes` names engine state, not every possible program effect. Existing
  effect/gen warnings keep their own contracts.

The current run-header verdict in `src/verdict.rs` is a useful formatting/logging
seam, but its fields are insufficient and it is currently emitted by the daemon
process, whose stderr is redirected to `daemon.log`. The client must emit the
context line. If structured perf logging is retained, add the same config source,
repo count, daemon pid/action, served root, and database destination as fields.

### Making daemon hijack visible

Do not infer a daemon database using the client's `XDG_STATE_HOME`: the running
daemon may have been spawned under a different environment. Extend the initial
ping/register/query handshake so the server returns route metadata:

```text
pid, build_id, config_source, config_repo_count,
served_root, served_root_key, db_path
```

`ensure_singleton()` should return whether it reused an existing daemon or spawned
one. After lazy registration, the first routed response must contain the selected
`ServedRoot` metadata. The client then emits `(attached)` or `(spawned)` accurately
before rendering results. Attach failure followed by in-process fallback emits a
normal failure line as today and then the in-process context banner; it must never
leave an earlier “execution=daemon” claim in the log.

This also exposes the currently surprising positional behavior: if a one-file
command is routed to the daemon's discovered program set, the banner's route and
served root are visible. A later tightening may refuse that mismatch, but it is
outside this item.

## Four-layer planning protocol

### 1. Type signatures

Proposed shapes (names illustrative; policy should be typed rather than encoded
by mutating process environment):

```rust
enum ConfigPolicy { Ambient, Disabled }
enum DaemonPolicy { Auto, Disabled }
enum DbPolicy { Explicit(PathBuf), ImplicitRootCache, Memory }

struct RunPolicy {
    config: ConfigPolicy,
    daemon: DaemonPolicy,
    db: DbPolicy,
}

enum ConfigSource {
    Disabled,
    Missing { candidate: Option<PathBuf> },
    File { path: PathBuf, repo_count: usize },
}

enum ExecutionRoute {
    InProcess { root: PathBuf },
    Daemon { pid: u32, action: AttachAction, root: PathBuf,
             root_key: String, db_path: PathBuf },
}

enum AttachAction { Attached, Spawned }

fn resolve_config(policy: ConfigPolicy) -> Result<(SprfConfig, ConfigSource)>;
fn resolve_run_policy(cli: &Cli, root: &Path, programs: &[String]) -> RunPolicy;
fn ensure_singleton() -> Result<(AttachAction, DaemonIdentity)>;
fn emit_context_banner(config: &ConfigSource, route: &ExecutionRoute,
                       db: &DbPolicy);
```

`SprfConfig::load_default()` may remain as an ambient convenience wrapper, but
engine construction should consume the resolved config and source rather than
re-resolving global environment at multiple call sites.

### 2. Pseudo-code

```text
parse CLI, including --hermetic
resolve root and expand programs

policy = if hermetic:
           { config: Disabled,
             daemon: Disabled,
             db: explicit --db or Memory }
         else:
           existing config/daemon/db policy

if parse-only:
  parse/typecheck and return                # no config resolution, no banner

(repos, config_source) = resolve_config(policy.config)

if policy.daemon == Auto and daemon gate passes:
  (action, daemon_identity) = ensure daemon
  response = route/register root
  route = Daemon(response.pid, action, response.root, response.key,
                 response.db_path)
  config_source = response.config_source    # server truth wins
  emit banner(config_source, route, response.db_path)
  execute RPC
else:
  db = open(explicit path or root cache or memory)
  route = InProcess(root)
  emit banner(config_source, route, db)
  construct Engine with already-resolved repos
  execute locally
```

### 3. Instance lifetimes

- `RunPolicy`, `ConfigSource`, and `ExecutionRoute` live for one CLI invocation.
- The resolved `SprfConfig` is loaded once per in-process engine construction and
  borrowed/moved into that engine; watchers replace it on an observed config
  change only when policy is `Ambient`. A disabled policy installs no config
  watch.
- `DaemonIdentity` is process-wide on the server. Each `ServedRoot` owns immutable
  identity (`canonical root`, `key`, `db_path`) for its registration lifetime;
  its repo set may reload when the daemon's selected config file changes.
- The banner is invocation-scoped. A long-running LSP/MCP/watch session prints it
  once at startup, not once per tick or request.
- No policy is stored in a global `OnceLock`; tests must construct multiple policy
  values safely in one process.

### 4. Storage, reads, and writes

Normal ambient mode reads the selected config and selected roots as today. It may
write `.dl/cache.db` plus `.dl/.gitignore`, or daemon control/state files and the
served-root DB under the daemon home. The banner reports the engine-state
destination before use.

Hermetic mode performs no config-file stat/read, no daemon socket or pid/control
file read, no daemon-home write, and no `.dl/cache.db`/`.gitignore` write. Its
implicit SQLite database is memory-only. If `--db PATH` is explicit, that path and
SQLite sidecars are the only engine-state files it may write. Root/program/source
reads continue. Explicit program effects retain their existing reads/writes.

The banner itself writes one stderr line. Structured logging must not create a
root-local `perf.jsonl` merely to record the banner; it may append only when that
log was already explicitly enabled under existing policy.

## Migration and compatibility

- `--no-daemon` and `DL_NO_DAEMON=1` remain supported. Documentation can prefer
  `--hermetic` for tests and ad-hoc experiments, while `--no-daemon` remains the
  narrow “socket is wedged / run locally with normal config/cache” control.
- `SPREFA_CONFIG=/nonexistent` remains a valid way to select an empty config. The
  new typed disabled policy avoids inventing a platform-sensitive sentinel path.
- Existing automation that asserts exact stderr will see a new line only when it
  was exposed to ambient config, daemon routing, implicit persistence, or opts
  into hermetic confirmation. Update fixtures deliberately; stdout contracts do
  not change.
- The strict option makes `dl --hermetic` discovery runs cold unless `--db` is
  supplied. This is intentional. Reproducibility outranks warm-cache speed for an
  explicitly hermetic run.
- An explicit `--db` continues to opt out of daemon use. Under `--hermetic`, it is
  also the only persistent engine-state destination.
- Daemons predating the metadata RPC cannot prove their destination. Treat a
  missing metadata field as an attach incompatibility and fall back in-process;
  do not print guessed paths. The existing build-id restart path should normally
  upgrade them automatically.
- The banner must not be localized or colorized initially; stable plain text is
  more useful in agent logs and tests.

## Verification

Add focused tests before implementation is considered complete:

- config precedence: explicit env only; XDG before HOME; disabled performs no
  fallback and reports `Disabled`;
- clap/policy table: hermetic plus absent DB yields memory, hermetic plus explicit
  DB honors it, `--no-daemon` alone preserves ambient config/cache;
- filesystem sentinel test: hermetic discovery creates neither `cache.db*` nor
  `.dl/.gitignore` and never touches a sandbox daemon home;
- hostile-environment test: set HOME/XDG config to a real multi-repo config and
  expose a live fake daemon socket; hermetic still sees zero configured repos and
  makes no connection;
- daemon route test: attach and spawn cases report server pid, canonical root,
  server config source, and exact served-root DB on one stderr line;
- fallback test: failed daemon attach prints no false daemon context and the final
  banner says in-process;
- stdout snapshots for query JSON, diag JSON, hook, MCP, and LSP handshake remain
  byte-compatible;
- one-line invariant: banner contains no newline-bearing user value and prints
  once per long-lived invocation.

Implementation gate: targeted unit/integration tests first, then at most two full
integration-suite runs as required by the feedback queue. Test commands themselves
must use `--hermetic` once available (or the current three-part incantation while
bootstrapping).

## Staffing

- Implementation: one high-reasoning agent in a dedicated worktree, because the
  policy crosses CLI pre-dispatch, config ownership, daemon RPC identity, and
  several output adapters.
- Base: `07030b174f68372289e793b23d73de1f32d83ed1` or a freshly rebased descendant
  after this design is signed off.
- Current arc: proposal only; no source changes and no runtime-suite budget used.
- Implementation suite budget: focused tests freely; full integration suite at
  most twice.

## Sign-off needed

- [ ] Chris chooses strict option 1: config off, daemon off, implicit DB in memory.
- [ ] Chris confirms that explicit `--db PATH` remains legal under `--hermetic`.
- [ ] Chris approves the exact `[dl context] ...` stderr wording and that
      `DL_LOG=quiet` does not suppress it.
- [ ] Chris confirms the banner trigger: risk-present normal runs plus every
      hermetic run, not every harmless in-memory run.
- [ ] Chris chooses whether `what`/`summary`/`q` ship with `--hermetic` in the
      first implementation and agrees that daemon-control subcommands reject it.
- [ ] Chris confirms that hermeticity does not imply read-only effects or disable
      explicitly requested `--apply`/`--fix`/`--verify` behavior.

