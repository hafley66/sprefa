# Changelog

All notable changes to `dl` (sprefa v5) are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions track the `v*` release
tags consumed by cargo-dist.

## [Unreleased]

## [0.6.14] - 2026-07-08

### Added
- **`norm(str)` string builtin** — normalize for comparison (keep ASCII
  alphanumerics, lowercase, drop the rest; the same fold as the
  `string(id,text,norm)` rel's `norm` column). `nx = norm(a), nx = norm(b)` is a
  punctuation/case-blind compare, and arbitrary text joins against `string.norm`.
- **VS Code extension activates on startup** (`onStartupFinished`), so the LSP
  and the daemon it attaches to serve a workspace that has no `.dl` file yet
  (pure source repos, background indexing from the moment the window opens).
  Empty windows still no-op via the existing no-folders guard.

### Changed
- **`resolve_rev` fetches a missing rev on demand instead of bailing.** A scanned
  `repo`/`rev` whose object is absent locally — an unknown ref/name, or a pinned
  full SHA not in the object db — now triggers `git fetch origin <rev>` and one
  re-resolve rather than failing the tick. Offline mode (`DL_NO_FETCH=1`) skips
  the network and throws instead. Present revs are untouched: the sha returned
  and the `rev_cache` behavior are identical; only the miss path is new.

## [0.6.13] - 2026-07-07

### Changed
- **Flow panel canvas view now renders on a real `<canvas>` via cytoscape.** The
  hand-rolled renderer built one DOM card per node plus one SVG per edge and ran
  its own layered layout, which hung or crashed the webview at high volume
  (custom queries and the graph-layer UNIONs out-run the presets' `LIMIT`s, and
  the multi-repo db is much larger). Cytoscape (vendored into `media/`, loaded
  before the panel script — CSP blocks CDNs) owns layout, pan, zoom, and edges on
  canvas; the query rows map straight to cy elements. Two volume guards added
  regardless of view: the `dl/query` postMessage caps at 20000 rows, and
  `render()` slices to 2000 nodes / 4000 edges with a warn pill showing the true
  total. Canvas mode drops the per-node DOM affordances that lived on the old
  cards (member pins, hover cards, mark highlight, follow-cursor centering,
  marquee select, flip arrows); **list view keeps all of them**.

## [0.6.12] - 2026-07-07

### Added
- **Multi-root workspaces: one daemon, one database, every open folder.** The VS
  Code extension writes the open workspace folders as `[[repos]]` into a
  per-workspace `$SPREFA_CONFIG`, so a single dl engine (and its single
  `cache.db`) serves them all instead of only `folders[0]`. The flow panel spans
  every repo (nodes prefixed by folder); jump-to-disk, follow-cursor, marks, and
  type-seed resolve to the folder that OWNS each file. A git-excluded
  `_workspace-scan.dl` dropped into the primary folder's `.dl/` fans extraction
  over all repos (`scan("*")`) beside the existing rails, and both the LSP and
  the daemon it attaches to discover the same set — one shared database, no
  clobber. The LSP engine now loads the same repo set as the daemon
  (`set_repos`), and `load_repos_eager` is reused for that. Single-folder
  workspaces (empty config) are byte-for-byte unchanged. (Connected cross-repo
  *flow edges* — a call in repo A resolving into repo B — remain a follow-up: the
  resolver deliberately repo-scopes today.)

## [0.6.11] - 2026-07-07

### Changed

- The VS Code extension and the `dl` binary now ship as ONE version. Cargo.toml
  is the single source of truth: `scripts/build-vsix.sh` stamps the extension's
  `package.json` to the crate version and rebuilds the VSIX at a FIXED filename
  (`editors/vscode-dl/dl-lsp.vsix`, no version in it), so `src/setup.rs`'s
  `include_bytes!` never changes per release. `build.rs` refuses to compile if
  the two versions drift, and `.dl/vsix-version-drift.dl` is the same guard for
  `dl --check` / the LSP. No more hand-bumping `package.json` + re-embedding.
- Flow panel empty-state now tells you what to do instead of going blank:
  seed-driven presets say "press `cmd+alt+t` / follow cursor", scan presets name
  the SCIP index (`dl index`), and derived presets name the missing rel. Dropped
  the stale `dl --daemon --root .` hint (`--root` was removed).

## [0.6.10] - 2026-07-07

Consolidated release: builds the prebuilt binaries for all the 0.6.6–0.6.9 work
(diag markdown hover + graph sinks + panel fixes, the `--root` removal, the
`--load`/zero-match/config-warn trio, and use-yell) plus the CI dogfood fix.
This is the version `dl update` fetches.

## [0.6.9] - 2026-07-07

### Fixed
- **An unresolvable `use` no longer crashes the LSP server (or aborts the
  load).** A `use "missing.dl"` that resolves on no disk root and has no
  embedded-std fallback now emits a `use-unresolved` **diagnostic** at the
  `use` line and skips that import, so the rest of the program still loads —
  the LSP stays up and squiggles the bad line instead of dying, and `--check`
  reports it (exit non-zero) with the roots it tried. Downstream unknown-rel
  diagnostics from the missing import fire too, which is intended and more
  informative than a single opaque bail.

## [0.6.8] - 2026-07-07

### Fixed
- **A bad `dl --load` no longer wedges the daemon.** A watched load that fails
  to reload (parse/type error) rolls the file back out of the program set, so
  the daemon keeps ticking on its last-good program and a subsequent good load
  still succeeds. A deleted watched program file is skipped on reload (parse the
  files that still exist) instead of failing every tick.
- **Zero-match scan warning is quieter for expected-empty shapes.** A polyglot
  rel headed by several scans (one per language) no longer warns about the empty
  globs when a sibling glob matched (`seen` scanning both Rust and `{ts,tsx}` in
  a Rust-only repo went silent); a scan whose rel feeds a downstream rule
  (consumed) gets a one-line note instead of the loud fix-it text. Only a
  genuinely dead scan (unmatched, no sibling, unread) still gets the full
  warning — now worded for the cwd root (there is no `--root`).

### Added
- **Unknown config keys warn instead of silently vanishing.** A typo'd or
  renamed key in `config.toml` (`folder` vs `foldername`, a misspelled
  `[[repos]]`/`[[org]]` field) now prints a `[config] unknown key` line naming
  the table and key, rather than deserializing to the default and being ignored.

## [0.6.7] - 2026-07-07

### Changed
- **`--root` is gone. `dl` is a daemon over a repo SET, not a rooted tool.**
  The CLI no longer has a `--root` flag. The working root is the current
  directory: a client (the vscode extension, a test harness, a shell) points
  `dl` at a folder by spawning it with that `cwd`.
  - `dl --daemon` / `--stop` / `--load` / `--settle` address the **rootless
    singleton** at the XDG state home, which serves the config repo set (static
    `[[repos]]` / `[[org]]` allowlist) plus dynamic runtime adds (the `repo`
    sink). No privileged self repo — `df_node`/`type`/`call`/`doc` lift every
    repo in view, not just one root.
  - One-shots (`dl prog.dl`, `--check`, `--lsp`, `--move`) resolve the root
    from cwd; the program path itself may live anywhere.
  - The per-repo auto-attach daemon a one-shot spawns learns its root from the
    internal `DL_DAEMON_ROOT` env (+ cwd), never a user-facing flag.
  - The VS Code extension spawns the LSP server with `cwd` = the workspace
    folder instead of passing `--root`.

## [0.6.6] - 2026-07-07

### Added
- **`graph_node` / `graph_edge` builtin sinks** (group `graph`): head-writable
  builtin rels (mirror the `diag`/`repo` pattern) for drawing a graph in the
  flow panel without bespoke per-preset SQL. `graph_node(id, label, kind, file,
  line, parent)` and `graph_edge(src, dst, kind)`; the tables always exist
  (empty until a rule heads them), so the panel's "Graph (node/edge sink)"
  preset is always available. `.dl/git-graph.dl` migrated to the named-head
  form; `examples/madge.dl`'s 1-ary module node renamed `graph_node` ->
  `mod_node` to free the reserved name.
- **diag messages render markdown on hover**: the VS Code extension registers a
  HoverProvider that re-renders any dl-sourced diagnostic overlapping the cursor
  as a MarkdownString (the LSP `Diagnostic.message` field is plain-text-only).
  `.dl` programs can now write markdown (links, `code`, lists) into diag's
  msg/hint and it renders in the editor hover.

### Fixed
- **Flow panel no longer shows a red wall of text** when a preset's `rel_*`
  tables are missing: `run()` renders an empty graph plus a one-line grey note
  for any `no such table: rel_X`, `updatePresetAvailability()` disables presets
  whose tables are absent (labelled " - needs .dl"), and an empty-state overlay
  names the daemon/data cause. The `madge` preset synthesizes nodes from builtin
  `rel_module_edge` endpoints so it works on a bare scan.
- **VS Code extension is no longer blank under a GUI-launched editor**: probe
  `~/.cargo/bin` (and Homebrew paths) for the `dl` binary, augment the server's
  spawn PATH, and surface an actionable error with an "Open Settings" action
  when the binary is missing (GUI editors don't inherit the shell PATH).

## [0.6.5] - 2026-07-06

### Fixed
- **daemon.log no longer floods**. Three per-tick spam sources were writing to
  the log on every reactive wake-up (observed 3.9 MB / 118k lines):
  (1) the tick re-rendered every `?` query table to stdout on each tick — now
  suppressed on quiet (reactive) ticks; the RPC `query` capture is the daemon's
  read path (foreground `dl prog.dl` / `--watch` still print). The daemon's
  reactive tick calls now pass `quiet=true`, so the `[tick]` telemetry line is
  suppressed too. (2) `load_repos_eager` printed `[config] N repo(s)
  registered` on every call, and it is called from `on_git_event` (every `.git`
  change) — silenced; the cold-serve path announces the count once. (3) the
  `[daemon] git change` line logged even for pure metadata churn (`0 refs
  advanced, 0 files`) — now only logs a real advance/diff. Plus a backstop:
  a respawn starts the log fresh once it exceeds 8 MB.

## [0.6.4] - 2026-07-06

### Added
- **Self-contained flow-panel presets**: "Call graph (all)" and "Data flow
  (all)" query only PREBAKED builtin rels (`rel_call_def` / `rel_call_name` /
  `rel_call_edge`, `rel_df_node` / `rel_df_edge`), so they render on a bare
  `scan` with no derived `.dl`. The older flow-oriented presets still depend on
  their program's derived `rel_*` tables.
- **"Follow cursor" toggle** in the flow panel: with it on, moving the editor
  caret highlights and centers the graph/list node at the cursor's file:line
  (falls back to the word under the caret). The extension posts a debounced
  selection event to the webview; the panel acts on it only when the box is
  checked. Purely visual — no pin, no re-query. vsix bumped to 0.4.7.

## [0.6.3] - 2026-07-06

### Added
- **JSX/TSX prop-value dataflow** now chases the expression shapes that
  previously dead-ended at an unlinked node, so `df_field` prop values flow in
  both diet-SCIP and SCIP modes: conditional (`ok ? a : b` — both branches),
  logical (`a && b` flows the value side, `a || b` / `a ?? b` flow both;
  the `&&` guard is excluded), parenthesized, template + tagged-template
  interpolations, optional chaining (`obj?.title`), arrays (`[a, b, ...rest]`),
  sequence, assignment value, and the transparent TS casts (`as` / `satisfies`
  / `<T>x` / `x!` / `f<T>` / `await`). `UnaryExpression` is a deliberate
  non-flow (a `!x` value is a fresh boolean). The call/member arms were
  factored into `ts_flow_call` / `ts_flow_member` so optional-chained calls and
  members reuse the exact positional-`df_arg` and member-name logic.
- **Flow-panel saved queries**: name any node/edge SQL and store it locally;
  saved queries appear under a "Saved" group in the preset dropdown and render
  in both the graph and list views (same node/edge path as the built-in
  presets). vsix bumped to 0.4.6.

### Fixed
- **`.dl` discovery walks the ancestry**: a subdir with no `.dl/` inherits the
  nearest ancestor's chain, and every `.dl/` up to (and not past) the git root
  merges into one program. Unblocks `--lsp` when the editor opens a nested
  folder whose parent holds the `.dl/`. No new flag.

## [0.6.2] - 2026-07-06

### Added
- **`[[org]] foldername`** overrides the slug prefix (default: the `dir`
  basename). Set `foldername = "."` to FLATTEN — drop the org prefix so a repo
  addresses by its bare path under `dir` (`~/projects/my-long-ass-org-name/repo-a`
  → slug `repo-a`, not `my-long-ass-org-name/repo-a`). Only the slug flattens;
  the on-disk path is unchanged. Flattening can collide same-named repos across
  subfolders — the caller's call.

## [0.6.1] - 2026-07-06

### Changed
- **Magic-rel pattern eliminated: demand/overlay conventions are now first-class
  builtin sinks.** `scip_want`, `rev_cmp_want`, `def_target`, and `effect_cmd`
  used to be relations the engine read back by a hardcoded string name with
  nothing in `rel_catalog` advertising them — an invisible API. They are now
  pre-declared, catalogued builtin **sinks** (group `demand`), head-written from
  a rule exactly like `diag`/`repo`. Head them directly; a `rel scip_want(...)`
  declaration now bails ("head it directly, like diag/repo"). They appear in
  `dl docs relations` and `docs/reference/magic-rels.md`.

### Added
- **Magic-rel ban rail (`.dl/magic-rel-audit.dl`).** Dogfood check: scans the
  engine's own `src/**/*.rs` for any `rels.get("<name>")` / `FROM rel_<name>`
  literal and fails `dl --check` (exit 2) if the name is not a catalogued
  relation. Runs in CI's bare `dl --check` and the PostToolUse hook, so the set
  of name-matched relations can only shrink or become catalogued, never silently
  grow. Regression test `tests/it/magic_rel_audit.rs`; maintainer skill
  `assets/sprefa-v5-no-magic-rels.skill.md`.

## [0.6.0] - 2026-07-06

### Added
- **`dl --settle` — run a program to a fixpoint.** A plain one-shot ticks once,
  which leaves effectful (`@async`/`sh`/`sh*`), demand-tier (`scip_want`), and
  `repo`-sink programs half-run — their requests stuck queued, their demanded
  rows absent. `--settle` drives tick + off-tick effect drain in-process until
  the program is quiescent (no non-timer rel moved, no `@next` carry staged, no
  non-stream effect in-flight), then prints `?` once. It is the first non-daemon
  path that runs the effect runtime. `--settle-max N` (default 200) bounds it; a
  non-converging program bails loudly naming the still-moving rels/effects
  instead of hanging. Recurring timers (`every`/`clock`/`@stream`) are steady
  state and excluded, so a poller still settles at a quiet point.
- **`dl --await-settle` + `await_quiescent` RPC.** The daemon-side twin: block on
  a running daemon until its poll loop reaches the same quiescent state (exit 0
  settled / 3 timed out). `ping` gains a `settled` field.
- **`[[org]] dir=` multi-root config.** Point at a folder of checkouts and every
  git repo under it expands, at load, into a `[[repos]]` entry (slug
  `<dir-basename>/<path-under-dir>`, descent stops at each `.git`, an explicit
  `[[repos]]` at the same root wins). `max_depth` (default 3) caps the walk; a
  leading `~` in `dir` expands to `$HOME`. The declarative multi-root shape,
  usable from one-shot / `--check` with no daemon — the single way to point `dl`
  at an org-of-repos folder.

### Changed
- Documented the effect/settle model in `docs/daemon.md` (a "Running effectful
  programs to completion" section) and `book/tutorial/12`.

### Fixed
- Two stale examples (`propose_demo`, `kernel_compare`) called the 1-arg
  `scip_import::load`; updated to the 3-arg `(path, root, slug)` form.

## [0.5.0] - 2026-07-06

### Added
- **Learner GitBook** (`book/`, `.gitbook.yaml`). A `quickstart` (install → query
  → CI gate against your own repo), a hands-on `tutorial` track (setup through a
  server-made-of-rules, one lesson at a time, every transcript a real capture),
  the existing theory + math tracks, and `what-if` essays (rendering HTML trees
  from relations, the exits from stratification, bridging to v0's nested-block
  DSL). Every list — `SUMMARY.md` zones, both track READMEs, `dl docs` indexes —
  is spliced from one scan of `book/` by `gen-doc-indexes.dl`, with `--check`
  drift rails.
- **Turnkey VSCode extension install.** `dl setup --vscode` now builds a fresh
  VSIX from `editors/vscode-dl` when run in a checkout (always current), falling
  back to the VSIX embedded at build time for a prebuilt `dl`. It installs
  uninstall-first to dodge the same-version reinstall no-op. A new `.dl/`
  drift rail (`vsix-version-drift`) fails `dl --check` if the embedded VSIX
  version and `editors/vscode-dl/package.json` disagree — the coupling that
  silently rotted the embedded VSIX to 0.3.0.
- **VSCode "Add Type Seed"** command + keybinding (extension 0.4.4).

### Changed
- **File-watcher scaling.** The daemon watcher now mirrors the scan corpus: a
  shared `WatchGate` drops gitignored build output and `.git/objects` churn the
  engine would never scan, watching `.git` only at the narrow
  `HEAD`/`packed-refs`/`refs/` ref paths. Bursts coalesce through a quiet-period
  debounce (was a fixed 150 ms drain); a dropped/overflowed event forces a loud
  full-corpus recovery tick; and the idle timer resets only on events that
  survive the gate, so a repo under pure build churn can finally idle out.

### Fixed
- **Deep-root daemon could not bind.** `<root>/.dl/daemon.sock` for a deeply
  nested root overran the macOS `sun_path` cap (104 bytes), so `bind` failed and
  every invocation fell back to in-process after the attach timeout. The socket
  now relocates to a short hashed path under `$TMPDIR/dl-sock/` when the natural
  path is too long; bind and every connect derive it from the same root.

## [0.4.4] - 2026-07-05

### Fixed
- **Daemon self-write tick loop.** The watcher watches the scan root
  recursively, but the daemon writes its own bookkeeping there every tick
  (`.dl/cache.db*` sqlite WAL, `.dl/daemon.log` stderr redirect). Those writes
  re-fired the watcher and re-ticked forever — a no-op "files 0/0 parsed" loop
  that also kept resetting the idle timer, so the daemon never idled out (seen
  as a daemon pinned at high CPU doing nothing). The watcher now drops its own
  bookkeeping paths (`is_daemon_internal`) from each batch; a batch that is
  entirely self-writes is skipped before it can tick or reset idle. Program
  files (`.dl`, `marks.dl`) are unaffected and still trigger reloads.
- **Rebuilt/reinstalled `dl` attached to a stale daemon.** `ensure_daemon` only
  checked that a socket answered `ping`, so a freshly built binary attached to
  the old daemon and the new code never ran. The daemon now reports a
  `build_id` (crate version + exe mtime) captured at startup; the client
  respawns on mismatch, attaches on match, and leaves a pre-`build_id` daemon
  alone.

## [0.4.3] - 2026-07-05

### Fixed
- **VS Code extension: LSP client failed to start with `command
  'dl.toggleDiagCode' already exists`.** v0.4.2 registered the diag-mute
  quick-pick under the same id the server advertises in
  `executeCommandProvider`, so vscode-languageclient's auto-registration
  collided, initialize failed, and every flow-panel query returned "Client is
  not running". The palette command is now `dl.pickDiagCode`; the server-side
  `dl.toggleDiagCode` / `dl.listDiagCodes` executeCommand ids are unchanged.
  Extension 0.4.3 (`dl-lsp-0.4.3.vsix`); no engine changes.

## [0.4.2] - 2026-07-04

### Added
- **Rev-aware extraction (the diff spine).** Per-rev `extract:<family>:<rev>`
  digests; `type_entity_rev`/`type_link_rev`/`call_def_rev` twins (rev is a
  column, syms stable cross-rev) and df twins with rev-salted ids; a vanished
  rev retracts from all twins the same tick. `diff_pair(base_rev, head_rev)`
  drives `.dl/graph-diff.dl` on ONE checkout (shipped inert as
  `("WORK","WORK")`); `examples/pr-diff.dl` diffs a PR via `gh` -> shas -> scan
  rev slots.
- **`hook_event` seam + chat-marks.** Built-in
  `hook_event(kind, session, seq, json)` fed by `dl --hook` (daemon RPC with
  in-process fallback); `dl setup` registers UserPromptSubmit + PostToolUse.
  `examples/chat-marks.dl` sections chat logs on an `@@mark <title>` phrase —
  the phrase lives in the .dl program, never the engine.
- **CLI discovery + learning surfaces.** Grouped `--help` with SUBCOMMANDS /
  LEARN MORE / AUTHORING trailers; `dl docs` embeds the reference, the book
  (now 9 chapters incl. argmax), a hands-on 9-lesson tutorial, and the
  authoring skill; doc indexes are generated by `examples/gen-doc-indexes.dl`
  with a drift rail.
- **Authoring sharp edges closed.** `--parse-only` no-scan validate (parse +
  typecheck + metavar sanity + every regex literal compiled — lookahead
  fast-fails sub-second); `lowercase-metavar` warn lint; head-var-not-bound /
  unbound-constraint / regex errors now name the fix; per-op language matrix
  kept honest by `tests/it/lang_matrix.rs`.
- **23 `sg` grammars + term-form `sg`.** css, html, bash, csharp, java, scala,
  swift, ruby, php, lua, elixir, haskell, yaml join the table;
  `sg(:lang, bound_str, "pattern")` matches over a bound string for
  embedded-language rules (styled-components, markdown fences).
- **`comment_node` + `std/suppress.dl`.** Grammar-backed comment relation
  (line/block/doc, inline included, string-literal-safe); the
  eslint/biome-style disable grammar (`dl-disable-line`/`-next-line`/block
  pairs, code scoping, `-- reason`) written entirely in dl, with directive
  visibility diags (`dl-directive` info dots, malformed + unused warns).
- **`diag_mute` + editor toggle.** Writable `diag_mute(code)` builtin; LSP
  executeCommand `dl.toggleDiagCode`/`dl.listDiagCodes`; the filter sits at
  the publish seam only, so `--check`/`--parse-only` are unaffected. VS Code
  extension 0.4.2 ships the quick-pick command + `cmd+alt+d cmd+alt+d` chord.
- `examples/endpoint-flows.dl` (axum route -> call-graph reach -> hover shows
  "in endpoint flows: GET /users") and a presenterm slide deck under `deck/`.

### Fixed
- **Scope-correctness sweep.** Resolver double-registration made a whole repo
  resolve bare; SCIP importer collapsed multiple indexes across roots
  (`scip_def`/`scip_ref`/`scip_edge` gain a `repo` column); the dataflow
  family read config-repo files at the wrong root; cross-file `impl` parents
  now resolve to the declaring file; lattice `key(...)`/`merge(...)` edits
  with identical columns no longer wedge every tick on a stale primary key.
- **NULL-padded heads in recursive rules now refuse instead of hanging.** A `_`
  head slot (explicit, or the named-arg padding v0.4.0 introduced) lowers to SQL
  NULL, and NULL rows never dedup in the fixpoint delta (`NULL != NULL` under
  `INSERT OR IGNORE`) — a recursive rule like `n(a: y) <- n(y, _).` re-inserted
  the same row every iteration forever (measured: 2^24 rows, 422 MB, still
  climbing at kill). Two guards: typecheck emits `recursive-null-pad`
  (`--check`/LSP), and `rebuild_derived` bails before entering the fixpoint loop
  as the runtime defense. Non-recursive padded sinks (the `diag` shape) are
  untouched.

## [0.4.1] - 2026-07-02

### Fixed
- **Marking a selection no longer kills the daemon.** A discovery-mode daemon
  (serving `<root>/.dl/*.dl`) treated any content edit to an already-discovered
  program file as exit-for-respawn — but a discovery daemon has no positional
  args to respawn from, so the VS Code extension's mark command (one appended
  fact line in `.dl/marks.dl`) left it dead until the next `dl` client happened
  to run. A discovery daemon now hot-reloads the edit in place (re-parse, swap,
  re-tick) and keeps serving. Explicit-program daemons keep exit-for-respawn.
- **Flow panel "Module graph" preset now populates on any discovery daemon.**
  The preset read rels derived only by `examples/madge.dl` (`rel_dep`,
  `rel_cycle_member`), so it errored unless that example had run against the
  db — while silently reading `rel_seen` rows from flow-panel.dl's unrelated
  `seen` rel. It now reads `module_node`/`module_edge`, derived in
  `.dl/flow-panel.dl` from the engine's built-in module graph (cycle detection
  via a recursive reach rule). The panel's error banner also explains a
  `no such table: rel_*` failure as a .dl program the daemon hasn't loaded.

## [0.4.0] - 2026-07-02

### Changed
- **`diag` is now a fixed-schema built-in relation, not a magic name.** It was
  a user-declared rel whose columns the engine mapped BY NAME at read time —
  which meant every rail file carried its own `rel diag(...)` decl, and the
  merged `.dl/` discovery namespace collided the moment two files declared it
  with different columns. `diag` is now engine-declared with a fixed 9-col
  schema `(path, line, col, end_line, end_col, severity, code, msg, hint)`,
  reserved like every other built-in (a `rel diag(...)` decl is now an error
  pointing you at the sink form). `path` is TEXT so a synthetic origin
  (`"(engine)"`, `"(checked-notes)"`) is not file-checked away. No compatibility
  fallback — every example and test writes the built-in directly. **Migration:
  drop the `rel diag(...)` line and write only the columns you use** (see below).

### Added
- **Named args in rule heads.** `diag(path: p, line: l, msg: m) <- ...` names
  only the columns a rule writes; every unnamed column pads to `NULL` (the
  reader defaults it — severity `warn`, `end_line = line`, ints `0`). Works for
  any rel head, not just `diag`. A head can't mix named args with an aggregate
  call (the two shapes are incompatible).
- **Bare-name shorthand with no anchor.** A fully-positional atom whose terms
  are all Vars naming columns, and which has fewer terms than the rel has
  columns, resolves as all-puns — `diag(path, line, msg)` ==
  `diag(path: path, line: line, msg: msg)`, the JS `{a, b}` / Rust `Foo { c }`
  struct shorthand. It only fires when the atom would otherwise be an arity
  error, so a genuinely positional atom (term count == arity) is never
  reinterpreted and existing programs are untouched. `? diag(path, line, msg)`
  just works.
- **Positional literals mixed with named args (Python-style prefix).** In named
  mode, binding follows one rule: a term that carries a name binds by name (a
  bare var puns, interleavable in any order — unlike Python), a nameless literal
  fills the next column left open by the named + pun args, in declaration order.
  So `diag("synth.rs", 1, severity: "error")` puts `"synth.rs"` in `path` and `1`
  in `line` without spelling those column names. Over-filling (more literals than
  open columns) is a clear error. (Previously a bare literal alongside named args
  was rejected as ambiguous.)
- **`ast::Value::Null`** — the value model gains a null so a padded head column
  round-trips to SQL `NULL` through both the derived (SQL) and source-rule
  (Rust) head-projection paths.

## [0.3.0] - 2026-07-02

### Added
- **`std/flow.dl` — the shared value-flow base as a `use` module** — the
  lines every flow program copy-pasted (the `call_edge_bare` sym-space
  bridge, the `flow_edge` union of `df_edge` + the interprocedural hops,
  the `call_node` call-site name join) now live in one importable std lib;
  `examples/flow-interproc.dl`, `examples/taint.dl`, and
  `examples/flow-jsx.dl` are rebased on `use "std/flow.dl".` and keep only
  their own layers. New surfaces riding it:
  - **`flow_summary(callee, pos)` / `flow_sanitizer(callee)`** — user-asserted
    propagation MODELS for callees the lift can't see into (the
    CodeQL-models move as plain facts). The lift's default is maximal:
    every argument gets a blanket edge into the call result; a summary
    overrides that for its callee, keeping ONLY the summarized slots
    (`flow_sanitizer` = the zero-slot instance: nothing flows). Stratified
    cut via `flow_cut`; free when no facts are asserted.
  - **`call_target(call, caller, callee, callee_q)`** — per-CALL-SITE
    resolution: each call node tied to the defs carrying its own callee
    name (`call_node` ⨝ `call_edge_bare` ⨝ `call_name`). Both
    interprocedural hops now ride it, so `f(secret); g(benign)` in one
    caller no longer cross-talks (the old per-caller hop leaked every arg
    into every callee of the caller, and every callee's return into every
    call result). Factored as its own rel for the planner too: the inlined
    7-atom forward hop measured ~7s per tick on this repo, the factored
    shape ~0.5s for the whole graph.
  - **`arg_field_flow(value, field, call, target)`** — the JSX `prop_edge`
    pattern generalized to plain calls: a value stored into field F of a
    composite passed as an argument reaches the resolved callee's reads of
    the SAME field name (member reads and TS destructured-param pieces).
  - **`flow_lambda(callee, lam_pos, src_pos, param_pos)` /
    `flow_lambda_ret(callee, lam_pos)`** — higher-order propagation facts:
    how a callee invokes a lambda it receives (element hop + result hop).
    `std/flow-collections.dl` ships facts for the common combinator names
    (map/filter/forEach/fold/reduce/...), language-blind by name equality.
  Tests: `tests/it/flow_std.rs` (summary cut, sanitizer, field view,
  fact-driven collection hops per language, per-call-site cross-talk gates
  per language).
- **Inline lambdas lift as their own fn scopes** — Rust `|x| ..` closures,
  TS inline arrows / function expressions, and Kotlin `{ it + 1 }` /
  `{ x -> .. }` lambda literals (including trailing-lambda call syntax,
  which previously wasn't even an argument) now produce a lifted scope:
  kind `param` nodes with `df_param` slots (Kotlin's implicit `it` at slot
  0), the body walked under a synthetic `<enclosing>::closure::<pos>` sym,
  and a `ret` node fed by the body result. The `closure` VALUE node stays
  in the enclosing fn at the argument position and carries the lifted sym
  in `var` — the join key the flow_lambda hops ride. Rust/Kotlin share the
  enclosing scope so captures still resolve; `nest` still counts a call
  inside a closure inside a loop (loop-fn matching is `::closure::`
  prefix-aware). Tests: 3 typegraph units + the e2e collection gates.
- **`examples/flow-slice.dl` — the value slice of one local / instance** —
  forward ("what does `token` reach?"), backward ("what feeds it?"), and the
  field-accurate reads of a single instantiation, each a seeded recursive
  walk of `std/flow.dl`'s `flow_edge` (the closure-can't-be-read-unpinned
  idiom, cheaper by magic-set). A copy template for slicing on any repo:
  edit the seed's var literal, or seed one exact node id from a `? df_node`
  dump.
- **`examples/flow-services.dl` — the wire hop** — cross-SERVICE value flow
  where no call edge exists: a spec-seeded `service_op` inventory (every
  `operationId` in a scanned `openapi.yaml`; assert `service_op("x").`
  facts for runtime-only topologies), `op_endpoint` (every def carrying an
  operation's name), and two hops unioned into `flow_edge` — client
  argument -> endpoint param (positional) and endpoint return -> client
  call result. The stub and the handler usually SHARE the operation's
  name, so single-def resolution refuses exactly where the wire hop takes
  over. Tests: `tests/it/flow_services.rs` (end-to-end reach through the
  spec + the no-spec negative).

- **JSX dataflow** — `<Card title={t} {...rest}>{kids}</Card>` lifts as what
  it desugars to, `jsx(Card, {title: t, ...rest, children: kids})`: the
  element is a `new` df_node carrying the component/tag name, each
  attribute a `df_field` row (bare boolean prop = lit, spread under `".."`,
  non-text children under the `"children"` pseudo-prop). A component usage
  is also a call SITE (host elements skipped), so `call_edge` resolves
  caller -> Card and `call_name` gives an indexable name handle. TS
  object-destructured params (`function Card({title, count: n})`) now mint
  one param df_node per property — var carries the PROPERTY name (the
  JSX/name-match target), scope binds the LOCAL name, all pieces share the
  slot index (previously destructured params bound NOTHING — every React
  component body was a flow hole). New `examples/flow-jsx.dl`: `jsx_use`
  inventory + the `prop_edge` hop (prop value -> matching destructured
  param or `props.x` member read, `call_name` equality join, no suffix
  test). Tests: 2 typegraph units, `tests/it/flow_jsx.rs` (name-match
  positive + undeclared-prop negative + member-read shape).
- **Positional + constructor dataflow: `df_arg`, `df_field`, `new`/`member`
  nodes** — the intra-procedural lift now records WHICH slot each argument
  feeds (`df_arg(call, pos, arg)`, 0-based, method receiver at -1, aligned
  with `df_param.pos`/`type_sig.pos`) and named flow into composites
  (`df_field(id, field, value)`: Rust struct-literal fields, TS
  object-literal properties, Kotlin named arguments; `".."` for
  spread/functional-update bases). Instantiations are first-class `new`
  df_nodes carrying the constructed type name: Rust struct literals and
  capitalized tuple-struct/variant ctors, TS `new Foo()` and object
  literals, Kotlin capitalized ctor calls. Field reads become `member`
  nodes carrying the accessed name (Rust `Expr::Field` and Kotlin
  navigation previously fell into the `expr` catch-all with NO base edge —
  a real flow hole, now closed); method receivers flow into call results
  in all three languages. `examples/flow-interproc.dl` and
  `examples/taint.dl` upgrade the arg->param hop from positional-blind to
  positional (`df_arg.pos = df_param.pos`); new `examples/flow-ctor.dl`
  demos the instantiation inventory, per-field fills, and field-SENSITIVE
  flow (a value stored into field F reaches a member read of F, and only
  F, via a new-seeded recursive rule — closure rels can't be read unpinned
  in a rule body). `nest` now also counts `new` nodes (a ctor in a loop
  allocates per iteration). Tests: typegraph units per language,
  `tests/it/flow_ctor.rs`, and the position gate in
  `tests/it/flow_interproc.rs` (arg 0 must NOT reach param 1).
- **Per-family extraction skip + per-file fact cache (perf gap A)** — the
  type/call/dataflow/doc refreshers persist an `extract:<family>` input
  digest (corpus (repo, path, rev, content hash) rows + the `scip_ref`
  override + the running binary's identity) and skip the whole
  parse/resolve/write pass on a warm tick; when a file DOES move, an
  in-memory (repo, path, content hash)-keyed fact cache re-parses only it.
  Measured on this repo's flow-interproc program: type/call/dataflow refresh
  183/281/930ms -> ~0.3ms each on the no-change tick, which drops from
  ~1.5s to ~35ms in-engine. `Engine::extract_files_parsed` is the
  instrumentation; `tests/it/extract_cache.rs` pins both the skip and the
  single-file re-parse (including cross-process skip over a warm db).
- **Full-tick scoped rebuild (perf gap B)** — the full `tick` now attributes
  changes per relation (source-rel digests, family refresh results, RelKind
  returns, an `async:` content digest for @async/@stream response rels the
  off-tick drain writes) and rebuilds only the derived rels
  dependency-reachable from what moved — the same `affected_derived` walk
  `tick_paths` uses, now on the full path. A blank slate, a program edit, or
  a carried @next change still rebuilds everything.
  `Engine::last_derived_rebuilt` is the instrumentation;
  `tests/it/scoped_tick.rs` pins the two-chain isolation.
- **Family change reporting (perf gap C)** — `tick_paths` marks a family's
  rels changed only when its input digest actually moved, so an edited `.md`
  under a type-graph program (or an edited `.rs` under a doc-only program)
  no longer re-derives the other family's dependents.
- **`dl setup --project` wires repo-tracked skills** — every
  `assets/*.skill.md` in the target repo gets a gitignored
  `.claude/skills/<name>/SKILL.md` relative symlink (copy on non-unix), so a
  fresh clone of a repo following that convention (this one: the three
  maintainer checklists) exposes its project skills after one setup run.

- **`rel_count(rel, rows)` / `stmt_ms(rel, ms)` telemetry built-ins** — tick
  cardinalities and per-rel derived-statement wall costs as queryable facts
  (`--tick-audit` / `--profile` output, made joinable). Derived rels report
  the previous tick's counts (source-phase refresh); `stmt_ms` is empty until
  a rebuild has landed in the db. Closure-head VIEWS are excluded from the
  counts (counting one materializes the full closure).
- **`examples/perf-rails.dl`** — cardinality-blowup + slow-rule diags over the
  telemetry built-ins with budget facts; merge it beside the program under
  watch.
- **Multi-file one-shot merge** — `dl a.dl b.dl` now merges ALL positionals
  into one program for every one-shot mode (run/check/lsp/hook/mcp/verify/
  changed/watch), as the help text always claimed; previously everything
  after the first file was silently ignored. An explicit multi-file merge
  runs in-process (the daemon serves its own loaded set).
- **`closure-unpinned` lint in `dl_diag`** — a `?` query on a closure head
  with both endpoints free warns with the pin hint (the lint twin of the
  runtime guard below).

- **`git_ref(repo, refname, kind, sha)` built-in** — ref inventory across the
  self repo and every config repo: one row per branch/tag/remote ref plus a
  `("HEAD", "head")` row, annotated tags peeled to their commit.
- **`rev_behind(repo, refname, upstream, behind, ahead)` built-in,
  demand-driven** — derive an ordinary relation named
  `rev_cmp_want(repo, refname, upstream)` and each wanted pair fills with
  behind/ahead commit counts (`ahead > 0` = the ref diverged from upstream).
  One-tick latency, like a data-driven scan. Unresolvable refs skip loudly;
  a SHALLOW clone skips loudly per repo (grafted history makes ancestry
  counts wrong, not just incomplete — `git fetch --unshallow` fixes it).
- **`scip_want(repo)` — lazy multi-repo SCIP.** Derive `scip_want` rows and
  each wanted repo's index is ensured (an existing `index.scip` wins;
  otherwise detected+installed indexers run once to `.dl/index.scip`), then
  the self index and all wanted indexes merge into ONE load — so a
  cross-repo reference resolves its `def_file`. No schema change; monikers
  self-disambiguate.
- **`examples/pin-skew.dl`** — which repos pin an internal dep at a ref the
  dep's main line moved past (stale) or never contained (diverged)? go.mod
  manifest seam -> `pin` -> `rev_cmp_want` -> `stale_pin`/`diverged_pin`;
  bespoke lockfile formats union into `pin` with one rule per format.
- **Seven cross-repo / dataflow recipe examples** on the existing built-ins,
  each with honest-limit headers and validated end to end:
  - `taint.dl` — source/sink/sanitizer preset over the interprocedural flow
    graph; taint propagates recursively, stops at sanitized nodes, reports
    sink hits as `diag`.
  - `route-norm.dl` — client request paths vs declared server routes across
    template dialects (`{id}`/`:id`/`%s`), joined on the punctuation-stripped
    lowercase normal form; `route_hit`/`route_orphan`/`route_dead`.
  - `stale-doc.dl` — a documented declaration whose decl line is in the
    working diff (a pre-commit "confirm the doc is still true" rail).
  - `arch-conformance.dl` — declared layers (path prefixes) + allowed
    dependency arrows vs the real `module_edge` graph; every cross-layer edge
    without an arrow is a `violation`.
  - `version-skew.dl` — one dependency pinned at differing versions across the
    org (min/max witnesses per module, blast-radius by repo count).
  - `phantom-deps.dl` — Go imports covered by no `require` line in any of the
    repo's go.mod files (the transitively-available import that breaks the day
    its provider drops it).
  - `vendored-drift.dl` — a `third_party/` copy vs its upstream config repo by
    content address: `in_sync` / `drift` / `local_only`.

### Changed
- **df_node lines are 1-based in ALL three lifts** — the Kotlin dataflow
  lift normalizes tree-sitter's 0-based rows (+1, loop spans bumped in
  step so `nest` containment is unchanged), and the Rust method-call
  `call_res` node now sits at the METHOD ident's line (where the call-site
  extractor records it) instead of the receiver expression's start, so a
  multiline builder chain still joins. `call_node` (std/flow.dl) is
  therefore ONE equality join; the old dual-offset form (`cl = dl + 1` for
  the 0-based languages) is gone, and with it the false match against a
  call site on the line after an unrelated call.
- **taint.dl findings tighten under the per-call-site pin** — on this repo
  the demo preset drops from 161 findings to 9; the removed rows were the
  per-caller cross-talk (any tainted value in any fn that also calls a
  sink), not real flows.
- **`RelDecl` carries `group`/`doc`** — the parallel `builtin_rel_docs()`
  tuple registry is gone; every built-in relation's one-line doc and group
  live on its declaration, so the schema and the doc cannot drift.
  `rel_catalog`, the generated README table, and the `undocumented_builtins`
  CI guard all read the decls; rendered output is byte-identical.
- **Non-recursive derived rules evaluate in ONE pass.** `rebuild_derived` now
  splits each stratum into rel-level dependency components (Tarjan,
  dependencies first); only genuinely recursive components iterate to a
  fixpoint. Previously every statement re-ran until a whole-stratum delta hit
  zero, so every expensive non-recursive rule paid its cost twice (measured:
  a 40s join statement executed 2x per tick).
- **Unpinned closure queries are guarded.** A `?` on a closure head that
  falls through to the SQL reachability view is refused loudly when the edge
  rel exceeds `DL_CLOSURE_QUERY_MAX_EDGES` (default 20k, `0` disables) — a
  LIMIT cannot short-circuit the view, so on a dense graph it is effectively
  unbounded (measured minutes of CPU on a 471k-edge flow graph). Both-pinned
  closure queries now answer as an existence probe via the seeded
  condensation walk.
- **`flow-interproc.dl` / `taint.dl` sym bridge is an equality join.** The
  per-pair `replace(qual, bare, "") != qual` suffix test (unindexable; ~25M
  string evals, 40s per fixpoint pass on this repo) is replaced by
  `call_edge_bare`, which strips the repo qualifier once per `call_edge` row;
  the interproc hops join on it by equality. Cold derived phase on this
  repo: 130s+ -> 1.4s.

### Fixed
- **`Engine::rel_rows` no longer drops rows containing non-text columns.**
  Reading an INTEGER column as String is a per-row rusqlite type error that
  silently filtered the whole row from diagnostic reads; values now
  stringify from their stored type.

## [0.2.1] - 2026-07-01

### Fixed
- **`@async`/`@stream` effects now drain by default under `dl --daemon`.** The
  effect drain runs inside the daemon poll loop, which was opt-in behind
  `DL_POLL_SECS` — so a program with effects sat at `state='queued'` forever under
  a bare `dl --daemon` with no indication why. The daemon now polls at
  `DEFAULT_POLL_SECS=2` by default; the loop no-ops cheaply when the loaded program
  has no effect rules, so an effect-free daemon is unaffected. `DL_POLL_SECS=N`
  overrides the cadence, `DL_POLL_SECS=0` disables the drain entirely.
- **Actionable diagnostic for the multi-repo scan fan.** A source rule whose head
  var isn't produced by its source op (the common mistake: `scan("*", …), file(r,
  …)` trying to recover the repo by a join) reported a bare `head var r unbound in
  source rule`. It now explains that a source rule binds head vars only from its
  source op, and shows the fix — put the var in scan's repo slot: `repo(r, _, _),
  scan(r, rev, glob, path, rev_out)`.

### Added
- **`examples/npm-crawl.dl` + `examples/crawl` — progressive dependency-graph crawl
  of any public npm package.** Name one package; the `@stream` effect runtime
  crawls its dependency graph straight from the npm registry (one `curl` per
  package, content-addressed so each is fetched once), expands the frontier one BFS
  layer per tick, rewrites a d2 graph progressively as edges land, and optionally
  shallow-pulls each dep's source repo at its rev (`git clone --depth 1` — source
  only, no `npm install`, no build). The `crawl` driver owns the whole
  daemon+load+render lifecycle as one command; fan-in hubs fall out of the same
  graph. The self-seeding counterpart to the org-scale corpus scan — no pre-clone,
  no `config.toml`.

## [0.2.0] - 2026-07-01

### Added
- **Named args + field punning on relation atoms.** A body atom or `?` query may
  pass args by declared column: `type_edge(from: f, kind: "impl")`. Once any
  `col:` appears the atom is in named mode, where a bare identifier puns to its
  own column (`from` == `from: from`, the JS/Rust-struct shorthand), and any
  unmentioned column is a don't-care — so you name only the columns you use
  instead of counting positional `_`. Resolution rides the relation's declared
  columns (user `rel` decls and built-in schemas alike) in a frontend pass, so it
  works across a forward reference. Positional atoms are unchanged. Named args in
  a rule head are rejected for now (aggregate interaction deferred).
- **`dl update` — self-update to the latest release.** Re-runs the cargo-dist
  installer for the newest tag; `--check` reports the installed vs latest version.
- **`dl index` — turnkey SCIP generation.** Detects the language(s) at a root by
  marker file (Cargo.toml / tsconfig / package.json / pyproject / go.mod /
  build.gradle / pom.xml / compile_commands.json / CMakeLists), runs the matching
  indexer (rust-analyzer, scip-typescript, scip-python, scip-go, scip-java,
  scip-clang), and places the result at `<root>/.dl/index.scip`. `--install` runs
  the per-indexer install command; `--rev REV` prints the worktree-and-index
  recipe (SCIP covers the working tree only). A polyglot workspace produces one
  merged index via `scip_import::merge_files`.
- **`dl doctor` — SCIP health screen.** Reports detected languages, indexer
  availability, index presence + freshness (mtime vs HEAD), path-join sanity, and
  `scip_*` row counts. Turns each formerly-silent SCIP failure into a visible line.

### Changed
- The SCIP importer auto-loads `<root>/.dl/index.scip` in addition to
  `$SPREFA_SCIP_INDEX` and `<root>/index.scip`, so a `dl index`-generated index is
  found with no configuration. `dl index` appends `index.scip*` to
  `.dl/.gitignore`, so a generated index (often 100MB+) never lands in git.
- The indexer always runs with `cwd = root`, so SCIP `relative_path` keys join the
  paths the scanners see (removes the silent-empty-from-wrong-dir failure mode).

### Fixed
- **Undeclared head relation is a clear diagnostic, not a SQLite leak.** A rule or
  `?` query over a relation with no `rel` decl now reports `unknown-relation`
  (through `--check`/LSP) naming the relation, instead of failing at execution as
  a raw `no such table: rel_X`.
- **Independent `?` queries.** A query that fails at evaluation (e.g. wrong arity)
  reports its own failure and no longer aborts the rest of the query chain.
- **Zero-match `scan` warns.** A source rule whose glob matches no files prints a
  warning naming the rule, glob, and `repo@rev (root)` it looked under, instead of
  silently producing 0 rows downstream.
- **A bare `//` gives a clear message** ("dl comments start with `#`") instead of a
  baffling `Regex("")` parse error.

### Guardrails
- SCIP generation is explicit and single-root only. Nothing (daemon, reload gate,
  `scan("*")` fan-out) generates an index automatically; the daemon only imports
  one that already exists. `dl index` refuses an aggregation directory — the XDG
  serving home, or a folder containing nested git repos — unless `--force`, so on
  a machine whose daemon watches hundreds of repos a stray marker file cannot turn
  one command into hundreds of indexer runs.
