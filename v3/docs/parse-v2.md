# sprefa v3 — language spec (consolidated)

> Single source of truth. Replaces scattered prose in
> `crates/sprefa_parse/parse.md` (2759 lines, mixed live/retired/spike),
> bd cards, and chat_log archives. **Locked** sections describe behavior
> shipping in `master` today. **Target** sections are pinned designs
> with bd cards filed; do not author against them yet. **Retired**
> sections are removed; nothing should reference them.

## At a glance

```sprf
# Drift detection: every Rust unwrap() ships a warning diagnostic.
fs(glob(crates/**/*.rs))
  > ast[rust](${V?}.unwrap())
  > lsp[warn](
      message = "unwrap() in shipped path",
      code    = "selfcheck/no-unwrap");
```

Cursor-pipeline language. Statements are `;`-separated pipes. A pipe is
a left-to-right `>` chain of ops. Each op consumes a stream of
`Cursor`s and emits a stream. A cursor carries `(content, byte_range,
captures, repo, rev, fs, slots, last_bound)`. Every op file under
`crates/pipeline/src/ops/` declares `from_values` (lowered values) or
`from_paren_node` (raw CST) — those signatures are the language.

Reading order: `crates/sprefa_parse/parse.md` for grammar history,
`crates/pipeline/src/_0_cursor.rs` and `_1_op.rs` for the runtime
contract, `crates/tree-sitter-sprefa/grammar.js` for the host grammar.

## Lexical

- **Casing** (`project_v3_casing`): lowercase identifiers = ops + rules
  (`repo`, `fs`, `ast`, `tag`, `my_rule`). UPPERCASE identifiers =
  captures + terms (`R`, `HOOK`, `SCOPE`). Prolog convention.
- **Sigils**:
  - `${NAME}` — Read-mode term (must already be bound).
  - `${NAME?}` — Unbound introducer (must NOT be bound; binds from
    the match). Brace-mandatory (`sprefa-9lt`).
  - `&.PATH` — cursor-ref (read a cursor field; see `cursor_ref` op).
  - `:name` — atom literal.
  - `$$$NAME` — multi-node ast-grep metavar (only inside `ast[lang]` /
    `ast_yaml[lang]` bodies).
- **Punctuation**: `>` sequence, `;` statement / fork-arm separator,
  `{...}` brace block (rule body or fork), `(...)` op paren-slot,
  `[...]` op bracket-slot, `#` line comment.
- **Strings**: `"..."` double-quoted, `r"..."` raw, plus integer/float
  literals.
- **`_NAME?` convention** (NOT a primitive): leading `_` on a capture
  name is stylistic ("don't care"). The runtime treats it as any other
  Unbound introducer. The bare `_` token is reserved for future use;
  do not document it as wildcard.

### Retired tokens

- Bare `$NAME` (no braces) outside ast-grep bodies. Brace-mandatory.
- `$$` (ans-ref) — replaced by `last_bound` field + ops that consult it.
- `&&` — see `parse.md §6.4`.

## Cursor + bindings

`crates/pipeline/src/_0_cursor.rs`:

```rust
pub struct Cursor {
  content:      Arc<[u8]>,
  byte_range:   Range<usize>,
  captures:     Vec<Capture>,
  path:         SprfPath,
  last_bound:   Option<Arc<str>>,
  repo:         Arc<str>,
  rev:          Arc<str>,
  fs:           Option<Arc<Path>>,
  content_hash: Option<Arc<[u8;32]>>,
  slots:        HashMap<TypeId, Arc<dyn Any+Send+Sync>>,
}
```

- `cursor.active()` = `&content[byte_range]`. PATH-B content contract:
  every byte-reading op reads `active()` first
  (`project_content_byte_range_contract`).
- `narrow(range)` keeps slots; `rebase(content, range)` clears slots,
  `last_bound`, `content_hash`.
- **Captures** carry `name`, `byte_range`, `kind`. Two kinds:
  - `SpanBacked` — bytes live at `content[byte_range]` (ast/regex/glob
    matches inside loaded file content).
  - `Synthesized { value }` — bytes carried inline. Used by
    `repo`/`rev`/`fs` bind, by tag-projected captures, by sh stdout.
- **Term modes** (`crates/pipeline/src/value.rs`):
  - `TermMode::Read` (`${X}`) — must be bound; lower-time error otherwise.
  - `TermMode::Unbound` (`${X?}`) — introducer; must not already be
    bound.
- **`last_bound`** — `Option<Arc<str>>`, name of the most recent
  capture written upstream. Advisory: ops MAY consult, MUST NOT
  depend. Cleared by `rebase`.

## Composition

- `op_a > op_b > op_c` — `Pipeline::Seq`. Each op's batch output flows
  to the next.
- `op_a > { > arm_x; > arm_y }` — fork. Each arm runs in parallel from
  the same upstream batch; outputs interleave via mergeMap. See
  `copy_audit_fork.sprf`.
- Top-level statements separated by `;`. Each statement is a pipe (or
  rule definition).
- Brace-block `{ ... }` body of `rule(:name, ...)` — sub-pipes inside
  fire as one rule body.

## Op surface

Per op: name, paren-body kind, signature (`arg_spec`), behavior, fixture
citation. All paths are absolute under
`/Users/chrishafley/projects/sprefa/`.

### `str(...)` — value template
- Body: literal bytes plus `${NAME}` reads. No introducers.
- One cursor in, one cursor out; `cursor.content` becomes the rendered
  bytes (rebase).
- Fixture: `v3/crates/server/fixtures/golden_kitchen_sink.sprf` line
  `str("hello-from-golden-demo") > print(:str_demo);`.
- Diags: `str/term-introducer-unsupported`, `str/term-not-bound`.

### `repo(arg)` — cursor.repo filter / bind / arg-pipe
`from_values` (`crates/pipeline/src/ops/repo/mod.rs:65-92`):
- `[Atom|Str]` glob → `RepoMode::Filter` via `glob::compile_str`.
- `[Term Read]` → `Bind` (synthesized capture from `cursor.repo`,
  `last_bound = name`).
- `[Term Unbound]` → also `Bind` today (lower path collapses both
  modes for the bind side).
- `[Op]` with `try_raw_regex().is_some()` → `Filter(op)`.
- `[Op]` otherwise → `RepoMode::ArgPipe(op)` **(retired shape, see
  Retired section)**.
- Diags: `repo/missing-arg`, `value/arity-mismatch`.

### `rev(arg)` — cursor.rev filter / bind / arg-pipe
Same shape as `repo`. Adds parse-time guard
(`crates/pipeline/src/ops/rev/mod.rs:37-70`): rejects unbounded
wildcards (`*`, `**`, `**/*`, `*/*`) and bare `$NAME` capture (each rev
materializes a worktree).
- Diags: `rev/missing-arg`, `rev/unbounded-wildcard`,
  `rev/unbounded-capture`.
- Special atoms: `rev(:HEAD)`, `rev(:wt)` (working tree),
  `rev(:prod)`, etc. — atom flows through as scalar equality on
  `cursor.rev`.

### `fs(arg)` — file enumeration
`from_values` (`crates/pipeline/src/ops/fs/mod.rs:78-107`):
- `[Term]` → `Bind { name, pat = ** }`.
- `[Atom|Str]` → `Filter` via glob compile.
- `[Op]` raw-regex → `Filter(op)` (typical path — `glob(...)` produces
  the regex).
- `[Op]` otherwise → `ArgPipe(op)` **(retired)**.
- Pulls file list via `FsListFilesEffect { repo, rev }`. One input
  cursor expands to N output cursors with `cursor.fs = Some(path)`.
  Glob's named groups become Synthesized captures.
- Fixtures: `fs(glob(**/*.rs))`, `fs(glob(crates/**/Cargo.toml))`,
  `fs(glob(HOOKS.md))` (disk-mode when no upstream `repo > rev`).
- Diags: `fs/missing-arg`.

### `glob(arg)` — pattern op
- Used inside `repo()` / `rev()` / `fs()`. Compiles a glob to a regex.
- Body sub-grammar: `crates/pipeline/src/ops/glob/grammar.js`.
- Standalone use is rare; canonically `fs(glob(**/*.rs))`.

### `re(arg)` — regex pattern op
- Mirror of `glob` for raw regex. Body grammar:
  `crates/pipeline/src/ops/re/grammar.js`.
- Used inside `repo` / `fs` filter slots, or directly as a pipe op
  for in-content matching.

### `ast[lang](pattern)` — ast-grep, single-pattern surface
- `from_paren_node` only (`crates/pipeline/src/ops/ast_grep.rs`).
- Bracket-arg `[lang]` is required: `[rust]` / `[rs]`, `[ts]`, `[c]`,
  `[python]`, etc.
- Body is an ast-grep pattern with sprf carveouts:
  - `${VAR}` / `${VAR?}` → single-node capture.
  - `$$${VAR}` / `$$${VAR?}` → multi-node capture (must have no
    identifier-char neighbours).
  - Bare `$NAME` and `$$$NAME` are rejected.
- Fixtures: `ast_grep_smoke.sprf`, `golden_kitchen_sink.sprf` §3,
  `selfcheck.sprf`, `copy_audit.sprf`, `ast_grep_kernel_perf.sprf`.
- Diags: `ast/missing-lang`, `ast/unsupported-lang`,
  `ast/pattern-error`.

### `ast_yaml[lang](yaml)` — ast-grep RuleConfig surface
- `from_paren_node` only (`crates/pipeline/src/ops/ast_yaml.rs`).
- Body is YAML deserialised into ast-grep `SerializableRuleCore`. sprf
  carveouts inside the YAML rewrite to native ast-grep `$VAR` /
  `$$$VAR` before deserialisation.
- Sugared form (no `rule:` envelope) wraps automatically. See
  `ast_yaml_audit.sprf`, `ast_yaml_parity.sprf`.

### `json({pattern})` — structural walker over JSON / YAML / TOML
- `from_paren_node`. Body grammar: brace pattern with bare keys
  (`name: ${N}`), captured keys (`${K}: ${V}`), recursion (`**`),
  arrays (`[...{...}]`), regex/glob keys, quoted leaf templates.
- Dispatch on file extension: `.json` / `.yaml` / `.yml` / `.toml`
  (single op, three formats). See `json_yaml_toml_smoke.sprf`,
  `golden_kitchen_sink.sprf` §1-2.

### `comment(open[, scope_term, close])` — marker-bounded narrowing
- `from_values` (`crates/pipeline/src/ops/comment.rs`).
- Single-marker: `comment("// TODO")` — narrows to bytes after the
  marker on each matching line.
- Paired: `comment("@sprf-begin rtkq", ${SCOPE?}, "@sprf-end rtkq")` —
  binds `SCOPE` to the inter-marker byte_range. Used to set up
  `write_cursor` targets.
- Polyglot: strips language-specific comment delimiters before
  matching (planned full surface for `@sprf` re-entry is parked, see
  `comment_sprf_eval_smoke.sprf`).
- Fixtures: `comment_smoke.sprf`, `golden_kitchen_sink.sprf` §5+§7,
  `selfcheck.sprf` §3, `LANDMINES.sprf`.

### `cursor_ref` — `&.PATH`
- Routed via `from_paren_node` on `cursor_ref` CST node.
- Reads a fixed cursor field, emits a synthesized capture under the
  joined path name. Recognized: `&.repo`, `&.rev`, `&.fs`,
  `&.fs.path|ext|stem|name`, `&.byte_range(.start|.end|.len)`.

### `print([prefix])` — emit cursor.active() via PrintEffect
- `from_values`: zero or one arg (`Atom` / `Str`).
- Cursor flows through unchanged.
- Fixtures: ubiquitous (`print(:edge)`, `print(:rtkq_query)`).

### `read` — load file bytes
- `from_values`: zero args.
- Loads `cursor.fs` into `cursor.content` via `ReadBytesEffect`.
- Most ops auto-trigger `ensure_content_loaded`; explicit `read` is
  rare. Used in `copy_audit.sprf`.

### `render[fmt](template)` — output template
- `from_paren_node` only (`crates/pipeline/src/ops/render.rs`).
- Bracket-arg: `md|markdown`, `ascii|txt`, `rust|rs`, `json`, `sql`,
  `plain`. Default `plain`.
- Body: literal bytes + `${NAME}` reads (same shape as `str`).
- Rebases cursor: `cursor.content = rendered_bytes`,
  `byte_range = 0..len`, captures cleared.
- Fixtures: `golden_kitchen_sink.sprf` §7
  (`render[plain](- discovered ...)`), `sh_oasdiff_demo.sprf`.

### `sh[policy](body)` — shell out
- `from_paren_node` only.
- Bracket policy: `auto`, `cache`, `approve`, `dry`. Default `cache`.
- Body: literal bash. `$$X` / `$${X}` reads sprf captures; `$X` stays
  as bash. Output cursor.content = stdout. Synthesized captures:
  `EXIT`, `STDERR`, `ELAPSED`.
- Fixture: `sh_oasdiff_demo.sprf`.

### `fact(:name, ...)` — relational bag write or read
- First arg must be an `Atom` (bag name).
- Remaining args classified at lower time (`fact.rs:from_values`):
  - **all bound** (Read terms / literals) → write one row.
  - **all unbound** (Unbound terms) → drain bag + perpetually
    subscribe; one cursor per row; captures bind positionally.
  - **mixed** → `fact/mixed-mode` lower error (use `fact?` for the
    join shape).
- No op-args; `fact/op-arg-unsupported`.
- Fixtures: `fact_mvp_smoke.sprf`, `fact_smoke.sprf`.
- **Retired name**: `tag(...)` accepted during alias period; emits
  diag `tag/renamed-to-fact`.

### `fact?(:name, ...)` — predicate / probe / join / drain
- Predicate-mode facade (`FactPredicateOp`).
- Classification (`fact.rs:208+`):
  - all-bound → Probe (filter; cursor passes if matching row
    exists).
  - mixed → Join (filter on bound prefix, project unbound suffix
    into fresh captures, fan out per row).
  - all-unbound → Query (drain + subscribe — same path as
    `fact(...)` read).
- Fixture: `fact_predicate_smoke.sprf`.
- **Retired name**: `tag?(...)` accepted during alias period; emits
  diag `tag/renamed-to-fact`.

### `rule(:name, ${P1?}, ...) { body }` — definition
- Top-level only. Atom name. Params are Unbound introducers.
- Pass-1 lower binds body + params on `RelationStore`.
- Bodied rule with any Read-mode param is a binding-graph error.
- Fixtures: `rule_smoke.sprf`, `rule_arity_smoke.sprf`.

### `name(args)` — rule call
- `RuleCallOp` (`rule.rs:43-87`). Per input cursor: materialize args,
  seed body with synthesized captures bound to param names, run
  body sub-pipeline, drain terminals into `RelationStore.push_rule_row`.
- Cursor flows through unchanged (rule call is a side-effecting sink).
- Lazy + memoized per `(rule_name, hash(args))` for the run.

### `name?(args)` — rule predicate **(retired, see Retired)**
Code path exists (`RulePredicateOp` / `RuleMode` /
`RuleSlot::Bound|Project` in `rule.rs:121+`). The pinned design from
session 20260427 deletes this predicate path. `rule_predicate_smoke.sprf`
exercises the current code; will be removed when the deletion lands.

### `lsp[severity](...kwargs)` — diagnostic emission
- `from_paren_node` only. Severity: `error|warn|hint|info`.
- kwargs: `message="..."`, optional `code="..."`, `hint="..."`.
- Emits one `LspDiagEffect` per cursor at `cursor.byte_range` against
  `cursor.fs`.
- Cursor flows through unchanged.
- Fixtures: `lsp_severity_smoke.sprf`, `selfcheck.sprf`,
  `LANDMINES.sprf`.

### `write_cursor(${TARGET}, [:mode])` — splice into capture range
- `from_values`. First arg: Read-mode `Term` (target capture). Second
  optional: atom mode `:replace` (default) | `:append` | `:prepend` |
  `:wrap`.
- Diags: `write_cursor/missing-target`, `write_cursor/bad-mode`,
  `term/unbound-not-allowed-in-write-cursor`.
- Backed by `WriteRangeEffect`. Pairs with `comment(...)` paired form
  to bind `${SCOPE}`. Fixture: `golden_kitchen_sink.sprf` §7.

### `write_file(path)` — emit cursor.active() to a path
- `from_values`. One arg: `Str|Atom` literal, Read-mode `Term`, or
  sub-op producing `StrValue` slot (typical: `str("dir/", ${X},
  ".out")`).
- Diags: `write_file/missing-path`, `write_file/too-many-args`,
  `term/unbound-not-allowed-in-write-file`, `value/wrong-kind`.

### `void` — drop cursor
- Zero args. Used at the tail of a fork arm.

## Facts + rules — semantic table

### Fact dispatch (post-cleanup, partially Target)

| Form | Args | Action |
|---|---|---|
| `fact(:r, ${A}, ${B})` | all bound | INSERT row |
| `fact(:r, ${A?}, ${B?})` | all unbound | drain + subscribe |
| `fact?(:r, ${A}, ${B})` | all bound | predicate (subscribes; fires on row) |
| `fact?(:r, ${A}, ${B?})` | mixed | join (filter bound prefix, project unbound suffix) |
| `fact(:r, ${A}, ${B?})` | mixed (no `?`) | **Target**: SELECT B WHERE col0=A; today is `fact/mixed-mode` |
| `!fact?(:r, ${A}, ${B})` | all bound | **Target**: anti-join; fires when row is absent at seal |

The `?` exists only to disambiguate fully-bound write from fully-bound
predicate. `fact?` with mixed-or-all-unbound currently routes to Join /
Query; the pinned cleanup makes those forms `fact/predicate-mode`
errors and reserves `?` for the predicate (subscribe-on-match) shape.

**Retired / renamed**: `tag` / `tag?` are accepted as aliases during
the alias period. Each use emits diag `tag/renamed-to-fact`. Drop date
TBD per release cycle.

### Rule semantics

- Bodied rule with all-Unbound params + body pipeline: lazy, pure,
  memoized per `(name, args)`.
- Auto-fire: a top-level rule definition fires once at run start with
  empty params (drives `rule_arity_smoke.sprf`'s schema emission).
- Recursive rules (self or mutual) → `rule/cycle` lower error
  (v0; `rule_self_recursion_smoke.sprf`).

### Auto-seal (RAII) — Target

Tags do not have an explicit `tag_seal` op. Each tag carries a
refcount of "potential writer frames" within a generation. Lower-time
analysis maps each rule body and each top-level pipe to its
write-target tag set; runtime frames hold writer-shares; on Drop, the
count decrements; when it hits zero AND the run's outer scope has
completed, the generation seals. Anti-join subscribers wake on seal.

### Anti-join — Target

`!tag?(:r, ${A}, ${B})` fires when the (A, B) tuple is absent in the
sealed generation. Parks until seal. No bd card filed yet.

### Event primitive — Target (proposed)

`event(:e, ${A}, ${B})` (write) and `event?(:e, ${A?}, ${B?})`
(subscribe). Subject (no replay), distinct from tag's ReplaySubject.
Strictly intra-tick. Lower-time: subscribers must activate before
publishers (transitive over rule edges). Diags `event/late-subscriber`,
`event/cycle`. Cross-tick signaling stays the daemon's job.

## Patterns + carveouts

- Pattern ops: `re`, `glob`, `ast`, `ast_yaml`, `json`, `comment`.
- Carveout `${...}` inside a pattern body re-enters the host parser
  for term references. Pattern op rewrites carveouts to its native
  hole syntax (regex named group, ast-grep `$VAR`, etc.) before
  matching.
- Capability surface ops use to compose: `Op::try_raw_regex`,
  `Op::materialize_with`, `Op::bound_captures`, `Op::term_positions`.
  These are how `repo(re(...))` and `fs(glob(...))` work — the outer
  op pulls the inner op's regex via `try_raw_regex`.
- Per-pattern sub-grammars live alongside the op:
  `crates/pipeline/src/ops/{glob,re}/grammar.js`. Tree-sitter-sprefa
  injects them by op name (host grammar
  `crates/tree-sitter-sprefa/grammar.js`).

## Effects

`crates/pipeline/src/effects.rs`:

| Effect | Domain | Purpose |
|---|---|---|
| `FsListFilesEffect{repo, rev}` | `fs` | enumerate files (cached) |
| `ReadBytesEffect{repo, rev, path}` | `read` | load file bytes |
| `ReadBytesBatchEffect` | `read` | bulk read coalesce |
| `PrintEffect{prefix, line}` | `print` | stdout/sink emission |
| `WriteFileEffect{file, bytes, mode}` | `write_file` | whole-file write |
| `WriteRangeEffect{file, byte_range, new_bytes, mode}` | `write_cursor` | splice |
| `LspDiagEffect{file, byte_range, severity, msg, code, hint}` | `lsp` | diagnostic |
| `AstParseEffect` | internal | shared ast-grep parse cache |
| `ShEffect{policy, body, env}` | `sh` | bash exec, fingerprinted |

- Pure-effect cache: `register_pure::<E, _>(N, batcher)` on
  `RtCtxBuilder`. Keys via blake3 of effect serialization.
- Stage / approve / commit pipeline for write effects:
  `WritePolicy::{Auto, Approve, DryRun}` →
  `WriteDecision::{Approved, Pending, Rejected, DryRun}`. Sinks:
  `Disk` vs `Buffer`.
- `sh` mirrors the same shape: `ShPolicy::{Auto, Cache, Approve,
  DryRun}` → `ShDecision`.

## Diagnostics taxonomy

Codes are op-owned (`feedback_op_owns_everything`). One representative
sample per op; not exhaustive.

| Code | Op | When |
|---|---|---|
| `repo/missing-arg` / `rev/missing-arg` / `fs/missing-arg` | source ops | empty paren |
| `rev/unbounded-wildcard` / `rev/unbounded-capture` | rev | `rev(*)`, `rev($V)` |
| `value/arity-mismatch` | many | wrong arg count |
| `value/wrong-kind` | write_file | non-string path |
| `ast/missing-lang` / `ast/unsupported-lang` | ast | `ast(...)` no bracket |
| `tag/missing-name` / `tag/non-atom-name` / `tag/mixed-mode` / `tag/op-arg-unsupported` | tag | classification failures |
| `rule/cycle` / `rule/op-arg-unsupported` | rule | binding-graph |
| `write_cursor/missing-target` / `write_cursor/bad-mode` | write_cursor | |
| `term/unbound-not-allowed-in-write-cursor` / `term/unbound-not-allowed-in-write-file` | terms | |
| `read/unexpected-arg` | read | non-empty paren |
| `lsp/value-path-unsupported` / `render/value-path-unsupported` / `sh/value-path-unsupported` | paren-node-only ops | wrong lower path |
| `<op>/no-output` | any | **Target**: zero-cursor diag (`chat_log/20260427.3`) |

## Target features (pinned, not implemented)

- **WriteTarget tuple**: replace `WriteRangeEffect.file: Arc<Path>`
  with `WriteTarget { repo, rev, fs }` + `WriteTargetResolver` on
  RtCtx. Aggregate per-`(repo, rev, fs)`; merge N ranges into one
  splice (sorted right-to-left).
- **Rev-target ladder**: `:wt` = working tree; `:HEAD` = WT + warn;
  branch/sha/tag → auto-worktree under
  `~/.cache/sprefa/wt/<repo>/<rev>/`, leave dirty + emit
  `write/orphan-worktree`.
- **Render aggregation mode**: `render[plain](${SCOPE}, "template")`
  collects per-(fs, SCOPE) cursor group, joins per-cursor templates,
  one rebased cursor out.
- **Anti-join `!tag?`** + RAII seal.
- **Event primitive** `event(:e, ...)` / `event?(:e, ...)`.
- **`tag_def(:r, ...)`**: schema declaration with named columns (today
  positional only).
- **`ListReposEffect` / `ListRevsEffect`** — mirror
  `FsListFilesEffect` so source ops can fan out from a coordinate
  effect rather than seed_upstream.
- **Per-key cache invalidation**.
- **Source-from-effect**: replaces today's `*Mode::ArgPipe` shape.
- **Suspension durability** — out of scope for this spec.
- **Cardinality**, **persistence**, **type-graph translation**,
  **manifest pinning**, **scan-pointer**, **LSP code-actions**,
  **pattern re-hole** (`${re(...)}` inside ast bodies),
  **dotted carveout** (`${fs.stem}`), **backtick template literals**,
  **`@sprf` comment re-entry + `eval`** — each tracked separately.

## Retired (do not use)

- `$$` ans-ref. Scan-pointer subsumed into ops + `cursor.last_bound`.
- `&&` (parse.md §6.4).
- Bare `$NAME` outside ast-grep bodies. Brace-mandatory
  (`sprefa-9lt`).
- Snapshot-only reads on `RelationStore`. All reads drain-then-subscribe.
- `*Mode::ArgPipe(Arc<dyn Op>)` on `RepoOp` / `RevOp` / `FsOp`
  (commit `3134b39`, `sprefa-4iv` first pass). Source ops do not take
  a sub-op as a producer; they filter or bind on the cursor's existing
  field. Replacement is the future `ListReposEffect` / `ListRevsEffect`
  source-from-effect path.
- `RulePredicateOp` / `RuleMode` / `RuleSlot` / `name?(args)` predicate
  syntax. Rules are call-only; rule rows persist as internal
  accounting. `rule_predicate_smoke.sprf` exercises code due for
  removal.
- `PatternValue` trait, `GlobPattern`, `RegexPattern`, `JsonPattern`,
  `AstPattern` placeholder structs (parse.md §14.5m.2).
- `op` keyword (parse.md line 2219). User-defined ops in Rust remain;
  rules subsume the in-language reusable-op surface.

## Examples

All cited fixtures live under
`/Users/chrishafley/projects/sprefa/v3/crates/server/fixtures/`.

### 1. Hello world: regex match + print

```sprf
str("selfcheck-heartbeat") > print(:heartbeat);
```
(`selfcheck.sprf` §8.)

### 2. ast match + diagnostic

```sprf
fs(glob(crates/**/src/**/*.rs))
  > ast[rust](${V?}.unwrap())
  > lsp[warn](
      message = "unwrap() in shipped path",
      code    = "selfcheck/no-unwrap",
      hint    = "use ? or .context(...) or expect(\"why\")");
```
(`selfcheck.sprf` §1.)

### 3. Comment-bounded write_cursor (anchored splice)

```sprf
fs(glob(**/HOOKS.md))
  > comment("@sprf-begin rtkq", ${SCOPE?}, "@sprf-end rtkq")
  > render[plain](
- discovered RTKQ hook scopes go here
)
  > write_cursor(${SCOPE}, :replace);
```
(`golden_kitchen_sink.sprf` §7.)

### 4. Fact write + drain (relational bag)

```sprf
fact(:hits, "alpha");
fact(:hits, "beta");
fact(:hits, ${A?});      # drains both rows; subscribes for future writes
```
(`fact_mvp_smoke.sprf`.)

### 5. Rule definition + call

```sprf
rule(:user, ${name?}, ${role?}) {
  repo($R)
}
repo($Z) > user("alpha", "admin");
repo($Z) > user("beta",  "user");
```
(`rule_predicate_smoke.sprf`; the predicate-call portion is Retired.)

### 6. sh shell-out chained with json + lsp

```sprf
sh[cache](oasdiff breaking openapi.json openapi.head.json -f json)
  > json([...{ operationId: ${OPID?}, path: ${PATH?}, operation: ${METHOD?} }])
  > print(:oasdiff_breaking);
```
(`sh_oasdiff_demo.sprf` pipe 3.)

### 7. Drift detection skeleton — Target (anti-join, not implemented)

```sprf
# Today: only the producer side runs.
fs(glob(api/*.proto)) > ast[proto](field ${F?}) > fact(:proto, ${F});
fs(glob(api/*.ts))    > ast[ts](${F?}: \w+)     > fact(:ts,    ${F});

# Target shape — anti-join + RAII seal:
# !fact?(:proto, ${F}) > fact?(:ts, ${F}) > lsp[error](
#   message = "TS field with no proto declaration", code = "drift/ts-only");
```

## Audit notes

Followups for triage. None of these are blockers — each is a
contradiction, undocumented shape, or fixture/code drift.

### Contradictions between `parse.md` and current code

1. `parse.md §6.4` retires `&&`, `§6.5` retires `$$` — but `parse.md`
   elsewhere still references scan-pointer mechanics. Consolidate per
   `project_no_ans_ref`.
2. `parse.md §14.5m.2` says `PatternValue` and the placeholder pattern
   structs are deleted; lines 1216-1217 / 1841-1843 confirm. Tree
   structure is consistent with this; nothing to do, but the doc
   doubles back on it twice in 600 lines.
3. `parse.md` describes `fact` mixed-mode as a Join (datalog). Code
   path lives on `fact?` (`FactPredicateOp`). Bare `fact(:r, ${A},
   ${B?})` returns `fact/mixed-mode`. The pinned cleanup makes mixed
   on `fact` legal as SELECT-with-prefix; not implemented today.
4. `parse.md §13` describes `op` keyword (line 2219); code does not
   register it — rules subsume that surface, but the doc still
   carries the keyword. Retired.
5. `parse.md §6` brace-mandatory: confirmed in code (Term mode
   classification). But fixtures (`smoke.sprf`, `rule_smoke.sprf`,
   `rule_predicate_smoke.sprf`, `fact_predicate_smoke.sprf`,
   `selfcheck.sprf` "TODO ${BODY?}") use bare `$R`, `$Z`, `$LOL`,
   `$BODY`. Either grammar accepts both forms or fixtures are stale.
   Worth confirming the grammar `_brace_term_unbound` rule's actual
   permissiveness.
6. `parse.md` describes `RuleMode::{Probe, Join, Query}` and the
   `name?` predicate as live; pinned design retires it.
   `rule_predicate_smoke.sprf` is fixture-load-bearing for code due to
   delete.

### Fixtures using syntax not in `parse.md`

7. `argpipe_demo.sprf` uses `rev(fact?(:revs, ${REV?}))` — the
   `*Mode::ArgPipe` shape (`sprefa-4iv`). Parses + runs; pinned
   for retirement. Not described in parse.md.
8. `golden_kitchen_sink.sprf` §4 uses `ast_yaml[ts](pattern: "...")` —
   the sugared shape (no `rule:` envelope). Parse.md §14.5 mentions
   `ast_yaml` but the sugar isn't documented there.
9. `sh_oasdiff_demo.sprf` uses `[...{ ...: ${X?} }]` array-spread
   pattern in `json(...)`. The brace-pattern surface in
   `parse.md` §14.5 documents `[...]` but the per-op grammar
   reference is loose; the JSON op DOC mentions it inline only.
10. `selfcheck.sprf` uses bare `$$${ARGS?}` inside ast bodies. Code
    accepts; parse.md's metavar-grammar section calls this out only
    obliquely.
11. `smoke.sprf` and `rev_atom_filter_smoke.sprf` use bare `$R` /
    `$LOL` / `$Z` in op-arg position. Either still accepted by the
    grammar contrary to `sprefa-9lt`, or fixtures are stale.

### Op `from_values` shapes accepting undocumented forms

12. `repo::from_values` accepts `Term { mode: TermMode::Unbound }` and
    treats it identically to Read in the `Bind` arm (the destructure
    `[Term { name, .. }]` ignores `mode`). Parse.md treats only
    Read as the bind form. Either documentation or code should
    constrain.
13. `fs::from_values` accepts `Term`, `Atom`, `Str`, `Op` (raw-regex
    or arg-pipe). Parse.md only documents the glob filter and
    `${P?}` bind shapes; the sub-op forms are undocumented outside
    the source doc-comment.
14. `repo::from_values` / `rev::from_values` / `fs::from_values` all
    fall through unknown sub-ops to `ArgPipe` mode silently. Pinned
    for retirement; pragmatically these signatures should narrow
    once the source-from-effect path lands so a stray non-regex op
    yields a real diag instead of a bind on `last_bound`.
15. `write_file::from_values` accepts `Value::Op(op)` as
    `PathSpec::SubOp`. Parse.md mentions string/term only.
16. `print::from_values` accepts zero or one `Atom|Str` prefix; the
    no-arg form is undocumented.
17. `lsp[severity](...)` kwargs are documented only inside the op's
    `DOC` constant. The kwarg shape is not specified in parse.md.
18. `comment::from_values` accepts 1-arg, 2-arg, and 3-arg forms (the
    last with a `Term` as the scope binder). Parse.md describes the
    single-marker and paired-marker shapes but not the explicit scope
    Term position.
19. `RuleCallOp::pipe` uses `materialize_row` on `RelationArg` — the
    full set of accepted arg kinds for a rule call (atom/str/int/
    float/term/op?) lives only in `relation/mod.rs::RelationArg` and
    `materialize_row`; not surfaced in parse.md.
