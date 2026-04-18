# sprefa v2

Pipeline engine for `.sprf` files. Parses declarative rules into op pipelines, runs them against repos/revisions/files, produces cursors with typed captures and slots.

Hot-path rules and the measured Reader/Store drift live in `CLAUDE.md`.
Read that before editing reader/store/op code.

---

## System diagram

```
                         .sprf source text
                              │
                    ┌─────────▼──────────┐
                    │  _8_parse.rs        │
                    │  host_parse()       │
                    │                     │
                    │  tokens:            │
                    │   IDENT  ( ) { }    │
                    │   > ; #             │
                    │   &.$X desugar      │
                    └─────────┬──────────┘
                              │ Vec<(name, Pipe)>
                              │ Pipe = Vec<OpInvocation>
                    ┌─────────▼──────────┐
                    │  Operator::parse()  │
                    │  per-op lowering    │
                    │                     │
                    │  OpInvocation       │
                    │  → LoweredOp        │
                    │  → Pipeline tree    │
                    └─────────┬──────────┘
                              │ Vec<(rule_name, Pipeline)>
                              │
         ┌────────────────────▼────────────────────┐
         │              Pipeline enum               │
         │  Op(LoweredOp)  Seq([P])  Fork([Arm])   │
         │                Switch{arms}              │
         └────────────────────┬────────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │  Pipeline::run()    │
                    │                     │
                    │  Op  → op.pipe()    │
                    │  Seq → chain        │
                    │  Fork → union       │
                    └─────────┬──────────┘
                              │ BoxStream<Arc<[Cursor]>>
                              │
         ┌────────────────────▼────────────────────┐
         │            DocSession / Runner            │
         │                                          │
         │  cursors_by_rule: HashMap<rule, Vec<C>>  │
         │  span_ix: Vec<SpanEntry>                 │
         │  hover_at(pos) → markdown                │
         │  completions_at(pos) → suggestions       │
         └──────────────────────────────────────────┘
```

## Anatomy of a .sprf file

```sprf
# one rule = one named pipeline
rule(deploy_refs) {
  > repo(myorg/*)                          # filter/bind repos
  > rev(main|release/*)                    # filter/bind revisions
  > fs(**/values.yaml)                     # fan-out to files
  > json({ image: { repository: $REPO,    # walk JSON structure
                     tag: $TAG } })
};

# cross-rule reference: rows from deploy_refs seed this rule
rule(internal_deps) {
  > repo(${deploy_refs.$REPO})
  > rev(${deploy_refs.$TAG})
  > fs(**/package.json)
  > json({ dependencies: { $DEP: $_ } })
};

# cursor rebase: extract sub-content, chain into next op
rule(nested) {
  > fs(**/config.json)
  > json({ code: $C })
  > &.$C                                  # rebase cursor onto $C value
  > json({ version: $V })                 # parse the sub-content as JSON
};

# fork: multiple extraction paths from same input
rule(config) {
  > fs(**/config.yaml)
  > json({
    database: { host: $HOST };             # arm 1
    cache: { ttl: $TTL };                  # arm 2
  })
};
```

## Host grammar (v2.1)

```
program := stmt*
stmt    := chain ';'?
chain   := op ('>' op)*
op      := IDENT ('[' bracket ']')? ('(' paren ')')? ('{' brace '}')?
brace   := stmt*
```

- `>` = Seq (chain ops in pipeline order)
- `;` = Fork (distribute: each arm runs against parent cursors, results union)
- `#` = line comment
- `&.path` desugars to `cursor_ref(.path)` at parse time

## Token classification

| Token | Where | Meaning |
|---|---|---|
| `$NAME` | paren/brace body | capture variable (bind + column) |
| `$_` | paren/brace body | wildcard (match, no bind) |
| `$$sigil` | walker body | scan annotation (repo/rev/fs discovery) |
| `${rule.$VAR}` | paren body | cross-rule reference (seeds from upstream rule) |
| `&.$NAME` | pipe position | cursor rebase onto capture value |
| `&.fs` `&.repo` `&.rev` | pipe position | cursor rebase onto field |

## Op registry

| # | Op | Paren | Brace | Effect |
|---|---|---|---|---|
| 0 | `rule(name)` | rule name | CustomSprf (full grammar) | define named pipeline, seed cursors from (repo,rev) combos |
| 1 | `repo(pattern)` | glob or `$R` | DefaultFork | filter/bind cursor.repo |
| 2 | `rev(pattern)` | glob or `$R` | DefaultFork | filter/bind cursor.rev |
| 3 | `fs(glob)` | glob or `$F` | DefaultFork | fan-out: 1 cursor → N (one per file match) |
| 4 | `read` | none | WalkerPattern | load file bytes → cursor.content |
| 5 | `json({...})` | walker pattern | WalkerPattern | walk JSON/YAML/TOML, emit rows with captures |
| 6 | `cursor_ref` | path expr | DefaultFork | rebase cursor onto captured sub-content (desugar only) |

## Cursor lifecycle

```
rule(R) seeds:     Cursor { repo: "x", rev: "main", fs: None, content: None, captures: {} }
    │
repo(myorg/*):     filter on cursor.repo, or bind $R
    │
rev(main):         filter on cursor.rev, or bind $R
    │
fs(**/*.json):     fan-out → N cursors, each with cursor.fs set
    │
json({k:$V}):     walk content, emit rows → cursor.captures["V"] = Capture { value, kind, byte_range }
    │               also stamps: cursor.content = Some(arc), slots[JSON_TREE] = parsed tree
    │
&.$V:              rebase → cursor.content = captured_value, cursor.byte_range = span
    │               downstream sees only the sub-content
    │
json({inner:$I}):  re-parses cursor.content[byte_range] (content-contract PATH B)
```

## Content source-of-truth contract

Every byte-reading op follows this dispatch order:

1. **PATH A** -- slot reuse: if typed slot (e.g. JSON_TREE) is set and applicable, walk pre-parsed tree
2. **PATH B** -- content priority: if `cursor.content` is Some, parse `content[byte_range or 0..]`
3. **PATH C** -- reader fallback: if `cursor.content` is None, call `reader.bytes(repo, rev, fs)`

This enables `&.$X` rebase to work: cursor_ref sets content/byte_range, downstream op parses from content.

## Walker DSL (json body)

```
pattern    := object | array | capture | annotation | wildcard | value_glob
object     := '{' (entry (',' entry)*)? '}'
entry      := key ':' pattern
key        := '**' | '$NAME' | '$_' | 're:' REGEX | glob_str
array      := '[' '...' pattern ']'
capture    := '$NAME'              # bare → CaptureAny (matches any node kind)
wildcard   := '$_'                 # assert presence, discard value
annotation := '$$' IDENT '(' '$' NAME ')'    # scan pointer: $$repo($R), $$rev($T)
```

- `{key: $V}` → walk object, bind value to V
- `{$K: $V}` → iterate all keys, bind each pair
- `{**: pat}` → recursive descent
- `[...$ITEM]` → array iteration
- `{re:^prefix: $V}` → regex on key name
- `$$repo($R)` → bind + register for demand scanning
- Multi-key captures `{a:$A, b:$B}` merge into one row (is_row_field)

## Pipeline tree shape

```
Pipeline::Op(LoweredOp)           # single op node
Pipeline::Seq([Pipeline])         # chain: A > B > C
Pipeline::Fork([ForkArm])         # distribute: A ; B (union results)
Pipeline::Switch { key, arms }    # conditional dispatch
```

op_path encoding in SpanEntry (for hover/analysis):
- `[rule_idx, 0]` → the rule op itself
- `[rule_idx, 0, fork_idx, op_idx]` → op inside rule body

## Key files

```
v2/src/
  _0_types.rs          Cursor, Capture, CaptureKind, ParseSite, FilePath, Slots, SlotKey
  _1_diagnostic.rs     Diagnostic trait, Renderer
  _5_op.rs             Op trait, Operator trait, Pipeline enum, LoweredOp, OpCtx, hover helpers
  _8_parse.rs          host_parse(), &. desugar, OpInvocation
  analysis.rs          DocSession: run_pipelines, hover_at, completions_at, span_ix
  path_expr.rs         PathExpr: .$NAME, .fs, .repo, .rev (used by cursor_ref)
  ops/
    _0_rule.rs         rule(name) op
    _1_repo.rs         repo(pattern) op
    _2_rev.rs          rev(pattern) op
    _3_fs.rs           fs(glob) op
    _4_read.rs         read op
    _5_json.rs         json({...}) op + content contract impl
    _6_cursor_ref.rs   cursor_ref (desugar-only) + hover delegation
  walk/
    _1_compiled.rs     CompiledStep enum (Key, Leaf, CaptureAny, Object, Array, ...)
    _2_compile.rs      SelectStep → CompiledStep lowering
    _3_walker.rs       tree walker: execute CompiledStep against parsed JSON/YAML/TOML
    _4_brace_parse.rs  walker DSL parser: brace body → SelectStep tree
```

## Tests

```
v2/tests/
  cursor_ref.rs              rebase shape + chained-json contract
  hover_render.rs            hover via DocSession (capture delegation, field hover)
  doc_session.rs             DocSession lifecycle
  rule_json.rs               json extraction end-to-end
  rule_json_git.rs           json + git rev extraction
  json_slot_byte_range.rs    slot/byte_range narrowing
  scan_pointer_grammar.rs    $$annotation parsing
  scan_pointer_runtime.rs    demand scanning loop
  lsp_smoke.rs               stdio LSP binary round-trip
  smoke/                     bash HTTP/SSE smoke scripts (dogfooded vs sprefa repo)
    _0_status.sh             GET /status
    _1_run.sh                POST /run vs examples/g1_fns.sprf
    _9_all.sh                driver
```

## Binaries + HTTP surface

```
bin/
  sprefa-server         long-lived HTTP daemon. Owns tokio runtime, SQLite
                        store, ResultStore, LspLayer, workspaces. Listens
                        on unix socket (preferred) + TCP (escape hatch).
                        Writes server.json (pid, transports, paths) on
                        startup. Catches SIGINT + SIGTERM; in-band
                        shutdown via POST /shutdown.

  sprefa                thin HTTP CLI. Reads server.json, dials unix first,
                        POSTs /run, streams SSE. Subcommands: run, status,
                        stop.

  sprefa-lsp            thin stdio ↔ /lsp WebSocket proxy. Editors spawn
                        it like a normal stdio LSP; bytes shuttle verbatim
                        onto the WS channel; server reassembles into one
                        stream and feeds tower-lsp's Content-Length framer.

  sprefa-v2-lsp         legacy stdio LSP (pre-daemon). Kept while
                        lsp_smoke.rs depends on it; removed when D5 lands
                        auto-spawn + a daemon-backed smoke.
```

HTTP routes:

| Route | Method | Purpose |
|---|---|---|
| `/status` | GET | version + lsp_doc_count + workspaces |
| `/run` | POST | streamed SSE of run events (meta, cursor, expr_done, diag, done) |
| `/lsp` | GET | WebSocket upgrade — LSP byte channel |
| `/shutdown` | POST | trip cancel_root + return |

Server state modules (`src/server/`):

```
_0_state.rs          ServerState, ServerOpts, ServerInfo, default paths
_1_workspace.rs      WorkspaceRegistry — path hint → WorkspaceCtx cache
_2_lsp_layer.rs      LspLayer: per-URI session, subscriber fan-out
_3_run.rs            run_pipeline: workspace → parse → lower → init_cursors
_4_transport_lsp.rs  /lsp WS upgrade + Backend (hover, completion, diags)
_5_transport_http.rs axum Router, SSE encoder, unix + TCP listen loops
```
