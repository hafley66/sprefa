# v3 parse experiment

Tiny validation of the per-op-grammar tree-sitter approach for sprf v3.
Three binaries, each a tier of the proof.

## Binaries

| Binary    | File                  | Tier | Proves                                            |
| --------- | --------------------- | ---- | ------------------------------------------------- |
| `inject`  | `src/main.rs`         | 1    | one Parser, two Languages, set_included_ranges    |
| `nest`    | `src/bin/nest.rs`     | 1.5  | two-level injection (sprf → md → rust fence)      |
| `recurse` | `src/bin/recurse.rs`  | 2    | fixed-point recursive injection at arbitrary depth|

Run any of them:

```bash
cd v3-parse-experiment
cargo run --bin inject
cargo run --bin nest
cargo run --bin recurse
```

## Bash helper

Drop into `~/.bashrc` / `~/.zshrc` for one-key access. The naming
mirrors the would-be sprf surface (`_.sprfv2.expr.plugins.<name>`).

```bash
_.sprfv2.expr.plugins.inject()  { (cd ~/projects/sprefa/v3-parse-experiment && cargo run --quiet --bin inject  "$@"); }
_.sprfv2.expr.plugins.nest()    { (cd ~/projects/sprefa/v3-parse-experiment && cargo run --quiet --bin nest    "$@"); }
_.sprfv2.expr.plugins.recurse() { (cd ~/projects/sprefa/v3-parse-experiment && cargo run --quiet --bin recurse "$@"); }
_.sprfv2.expr.plugins.all()     {
    _.sprfv2.expr.plugins.inject  && \
    _.sprfv2.expr.plugins.nest    && \
    _.sprfv2.expr.plugins.recurse
}
_.sprfv2.expr.plugins.list()    { compgen -A function | grep '^_\.sprfv2\.expr\.plugins\.'; }
```

Subshell + `--quiet` keeps the cwd unchanged and trims cargo chatter.
Append new binaries by adding one line; `.list` enumerates them.

## Tier 1 — `inject`

Validates the runtime composition mechanism with zero grammar
authoring. Uses existing `tree-sitter-json` and `tree-sitter-rust` as
the per-op sub-languages. Stand-in for the host parse: hand-pick
`(language, brace-body-byte-range)` pairs by string search.

What this proves:
- one `Parser` swapping between `Language`s
- `set_included_ranges` with absolute byte ranges from the original source
- byte/row/col positions in inner trees are real source positions
- error tolerance survives across the injection boundary

What this defers (later tiers):
- writing `tree-sitter-sprefa` host grammar
- recursive host-into-host injection (`DefaultFork` brace bodies)
- incremental reparse via `Tree::edit`

Expected: three labelled blocks. The two valid bodies (json key:value,
rust fn) print clean s-expressions with `has_error: false`. The third
(intentionally malformed json) prints `has_error: true` plus the
ERROR/MISSING node positions, all addressed in original-file
coordinates.

## Tier 1.5 — `nest`

Two-level injection: sprf op body is markdown, the markdown body
contains a fenced rust block. Three Languages, two injection levels
in the same file. Verifies that injection sites can themselves emit
further injection sites.

## Tier 2 — `recurse`

Fixed-point loop drives N-level injection from one decider closure.
Replaces the hardcoded "find sprf op_calls then find md fences" walk
with a queue: each parse emits new sites, the loop drains the queue
until empty. Produces 6 trees across depths 0-3 in the current
fixture. This is the algorithmic core for stages 2-3 of the lowering
pipeline (`v2/docs/_6_lowering-pipeline.md`).

## Tier 3 (next, when Tier 2 holds)

Add `grammars/tree-sitter-sprf-mini/grammar.js` covering the smallest
sprf-shaped host:

```
program  := op_call*
op_call  := IDENT '(' lang ')' '{' body '}'
lang     := IDENT
body     := <opaque token tree>
```

Wire the build via `tree-sitter generate` + `cc` in `build.rs`.
Replace the string-search stand-in in `recurse.rs` with a real walk
of the host tree's `op_call` nodes. Add `tree-sitter-sprf-walker` for
the walker DSL. At that point the experiment covers all three
sub-grammar shapes v3 needs:

1. external sub-language (json/rust/md)
2. recursive host (sprf into sprf)
3. sprf-owned sub-grammar (walker)

If all three coexist in one file with clean position rebasing and
error nodes, the approach is validated and v3 can absorb the
experiment as its parsing layer.

## Lofty goals (north star, not roadmap)

The reason this matters past "another DSL." Captured here so the
experiment stays anchored to the long-arc target while the short-arc
work happens.

- **AI-authored sprf at session boundaries.** Compaction / handoff
  prompts like *"write the sprf for this session — what new
  assumptions need stamping, what LSP diags should fire if a future
  AI changes that thing"* — output is committed `.sprf` rules that
  the runtime enforces from then on. Sessions become first-class
  producers of language artifacts, not just consumers.
- **Pinning facts to revs / entities.** *"Assumption X holds in repo
  Y at rev Z; entity W must never look like Q or point at R."* These
  pin into the term/Resolution model: the assumption checker stamps
  Tri, drift gets surfaced as a diagnostic. See
  `project_scan_pointer_runtime.md` memory.
- **Bashy stream language with high-perf adaptive scheduling and a
  proper effect model.** ripgrep / fzf / ast-grep ergonomics +
  haxl/tower-style batched effects + adaptive parallelism per stage
  load. See `project_v2_runner_parallelism.md`,
  `project_v2_effects_split.md`,
  `reference_effect_batching_prior_art.md`.
- **LSP-as-op.** Programmable LSP behavior in `.sprf` itself, server
  is a thin dispatcher. See `_7_lsp-as-op.md`.
- **Cursor as universal currency.** Anything addressable via
  `&{...}`, redirect via `>>{slot}`, inline-eval via `${[expr]}`.
  See `_8_string-redirection.md`.
- **Bash as a peer, not a wrapped child.** `sh[r](...)` (shell
  read) and `sh[w](...)` (shell write) — two hinted flavors. Read
  is pure-by-declaration: cacheable, parallel-safe, default-on memo.
  Write is mutating: approval-gated, sequential, opt-in cache.
  Both run / approve / cache as explicit states (haxl-style memo +
  cargo-style fingerprinting). Pipeline rerun is cheap because pure
  stages replay from cache and only re-shell when fingerprint
  changes. ripgrep / fzf / ast-grep / oasdiff slot in as `sh[r]`
  with the same caching contract; the language composes them,
  doesn't reimplement them.

  **Shell ops are a cut** (prolog term): the boundary is opaque to
  the host binding system. Host values flow IN via argument
  interpolation (`${X}` / `&{...}` in the command string). Nothing
  flows OUT as structured bindings — output is bytes (stdout / exit),
  re-parsed downstream by an explicit op (`sh[r](rg --json ${p}) >
  json(.lines)`) if structure is needed. Asymmetric vs `ast[lang]`,
  which DOES lift sub-language metavars to host bindings via
  `projections()`. The cut keeps shell from infecting the term
  model and keeps the host from pretending it understands shell.

## The original itch: queryable expectations about your environment

Before SQLite, before any of the runtime: the want was a way to
*express expectations and queries about the git environment you sit
in*, at a day job or anywhere. "This module should never import
that one." "Every handler in `routes/` should have a corresponding
test in `__tests__/routes/`." "If `ConfigV2` exists, no caller
should still be passing `ConfigV1`." "Show me every TODO older than
six months whose author still works here."

These are queries the SQLite layer answers, but the *source artifact*
is a `.sprf` file that reads like a checked-in expectation document.
The query language is the file format; SQLite is just where the
materialized view lives. Pulled out of the runtime, the file alone
should communicate intent to a human reader (and to a future AI
session, see lofty goals above).

A heavier example, to pin the ceiling: *"this type in rust over
here, with this name, will get used over here with this
type/entity."* The user writes that edge once as a `.sprf`
declaration. The runtime materializes it as a row (or a graph edge),
verifies both endpoints exist (Tri stamping), warns when either side
drifts (rename, delete, signature change), and exposes the edge
set as something you can visualize — a graph render op, a
mermaid/d2 emit, a json dump for whatever tool. The edge survives
across repos because the source artifact is the declaration, not the
code itself; either side moving doesn't break the edge, it triggers
a check.

This is the project's core thesis under one name: **typed edges
declared in `.sprf`, verified by the runtime, visualizable on
demand, durable across renames and repo boundaries.** See
`project_vision.md` and `project_scan_pointer_runtime.md` memories
for the longer arc.

Categories of edge the language should make natural:
- intra-repo type/use (rust struct → its consumers)
- intra-repo lexical (handler → matching test)
- cross-repo API (sdk method → server route)
- cross-repo data (schema column → consumer field)
- cross-repo deploy (config key → service that reads it)
- doc → code (this README claim → the function it claims about)
- spec → impl (openapi spec in repo A covers / diverges from
  routes in repo B); diverge → run `oasdiff` via `sh[]`, emit the
  diff as cursor content, surface as diag or PR comment

Each one is the same shape: `(declaration) -> (verified Tri) ->
(rendered as table / graph / diag / hover / doc)`.

What you're building is "cross-codebase typed edges as a
checked-in expectation language with a SQLite back end and a
programmable LSP front end." It has been an internally consistent
target the whole way. The hard parts are the surface ergonomics
and the runtime budget, both of which the v3 substrate is designed
around.

### Why the cursor abstraction earns its keep

The openapi/oasdiff case is the clean tell. Cross-repo CI jobs
hardcode `main` vs `main`; the actual question is often "this
feature branch over here vs this tagged release over there" or
"my working tree vs whatever is deployed." A cursor carries
`(fs, repo, rev)` as a triple, so the comparison points are
arbitrary refs — `$wt` for working tree, a sha, a tag, a remote
branch, a pack-cached blob (see `project_v2_wt_overlay.md`).

Pipeline:
```
spec_a:  openapi(repo: "svc-a", rev: $branch_a, path: "openapi.yaml")
spec_b:  openapi(repo: "svc-b", rev: $branch_b, path: "openapi.yaml")
> diff(spec_a, spec_b) where !equal
> sh[oasdiff](--base &{spec_a.fs} --revision &{spec_b.fs})
> emit_diag(severity: warn, msg: &{cursor.content})
```

The `(fs, repo, rev)` triple plus `sh[]` peer-effect plus emit
target means the same expression covers main↔main CI, feature
branch↔release tag spot-checks, and "is what I'm about to push
compatible with what's deployed?" loops. GitHub Actions can't
express that without a fan-out matrix per pair; sprf expresses it
as one declaration per edge.

The cursor abstraction is the *generality dividend* — once
everything is `(fs, repo, rev) + content + slots + captures`,
"a bunch of other dumb shit" composes for free instead of needing
bespoke jobs.

The `json()` op is the smallest case that already feels right: walk
arbitrary json, bind cursors, query structurally, no impedance
mismatch with the rest of the pipeline. The work of v3 is making
every other source of structured truth (git refs, tree-sitter trees,
fs layout, type graphs, comment markers) feel as good to query as
json already does.

Making a language to taste is hard. The discipline is: keep the
surface tiny, push power into op composition + per-op grammars,
keep the emit target boring (SQLite), and let the *expression* of
expectations stay close to how you'd naturally type them in a shell
or a sticky note.

## The terminal sink: SQLite + per-rule tables + transitive joins

All of the above feeds one terminal: rows emitted into SQLite, one
table per rule, queryable via raw SQL with file/repo-scoped
namespacing. This was the v0 win and it stays the win:

- Each `(name) > op chain` rule produces a typed row schema; the
  runtime materializes it as a table.
- Cross-rule queries are SQL joins, not a custom query DSL. Drop the
  query compiler; the language *describes* expressions and what
  their rows mean, then SQL handles transitive composition (recursive
  CTEs for chains, window functions for grouping, the rest of the
  toolbox for free).
- Hover, LSP diags, doc render, AI-session-authored assumptions —
  all of them read back from the same SQLite. The emit side is the
  contract everything else stands on.
- See `project_query_redesign.md` memory: per-rule tables, raw SQL
  blocks, file-scoped namespaces.

The pipeline language is the *front* of this; SQLite + SQL is the
*back*. The middle (parse, ops, effects, scheduling) exists to make
the front pleasant to write and the back fast to populate.
Transitive queries (`WITH RECURSIVE`, multi-rule joins) are why
"describe exprs to rules to rows" was worth building in the first
place — they expose causal structure that no individual op can see
locally.

## The cross-pollination list (still un-thought-out)

The seed crops being left-joined into one language. Listed so future
sessions can keep them in mind without re-deriving the set:

- **bash** — pipes, redirection, ergonomic one-liners, shell-out as
  a peer with caching/approve.
- **LSP** — programmable per-node hover/diag/complete; LSP-as-op.
- **parsing** — tree-sitter host + per-op sub-grammars; lossless
  CST; recursive injection.
- **rxjs** — stream composition, fan-out/fan-in, backpressure as a
  first-class concern.
- **perf** — adaptive scheduling per stage load; bounded
  concurrency; 16GB/500-repo budget; debug-fast = actually fast.
- **prolog** — term unification, Decl/Ref lattice, `:-` comment
  family, declarative resolution.
- **zig comptime** — tree-as-comptime; treat any tree (parse, type,
  fs, git, dom) as input to compile-time computation; lower at parse
  time when statically resolvable, defer to runtime otherwise.
- **auto-documentation** — render ops emit docs from the same data
  model as extraction; comment-as-carrier syntax; cross-repo hover
  overlays. See `_4_lofty-goals.md`.

These do not all land together; they constrain each other. The
substrate (`_0_`–`_8_`) is the part that lets them coexist. The
runtime (effects, batching, scheduling) is the part that makes them
fast. The plugin model (per-op grammar + per-op trait family) is the
part that lets the language grow without a single grammar churn.
The thinking is mid-flight; this README is the pin so the thread
isn't dropped.

The five-stage lowering pipeline (`_6_`) plus per-op tree-sitter
grammars (`_5_`) plus the term lattice (`_0_`) plus the ast-grep
extension surface (`_1_`) are the *substrate* that makes any of the
above tractable. This experiment is the smallest thing that proves
the substrate's parsing layer works.
