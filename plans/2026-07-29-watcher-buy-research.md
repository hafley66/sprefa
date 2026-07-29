# File-watcher buy analysis (2026-07-29)

Owed by the golden plan Phase 2 ("extraction live") per the standing
build-vs-buy law: no bespoke line for a common-shaped problem (queue,
server, scheduler, parser, telemetry, **file watcher**) without a
written candidate-by-candidate analysis first. Research only, no code
changed. Base sha `fe99a81b9bea9288c505a9977b65e7ced7f17ccb` (fast-forward
merge, ARCH task row `watcher research`).

Primary sources: npm registry API (`registry.npmjs.org`, `api.npmjs.org`),
GitHub REST API via `gh api` (stars/issues/pushed_at/license, all live
2026-07-28/29), each library's own `README.md` fetched raw from GitHub,
Node's own `doc/api/fs.md` at the `v24.15.0` tag (the version installed
in this worktree, matched to `v6/tsv2/package.json`'s runtime).
`~/projects/claude-research/skills/file-watching/SKILL.md` supplied the
kernel-API background (inotify/FSEvents/RDCW caveats, editor save
patterns, overflow failure modes) cited below as "(skill)". Anything
not independently re-verified this session is marked accordingly.

## Where this bolts on

`v6/tsv2/serve/2_binds.ts` is the shape to fit: `IIntervalBindRunner`
implements a cold `firings$: Observable<IBindFired>`, built from
`plans: readonly IBindPlan[]` (`v6/tsv2/runtime/types.ts:361-366`).
Each firing constructs an `IArrivalRow` (`rel`, `sign`, `row`) and
calls `engine.submit([arrival])` — one arrival, one settle tick. A
watcher bind differs in exactly one respect the type already
supports: `engine.submit(arrivals: IArrivalBatch)` takes an **array**
(`types.ts:64`), so a burst of file-change events (a `git checkout`)
submits as one batch, one settle tick, matching "tsv2 commits per
tick" in the task brief. Per `rulings.pl spine_residency`
("world_fed_bind_not_construct" for the clock bind, same shape): the
watcher is an ordinary **push bind** feeding an EDB rel, never kernel,
never a construct. The hosts-extraction verdict's A1 finding already
separates push binds from demand hosts (`plans/2026-07-29-hosts-
extraction-verdict.md`); `enumerate(glob)` / `enumerate_at(rev, glob)`
are the demand-side hosts the golden plan names for Phase 2 — this
bind is the thing that makes `enumerate(glob)`'s cached answer go
stale, i.e. the watcher's job is to mint a fresh salt / emit a
`file_changed(path, kind)` row, not to do extraction itself.

## Candidates

All four named in the task, plus what actually turned up as serious
contenders during the search: `@parcel/watcher-wasm` (drop-in for
unsupported platforms) and `notify-rs` (not a JS candidate — it's
what `src/watchgate.rs` / `src/daemon/shell/watch.rs` already use on
the v5 Rust side, cited only as an internal consistency check, not
scored).

| Candidate | Verify method |
|---|---|
| chokidar | npm registry + GitHub API + raw README |
| @parcel/watcher | npm registry + GitHub API + raw README |
| node `fs.watch`/`fsPromises.watch` | Node `doc/api/fs.md` at installed version |
| watchman (`fb-watchman` npm client + `watchman` binary) | npm registry + GitHub API |

### Version, cadence, maintenance signals (verified 2026-07-28/29)

| | chokidar | @parcel/watcher | node fs.watch | watchman |
|---|---|---|---|---|
| Current version | 5.0.0 | 2.6.0 | n/a (Node core, `v24.15.0` installed) | binary + `fb-watchman` 2.0.2 client |
| Last publish | 2025-11-25 (v5.0.0) | **2026-07-20** (this week) | Node release cadence; recursive landed v19.1.0 (2022-11-14) | repo `pushed_at` **2026-07-28** (today); client last publish 2026-06-16 |
| GitHub stars / open issues | 12,195 / 49 | 829 / 76 | n/a | 13,654 / 252 |
| Repo archived? | no | no | n/a | no |
| Weekly npm downloads | not pulled directly (chokidar's own README cites "~30M repositories" depend on it, unverified claim from their doc) | 31,309,103 (npm downloads API, last-week 2026-07-18..24) | n/a | not pulled (fb-watchman is a thin client, downloads not representative) |
| License | MIT | MIT | n/a (Node core) | MIT (GitHub API `license.spdx_id`) |

Reading: `@parcel/watcher`'s repo and package both moved *this week*.
chokidar's last release is 8 months old but the repo itself was
pushed 2026-07-21 (a week ago) and 49 open issues on a 12k-star repo
is a healthy ratio. watchman's open-issue count (252) is the highest
of the four, but it is also the widest-scope project (Mercurial/hg
triggers, cross-language clients, a query language) — not a like-for-
like comparison against a single-purpose watch library.

### Native-dep story

| | chokidar | @parcel/watcher | node fs.watch | watchman |
|---|---|---|---|---|
| Native code | **none** as of v4 (Sept 2024) — "Remove bundled fsevents", dependency count 13 -> 1 (`readdirp` only) | N-API C++ addon, prebuilt binaries via `optionalDependencies` for `darwin-{x64,arm64}`, `win32-{x64,arm64}`, `linux-{x64,arm64,arm}-{glibc,musl}`, `freebsd-x64`, `android-arm64` (12 targets, verified from the package's own `optionalDependencies` list) | none — built into the Node binary | separate compiled `watchman` binary (brew/apt/build-from-source) + thin JS socket client |
| Install-time compile risk | zero | zero on the 12 covered targets; `@parcel/watcher-wasm` fallback exists for anything else (crawls the FS per-directory, explicitly documented as "significantly less efficient") | zero | the binary is **not** an npm artifact — provisioning it is an ops/CI concern (brew install, apt package, or Meta's build script), outside npm's dependency graph entirely |
| Consequence for us | simplest possible dependency footprint | one more moving part (prebuild matrix) but zero build step in practice; matches the UDF-lab's already-proven pattern of accepting prebuilt native addons (`better-sqlite3` was proven working there) | simplest possible — already in the runtime | heaviest operational cost: a second install path outside `pnpm install`, a version-skew surface between the binary and the client, and a dependency the CI image must carry separately |

### Platform coverage, including the macOS recursive story

Per the file-watching skill and Node's own doc/api/fs.md (`Caveats
> Availability`, verified against the `v24.15.0` tag):

| | macOS | Linux | Windows |
|---|---|---|---|
| chokidar (default backend = node `fs.watch`) | FSEvents for directories, kqueue for files (Node's own backend split, confirmed in `fs.md`); recursive **native**, one watch | inotify; recursive **emulated by walking + one watch per subdirectory** (Node added Linux recursive support in v19.1.0, PR #45098 — same shape notify-rs uses per the skill's 4.2/4.4 tables, so it inherits the same `max_user_watches` ceiling at repo scale) | ReadDirectoryChangesW; recursive native (`bWatchSubtree`) |
| @parcel/watcher | **native FSEvents**, own C++ binding (not routed through Node's `fs.watch`), Watchman as an optional higher-priority backend if installed | **native inotify** C++ binding; backend priority list in its own README puts inotify below Watchman but above nothing else — still one-inotify-watch-per-directory under the hood, same ceiling class, but throttled/coalesced in C++ before crossing into JS | **native ReadDirectoryChangesW** C++ binding |
| node `fs.watch`/`fsPromises.watch` | same FSEvents/kqueue split as chokidar (it *is* chokidar's default backend) | inotify, recursive since v19.1.0, same per-directory-watch caveat | native RDCW |
| watchman | own FSEvents watcher, with "cookie" file synchronization for high-load/network cases (skill section 8, "worth copying") | own inotify watcher | own RDCW watcher |

The macOS "recursive story" is a wash between the top three: FSEvents
recursion is free at the kernel level regardless of which library
calls it, because it is a directory-tree property of FSEvents itself,
not something any of these libraries add. The real platform question
is Linux at scale, and none of the four candidates escapes the
per-directory-inotify-watch structure the skill documents (section
4.4: "the Linux behavior is the load-bearing surprise" for `notify`-
rs applies equally here, verified by reading Node's own PR title:
"add recursive watch **for** linux", i.e. emulated, not native).

### Rename / atomic-save handling (the editor-save problem, skill section 5)

| | Handling |
|---|---|
| chokidar | explicit `atomic` option (write-temp-then-rename detection, "if a file is re-added within 100ms of unlink, emit `change` not `unlink`+`add`") and `awaitWriteFinish` (polls file size until stable, for chunked writes) — both from the raw README, both still present in v5 |
| @parcel/watcher | no editor-specific dedup logic documented; raw events only (`create`/`update`/`delete`); README states plainly: "Renames cause two events: a `delete` for the old name, and a `create` for the new name" — the atomic-save stitching (skill 5.1-5.4) would have to happen downstream, in the bind adapter |
| node fs.watch | raw `rename`/`change` eventType only, no stitching at all — chokidar's whole reason to exist per its own README ("changes are reported as add/change/unlink instead of useless `rename`") |
| watchman | has explicit settle/cookie-file synchronization (skill source 17) but no built-in atomic-save collapsing comparable to chokidar's `atomic` option (unverified this session — not re-checked against watchman's own docs, flagged) |

### Event coalescing / batching shape (matters: tsv2 commits per tick)

| | Shape |
|---|---|
| @parcel/watcher | **native batching**: "events are throttled and coalesced for performance during large changes... a single notification will be emitted with all of the events at the end" — `subscribe(dir, (err, events) => ...)`, `events` is already an array. This is the closest fit to `engine.submit(IArrivalBatch)` with zero extra plumbing. |
| chokidar | per-event callbacks (`add`/`change`/`unlink`, one call each); no batching primitive of its own. An adapter would need `bufferTime`/`bufferWhen` in rxjs to coalesce N events into one `submit()` call per tick — extra code, but a few lines, and it's exactly the kind of thing rxjs is for. |
| node fs.watch | same per-event shape as chokidar (chokidar sits on top of it and still doesn't batch by default) |
| watchman | delivers batched results per subscription trigger by design (it is itself a coalescing daemon) — not independently re-verified this session against its own API docs, flagged as (Open) |

### Symlink and gitignore-scale behavior

| | Symlinks | node_modules-scale ignore |
|---|---|---|
| chokidar | `followSymlinks: true` default, configurable | `ignored` option: function / RegExp / glob-string array, checked via the library's own walk — filtering happens as chokidar builds its watch list, so an ignored directory is not itself watched (per the README's "Path filtering" section); this is watch-install-time exclusion, the cheap kind the skill recommends (section 7, "Exclude lists in practice") |
| @parcel/watcher | not explicitly documented; native backend presumably follows OS symlink semantics (unverified, flagged) | `ignore` option: array of paths or globs, uses `is-glob` + `picomatch`, resolved in **C++** before events reach JS — the exclusion is native-side, cheapest possible shape for a `node_modules`-heavy repo |
| node fs.watch | `followSymlinks` not a first-class option; symlink target-vs-link semantics inherit the kernel API caveats directly (skill 11.4) | `ignore` option exists on both `fs.watch` and `fsPromises.watch` (string/RegExp/Function/Array, minimatch-based) — **unclear from the docs whether this filters before or after the recursive walk installs watches on Linux**; not resolved this session, flagged (Open), matters directly for the "can you scope/ignore cheaply" question |
| watchman | designed around exactly this use case (Meta-scale monorepos); has query-time and subscription-time filtering, plus the cookie-sync mechanism for correctness under load (skill source 17) |

### License

MIT for chokidar, @parcel/watcher, and node core (not applicable — no
license needed for a runtime built-in). watchman: MIT per GitHub's
`license.spdx_id` field (verified via `gh api repos/facebook/watchman`
this session — this corrects an unverified assumption of Apache-2.0
carried into this task from memory; GitHub's repo-level license field
is the source of truth used here).

### API sketch: the BindDef adapter each candidate would need

All four fit the same shape: a cold `Observable<IBindFired>` built
from a declared `IBindPlan`, submitting one `IArrivalBatch` per
coalesced burst, mirroring `2_binds.ts`'s `IntervalBindRunner`
exactly. `IArrivalRow.row` for a hypothetical `file_changed(path,
kind)` rel: `[path, kind]`. New pieces the plan would carry that the
interval bind's `IBindPlan` does not need: a `roots`/`glob` field
(the interval bind has none — cadence is enough, a watcher needs
paths) and an `ignore` list, likely reusing whatever the `enumerate`
host already computes for scope.

**chokidar** (event-per-file, buffer to batch):
```ts
const watcher = chokidar.watch(root, { ignored, atomic: true, awaitWriteFinish: true });
const changes$ = fromEvent(watcher, "all").pipe(
  map(([kind, path]) => ({ rel: plan.name, sign: "add", row: [path, kind] } as IArrivalRow)),
  bufferTime(tickWindowMs),
  filter((batch) => batch.length > 0),
  mergeMap((batch) => engine.submit(batch)),
);
```

**@parcel/watcher** (already batched, `subscribe` is a Promise):
```ts
const changes$ = defer(() => watcher.subscribe(root, (err, events) => {
  if (err) throw err;
  subject.next(events.map((e) => ({ rel: plan.name, sign: "add", row: [e.path, e.type] })));
}, { ignore })).pipe(
  switchMap(() => subject),
  mergeMap((batch) => engine.submit(batch)),
);
```

**node `fsPromises.watch`** (async iterator, `from()` bridges it directly):
```ts
const changes$ = from(fsPromises.watch(root, { recursive: true, ignore })).pipe(
  map((event) => ({ rel: plan.name, sign: "add", row: [event.filename, event.eventType] } as IArrivalRow)),
  bufferTime(tickWindowMs),
  filter((batch) => batch.length > 0),
  mergeMap((batch) => engine.submit(batch)),
);
```

**watchman** (`fb-watchman` client, own subscribe callback, similar shape to @parcel/watcher's):
```ts
const client = new watchman.Client();
const changes$ = fromEventPattern<WatchmanSub>(
  (handler) => client.on("subscription", handler),
).pipe(
  map((resp) => resp.files.map((f) => ({ rel: plan.name, sign: "add", row: [f.name, f.exists ? "update" : "delete"] }))),
  mergeMap((batch) => engine.submit(batch)),
);
```

## Verdict

**Ranked: @parcel/watcher > node `fsPromises.watch` > chokidar > watchman.**

1. **@parcel/watcher wins on fit, not just features.** Native
   coalesced batching is the one property none of the others give for
   free, and it is exactly the shape `engine.submit(IArrivalBatch)`
   wants — a `git checkout` produces one `submit()` call, not N. Its
   `ignore` filtering runs in C++ before events reach JS, the cheapest
   shape for a `node_modules`-heavy repo (skill section 7's stated
   goal — filter at watch-install time, not receive time). Prebuilt
   binaries cover every OS/arch/libc combination this project has ever
   named as a target (macOS, Linux glibc+musl, Windows), with a WASM
   fallback for anything uncovered. It is what VS Code and Nx already
   run at monorepo scale, which is close in shape to "500 repos" in
   the skill's own scale table. The one real cost: it is a native
   addon, which the UDF-lab already accepted as a tolerable class of
   dependency for `better-sqlite3` (`plans/2026-07-29-sqlite-udf-
   graft-verdict.md` — proven working, prebuild story real). This is
   the same shape of bet, already made once this arc.
2. **node `fsPromises.watch` is the zero-dependency fallback**, not a
   loser: it is what chokidar's own default backend already is, so
   picking it directly is "chokidar minus the parts chokidar adds"
   (rename/atomic stitching, per-file event ergonomics). If the
   dependency count of zero matters more than batching and cheap
   ignore, this is the one to take — but the `ignore` option's watch-
   install-time-vs-receive-time behavior is unverified (flagged
   Open above), and there is no evidence it dodges the Linux
   per-directory-inotify-watch cost the way @parcel/watcher's C++
   layer plausibly does.
3. **chokidar is the well-known safe default** with the best editor-
   save ergonomics (`atomic`, `awaitWriteFinish` solve exactly the
   skill's section 5 problem out of the box) and zero native code, but
   it buys that safety by sitting on top of node `fs.watch` with no
   batching primitive — the adapter would hand-roll `bufferTime`
   coalescing that @parcel/watcher gives for free in C++.
4. **watchman is not the buy here.** Its query/cookie-sync machinery
   is real prior art (skill section 8 calls it out explicitly as
   "worth copying"), but the separate-binary install path is a real
   operational cost this repo does not currently carry anywhere else
   in the JS toolchain (contrast: `@libsql/client` and `better-
   sqlite3` are both plain npm installs). Reaching for it would only
   make sense if @parcel/watcher's own optional Watchman backend
   auto-upgrade (mentioned in its README) becomes the actual path —
   i.e., install watchman later as a *performance* upgrade under
   @parcel/watcher, never as the primary dependency.

### Backtrack story (what the bind seam does and does not protect)

The bind seam means swapping libraries later costs one adapter file
(`2_binds.ts`'s own header states this explicitly: "TEARDOWN is
unsubscription and nothing else"). What would leak past the seam and
make a later swap expensive:

- **Event-shape assumptions baked into the downstream `.dl6` program.**
  If a program's rules read `kind` values as `"add"/"change"/"unlink"`
  (chokidar's vocabulary) versus `"create"/"update"/"delete"`
  (@parcel/watcher's vocabulary), every rule using that column is a
  swap cost, not just the adapter. Pick ONE vocabulary at the
  `IBindPlan`/EDB-rel level and translate every backend's own event
  names into it inside the adapter — never let a backend's raw event
  string reach a rule body. This is the one design decision that
  actually matters for backtrack cost; it costs nothing to do now and
  a rewrite to fix later.
- **Batching granularity as an implicit correctness assumption.** A
  rule that depends on "one row per file per tick" (chokidar/fs.watch,
  after buffering) behaves differently from one written against
  "coalesced, at-most-one-event-per-file-per-burst" (@parcel/watcher's
  own stated semantics: "if a file was both created and updated...
  you'll get only a `create` event"). These are NOT the same delta
  stream. Whichever backend ships first, the collapse-then-log law
  already standing for this repo (ruled_collapse trace,
  match-frontier lab) is the right place to absorb this — treat the
  watcher's batch as a raw arrival multiset and let the program's own
  distinctness rules decide what survives, not the library's internal
  coalescing.
- **`getEventsSince`/snapshot recovery is @parcel/watcher-specific**
  and does not exist on the other three. If it gets used for crash
  recovery (a real fit for the endurance law's "no boot replay of
  unanswered demand" goal), that is a one-way door — a later swap to
  chokidar or fs.watch loses that recovery path entirely and needs a
  different mechanism (full re-scan via the `enumerate` host,
  probably, which already exists as a fallback).
- **The `ignore` list itself should NOT be backend-specific glob
  syntax leaking into `.dl6` source.** Keep it a plain string-array
  column the compiler emits from the program's own declared scope, and
  translate to `is-glob`/`picomatch` (parcel), `minimatch` (chokidar/
  node), or watchman's query language inside the adapter only.

### Interim (darwin alpha) vs cross-platform end state

They do not differ. @parcel/watcher's macOS backend (native FSEvents,
its own C++ binding) is equally the fastest path to a working darwin-
only alpha AND the correct long-term cross-platform pick, because the
Linux/Windows backends are the same package, same API, already
prebuilt, already used at VS Code/Nx scale. There is no "best of both
worlds" tradeoff to make here — the two questions collapse to the
same answer specifically because the native-addon cost (usually the
argument for staying in pure-JS during an alpha) is already paid by
prebuilds with zero install-time compilation on every target platform
this project has named. If a genuinely zero-dependency interim were
wanted for some other reason (e.g. distributing tsv2 as a script with
no `node_modules` at all), node `fsPromises.watch` is the fallback,
not chokidar — chokidar adds a dependency to get features
`fsPromises.watch` already has natively (`ignore`, `recursive`,
`AbortSignal` teardown) minus the batching this repo actually wants.

## Open items (named, not resolved here)

- Whether node's `fs.watch`/`fsPromises.watch` `ignore` option filters
  at walk/watch-install time or at receive time on Linux recursion —
  undocumented in `doc/api/fs.md`, not resolved by reading the C++
  source this session.
- watchman's own atomic-save / rename stitching behavior — not
  independently re-verified against watchman's own protocol docs this
  session (only the skill's general watchman notes were used).
- @parcel/watcher's symlink-following default — not documented in its
  own README; would need a source read or an empirical probe before
  relying on either behavior.
