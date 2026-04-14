# sprefa v2 — design goals

This document captures the complete design rationale for v2, consolidated from
the "zero-copy-store-effects" session. It is the reference for implementation.

---

## 1. Identity

sprefa v2 is a **stream-based, effect-routed, provenance-first cross-codebase
causal-linking engine**. sprf is the DSL that marks "these strings matter";
ts/rs/ast-grep backbone runs by default. sprefa is not a query tool bolted on
a rewriter — it is a cursor pipeline that happens to write SQL rows, issue
shell effects, and rewrite code.

- Everything reactive. Every read is a stream. Every write is a bulk batch.
- Cursors are scalar tuples flowing over time. Stream topology is the relation.
- No sync fs/git calls anywhere except the initial repo/rev seed read.
- SQL stays behind the storage trait. Forever.

---

## 2. Top-level type graph

```
Sprefa {
    reader:     Arc<dyn Reader>,
    writer:     Arc<dyn Writer>,
    operators:  Vec<Arc<dyn Operator>>,       // op factories
    extractors: Vec<Arc<dyn Extractor>>,      // [Js, Rs, Sprf]
    runner:     Arc<dyn Runner>,
    config$:    Signal<Arc<Config>>,
}

SprefaCLI uses Sprefa
SprefaLSP uses Sprefa
```

### Roles (direct names, no aliases)

| trait | role |
|---|---|
| **Reader** | all reads: fs/git bytes, parsed trees, sqlite queries, cross-refs, unscanned, config |
| **Writer** | all writes: strings, refs, rows, repos, revs, provenance, run_visits, violations, effect log, rewrites, shell |
| **Operator** | factory — parses `.sprf` op invocation → `Pipeline` |
| **Op** | instance — `pipe(input$, ctx) -> output$` |
| **Extractor** | Js / Rs / Sprf — "the thing that extracts from files", carries a Pipeline + RuleTableSpec |
| **Runner** | orchestrator; `run_immediate / run_daemon / run_check / apply_config`; emits `Stream<RunEvent>` |
| **Config** | reactive signal with content-hash |

Logging is decorator middleware (`LoggingReader`, `LoggingWriter`) writing to
the effect_log table. **Not a third trait.**

---

## 3. Reader surface

```rust
pub trait Reader: Send + Sync {
    fn files$  (&self, repo:&str, rev:&str, pattern:&str) -> BoxStream<'static, Vec<FilePath>>;
    fn bytes$  (&self, repo:&str, rev:&str, fs:&FilePath) -> BoxStream<'static, Bytes>;
    fn parsed$ (&self, repo:&str, rev:&str, fs:&FilePath, kind:ParserKind)
               -> BoxStream<'static, Arc<ParsedTree>>;

    fn repos$        (&self)                                      -> BoxStream<'static, Vec<String>>;
    fn revs$         (&self, repo:&str)                           -> BoxStream<'static, Vec<String>>;
    fn cross_ref$    (&self, rule:&str, var:&str, repo:&str, rev:&str)
                     -> BoxStream<'static, Vec<CrossRefHit>>;
    fn unscanned$    (&self, table:&str, column:&str, kind:ScanKind, norm:bool)
                     -> BoxStream<'static, Vec<ScanCombo>>;
    fn violations$   (&self, check:Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>>;
    fn run_visited   (&self, run_id:&str, op_id:&str, cursor_hash:&str)
                     -> BoxStream<'static, bool>;

    fn config$       (&self) -> BoxStream<'static, Arc<Config>>;
}
```

- Every `bytes$` is `shareReplay(1)` keyed by `(repo, rev, fs)` — one read per
  tuple per scan depth. Daemon mode: re-emits on file change.
- Memory-only files (LSP buffers) are indistinguishable from disk files from
  the trait surface. The impl decides.
- Byte-range reads are available via a `Range<usize>` overload for mmap
  friendliness; whole-file is the default.

---

## 4. Writer surface

```rust
pub trait Writer: Send + Sync {
    fn create_rule_table(&self, spec: &RuleTableSpec) -> Result<()>;

    fn write_strings  (&self, values: &[&str])                           -> Result<Vec<StringId>>;
    fn write_refs     (&self, entries: &[RefEntry])                      -> Result<Vec<RefId>>;
    fn write_repos    (&self, names: &[&str])                            -> Result<()>;
    fn write_revs     (&self, revs: &[(&str,&str)])                      -> Result<()>;
    fn write_rev_files(&self, entries: &[(&str,&str,FileId)])            -> Result<()>;
    fn write_rows     (&self, spec: &RuleTableSpec, rows: &[ExtractionRow]) -> Result<()>;
    fn write_provenance(&self, rows: &[ProvenanceRow])                   -> Result<()>;
    fn write_run_visit (&self, rows: &[RunVisit])                        -> Result<()>;
    fn write_violations(&self, check:&str, rows:&[ViolationRow])         -> Result<()>;
    fn write_effect_log(&self, rows: &[EffectLogRow])                    -> Result<()>;

    fn rewrite_files   (&self, edits: &[FileEdit])                       -> Result<()>;
    fn shell_batch     (&self, calls: &[ShellCall])                      -> BoxStream<'static, Vec<ShellReply>>;

    fn flush(&self) -> BoxStream<'static, ()>;
}
```

### Hard rules

- **All writes take `&[T]`.** No singular write exists. SQLite hates per-row
  INSERT; the contract enforces batching at the API level.
- **All writes prefixed `write_` / `rewrite_` / `shell_`.** Clear signal.
- Ops never write direct — they emit cursors; sinks buffer and call `write_*`
  in bulk with `.bufferCount(N)` / `.bufferTime(t)`.

---

## 5. Op / Operator traits

```rust
pub trait Operator: Send + Sync {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &[&'static str];     // ["fs", "fs?", "!fs"] share one factory

    fn bracket_grammar(&self) -> Option<GrammarRef>;
    fn paren_grammar  (&self) -> GrammarRef;
    fn brace_mode     (&self) -> BraceMode;

    fn pre_register(&self, inv: &OpInvocation, pctx: &mut ProgramCtx) -> Result<()> { Ok(()) }
    fn parse       (&self, inv: &OpInvocation, pctx: &mut ProgramCtx)
                   -> Result<Pipeline, Vec<Diagnostic>>;
}

pub trait Op: Send + Sync {
    fn pipe(&self, input: BoxStream<'static, Cursor>, ctx: OpCtx)
        -> BoxStream<'static, Cursor>;
    fn name(&self) -> &'static str;
    fn step(&self) -> u16;
    fn tokens(&self)  -> &'static [TokenSpan];
    fn hover_at(&self, byte: usize) -> Option<HoverInfo>;
}
```

### Op.pipe is the entire behavior

One method. Stream transform. Immediate and daemon modes collapse here; ops
cannot tell which mode they're in — it's all observable.

### Lispy alias routing

The registry maps name and aliases to one factory. The factory sees the exact
name typed, decides behavior via flag:

```
register("fs", FsFactory, aliases = ["fs?", "!fs", "fs.absent"]);
```

`fs?` = empty_ok, `!fs` = negation (future), `fs.absent` = must-not-match.
Whitespace and `[({` are the only structurally significant chars.

---

## 6. Outer grammar (host parser) — v2.1, no `$` prefix on ops

```
program  := stmt*
stmt     := chain ';'?
chain    := op ('>' op)*
op       := IDENT ('[' bracket ']')? ('(' paren ')')? ('{' brace '}')?
brace    := stmt*
```

Line comments: `# ... \n`. `$NAME` / `$$prov` / `${rule.$V}` are leaf tokens
classified by ops at parse time (they never appear at the host level — paren
contents are op-opaque). Examples:

```
rule(demo) {
  repo(myorg/*) > rev($V);
  repo(other/*) > rev($V);
};
```

Lowering map:

| syntax | lowers to |
|---|---|
| `a > b > c` (one chain) | `Pipeline::Seq([a,b,c])` |
| `{ p1; p2; p3 }` (multi pipes) | `Pipeline::Fork([p1,p2,p3])` — distribute parent |
| single-pipe brace | the pipe directly, no wrapper |

`Pipe` (parser type) = one `>`-chain of `OpInvocation`s. `Pipeline` (lowered)
= the runtime tree. Both terms are deliberate.

| slot | role | optional |
|---|---|---|
| IDENT | op identifier (routed through aliases) | no |
| `[bracket]` | meta/config slot (e.g. `fs[absent]`, `ast[ts]`) | yes |
| `(paren)` | op's main DSL (often a pattern; for `rule(name)` it's the rule name) | yes |
| `{brace}` | sub-pipes (default) or op-internal DSL via `BraceMode::Opaque` | yes |

- Balance-aware scanner; language-agnostic.
- `parse_commons` classifies `$NAME`, `${rule.$VAR}`, `$$repo/$$rev/$$fs`; everything else is op-opaque.
- Ops declare their inner grammar via marker-bounded comment blocks in their
  source files; a `$marker` + `$render` op pair dogfoods the LSP grammar
  table generation.

---

## 7. Pipeline (chain shape — tree, not just vec)

```rust
pub enum Pipeline {
    Op(Arc<dyn Op>),
    Seq(Vec<Pipeline>),                  // default: sequential (what `>` or newline produces)
    Fork(Vec<Pipeline>),                  // parallel chains, outputs merged (future syntax)
    Switch { on: ChannelSelector, arms: Vec<(Pattern, Pipeline)> },
}
```

`Pipeline::run(input$, ctx)` folds `Seq`, broadcasts-then-merges `Fork`, and
routes `Switch` on a channel value. Braces lower to `Seq` by default.

Fork semantics preserve v1's denormalization — one seed cursor, N parallel
extractions, merged back into one rule-table row via groupBy on a provenance
key.

---

## 8. Reference model

Three classes of variable, framework-classified at parse time:

| syntax | kind | storage |
|---|---|---|
| `$NAME` | output capture | column `NAME_str` + `NAME_ref` in rule table |
| `$$repo` / `$$repo.norm` / `$$rev` / `$$rev.norm` / `$$fs` | scan / provenance capture | reserved columns; drive discovery queue |
| `${other_rule.$VAR}` | cross-ref FK | column `other_rule_id` FK to other's `_data` table |

- `$$` prefix is reserved vocabulary (inner composition, not pipe
  composition). Fixed set.
- Cross-refs are first-class streams with their own DAG edge. Op sees
  `CrossRefBinding { value, fk_id, span }` per cursor — uses value for
  matching, writes fk_id into output row.
- `$NAME` unifies: bound if already in cursor.captures (filter), unbound
  otherwise (bind). Prolog-style.

---

## 9. Table / view layout

### Backbone (v1-preserved)

```
repos        : PK name
repo_revs    : PK (repo, rev)  — unified branches + tags
files        : PK (repo, path, content_hash)  — content_hash is value identity
rev_files    : junction (repo, rev, file)
strings      : PK value, with norm, norm2, FTS5 trigram
refs         : PK (file, string, span_start)  — physical byte range that found string.value
sprf_meta    : rule change detection (content_hash per rule + inputs)
```

### Per-rule (v1-preserved + FK extension)

```sql
CREATE TABLE "{ns}__{rule}_data" (
  id           INTEGER PRIMARY KEY,
  repo         TEXT NOT NULL,     -- $$repo
  rev          TEXT NOT NULL,     -- $$rev
  file         TEXT NOT NULL,     -- $$fs
  file_hash    TEXT NOT NULL,     -- content_hash
  "NAME_ref"   INTEGER REFERENCES refs(id),
  "NAME_str"   INTEGER REFERENCES strings(id),
  "other_id"   INTEGER REFERENCES "{ns}__other_data"(id)   -- cross-ref FK
);
```

### Views (one per rule, one level only)

```sql
CREATE VIEW "{ns}__{rule}" AS
SELECT t.*, s_NAME.value AS "NAME", o1.NAME AS "other__NAME", ...
FROM "{ns}__{rule}_data" t
LEFT JOIN strings s_NAME ON t."NAME_str" = s_NAME.id
LEFT JOIN "{ns}__other" o1 ON t.other_id = o1.id;
```

**No `_deep` / transitive views.** One-hop FK joins only. Transitive queries
are user-authored SQL.

### Provenance

```sql
CREATE TABLE repo_rev_provenance (
  id, run_id, step_index, source_rule, source_column, source_ref_id,
  source_sprf_loc, selected_repo, selected_rev, reason, ts
);

CREATE TABLE run_visits (
  run_id, op_id, path_hash, cursor_hash,
  PRIMARY KEY(run_id, op_id, path_hash, cursor_hash)
);

CREATE TABLE effect_log (
  id, run_id, step, sprf_loc, kind, payload_json, status, started_at, ended_at
);
```

Every effect — every read, write, shell, rewrite, provenance record — has a
row. Audit trail is first-class.

---

## 10. Cursor

```rust
pub struct Cursor {
    pub run_id:   RunId,
    pub repo:     String,
    pub rev:      String,
    pub fs:       Option<FilePath>,
    pub captures: HashMap<String, Capture>,
    pub fks:      HashMap<String, RowId>,
    pub path:     SprfPath,                  // trail of op invocations
}

pub struct SprfPath(Arc<[PathSeg]>);         // structural sharing, cheap clone

pub enum PathSeg {
    Op       { name: Arc<str>, step: u16 },
    Named    { name: Arc<str>, key: Arc<str> },       // $rule[dep]
    ForkArm  { index: u16 },
    SwitchArm{ pat: Arc<str> },
    Iter     { index: u64 },
}
```

- Cursor = scalar tuple. `cursor$` = `Stream<Cursor>`. The stream is the
  relation. Not `Stream<Vec<Cursor>>` — batching lives at sinks.
- `byte_range` is a match attribute, not cursor state. Lives on the refs
  table row.
- `path` is framework-populated. Ops never touch it. Stringified for awk-style
  matching (`rule[dep]/fs[0]/fork[2]/ast[0]`).
- Identity hash includes path so fork-arm duplicates are distinguishable.

### Rationale for scalar-stream over batch-stream

| question | answer |
|---|---|
| cursor$ = tuple or batch? | tuple — prolog/SQL semantics from stream topology |
| where does batching happen? | at sinks: `.bufferTime(t)` before `write_rows` |
| daemon live updates? | natural — one new file = one new cursor |
| cross-cursor ops? | explicit rx operators (`groupBy`, `combineLatest`) |

---

## 11. Scheduling + reading

### Two-phase per level (min allocations)

- **Pass A (discovery)**: fs/repo/rev expand via Reader metadata streams.
  No byte reads. Produces the level-N cursor set.
- **Pass B (extraction)**: content ops request `bytes$`/`parsed$`. Reader
  internally coalesces via `shareReplay(1)` keyed by `(repo, rev, fs)`.

This is scheduler-side in batch mode, Reader-side in daemon mode. Same trait
surface. No op suspension in batch (reads are warm by the time execute runs);
daemon ops await naturally via the stream.

### Sync vs async

- Sync, once per run: `reader.repos$()` and `reader.revs$()` first emission —
  seed the pipeline.
- Async, everything else: every `bytes$`, `parsed$`, `cross_ref$`, `write_*`.

### Cycle prevention

`run_visits` table keyed on `(run_id, op_id, path_hash, cursor_hash)`. Runner
checks before dispatching an op for a cursor. Prevents re-entry.

---

## 12. Effect routing / audit

The Runner (via OpCtx) wires Reader + Writer into ops. Logging is a decorator
wrapping both:

```
LoggingReader  → delegates, also sends to effect_log channel
LoggingWriter  → delegates, also sends to effect_log channel
Runner owns the channel → buffers → write_effect_log in batches
```

Every effect record carries `run_id`, `op_id`, `sprf_loc`, `cursor_id` (if
tied). Tests inject a `RecordingReader + RecordingWriter` pair to golden-file
the effect log.

---

## 13. Diagnostics — fire-and-forget sink

```rust
pub struct OpCtx {
    pub run_id: RunId,
    pub op_id:  OpId,
    pub reader: Arc<dyn Reader>,
    pub writer: Arc<dyn Writer>,
    pub config: Arc<Config>,
    pub diags:  DiagSink,          // Arc<UnboundedSender<Diagnostic>>
    pub events: EventSink,
}
```

Ops call `ctx.diags.emit(d)`. Non-blocking. Runner rx-side multiplexes to:
- `RunEvent::DiagnosticBatch` (CLI / LSP subscribers)
- `writer.write_effect_log` (persisted batches)

Each diagnostic carries the originating cursor's `path.last().sprf_loc`
automatically, so no manual sprf-loc threading.

### Zero fs match = error by default

`fs(...)` emits an error diagnostic and halts the chain if `files$` returns
empty. Override: `fs[empty_ok](...)`. Content-match failures (json, ast) are
**info**-level (expected negative result), not errors.

---

## 14. Rule-as-operator

No special "rule" construct. `$rule[name](args){body}` is just an op whose
factory registers itself in `ProgramCtx.rules`:

```rust
pub struct ProgramCtx {
    pub rules:     HashMap<String, RuleHandle>,
    pub constants: HashMap<String, Capture>,
    pub config:    Arc<Config>,
}
```

Same for `$check`, `$render`, `$namespace`, `$let`, `$include`. The language
is the registry. Adding a top-level keyword = registering an `Operator`.

Two-pass parse:
1. `Operator::pre_register` collects rule/namespace/let names.
2. `Operator::parse` lowers bodies with full ProgramCtx available (so
   forward cross-refs resolve).

---

## 15. Extractors

```rust
pub trait Extractor: Send + Sync {
    fn name(&self)    -> &'static str;              // "js" / "rs" / "sprf:{rule_name}"
    fn accepts(&self, file: &FilePath) -> bool;
    fn chain(&self)   -> &Pipeline;                  // ready-to-run
    fn spec(&self)    -> &[RuleTableSpec];
}
```

- `JsExtractor`, `RsExtractor`, `AstConstExtractor` — hardcoded in Rust,
  accept by extension.
- `SprfExtractor` — one instance per parsed `$rule`. `chain()` returns the
  lowered Pipeline.

All three implement the **same trait**, feed the **same strings + refs**
tables via Writer. sprf is not a special case; it's data-driven parity.

---

## 16. Config + caching

```rust
pub struct Config {
    pub repos, revs, fs_exclude, sprf_files, cache, shell, runtime, ...
    pub content_hash: u64,
}

pub struct RuleCacheKey {
    pub rule_content_hash: u64,           // hash of rule source bytes
    pub input_content_hashes: Vec<u64>,   // cross-ref'd rules' hashes
    pub config_hash: u64,                 // relevant config slice
}
```

- Hot signal; `apply_config(new)` bumps hash, Runner computes diff, restarts
  only affected pipelines.
- `sprf_meta` table carries `RuleCacheKey` per rule; match = skip + emit
  `RunEvent::RuleSkipped`.
- Manual apply for now ("k8s apply" style). Auto-watch later.

---

## 17. Module layout

```
v2/src/
  lib.rs                       pub struct Sprefa
  main.rs                      SprefaCLI
  _0_types.rs                  Cursor, Span, Location, Diagnostic, ids, enums, SprfPath
  _1_config.rs                 Config, ConfigDiff, content_hash
  _2_reader.rs                 Reader trait + LoggingReader decorator
  _3_writer.rs                 Writer trait + LoggingWriter decorator
  _4_op.rs                     Op + Operator traits, OpCtx, OpInvocation, BraceMode, Pipeline
  _5_extractor.rs              Extractor trait
  _6_runner.rs                 Runner trait + default impl + RunEvent
  _7_parse.rs                  host parser → OpInvocation; parse_commons
  _8_registry.rs               OperatorRegistry (lispy aliases); lower_rules; two-pass
  readers/
    _0_mem.rs                  MemReader (tests + LSP buffers)
    _1_fs.rs                   FsReader (disk + git)
  writers/
    _0_mem.rs                  MemWriter
    _1_sqlite.rs               SqliteWriter
  operators/
    _0_repo.rs _1_rev.rs _2_fs.rs _3_json.rs _4_ast.rs _5_cross_ref.rs _6_check.rs _7_rule.rs _8_fork.rs _9_switch.rs
  extractors/
    _0_js.rs _1_rs.rs _2_sprf.rs
```

Dependency strictly numeric-ascending, underscore-prefixed per project
convention.

---

## 18. Boundary invariants (must never break)

1. **SQL stays behind Writer/Reader. Forever.** Tests with MemWriter +
   MemReader prove no SQL leaks into ops.
2. **Memory-only files indistinguishable from disk.** LSP buffer vs mmap is
   opaque at the Reader boundary.
3. **All writes `&[T]`, `write_*` prefixed.** No singular write exists.
4. **Ops never sync-read from storage.** Only the initial repo/rev seed is
   sync.
5. **Ops never touch cursor.path.** Framework populates.
6. **Effects log every operation.** Ops emit via OpCtx; Writer persists in
   batches.

---

## 19. Execution modes

| mode | Reader impl | Runner behavior | termination |
|---|---|---|---|
| immediate | FiniteReader | folds once, drains, writes | `Stream::Completed` |
| daemon | HotReader + notify | keeps hot, reruns affected | `ctrl-c` or config apply |
| check | FiniteReader (read-only) | runs check SQL after deps | drains |
| rewrite | FiniteReader + FsEditWriter | runs rules, applies edits | drains |

All four use the **same Pipeline** per rule. Mode = pick Reader impl + pick
Runner lifecycle.

---

## 20. CLI entrypoint (smallest slice)

```
main()
  ├── cli_parse()
  ├── load_config()
  ├── Sprefa::new(config, /* mem or fs reader, mem or sqlite writer */)
  └── sprefa.runner.run_immediate(sprf).for_each(print_event)
```

SprefaLSP: same `Sprefa`, different `Reader` impl (buffer-aware), subscribes
to RunEvents, replays Diagnostics into LSP publish messages.

---

## 21. Implementation order (step 1 MVP)

1. `_0_types.rs` with Cursor, SprfPath, Span, Location, Diagnostic, IDs.
2. `Reader` + `Writer` traits with `MemReader` + `MemWriter`.
3. `Op` + `Operator` traits + `Pipeline` enum + `OpCtx` with diag sink.
4. Host parser → `OpInvocation`; `parse_commons` for `$NAME` / `${rule.$V}` /
   `$$prefix`.
5. `Registry` with lispy aliases; two-pass lower.
6. Ops: `$rule`, `$fs`, `$json`. Enough for kitchen-sink parity.
7. Runner: `run_immediate` with fold-over-Pipeline + Writer flush.
8. One e2e test: parse `.sprf`, run against MemReader, assert rows in
   MemWriter + diagnostics stream.

Later:
- `$ast`, `$line`, `$check`, `$render`, `$cross_ref`, `$marker`
- `$fork` / `$switch` / `$if`
- SqliteWriter + FsReader
- Daemon + config apply
- LSP wiring
- Dogfood: `$marker` + `$render` regenerate op grammars into `_N_op_grammars.rs`

---

## 22. Deferred but designed-in

- Coroutine `OpStep::Await` for streaming suspension (currently implicit via
  Stream).
- Byte-range reads for huge files (whole-file is default).
- Transitive view generation (`_deep`) — user-opt-in per rule.
- Rewrite op negation / `fs.absent`.
- `$if` / `$switch` awk-style conditionals keying on cursor path or channel.
- Path-based `sprefa trace --path "rule[deps]/**"` CLI debug.

---

## 23. What we are deliberately not doing

- No EAV; one row per extraction event with captures as columns (v1 shape).
- No `_expanded` / `_deep` views by default; one view per rule, one-hop FK.
- No abstract check DSL; SQL templates with rule-name rewrite.
- No per-row writes; bulk-only.
- No cursor-batch streams; scalar streams with sink-side buffering.
- No Store/Effector/Router jargon; Reader, Writer, Runner.
- No threading of sprf_loc through returns; SprfPath on cursor + diag sink.
- No sync fs/storage in ops.

---

## 24. Glossary

| term | meaning |
|---|---|
| Cursor | one tuple of provenance + captures flowing through the pipeline |
| $$ prefix | reserved provenance capture (repo/rev/fs + norm variants) |
| ${rule.$V} | cross-ref — creates FK edge in DAG + column |
| SprfPath | per-cursor trail of op invocations for awk-style routing |
| RunVisit | memoization key to prevent cycle re-entry |
| ProgramCtx | parse-time symbol table (rules, constants, config) |
| Pipeline | Op / Seq / Fork / Switch — the runtime-executable tree |
| Backbone | shared strings + refs tables across all extractors |
| Discovery | unscanned_* streams that feed Runner seed with new combos |

---

---

## 25. ParseSite — compile-time stable coordinate

Runtime `SprfPath` is per-cursor. We also need a **parse-time coordinate**
for every OpInvocation and every pattern leaf — independent of runtime,
stable across re-parses when source is unchanged.

```rust
pub struct ParseSite {
    pub file:       Arc<Path>,
    pub path:       Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

pub enum ParseSeg {
    Top        { index: u16 },                // Nth top-level op in file
    BraceChild { index: u16 },                // Nth child of a brace body
    ParenChild { index: u16 },                // (rare) Nth child inside paren
    PatternLeaf{ key: Arc<str> },             // leaf inside op-internal pattern (json/ast)
}
```

Stringified: `rules.sprf#top[2]/brace[0]/leaf[a.x]`.

### Who carries ParseSite

- Every `Op` instance (via `op.parse_site()`)
- Every `LeafPattern` inside pattern-owning ops (json, ast, toml, etc.)
- Every `Diagnostic` (default `sprf_loc`)
- Every `PathSeg` in runtime `SprfPath` — back-link from cursor to source
- Cache keys (`RuleCacheKey.parse_site_hash`)
- LSP go-to-def / hover — click a row, jump to the exact leaf

```rust
pub enum PathSeg {
    Op      { name: Arc<str>, parse_site: Arc<ParseSite>, step: u16 },
    Named   { name: Arc<str>, key: Arc<str>, parse_site: Arc<ParseSite> },
    ForkArm { index: u16, parse_site: Arc<ParseSite> },
    SwitchArm { pat: Arc<str>, parse_site: Arc<ParseSite> },
    LeafArm { key: Arc<str>, parse_site: Arc<ParseSite> },
    Iter    { index: u64 },
}
```

One structure, two coordinate systems: `parse_site` (where in source) +
path ordinals + Iter indices (where in runtime traversal).

---

## 26. Implicit tree branching inside patterns

v1 json supported:

```
$json({
  a: { x: $X },
  b: { y: $Y }
})
```

Two leaves → two captures → two rows. "In tree terms these are just nested
rules" — but stays inside one op.

### Decomposition in factory

`JsonOp::parse` walks the pattern AST and produces `Vec<LeafPattern>`:

```rust
pub struct JsonOp {
    pub leaves:     Vec<LeafPattern>,
    pub parse_site: Arc<ParseSite>,
}

pub struct LeafPattern {
    pub key_path:   Vec<String>,          // ["a", "x"]
    pub capture:    String,                // "X"
    pub parse_site: Arc<ParseSite>,        // ends in PatternLeaf{"a.x"}
    pub cross_refs: Vec<CrossRef>,
}
```

### Strategy: op-internal fan-out (NOT Pipeline::Fork)

The op emits N cursors per input, one per leaf. Pipeline stays a single
`Pipeline::Op` node; tree structure is contained.

```coffee
JsonOp.pipe = (input$, ctx) ->
  input$.flat_map_unordered None, (cursor) ->
    ctx.reader.bytes$(cursor.repo, cursor.rev, cursor.fs).take 1
      .flat_map (bytes) ->
        tree = parse_json(bytes)
        Observable.from @leaves
          .filter_map (leaf) -> leaf.extract(tree, cursor, ctx)
      .map (emitted) ->
        emitted.push_path PathSeg.LeafArm key: leaf.key, parse_site: leaf.parse_site
```

One input cursor → N output cursors, each tagged with `LeafArm`.
Downstream ops and the rule-row sink see them as ordinary independent
cursors. 1-many problem handled at op granularity without exposing forking
at the Pipeline layer.

### Default write sink: one row per cursor

Rule `foo` with captures `X` and `Y` has columns `X_ref/X_str/Y_ref/Y_str`.
The example pattern on one file emits two rows:

| id | file | X_str | Y_str |
|---|---|---|---|
| 1 | pkg.json | "valX" | NULL |
| 2 | pkg.json | NULL | "valY" |

This is "OR / monomorph" semantics: default = OR across leaves, each row
carries one leaf's values, other captures are NULL.

### Opt-in merge: `$join_by`

For users who want one row with both columns filled:

```
$rule[foo] {
  $json({ a: {x: $X}, b: {y: $Y} })
  $join_by($$fs)                      # groupBy($$fs) + reduce(merge_captures)
}
```

`$join_by` is an explicit op. Default flat-emit preserved.

### Sugar equivalence

Implicit pattern branching is structurally equivalent to explicit fork:

```
$json({ a: {x: $X}, b: {y: $Y} })

# ≡ (conceptually)

$fork {
  $json({ a: {x: $X} })
  $json({ b: {y: $Y} })
}
```

Sugar lives inside the op so you don't pay Pipeline::Fork overhead or the
syntactic noise. Mental model: "nested captures via sugar, fork via syntax".

### Same pattern for ast/toml/yaml

`$ast[ts](function $F() { return $X })` with two capture positions follows
the same decomposition: factory parses into `Vec<LeafPattern>`, op fans
out per leaf at execute. One op, many cursors, default one row each.

---

## 27. v1 reference files (don't rediscover next session)

| concern | v1 path | what to reuse |
|---|---|---|
| backbone DDL | `crates/schema/src/migrations.rs` | repos/revs/files/strings/refs/rev_files preserved verbatim |
| per-rule table gen | `crates/schema/src/rule_tables.rs` | `RuleTableDef`, `_data`/view/`_refs` pattern, namespace rules, scan_targets |
| store trait shape | `crates/cache/src/store.rs` | `Store` trait signatures, `FileResult`/`ExtractionRow`/`CaptureEntry` shapes, `ScanContext` skip logic |
| sqlite impl reference | `crates/cache/src/sqlite_store.rs` | how FTS5 triggers + string intern + batch flush are wired |
| flush batching pattern | `crates/cache/src/flush.rs` | bulk write batching |
| cross-ref resolution | `crates/cache/src/resolve.rs` | how refs get resolved to target_file_id |
| discovery loop | `crates/cache/src/discovery.rs` | unscanned_* queries and the tier-2 discovery pattern |
| incremental scan | `crates/cache/src/scan_context.rs` | scanner_hash + sprf_meta logic |
| rule change detection | `crates/sprf/src/hash.rs` | RuleHashes, schema_hash, extract_hash |
| udfs + views | `crates/schema/src/udfs.rs` | sprf_norm and view creation |
| v1 parser | `crates/sprf/src/_1_parse.rs` (1088 lines) | reference; v2 re-writes lean host parser |
| v1 lower | `crates/sprf/src/_3_lower.rs` (1498 lines) | reference; v2 avoids the monolithic lower |
| v1 extractor | `crates/sprf/src/_4_extract.rs` | reference for ref output shape |
| v1 ops | `crates/sprf/src/ops/{fs,json,mod}.rs` | last-pass LSP-era op trait, 442 LOC total |
| JS extractor | `crates/js/` | backbone extractor; port to Extractor trait in v2 |
| RS extractor | `crates/rs/` | backbone extractor; port to Extractor trait in v2 |
| watcher | `crates/watch/` | Change/Edit/plan pipeline; refactor-mode behavior |
| cli | `crates/cli/` | subcommand structure, config loading |
| LSP | `crates/sprf-lsp/` | state.rs, snapshot.rs, hover.rs patterns |
| shadow rich types | `chat_log/v2_shadow_designs/` | CrossRef, Fix, CaptureSource — already absorbed into goals |

### What NOT to copy

- `crates/sprf/src/analyze.rs` — the guessing analyzer. Kill. Replaced by
  op-emitted diagnostics with exact byte ranges.
- `crates/sprf/src/_3_lower.rs` — 1498 LOC monolithic. Replaced by
  per-op factory.parse.
- LSP's `find_pattern_at_position()` — replaced by op.hover_at() delegation.
- v1's JSON path-mismatch heuristic. Replaced by op-internal
  `did_you_mean` + exact ParseSite on leaf diagnostics.

### Current v2 code (baseline for step 1)

```
v2/Cargo.toml
v2/src/
  lib.rs                         # exports + kitchen_sink test (passing)
  _0_types.rs    (117 LOC)       # Cursor, Capture, Context, Diagnostic, OpResult, Effect — MINIMAL, needs full expansion per §10
  _1_store.rs    ( 66 LOC)       # Store trait (old name) + MemStore — SPLIT into Reader + Writer per §3/§4
  _2_operator.rs ( 22 LOC)       # Operator trait — REWRITE per §5 (dyn-safe split)
  _3_registry.rs ( 54 LOC)       # Registry — REWRITE with lispy aliases per §5
  _4_parse_utils.rs (85 LOC)     # glob_match, levenshtein — KEEP, add parse_capture/parse_cross_ref/parse_scan
  operators/
    _0_repo.rs   ( 82 LOC)       # RepoOp — port to new Op trait
    _1_rev.rs    ( 44 LOC)       # RevOp — port
    _2_fs.rs     ( 65 LOC)       # FsOp — port + empty_ok flag + list$ via Reader
    _3_json.rs   (242 LOC)       # JsonOp — port + leaf decomposition per §26
    mod.rs       (  9 LOC)       # re-exports
```

Next session: rebuild v2/src from §21 step list, using this file map as the
"don't re-derive" reference.

---

## 28. Open design passes for next context

1. **Check SQL rewrite strategy** — which view (`{rule}` vs `_refs` vs data) gets substituted. Heuristic on referenced columns or explicit form `$check({rule}.{col}, ...)`.
2. **`$join_by` op semantics** — exactly which channels form the group key, how captures merge on collision, whether FKs merge.
3. **Negation / absence** — `$fs.absent(...)` vs `!fs(...)` vs `$require_no(fs(...))`. Pick one form; encode into factory aliases.
4. **Multi-hop cross-ref flatten** — when a rule FKs another that FKs a third, should the single view include `other__deeper__X`? Opt-in per rule, probably via annotation.
5. **`$include` / multi-file .sprf** — how ProgramCtx merges across files, namespace resolution across includes.
6. **Pattern unification** — shared infra between json/ast/toml/yaml pattern decomposers? or each op owns its own walker.
7. **Shell permissions** — where does the allow-list live (Config vs per-rule `$sh[allow="..."]`), how does LSP show pending cuts.
8. **Effect coroutines vs stream-await** — decide when to promote from "Stream await" to "OpStep::Await with resume token" (daemon streaming scale).
9. **Test harness shape** — golden-file effect_log + diag stream + run_event stream as the kitchen_sink assertion surface.
10. **LSP token dogfood bootstrap** — hand-write `$marker` + `$render` first, or ship LSP tokens hardcoded and retrofit.

---

---

## 29. Diagnostic = trait object, owned by the emitting op

Every op owns its diagnostics end-to-end. No central enum, no shared shape beyond what the sink needs to render and persist.

```rust
pub trait Diagnostic: Send + Sync + std::fmt::Debug {
    fn code(&self)     -> &str;             // namespaced: "json/leaf-miss", "fs/empty"
    fn severity(&self) -> Severity;
    fn primary(&self)  -> &ParseSite;
    fn render(&self, out: &mut dyn Renderer);   // op decides wording + related spans + fix
    fn run_ctx(&self)  -> Option<&RunCtx>;
}
```

- `ctx.diags.emit(Box::new(JsonLeafMiss { ... }))` — no cross-op enum to extend.
- `JsonOp` defines its own `JsonLeafMiss`, `JsonKeyTypo`, `JsonAbsentViolated`, their own fix shapes, their own `did_you_mean`.
- Renderer is a sink-defined trait; CLI impl emits terminal text, LSP impl emits LSP Diagnostic structs, RecordingWriter impl captures structured JSON for snapshots.
- Suppression: user writes `suppress = ["json/*"]` in Config; the string comes from `code()`. No registry of known codes — typos just match nothing.

Same principle applies to Fixes: each op defines its own Fix type, exposed through the Diagnostic's `render` call as whatever edit/code-action the op wanted. No shared `enum Fix`.

---

## 30. RunEvent stream

The Runner's sole public output. Everything else is a side effect through Writer.

```rust
pub enum RunEvent {
    RunStarted    { run_id: RunId, config_hash: u64 },
    RuleSkipped   { rule: Arc<str>, reason: SkipReason },
    RuleStarted   { rule: Arc<str>, parse_site: Arc<ParseSite> },
    CursorIn      { rule: Arc<str>, cursor_hash: u64 },
    CursorOut     { rule: Arc<str>, cursor_hash: u64, rows_written: u32 },
    DiagBatch     { diagnostics: Vec<Box<dyn Diagnostic>> },
    EffectBatch   { effects: Vec<EffectLogRow> },
    FlushStarted  { run_id: RunId, table_count: u32 },
    FlushCompleted{ run_id: RunId, rows: u64, bytes: u64, elapsed_ms: u32 },
    Backpressure  { rule: Arc<str>, lagged: u32 },
    RunCompleted  { run_id: RunId, status: RunStatus },
}
```

Structural framework enum. Ops cannot extend this; it's the Runner's surface.

---

## 31. Cursor identity hash

Deterministic, 64-bit, path-sensitive. Used in `run_visits`.

```
cursor_hash = fxhash((
    repo, rev, fs_or_null(),
    captures.sorted().map(|(k,v)| (k, v.ref_id_or_str_hash())),
    fks.sorted(),
    path.static_part_hash()          // op name + parse_site id per seg, Iter excluded
))
```

Iter excluded so fork-arm identity stays stable, iteration counter is observational.

---

## 32. Backpressure + watermarks (daemon)

- Upstream-of-sink buffer per rule: `buffer(N).drop_oldest_on_overflow()`, `N` from Config.
- Overflow emits `RunEvent::Backpressure { rule, lagged }`. LSP surfaces lag.
- Watermark per rule = max cursor_out_ts. Flush fires on earlier of `buffer_full` / `flush_interval_ms`.
- Immediate mode ignores backpressure; streams are finite, buffer unbounded.

---

## 33. Shell + rewrite safety

### Shell

```rust
pub struct ShellCall {
    pub program, args, cwd, stdin, timeout,
    pub parse_site: Arc<ParseSite>,
}
```

- Config allow-list: `shell.allow = ["git", "rg", "./scripts/*"]`, glob-matched on `program`. Per-rule override `$sh[allow="./tools/foo"]`.
- `Writer::shell_batch` is the only executor. Denial = ShellOp emits its own `ShellDenied` diagnostic.
- `LoggingWriter` logs every call with truncated stdout/stderr digest in `effect_log`.
- LSP renders pending calls as code-lens approvals.

### Rewrite

```rust
pub struct FileEdit {
    pub repo, rev, fs, byte_range, replacement,
    pub parse_site: Arc<ParseSite>,
    pub rewrite_kind: RewriteKind,
}
```

- Non-overlap enforced within a file per run. Conflict = RewriteOp emits its own conflict diagnostic with both ParseSites.
- Sort edits by `byte_range.start` descending, apply in order.
- Dry-run: `FsEditWriter` emits `RunEvent::ProposedEdit` instead of writing.

---

## 34. Check SQL rewrite (resolves §28 Q1)

```
$check[no_duplicate_imports]({
  SELECT path FROM {imports} GROUP BY path HAVING count(*) > 1
})
```

Substitution: `{name}` → rule view, `{name}.refs` → refs-join view, `{name}.data` → raw `_data` table. `CheckOp` scans its own SQL body for `{ident}` / `{ident.suffix}`; unresolved idents produce its own diagnostic.

Check body stored as view `CREATE VIEW "{ns}__check__{name}" AS {sql}`; violations persisted via Writer with columns = SELECT shape + stable hash column.

Cross-rule joins are user-written SQL — no magic join inference.

---

## 35. `$join_by` in Rx terms

`$join_by($$fs)` is literally:

```
cursor$
  .groupBy(cursor => cursor.repo + cursor.rev + cursor.fs)   // key selector
  .flatMap(group$ =>
    group$
      .scan(empty_cursor, (acc, c) => merge_captures(acc, c))
      .last()                                                 // emit when group$ completes
  )
```

- `groupBy` — same as RxJS / rx-rust. Emits one inner observable per distinct key.
- `scan` — fold that emits intermediate values; used here to accumulate captures.
- `last()` — waits for the inner group to complete, emits one cursor.

Immediate mode: upstream finite → each group's `last()` fires naturally.
Daemon mode: wrap inner with `.debounceTime(t)` instead of `.last()` so long-lived groups still flush.

No new concept. Standard Rx groupBy + reduce, where the reducer merges capture maps.

---

## 36. Absence (resolves §28 Q3)

Bracket flag. One factory per op owns its own negation semantics:

```
$fs[absent](pattern)
$json[absent]({a: {x: $X}})
$ast[absent][ts](function $F)
```

- Stackable: `$fs[absent, empty_ok]`.
- Success = one cursor passes the filter, no captures bound.
- Declaring a capture inside an absent body = the owning op's `CaptureInAbsent` diagnostic at lower.
- No `!fs` prefix. No `$require_no` wrapper. One mechanism.

---

## 37. `$include` semantics (resolves §28 Q5)

```
$include("./common.sprf")
$include("./lib/*.sprf")
```

- Resolved at parse time relative to including file. Each included file parses to its own ProgramCtx slice; IncludeOp merges before `lower_rules`.
- Late binding: including file wins on rule-name collision; IncludeOp warns with shadowed `ParseSite`.
- Cycle = IncludeOp's `IncludeCycle` diagnostic.
- Namespace: child `$namespace` wins over parent.
- Cache key folds every transitive include's content hash, so any change re-runs affected rules.

---

## 38. Pattern decomposers (resolves §28 Q6)

Each pattern-owning op owns its own walker. No shared `PatternTree` trait. JsonOp walks its own json, AstOp walks tree-sitter Query captures, TomlOp walks its own toml. The shape they emit downstream is a `Vec<LeafPattern>` inside their own module — structurally similar but independently evolved. Full dyn until duplication pain is concrete.

Common ground lives only at the cursor boundary: each leaf's fan-out produces a cursor with `PathSeg::LeafArm { key, parse_site }`, which is framework shape (§10).

---

## 39. Test harness (resolves §28 Q9)

```rust
#[test]
fn kitchen_sink() {
    let fixtures = load_fixtures("tests/fixtures/kitchen_sink");
    let reader   = MemReader::from(&fixtures);
    let writer   = RecordingMemWriter::new();
    let sprefa   = Sprefa::new(config, reader, writer.clone());

    let events = sprefa.runner.run_immediate(fixtures.sprf).collect().block_on();

    assert_yaml_snapshot!("events",  events);
    assert_yaml_snapshot!("rows",    writer.snapshot_rows());
    assert_yaml_snapshot!("effects", writer.snapshot_effects());
    assert_yaml_snapshot!("diags",   writer.snapshot_diags());
}
```

- Four golden files per scenario: events, rows, effects, diags. `insta` snapshots.
- RecordingMemWriter sorts rows by `(repo, rev, fs, parse_site_str, row_key)` for stable diffs.
- Hash-dependent fields (run_id, timestamps) replaced with stable tokens in the snapshot serializer.
- Diag snapshot uses each op's own Renderer output — ops that add diagnostics update only their own snapshots.
- Same harness for kitchen sink, per-op unit tests, regression fixtures.

---

## 40. LSP bootstrap order (resolves §28 Q10)

Hardcode tokens first, dogfood later.

| phase | source of truth | gate |
|---|---|---|
| 1 | hand-written `_N_op_grammars.rs`, OperatorRegistry exports `tokens_for_grammar()` | initial |
| 2 | `$marker` + `$render` generate the same file; assert identical to hand-written | after `$check` lands |
| 3 | delete hand-written; generated is truth | after phase 2 green in CI for 2 weeks |

---

## 41. Runtime model

- `tokio` multi-thread, worker count from Config. Streams are `BoxStream<'static, _>`.
- Ops, Operators, Readers, Writers all `Send + Sync + 'static`. All trait-object friendly.
- `SqliteWriter` wraps a connection pool; sqlite writes are `spawn_blocking` inside the Writer (ops see only async streams).
- `FsReader` wraps `Arc<gix::Repository>` per repo.
- CLI and LSP subscribe to `Stream<RunEvent>` on the same runtime.

---

## 42. Versioning + migrations

- Schema version in `sprf_meta.schema_version` (u32, monotonic).
- Backbone migrations: numbered SQL files in `migrations/`, applied in order inside one transaction by `SqliteWriter::open`.
- Per-rule tables are disposable — rule source hash change drops + recreates `_data`.
- Config version field on Config; explicit `migrate_config_v{N}_to_v{N+1}` functions. Mismatch = CLI error, never silent.

---

End of v2 design goals. Implementation is step 1 (§21) above, nothing more until
kitchen sink passes end-to-end with MemReader + MemWriter.
