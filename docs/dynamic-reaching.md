# Dynamic reaching

Two mechanisms turn the static `scan("<literal>", …)` model into a data-driven
one, so a program can **discover** repos and **scan each at its own rev** without
code-generation or a shell loop.

- **`repo`-sink** — write a rule whose head is `repo(slug, root, url)`. The engine
  clones + registers each row whose github org is allow-listed.
- **Data-driven scan** — a scan whose `repo`/`rev` slot is a *variable* bound by a
  preceding body atom. The scan fans one enumeration per binding.

Both read **last tick's** coordinate/pull state (the relation is derived after the
source phase), so there is a one-tick latency. The one-shot CLI auto-primes a
silent first tick; the daemon converges in two.

## 1. Dynamic repo pulling — `repo(...)` as a head

`repo(slug, root, url)` is an insertable sink. Asserting a row pulls it: the engine
parses the org out of the github URL and, if that org is in the `org` allow-list,
clones (when `root` is missing) and registers the repo into the engine's set. A
disallowed org is skipped with a stderr line. The hard filter is non-bypassable.

```dl
# the allow-list — only these orgs pull
rel org(name: text).
org("anim-labs").
org("sprefa").

# discover candidates however you like (facts here; normally a manifest scan)
rel candidate(slug: text, root: text, url: text).
candidate("anim", "/work/anim", "https://github.com/anim-labs/anim").
candidate("evil", "/work/evil", "https://github.com/evil-co/evil").   # not listed → skipped

# the sink: insert into `repo` to pull
repo(slug, root, url) <- candidate(slug, root, url).

# scanned repos are now in view: scan("*") fans every registered repo
rel src(p: file).
src(p) <- scan("*", "WORK", "src/**/*.rs", p, rev).
? repo(slug, root, url).
```

Run: `dl prog.dl`. `anim` clones into `/work/anim` and registers;
`evil` is skipped. Leave `root` empty to clone into `$XDG_STATE_HOME/sprefa/repos/<slug>`.

Pulls are idempotent — re-asserting each tick is cheap (an already-registered slug
is a no-op). A pulled repo reaches `scan("*")`, the `repo` relation, and the lazy
indexers (`type_entity`/call/doc) on the next tick.

## 2. Data-driven scan — variable repo/rev

A scan's `repo`/`rev` may be a `Term::Var` bound by a preceding positive atom. The
rule's Pos/Neg/Cmp body compiles to a SELECT over last tick's coordinate relation;
the scan enumerates once per distinct binding, and the head can reference the
repo/rev each row was scanned under.

```dl
# a pin set (facts here; normally derived from Cargo.lock / manifests / git refs)
rel pin(repo: text, rev: text).
pin(".", "HEAD").
pin(".", "abc123").

# scan EACH pin at its own rev, in one pass
rel seen(rev: text, path: file).
seen(V, p) <- pin(R, V), scan(R, V, "src/**/*.rs", p, rout).
? seen(V, p).
```

Run: `dl prog.dl`. Repo and rev can both be variables, or either can
stay literal (`scan(".", V, …)` = variable rev only). **Glob stays literal.**

## 3. Tracing — find hotspots

```bash
DL_TRACE=info  dl prog.dl     # tick timings
DL_TRACE=debug dl prog.dl     # per-phase durations + full-tick-fallback reasons
DL_TRACE=trace dl prog.dl     # + per-file parse_file
```

Span CLOSE carries `time.busy`; the hierarchy is `tick > reconcile_sources >
resolve_scan_bindings` etc. The `debug!` events say *why* a full tick happened
(e.g. `full-tick fallback: changed path outside self.root`). Orthogonal to the
older `DL_PROFILE` SQL/scan logging.

## 4. Daemon + watched scripts

```bash
dl daemon start                        # rootless singleton at the XDG state home
dl daemon load /path/to/script.dl      # join the daemon (starts it if down); hot-reloads on edit
dl daemon load-once /path/to/script.dl # ephemeral eval; prints `?` results, persists nothing
```

## Gotchas

- **One-tick latency.** Both features read last tick's derived state. One-shot `dl`
  auto-primes; the daemon converges in two ticks.
- **`tick_paths` full-ticks** any program with a data-driven scan or `repo`-sink
  (the incremental reconcile can't see coord churn). Daemon edits to such a program
  cost a full tick, not the ~40ms incremental path.
- **Org gate is hard.** Non-github URLs and unlisted orgs are skipped. An empty
  `org` relation pulls nothing.
- **Variable glob is not supported.** Only `repo`/`rev` may be variables; the file
  pattern is a literal.

## Keeping the docs fresh

The `repo`/`rev`/`content`/`file` column shapes in the README's built-in-relations
block are regenerated from `engine.rs` by `examples/builtin-rels.dl` (same match +
comment + gen twine as `examples/op-table.dl`, which regenerates the source-op
table). Run either manually, or `dl daemon load` it into the daemon so it regens on
source edits. Prose tables are hand-maintained.
