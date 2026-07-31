# Design Archaeology: coordinate model → v3/v4 → v5 → dl6

## Source note

This is an analysis of the surviving design record. The archive at
`~/projects/sprefa-archive-20260428` is a plain snapshot dated 2026-04-28.
The archive at `~/projects/sprefa-archive-20260701` is a plain snapshot dated
2026-07-01. Neither archive carries Git history, so archive citations name the
snapshot and path and quote the source verbatim. Citations into the main
repository use the short Git revision that supplied the quoted file.

The archive snapshots were read as files. Selected session logs were used for
decision points; the chat-log directories were not read exhaustively.

## Context

The design began as a storage and refactoring problem, acquired a language
surface, then returned to a relational runtime. The old record says the first
system indexed strings and byte locations, while the coordinate-era README
states the intended cross-repository join directly: “Every captured string lands
in a SQLite table. The connections between repos become SQL joins.” [archive
2026-04-28/README.md: “Every captured string lands in a SQLite table. The
connections between repos become SQL joins.”]

The current record uses a different surface and a more explicit runtime
contract. The dl6 tick model assigns relations to boolean, counting, and signed
delta planes, while the present ingestion boundary records raw-byte content
hashes and byte-counted spans. [v6/prolog/compile/TICK-MODEL.md @ 0c91db36:
“Every rel denotes a function from tick to an annotated relation.”] [v6/dl/src/4_ingest.ts @ 0c91db36: “content_hash is the sha-256 hex digest of the
file's raw bytes” and “columns counted in BYTES (not UTF-16 code units)”]

## Timeline

### Coordinate model: strings, refs, and byte locations

The coordinate model treated a codebase as an index of strings attached to file
locations. A ref was a byte offset, while normalized strings made cross-file and
cross-repository joins cheap enough to use as a refactoring substrate. [archive
2026-04-28/readme-old.md: “Every interesting string in a codebase is a **ref**:
a file contains a string at a byte offset.”] [archive 2026-04-28/README.md:
“The string is deduplicated and normalized for fuzzy matching.”]

The `.sprf` surface then made extraction, joins, recursive discovery, checks, and
generated output part of one file. The examples describe a discovered module
chain whose “Fixpoint converges when no new paths appear,” and the self-check
program describes itself as “dogfooding: ensure README stays in sync with source
code.” [archive 2026-04-28/EXAMPLES.sprf: “Fixpoint converges when no new paths
appear.”] [archive 2026-04-28/self_check.sprf: “self_check.sprf -- dogfooding:
ensure README stays in sync with source code”]

The early daemon was already an operating form of the same model: scan files,
extract refs, index them, watch for changes, and plan rewrites. [archive
2026-04-28/readme-old.md: “scan files -> extract refs -> index in SQLite -> watch
-> detect change -> plan rewrites -> apply”]

### v3: the cursor became the language value

v3 changed the unit of composition from a stored coordinate row to a cursor
stream. Its README describes a reactive runner over a pure-effect cache with
cancellation and batching, plus an LSP and CLI. [archive
2026-07-01/v3/README.md: “Runs pipes through a reactive runner built on top of
`effect_runtime` (pure-effect cache + cancellation + batching).”] [archive
2026-07-01/v3/README.md: “Ships an LSP (`sprefa-lsp`) + CLI (`sprefa-run`) + VS
Code extension (`editors/vscode/`).”]

The semantic model made that choice explicit: “The cursor is the *only*
first-class value that flows through the runtime,” and binding emitted a new
cursor carrying a byte range and captures. [archive
2026-07-01/v3/docs/v3-semantic-model.md: “The cursor is the *only* first-class
value that flows through the runtime.”] [archive
2026-07-01/v3/docs/v3-semantic-model.md: “Binding is not assignment — it is
*narrowing*.”]

Cross-rule references were intended as live subscriptions rather than direct
table reads. The v3 record calls them “static dependencies with dynamic
materialization” and says the runtime “subscribes to `rule`'s output stream and
performs a **semijoin**.” [archive
2026-07-01/v3/docs/v3-semantic-model.md: “Cross-references (xref) — `rule.$V` —
are **static dependencies with dynamic materialization**.”] [archive
2026-07-01/v3/docs/v3-semantic-model.md: “it subscribes to `rule`'s output
stream and performs a **semijoin**.”]

The v3 line also carried the LSP and diagnostic ambitions into the operator
model. Assertions emitted violation cursors with path, content, byte range, and
captures. [archive 2026-07-01/v3/docs/v3-semantic-model.md: “Assertions
(`assert`, `witness`, `check`) are ops that consume cursor streams and emit
**violation cursors**.”]

### v4: source-aware cursors, SQL-shaped rules, and durable runtime state

v4 tried to make the cursor carry small typed handles instead of whole source
buffers. The source-aware plan says that `fs > ast` should match without an
explicit `read`, and that queue rows should carry `WhereBytesId`, `StringId`, or
`BlobId` handles. [archive
2026-07-01/v4/docs/v4-cursor-value-where-bytes-plan.md: “`fs > ast` matches a
Rust function without `read`”] [archive
2026-07-01/v4/docs/v4-cursor-value-where-bytes-plan.md: “Queue rows should carry
small handles”]

The relational turn became executable through a batch-local SQL operator and
mounted query state. The SQL plan names the shape as “current cursor batch ->
temp input relation -> SQLite query -> output cursors -> generation commit,”
while the durable-query plan records parked continuations, persisted outputs,
dirty dependencies, and queue revival after reopen. [archive
2026-07-01/v4/docs/v4-sql-rule-query-plan.md: “current cursor batch -> temp input
relation -> SQLite query -> output cursors -> generation commit”] [archive
2026-07-01/v4/docs/v4-durable-mounted-query-plan.md: “`SqliteQueue` serializes
parked continuations and can revive them after reopen”]

The write side was also narrowed to source ranges. The self-documenting
architecture describes reading a file, narrowing to a comment region, rendering
Markdown, and replacing only the focal byte range. [archive
2026-07-01/v4/docs/v4-self-doc-architecture.md: “Only the inner generated region
is replaced.”]

### v5: facts and rules over code in repo/rev/time space

v5 moved the main language from cursor-pipe syntax to `.dl` relations and
rules. The v5 README names the model “Datalog over code, in repo/rev/time
space,” and says that extraction plus rules are lowered to a SQLite fixpoint.
`scan` selected files, source operators extracted rows, and a located spine
kept byte spans available for diagnostics and rewrites. [README.md @
295983e0: “Datalog over code, in repo/rev/time space.”] [README.md @
295983e0: “Extract facts from source files with `scan` + located matchers, write
recursive rules, and the engine lowers them to a SQLite fixpoint.”] [README.md
@ 295983e0: “A match is a coordinate you can squiggle (LSP) or rewrite
(`--move`).”]

The v5 tick separated source re-extraction from derived fixpoint evaluation.
Content and rule text supplied the source cache key, while the derived digest
guard skipped unchanged programs. [README.md @ 295983e0: “A source fact has
exactly one support (its file), so an edit retracts exactly the rows tagged that
file and re-extracts them.”] [README.md @ 295983e0: “A converged tick writes
zero rows.”]

The same generation exposed query, diagnostics, and generated files as sinks.
The language table lists `?`, `diag`, and `gen` beside relations and recursive
rules. [README.md @ 295983e0: “Sinks turn result rows into query output (`?`),
editor diagnostics (`diag` + `--lsp`/`--check`), or generated/spliced files
(`gen`).”]

### v6 / dl6: relations, hosts, ticks, and an explicit proof surface

dl6 keeps the relational language but makes the runtime boundary explicit. The
surface table distinguishes level rules from edge rules, and the CLI compiles
and runs a `.dl6` program against an in-process server or a long-running served
engine. [v6/prolog/compile/SYNTAX.md @ 0c91db36: “`(Head <- Body)` | `Head <-
Body.` | level rule” and “`(Head <+ Body)` | `Head <+ Body.` | edge rule”]
 [v6/prolog/compile/SYNTAX.md @ 0c91db36: “`bop run` ... compile + load a program
on an in-process ephemeral server”]

The central semantic unit is now the tick delta. The getting-started receipt
states that each JSON line is “the **delta** on every relation that moved, not a
snapshot,” while the tick model distinguishes boolean state, counting
occurrences, and signed boundary differences. [v6/GETTING-STARTED.md @
0c91db36: “One JSON line per tick; each line is the **delta** on every relation
that moved, not a snapshot.”] [v6/prolog/compile/TICK-MODEL.md @ 0c91db36:
“| Z (signed) | the delta stream / tick log”]

The current proof discipline is explicit. The compiler refuses constructs with
named terms, the conformance oracle is byte-diffed, and the served engine emits
SQL through a single observable execution seam. [v6/GETTING-STARTED.md @
0c91db36: “A refusal is ... the compiler declining to compile a construct it
will not silently get wrong.”] [v6/prolog/conformance/rulings.pl @ 0c91db36:
“byte-proven against the swipl oracle over the entire oracle-reachable corpus on
every sweep”] [v6/dl/src/3_runtime.ts @ 0c91db36: “every SQL statement this
runtime runs goes through here”]

## Idea ledger

The marks describe the current form of each idea in dl6. `SURVIVED` means the
same design pressure is still a first-class part of the language or runtime.
`TRANSFORMED` means the idea moved to a different layer or representation.
`DIED` means the record explicitly retired the earlier form or replaced the
wording with another contract.

| Idea | Born / coordinate model | v3 and v4 spelling | v5 spelling | dl6 today | Mark |
|---|---|---|---|---|---|
| Facts-from-code extraction | Strings, refs, and file metadata were the materialized index. [archive 2026-04-28/readme-old.md: “Every interesting string in a codebase is a **ref**”] | v3 ops emitted cursors; v4 `fs`, `read`, `ast`, and fact stores made source matches persistent. [archive 2026-07-01/v3/README.md: “`fs` ... enumerate files”] [archive 2026-07-01/v4/docs/v4-sql-rule-query-plan.md: “`rule(:name) { ... }` writes rows to that relation”] | `scan` plus source ops produced file-keyed facts, then recursive rules derived relationships. [README.md @ 295983e0: “`scan` selects files; a source op ... extracts rows”] | `extract` emits `file`, `node`, `edge`, `sig`, `site`, `const`, and `span_line` rows through the ingest boundary. [v6/dl/src/4_ingest.ts @ 0c91db36: “record=node|edge|sig|site|const -> spine rel rows”] | **SURVIVED** |
| Coordinates and byte spans | A ref was a string at a byte offset, with file and revision context. [archive 2026-04-28/readme-old.md: “a file contains a string at a byte offset”] | v3 put `byte_range` on the cursor; v4 split source location from materialized bytes through `WhereBytesId`. [archive 2026-07-01/v3/docs/v3-semantic-model.md: “a cursor inherits `content`, `byte_range`, `captures`, and `path`”] [archive 2026-07-01/v4/docs/v4-cursor-value-where-bytes-plan.md: “`CursorValue::WhereBytes(WhereBytesId)`”] | v5 exposed `ref(id, string, file, lo, hi)` for squiggles and rewrites. [README.md @ 295983e0: “Matched values record their byte spans”] | dl6 keeps raw-byte spans, `span_line`, typed `span` values, and content hashes. [v6/dl/src/4_ingest.ts @ 0c91db36: “columns counted in BYTES (not UTF-16 code units)”] [v6/prolog/conformance/fixtures/4_struct_values.pl @ 0c91db36: “SPANS ... `SpanOut { start: u32, end: u32 }`”] | **TRANSFORMED** into typed relation columns and boundary rows |
| Reactive recomputation | The coordinate model had a watcher that detected moves and edits, then recomputed affected refs and rewrites. [archive 2026-04-28/readme-old.md: “The watcher detects file moves”] | v3 made the stream itself reactive: “rule streams never complete” in the design record; v4 added parked continuations, dirty keys, and wake-up. [archive 2026-04-28/chat_log/20260412.3.reactive-operator-semantics.md: “daemon mode: rule streams never complete”] [archive 2026-07-01/v4/docs/v4-durable-mounted-query-plan.md: “late writes to referenced rule tables wake parked SQL continuations”] | v5 used content/rule digests to re-extract only moved source files and to guard derived work. [README.md @ 295983e0: “only moved files re-extract”] | dl6 uses RxJS streams for host and tick execution, with world-fed rows, effect-cache witnesses, and ordinary IVM retractions. [v6/dl/src/1_hosts.ts @ 0c91db36: “demand rows are durable but deltas are not”] [v6/prolog/conformance/rulings.pl @ 0c91db36: “CANCELLATION IS THE KERNEL PRIMITIVE”] | **SURVIVED** as rows plus stream edges |
| Datalog over SQL | The coordinate model stored per-rule tables and exposed SQL joins. [archive 2026-04-28/README.md: “The connections between repos become SQL joins.”] | v3 drained terminal cursors to SQLite; v4 made `sql`` a batch-local relation operator and added mounted output diffs. [archive 2026-07-01/v3/README.md: “`sprefa-run` writes every rule's terminal-cursor rows to a SQLite database”] [archive 2026-07-01/v4/docs/v4-sql-rule-query-plan.md: “`sql`` ... batch-local SQLite relation op”] | Ordinary `head <- body` rules lowered to a SQLite fixpoint loop. [README.md @ 295983e0: “the engine lowers them to a SQLite fixpoint”] | dl6 emits SQL statements for arrivals, level recomputation, edge application, retention, and delta recording; every statement passes the runtime seam. [v6/prolog/compile/emit_ts.pl @ 0c91db36: “function advanceTick ... `UPDATE "__tick" SET "n" = "n" + 1`”] [v6/dl/src/3_runtime.ts @ 0c91db36: “every SQL statement this runtime runs goes through here”] | **SURVIVED** and made observable |
| The daemon | The coordinate model presented `sprefa daemon` as scan + watch + serve, with a loopback HTTP API. [archive 2026-04-28/README.md: “`sprefa daemon` ... scan + watch + serve (all-in-one)”] | v3 unified CLI, server, and LSP around a reactive runner; v4 added an axum RPC router, durable queues, and a compile daemon. [archive 2026-07-01/v3/README.md: “Ships an LSP ... + CLI ... + VS Code extension”] [archive 2026-07-01/v4/docs/v4-durable-mounted-query-plan.md: “durable queue state and durable mounted SQL output state”] | v5 shipped `dl --lsp`, `--check`, watch, and daemon-backed execution around sync ticks. [README.md @ 295983e0: “`dl examples/glean.dl ...`” and “`dl ... --lsp`”] | dl6 has `bop serve`, `bop run`, and `bop check`; `run` and `check` boot in-process, while `serve` is the long-lived process. [v6/GETTING-STARTED.md @ 0c91db36: “There is no daemon — `run` and `check` boot a server in-process for exactly one job”] | **TRANSFORMED** into a served engine and in-process jobs |
| Globs and scan | The `.sprf` surface began with `fs(glob(...))`, repo/rev selectors, and demand scanning. [archive 2026-04-28/README.md: “`fs(**/Cargo.toml)`”] [archive 2026-04-28/README.md: “triggers demand scanning”] | v3 `glob`, `repo`, `rev`, and `fs` were cursor-producing/filtering ops; v4 carried repo/rev/source-aware reads. [archive 2026-07-01/v3/README.md: “`fs` ... enumerate files under `(repo, rev)`”] | `scan(glob, path, rev_out)` became the standard source selector, with worktree and arbitrary repo/rev forms. [README.md @ 295983e0: “`scan` ... select files”] | dl6 separates `files`/`files_at` or `enumerate`/`enumerate_at` from `bind watch`, and the ruling bans `scan` for file enumeration. [v6/prolog/conformance/rulings.pl @ 0c91db36: “The word scan is BANNED for file enumeration”] [v6/GETTING-STARTED.md @ 0c91db36: “Enumerating a tree on demand ... is a separate host (`enumerate`)”] | **TRANSFORMED** into distinct hosts and feed types |
| Diagnostics and LSP | The coordinate model had live parse diagnostics, completions, hover, and a rewrite-aware editor server. [archive 2026-04-28/README.md: “LSP integration via `sprf-lsp` binary (diagnostics, completions)”] | v3 made diagnostics violation cursors and shipped the LSP; v4 added cursor coordinates, LSP RPC, hover, and diagnostic ops. [archive 2026-07-01/v3/docs/v3-semantic-model.md: “A violation cursor is a cursor like any other”] [archive 2026-07-01/v4/docs/v4-sql-rule-query-plan.md: “`lsp_error/lsp_warn/...` runtime diagnostic ops”] | v5 made `diag` a relation sink with `--lsp` and `--check`, and its match spine supplied exact locations. [README.md @ 295983e0: “editor diagnostics (`diag` + `--lsp`/`--check`)”] | dl6 stores `diag` as an ordinary relation, serves `diag`, `definition`, and `hover`, and names refusal locations in the compile door. [v6/DECISIONS.md @ 0c91db36: “`diag` = plain `rel`, read by the LSP plugin”] [v6/GETTING-STARTED.md @ 0c91db36: “The refusal names the file and the line”] | **TRANSFORMED** into data rows plus boundary services |
| Code generation and writes | The coordinate era planned rewrite application after a move or rename, and `.sprf` checks were intended to drive code generation. [archive 2026-04-28/readme-old.md: “computes new values ... and applies the edits to disk”] [archive 2026-04-28/docs/urtsl-spec.md: “Rules compile via build.rs codegen to static Rust functions.”] | v4 rendered Markdown into marked comment ranges and replaced only the focal byte range. [archive 2026-07-01/v4/docs/v4-self-doc-architecture.md: “write_cursor(:replace) ... replaces only focal byte range”] | v5 had `gen` whole-file, append, and marker-splice forms. [README.md @ 295983e0: “`gen` (file)” and “`gen` (splice)”] | dl6 has a staged write host in the self-map rail, but the current construct table lists relation and host declarations while the self-map assembles and writes its own architecture document. [v6/dl/fixtures/self-map.dl6 @ 0c91db36: “It writes a file”] [v6/prolog/compile/SYNTAX.md @ 0c91db36: “Construct table”] | **TRANSFORMED**, with general codegen left as a shelf candidate |
| Tick and delta model | The coordinate model had incremental scan diffs and watcher events, but the `.sprf` surface mostly presented rows and checks. [archive 2026-04-28/README.md: “Incremental scanning”] | v3 used stream completion, effect batching, and parked cursors; v4 used generation boundaries, dirty keys, and queue wake-ups. [archive 2026-07-01/v4/docs/v4-runtime-batching.md: “generation and collection boundaries”] [archive 2026-07-01/v4/docs/v4-durable-mounted-query-plan.md: “Dirty keys should be specific enough to wake affected mounted queries”] | v5 made the unit a synchronous tick around source refresh, fixpoint, and sinks. [README.md @ 295983e0: “One tick is: refresh source facts → evaluate the fixpoint → fire sinks.”] | dl6 gives the tick a formal ring model, grades edges with integer offsets, emits signed deltas, and refuses incompatible paths. [v6/prolog/compile/TICK-MODEL.md @ 0c91db36: “Rule-graph edges carry a tick delay in the monoid (N, +)”] [v6/prolog/compile/TICK-MODEL.md @ 0c91db36: “unequal offsets into one relation from one origin are a refusal”] | **TRANSFORMED** into a graded contract |
| Content addressing | The coordinate model deduplicated strings and associated file content with hashes; the early storage plan used `content_hash`. [archive 2026-04-28/README.md: “The string is deduplicated”] [archive 2026-04-28/readme-old.md: “file contains a string at a byte offset”] | v4 made content-derived cursor/store ids explicit and reused Git blob OIDs for committed bytes. [archive 2026-07-01/v4/docs/v4-cursor-value-where-bytes-plan.md: “`WhereBytesId`”] [archive 2026-04-28/chat_log/20260407.16.sprefa-n1-blob-oid-optimization.md: “`git2::TreeEntry::id()` returns blob OIDs ... WITHOUT reading content”] | v5 keyed extraction and derived work by content and rule digests. [README.md @ 295983e0: “Source-op rows are cached by (file content hash, rule text)”] | dl6 salts host effects with content or clock witnesses, and its ingest file row retracts on any content edit. [v6/prolog/conformance/rulings.pl @ 0c91db36: “Salt minting: CONTENT-ADDRESSED, always.”] [v6/dl/src/4_ingest.ts @ 0c91db36: “A file row retracts+inserts on ANY content edit”] | **SURVIVED** as identity plus witness |
| Self-hosting and dogfood | The coordinate snapshot included `self_check.sprf` to compare source-of-truth tags, commands, crates, and README entries. [archive 2026-04-28/self_check.sprf: “Sources of truth”] | v4 generated its own architecture documentation through `sprf`, and the archive notes “dogfood config as sprf data.” [archive 2026-07-01/v4/docs/v4-self-doc-architecture.md: “This file is intentionally written through sprf.”] [archive 2026-07-01/MAIN.md: “dogfood config as sprf data”] | v5 maintained `.dl` examples and rails over the project itself. [README.md @ 295983e0: “This file is the reference for humans and agents alike”] | dl6 has a single `self-map.dl6` program that derives its own graph, writes `ARCH-MAP.md`, and is rerun for byte-identical output. [v6/dl/fixtures/self-map.dl6 @ 0c91db36: “THE SYSTEM DESCRIBING ITSELF, in its own language.”] [v6/ARCH-MAP.md @ 0c91db36: “Every fact below travelled through the served tsv2 engine.”] | **SURVIVED** and tightened into a rail |
| Legacy cursor/xref/capture-write surface | The coordinate model exposed refs as author-facing locations. [archive 2026-04-28/readme-old.md: “Every interesting string in a codebase is a **ref**”] | v3 named `rule.$V` xrefs, `&` cursor references, and `> $TARGET` capture writes before the surface retirement record. [archive 2026-04-28/chat_log/20260425.1.v3-retire-cursor-ref-xref-capture-write.md: “retire `&` sigil + dots + bare-term forms”] | v5 expressed the replacement in relations, `ref`, and `gen`. [README.md @ 295983e0: “Sinks turn result rows into query output (`?`), editor diagnostics (`diag` + `--lsp`/`--check`), or generated/spliced files (`gen`).”] | dl6 has relation terms, hosts, and self-map writes; the current construct table records the newer surface. [v6/prolog/compile/SYNTAX.md @ 0c91db36: “Construct table”] | **DIED** as an author-facing syntax |

### Ledger reading

The cursor did not disappear from the design history; its responsibilities
were split. Source identity, byte spans, rule rows, and effect witnesses became
data columns or cache keys, while the live sequencing role moved to ticks,
edges, and host streams. v6 names the split directly: “we want edb to be ... a
rel enum, that never has a body, is pure subject,” and the current host lowering turns
requests and responses into ordinary relations. [v6/prolog/conformance/rulings.pl
@ 0c91db36: “we want edb to be ... a rel enum, that never has a body, is pure subject”] [v6/prolog/compile/SYNTAX.md @ 0c91db36: “`sh_decl(Name, Inputs, Outputs, template(Text))` ...
commit the decoded response as an EDB arrival”]

The largest surface change is vocabulary. The old language used `fs`, `repo`,
`rev`, `read`, `comment`, `print`, `rule`, and cursor fields. v5 used `scan`,
typed relations, `gen`, `diag`, and `@next`. dl6 keeps the relation and source
ideas while separating worktree feeds, pinned revisions, watches, hosts, and
tick-aware edge rules. The current ruling records this as a naming decision:
“`files(glob, ...)` = live worktree feed” and “`files_at(rev, glob, ...)` = the
marked pinned case.” [v6/prolog/conformance/rulings.pl @ 0c91db36: “`files(glob,
...)` = live worktree feed” and “`files_at(rev, glob, ...)` = the marked pinned
case”]

## What only exists now

These are current dl6 forms with no earlier-generation equivalent in the
reviewed record. Earlier versions had related mechanisms, but the named
combination below appears only in the current conformance and rail documents.

1. **A byte-graded oracle contract.** Earlier generations had smoke tests,
   SQLite output, and runtime comparisons. dl6 makes the referee itself a
   standing rule: “byte-proven against the swipl oracle over the entire
   oracle-reachable corpus on every sweep,” with a final-state hash retained as
   a third check. [v6/prolog/conformance/rulings.pl @ 0c91db36: “byte-proven
   against the swipl oracle over the entire oracle-reachable corpus on every
   sweep; final-state hash retained as a third check at all scales”]

2. **Named refusals as part of the language contract.** v3 had static warnings
   and v5 had type and path diagnostics. dl6 makes unsupported semantics a
   named compile result with a file, line, construct, and exit code. [v6/GETTING-STARTED.md @ 0c91db36: “A refusal is ... the compiler declining to compile a construct it will not silently get wrong.”] [v6/GETTING-STARTED.md @ 0c91db36: “`2` a named refusal.”]

3. **An endurance law for durable demand replay.** v4 persisted parked
   continuations and mounted outputs. dl6 states the crash behavior at the host
   seam: demand rows survive, deltas do not, and every live request is replayed
   through a cache-deduped pipeline on boot. [v6/dl/src/1_hosts.ts @ 0c91db36:
   “BOOT REPLAY (endurance law ...): demand rows are durable but deltas are not”]

4. **Emitted SQL as an observable incremental implementation.** v4 could run a
   SQL operator over a batch and v5 lowered a fixpoint to SQLite. dl6 records the
   emitted tick statements, routes all SQL through `execute$`, and grades the
   incremental path against a naive referee. [v6/dl/src/3_runtime.ts @ 0c91db36:
   “every SQL statement this runtime runs goes through here”] [v6/prolog/compile/TICK-MODEL.md @ 0c91db36: “the emitted incremental path ... and the naive referee ... both report it, byte-identical to the oracle”]

5. **Executed documentation as a conformance artifact.** Earlier self-check and
   self-documenting records generated or checked pieces of documentation. dl6
   executes the getting-started page in a persistent shell and diffs the output,
   while the self-map program rebuilds its architecture document twice and
   requires byte-identical output. [v6/GETTING-STARTED.md @ 0c91db36: “Every
   command block below is **executed** ... and its output is diffed against the
   text printed here.”] [v6/dl/fixtures/self-map.dl6 @ 0c91db36: “`just self-map`
   twice produces byte-identical output”]

## WHAT-WAS-LOST

The following are shelf candidates. Each is an old capability or design shape
with a direct source record and no matching current writable construct in the
reviewed dl6 surface.

1. **General `gen` as a user-facing code-generation language.** v5 had whole-file,
   append, and marker-splice forms, and v4 had a focal-range renderer. The
   current dl6 syntax inventory contains hosts, probes, queries, and relations,
   while the self-map write is a dedicated rail rather than a general `gen`
   construct. [README.md @ 295983e0: “`gen` (file)” and “`gen` (splice)”]
   [archive 2026-07-01/v4/docs/v4-self-doc-architecture.md:
   “`write_cursor(:replace)` ... replaces only focal byte range”] [v6/dl/fixtures/self-map.dl6 @ 0c91db36: “It writes a file”]

2. **Arbitrary multi-repository, multi-revision extraction as a native source
   plane.** v5's `scan` accepted repository and revision coordinates, including
   other configured repositories. The current `enumerate` fixtures distinguish
   worktree and pinned revision hosts, while the v5 surface audit records that
   dl6's current extraction worktree does not produce multi-revision facts.
   [README.md @ 295983e0: “`scan(repo?, rev?, glob, path, rev_out?)`”]
   [v6/plans/2026-07-23-v5-surface-audit.md @ 0c91db36: “NOTHING produces
   multi-rev facts on the v6 side.”]

3. **The universal cursor as an author-facing value.** v3 made every expression
   return cursors and let operators dispatch on bound or unbound terms. v4
   carried this into typed cursor values and source handles. dl6 exposes relation
   rows, declared columns, hosts, and edge rules; the current surface table has
   no equivalent first-class cursor value or operator-local `Bound /
   Unbound` dispatch. [archive 2026-07-01/v3/docs/v3-semantic-model.md: “every
   expression returns cursors”] [archive 2026-07-01/v3/docs/v3-semantic-model.md:
   “The runtime computes `TermMode` per arg at the call boundary.”]
   [v6/prolog/compile/SYNTAX.md @ 0c91db36: “The construct table ... `live` rows
   have compiler wiring, while `refused` and `reserved` rows name refusal-only
   surface.”]

4. **Durable mounted SQL as a general user construct.** v4 had a durable
   mounted-query table, parked SQL continuations, dirty dependencies, output
   diffs, and queue revival. dl6 has emitted incremental SQL and durable host
   demand, but the current registry does not expose the v4 `sql`` mount as a
   user-facing construct. [archive 2026-07-01/v4/docs/v4-durable-mounted-query-plan.md:
   “mounted SQL parks continuations on referenced table dirty keys”] [archive
   2026-07-01/v4/docs/v4-durable-mounted-query-plan.md: “mounted query outputs
   are persisted through `mounted_query_output`”] [v6/prolog/compile/SYNTAX.md @
   0c91db36: “`query/1` ... `decl(query_plan)`”]

5. **The explicit effect journal and approval saga.** The v3 design record
   described serialized effect descriptors with pending, approved, running,
   completed, failed, and rolled-back states, including a durable audit trail and
   approval sources. Current dl6 has host demand, effect caches, cancellation
   attempts, and response rows; the reviewed surface has no equivalent generic
   effect journal with approval and rollback as language-level records. [archive
   2026-04-28/chat_log/20260412.6.effect-saga-architecture.md: “Effects stored
   in SQLite following existing Store trait pattern”] [archive
   2026-04-28/chat_log/20260412.6.effect-saga-architecture.md: “status TEXT NOT
   NULL -- pending, approved, running, completed, failed, rolled_back”] [v6/prolog/conformance/rulings.pl @ 0c91db36: “ABORT ... BEST-EFFORT world-cost machinery, never a semantic guarantee”]

6. **A programmable LSP operator layer.** v3 treated assertions, hover, and
   diagnostic-producing operators as part of the cursor pipeline, and v4 listed
   runtime LSP diagnostic ops. dl6 retains LSP-facing `diag`, `definition`, and
   `hover` services, but the present language record gives those services fixed
   boundary methods rather than an author-extensible LSP-op family. [archive
   2026-07-01/v3/docs/v3-semantic-model.md: “Assertions ... are first-class ops”]
   [archive 2026-07-01/v4/docs/v4-sql-rule-query-plan.md: “`lsp_error/lsp_warn/...`
   runtime diagnostic ops”] [v6/prolog/compile/SYNTAX.md @ 0c91db36: “`definition`
   ... `hover`”]

## Decisions

- Treat the archives as snapshot-dated sources, because the supplied archive
  directories have no Git metadata.
- Cite archive evidence by snapshot date, path, and verbatim quote. Cite main
  repository evidence by path, short revision, and verbatim quote.
- Treat v6's explicit rails as the present tense: conformance rulings, tick
  model, ingest boundary, executed getting-started document, and self-map.
- Keep lost capabilities as named shelf candidates rather than silently
  reclassifying them as failures.

## Verification

The evidence pass covered the coordinate snapshot's README, old README, notes,
examples, self-check, rules, docs, memory, v2 tree, and selected coordinate-era
chat logs; the 2026-07-01 snapshot's MAIN, TASKS, human goals, LLM notes, v3,
v4, and v5cozokuzu trees; the main repository's v5 history; and the present dl6
syntax, tick, ruling, ingest, getting-started, and self-map records. The archive
directories were read as plain files. No archive Git command was used after the
snapshot correction.

The document itself is the verification artifact for this analysis. Its ledger
contains the eleven recurring ideas named in the brief, its timeline covers the
five requested eras, its current-only section contains the five named dl6
features requested by the brief, and its lost section names shelf candidates
with source evidence.

## Staffing

Analysis-only document. Base worktree revision: `0c91db365a4648db8b5f20332a3b5ce1ec26b68c`.
No archive writes. No behavioral code changes.
