# NOTES

## 2026-04-14: Scan-pointer runtime (LOOSE — forward pass + tri-state verified)

Phase 0 landed: `ScanPointer` trait slot owns `$$sigil` dispatch; `provenance`/`ProvKind`/`ProvRef` terminology is dead. Next: runtime stamping.

**Thesis.** User programs a customizable rule-set of import/export relationships across repos. Runtime sics the whole crate tree on the fleet and records sigil'd pointers across repo boundaries. Mermaid/dot visualizations are projections over the store, composed in the DSL via `render` + `sh` — not first-class features. The store is the thing.

**Capture carries three facts**, not one:
- `value: Arc<str>` — matched string (today)
- `scan_sigil: Option<Arc<str>>` — user's type claim ("this string is a repo slug", or any op-declared sigil)
- `verified: Tri` — `Claimed` / `Verified` / `Missing`. Tri-state because streaming means the scan set is still growing when captures get stamped.

**Two populators write the same slot:**
- **Command side** — `repo($R)` binds `$R` from outer-pipe blast radius. Stamp `scan_sigil="repo"`, `verified=Verified` (value came from a real scan by construction).
- **Content side** — walker `$$sigil($VAR)` inside `json(...)`, and any future op that embeds scan pointers in content extraction. Stamp `scan_sigil`, `verified=Claimed`. Later pass cross-checks against the scan set; flips to `Verified` or `Missing`.

**Assumption checker is a pre-filter that does NOT filter.** Unverified captures stay in the row set. `Missing` emits `scan-pointer/unverified` Warn anchored at the capture's parse site. Rationale: streaming invalidates drop-on-miss; recording the claim is the right move; the Warn surfaces uncertainty.

**Forward-pass stamping is the shape.** Stamping happens as values stream through the pipeline, not as a post-hoc pass. `verified` is tri-state so a capture can stamp `Claimed` immediately and upgrade to `Verified`/`Missing` when the scan set becomes knowable (end-of-pass, or incrementally via reactive subscription to the scan registry).

**Column widening (later).** Rule table schema is per-capture. `$X` with `scan_sigil="repo"` → columns `X_str`, `X_repo_id` (FK to `repos` dimension table), `X_verified`. No sigil → plain `X_str`. Lands after runtime stamping works.

**Cross-repo edges.** Sigil'd captures across rules form a graph: `rule_a.X (sigil=repo)` → `repos.slug`; `rule_b.Y (sigil=rev)` → `revs.rev` scoped by `rule_b.R`. This graph is the output artifact — scanning, diagnostics, and rendering all project from it.

**Open tension.** Scan pointers are genericized (any op's sigil), but full generality without tying cursor keys 1:1 to ops is unclear. Command-side stamping is clean because cursor already has `repo`/`rev`/`fs` fields. Content-side is clean because walker annotations already carry a sigil. The gap is the register of "what scan sets exist to verify against" — currently `Config.repos` / `Config.revs` / `Config.fs_exclude` are hardcoded names, not sigil-indexed. Deferred.

**Naming hygiene.**
- `scan_sigil` = field on Capture. `ScanPointer` = trait-side op declaration. `ScanPointerRef` = parsed `$$sigil` token. `ScanAnnotation` = walker's internal record.
- Diag codes: `scan-pointer/…`.
- `provenance`, `Prov*`, `parse_provenance`: never.

## 2026-04-14: Op trait family (LOOSE design direction)

Session exploring a unified op trait family: state + reducer + pipe + effects + schema + projections, all dyn-safe, all op-owned. Core shrinks to a thin spine (registry, scheduler, effect bus, parser, DAG, path tagging). Three sub-ideas:

1. Op-owned annotations — core grammar must not hardcode op-specific sigils (`$$repo`/`$$rev`/`$$fs`); ops register their own projections.
2. Op-owned cursor/config slots via sprf codegen — aggregate Cursor/Config generated from per-op declarations using marker()/render(), not composed at Rust trait level.
3. DOM-validator grammar — ops declare `schema { allowed_parents, allowed_children, arity }`; parse-time validation anchored at child parse_site.

Captured in memory (private, not in repo): `project_op_trait_family.md`, `feedback_op_owned_annotations.md`, `project_op_owned_cursor_slots.md`, `project_dogfood_vision.md`.

Blocking dependencies: marker() + render() + Layer 5 effect bus + runner write path.

## 2026-04-12: Feature ambitions dump

### Verbatim from session

> i want to take this sprefa repo and add every tool i have ever wanted, they all involve parsing so here they are. anyways, i have immediate refactor, parse capture language, i have 2 new ambitions: first class shell integrations, ability to take sh( echo and declare whatever i want $currentFile "\n" derp ) > line(re:$DERP_SPACEderp)
>
> etc. or maybe it has other apps, wehave a way to say "these cli forms are allowed", not unlike agent permission globs lol.
>
> i also want to take any kind of enum/set of types/set of strings/disc unions etc. and make it possible to say in 1 file, and basically get rust match arm semantic check or autofill/autoarm codegen, actually that may be nice part of codegen tool: This file's xyz pattern must be codegen'd/match or be represented in some ast matches pattern in this other file.
>
> so code gen, code gen validate, basically a shiv for codegen, not some hoopy doopy thing but also maybe something like "codegen into this comment marker range".
>
> We also need to add "variables" to sprf, with let's, but its like a literal string not an eval'd thing, takes bashisms, $1, $2, $3
>
> also, does there exist a bash flavored interpreter in rust where i can take any strings and just do string expansions of bash as first pass then do my own stuff.

---

### Feature 1: sh() tag — first-class shell integration

**What:** New tag `sh(command)` that runs a shell command and feeds its stdout into the downstream chain.

```sprf
rule(dynamic_versions) {
  sh(cat $currentFile | jq -r '.version') > line($VERSION)
};

rule(custom_extract) {
  fs(**/*.proto) > sh(protoc --decode_raw < $currentFile) > line(re:field\s+$FIELD)
};
```

**Shape:** sh() is a driver tag (like fs/repo/rev). It produces text content. Downstream tags (line/json/ast) consume that content. The shell command gets access to extract-time constants ($currentFile, $currentDir, etc).

**Permission model:** "these CLI forms are allowed" — allowlist of command patterns in sprefa.toml, similar to agent permission globs:

```toml
[shell]
allow = [
  "jq *",
  "protoc --decode_raw *",
  "echo *",
  "cat *",
]
```

Or pattern-based: `allow = ["jq **", "protoc **"]`. Anything not matching gets rejected at parse time or scan time.

### Feature 1b: sh() refined — cuts, phases, cross-rule refs

**Four forms:**

| Form | Reads | Writes FS | Per-row or batch |
|------|-------|-----------|-----------------|
| `sh()` | stdout → downstream | no | per-file or per-row |
| `sh_mut()` | stdout logged | yes | per-file or per-row |
| `sh_batch()` | stdout → downstream | no | 1 call, all values |
| `sh_mut_batch()` | stdout logged | yes | 1 call, all values |

**Cross-rule variable refs in shell commands:**

```sprf
# per-row: runs once per dep_source row
rule(dep_versions) {
  sh(npm view ${dep_source.DEP} version --json) > json($VERSION)
};

# batch: collects all values, runs once
rule(tag_check) {
  sh_batch(git tag -l $$${deploy_img.TAG}) > line($FOUND)
};
```

`${rule.VAR}` — single value, iterates per row (like existing cross-ref).
`$$${rule.VAR}` — all values collected, space-separated (one call).
`$$${rule.VAR:,}` — comma-separated. `$$${rule.VAR:\n}` — newline-separated. `$$${rule.VAR:"}` — JSON-quoted array.

Dot syntax chosen because bash does not allow dots in variable names (function names yes, but those don't expand). No collision.

**Pending cuts — not error, not warning, side channel:**

sh rules whose command pattern is not in `[shell].allow` do not execute. Instead they collect into a `pending_cuts` side channel with the resolved command and all input values.

```
sprefa scan
✓ 47 rules extracted
⧖ 3 shell cuts pending (need approval):
  npm_versions: npm view * (3 inputs)
  proto_fields: protoc * (12 inputs)
  db_tables: psql * (1 input)

sprefa cuts              # list pending
sprefa cuts --approve npm  # add "npm view *" to [shell].allow
sprefa cuts --run npm      # one-shot execute without persisting to allow
```

LSP hover on sh() body shows:

```
⧖ Shell cut: not in allow list

Would run 3 commands:
  npm view express version --json
  npm view lodash version --json
  npm view @myorg/utils version --json

[Run now]  [Add to allow]  [Skip]
```

**Three-phase scan architecture:**

```
phase 1: pure extraction (fs/json/ast/line/comment)
  → all rule tables populated
  → zero side effects, always runs

phase 2: shell cuts (sh/sh_mut)
  → reads from phase 1 tables for ${rule.VAR} expansion
  → allowed cuts → execute, capture stdout, populate tables
  → disallowed cuts → pending_cuts collection
  → never blocks phase 1

phase 3: checks + codegen
  → runs on whatever tables are populated
  → missing sh-rule tables → check reports incomplete
  → codegen targets from sh rules → skipped if pending
```

**Sanitization per `${rule.VAR}` expansion:**

1. Shell-escape value (`shlex::quote`)
2. Reject if value contains: `; | & \` $( ${ \n`
3. Reject if value length > 4096 bytes
4. Log every expansion at trace level

**Permission config:**

```toml
[shell]
allow = ["jq *", "git tag *", "npm view *"]
allow_mut = ["sed -i *", "prettier --write *"]
timeout_ms = 10000
max_batch = 500
```

**Dependency order:** `sh_batch($$${rule.VAR})` creates implicit dep edge. Same DAG machinery as existing cross-rule refs. sh_batch adds "collect all rows" semantics vs cross-ref "iterate per row."

**Example data flow:**

```sprf
# phase 1
rule(dep_source) {
  fs(**/package.json) > json({ dependencies: { $DEP: $_ } })
};

# phase 2 (gated by allow list)
rule(dep_versions) {
  sh(npm view ${dep_source.DEP} version --json) > json($VERSION)
};

# phase 3
check(outdated_deps) {
  SELECT d.dep, d.version, dv.version
  FROM dep_source d
  LEFT JOIN dep_versions dv ON dv.dep = d.dep
  WHERE d.version != dv.version
};
```

### Feature 2: codegen validation — enum/union exhaustiveness across files

**What:** Declare a set of variants in one place. Assert that another file's pattern matches all of them. Like `#[non_exhaustive]` + match arm checking but across files via sprf rules.

```sprf
-- source of truth: all event types
rule(event_types) {
  fs(src/events.rs) > ast[rust](pub enum EventType { $$$VARIANTS })
};

-- must be handled: switch/match must cover all event_types
check(event_handler_exhaustive) {
  SELECT et.variant
  FROM event_types et
  LEFT JOIN handler_arms ha ON ha.arm = et.variant
  WHERE ha.arm IS NULL
};
```

**Codegen mode:** Instead of just checking, generate the missing arms into a marker range:

```sprf
codegen(event_handler_arms) {
  source: event_types(variant: $V)
  target: fs(src/handler.rs) > marker("// BEGIN:event_arms", "// END:event_arms")
  template: "EventType::$V => handle_${V}(),"
};
```

**This is two features:**
1. **codegen validate** — check block that fails when a pattern set is incomplete
2. **codegen emit** — write generated code into marker-bounded regions

The validate part already works with existing check blocks + marker extraction. The emit part needs a new `codegen()` statement type and a write-back mechanism.

### Feature 2b: auto-watch sprf directives in comments

Any comment starting with `sprf` (trimmed) becomes a directive. Already expressible as a rule with existing language:

```sprf
rule(sprf_directives) {
  fs(**/*) > comment("sprf:") > line(re:(?P<DIRECTIVE>\w+)\s*(?P<ARGS>.*))
};

check(unhandled_sprf_directives) {
  SELECT d.directive, d.args, file_path(d.file_id)
  FROM sprf_directives d
  WHERE d.directive NOT IN ('codegen', 'check', 'todo', 'sync')
};
```

No new machinery needed. `comment()` (formerly `marker()`) + `line(re:)` + `check()` compose into this.

### Feature 3: let bindings — string variables in .sprf

**What:** Named string constants, not evaluated, just literal substitution.

```sprf
let PROTO_DIR = "src/proto";
let VERSION_PATTERN = "re:\\d+\\.\\d+\\.\\d+";

rule(proto_messages) {
  fs($PROTO_DIR/**/*.proto) > line(re:message\s+$NAME)
};

rule(semver_pins) {
  fs(**/package.json) > json({ version: "$VERSION_PATTERN" })
};
```

**Bash-ism positional args:** `$1`, `$2`, `$3` for parameterized .sprf files:

```bash
sprefa eval -f rules/check.sprf -- myorg/backend main
# $1 = myorg/backend, $2 = main
```

```sprf
let REPO = $1;
let REV = $2;

rule(targeted_scan) {
  repo($REPO) { rev($REV) { fs(**/deploy.yaml) > json({ image: $IMG }) } }
};
```

**Implementation:** Pre-pass before parsing. Walk the source, find `let NAME = "value";` lines, build a substitution map, expand all `$NAME` occurrences (non-SCREAMING already excluded from capture collection since captures require SCREAMING_CASE). Positional args from CLI injected into the map before expansion.

### Feature 4: bash string expansion in Rust

**Question:** Does a Rust crate exist that does bash-style string expansion?

**Known crates:**
- `shellexpand` — tilde and env var expansion (`~`, `$HOME`, `${VAR:-default}`). No globbing, no command substitution. Lightweight.
- `shell-words` — splits/joins shell-quoted strings. No expansion.
- `shlex` — POSIX shell lexing/quoting. No expansion.

**Missing:** Full bash parameter expansion (`${var//pattern/replace}`, `${var:offset:length}`, `${var%suffix}`, array syntax). No Rust crate does this completely.

**Options:**
1. `shellexpand` for simple `$VAR` / `${VAR}` / `${VAR:-default}` — covers 90% of use cases
2. Port bash parameter expansion rules into sprefa's own expander (the capture language already handles `$VAR` and `${VAR}`)
3. Actually shell out to `bash -c 'echo "expanded"'` for full bashism support (heaviest, most correct)

For let-bindings in .sprf, option 1 (`shellexpand`-style) is probably sufficient. The capture variable syntax already handles `$SCREAMING` and `${BRACED}`. Adding `${VAR:-default}` and `${VAR:+alt}` covers the useful bash-isms without needing a full bash interpreter.

### Feature 5: reactive execution model (rxjs mental model)

The rule chain is a reactive pipeline. Every tag is an operator. Cursor is accumulated state. Everything is strings.

```coffeescript
# ── cursor: the reactive state threaded through every operator ──

# there are no "types" of data (files vs content vs rows vs captures)
# there is cursor. cursor has strings. operators read/write cursor fields.

Cursor =
  repo: null        # string
  rev: null          # string
  folder: null       # string
  file: null         # string
  byteRange: null    # [start, end]
  line: null         # number
  content: null      # string (file content, sh stdout, whatever)
  captures: {}       # Record<string, string>


# ── every tag is the same shape: cursor in, cursors out ──

# creation operators (start a stream)
#   fs(), repo(), sh() at chain start
#
# pipeline operators (narrow/enrich cursor)
#   json(), line(), ast(), comment(), sh() mid-chain
#
# no distinction needed. operator reads what it needs from cursor.
# if cursor.content is null and operator needs content, it creates content.
# if cursor.content exists, it transforms content.

Operator = (input$, args, side) -> Observable<Cursor>


# ── chain composition: just reduce ──

# fs(**/values.yaml) > json({ image: { repo: $REPO } }) > line(re:tag:\s+$TAG)
#
# cursor flows:
# {} → {file, content} → {file, content, captures:{REPO}} → {+captures:{TAG}, +line}

compileChain = (nodes) -> (input$, _, side) ->
  nodes.reduce(
    (stream$, node) ->
      registry.get(node.name)(stream$, node.body, side)
    input$
  )


# ── scope blocks: mergeMap (switchMap in spirit) ──

# repo($R) { rev(main) { fs(values.yaml) > json({name: $N}) } }

compileTree = (body) -> (input$, _, side) ->
  switch body.kind
    when "step"
      registry.get(body.tag.name)(input$, body.tag.body, side)

    when "block"
      scopeOp = registry.get(body.tag.name)
      scopeOp(input$, body.tag.body, side).pipe(
        mergeMap (scopedCursor) ->
          merge(body.children.map (child) ->
            compileTree(child)(of(scopedCursor), "", side)
          ...)
      )

    when "ref"
      # REACTIVE cross-rule ref: subscribe to upstream rule's stream
      # not "query table" — mergeWith on live stream
      input$.pipe(
        mergeWith(
          ruleStreams.get(body.ref.rule).pipe(
            map (outerCursor) ->
              type: "outer"
              cursor: bindColumns(outerCursor, body.ref.bindings)
          )
        )
        scan (state, event) ->
          if event.type is "outer"
            { ...state, cursor: { ...state.cursor, captures: { ...state.cursor.captures, ...event.cursor.captures } }, dirty: true }
          else
            { ...state, cursor: event, dirty: true }
        , { cursor: emptyCursor(), dirty: false }

        filter (state) -> state.dirty
        mergeMap (state) ->
          merge(body.children.map (child) ->
            compileTree(child)(of(state.cursor), "", side)
          ...)
      )


# ── individual operators ──

fs_op = makeOperator "fs", (cursor, side) ->
  files = globMatch(cursor, pattern)
  from(files).pipe(
    map (f) ->
      { ...cursor, file: f.path, folder: dirname(f.path), content: readFileSync(f.path), byteRange: null }
  )

json_op = makeOperator "json", (cursor, side) ->
  doc = parseStructured(cursor.content)
  matches = walkPattern(doc, pattern)
  from(matches).pipe(
    map (m) ->
      { ...cursor, captures: { ...cursor.captures, ...m.bindings }, byteRange: m.span }
  )

sh_op = makeOperator "sh", (cursor, side) ->
  cmd = expandVars(pattern, cursor)
  unless allowed(cmd, config)
    side.cuts.next { cmd, cursor, pattern: inferPattern(cmd) }
    return EMPTY
  stdout = execSync(cmd)
  of { ...cursor, content: stdout, byteRange: null }

comment_op = makeOperator "comment", (cursor, side) ->
  regions = findCommentRegions(cursor.content, pattern)
  from(regions).pipe(
    map (r) ->
      { ...cursor, byteRange: [r.start, r.end] }
  )

line_op = makeOperator "line", (cursor, side) ->
  slice = if cursor.byteRange
    cursor.content[cursor.byteRange[0]...cursor.byteRange[1]]
  else
    cursor.content
  from(slice.split('\n')).pipe(
    filter (line) -> matchLine(line, pattern)
    map (line, i) ->
      { ...cursor, line: i, captures: { ...cursor.captures, ...extractCaptures(line, pattern) } }
  )


# ── rule streams are subjects ──

ruleStreams = new Map()

compileRule = (rule) ->
  output$ = new ReplaySubject()
  ruleStreams.set(rule.name, output$)

  chain$ = compileTree(rule.body)(of(emptyCursor()), "", side)
  chain$.subscribe (cursor) -> output$.next(cursor)
  output$


# ── full pipeline: topo sort, then everything reacts ──

reactivePipeline = (rules) ->
  sorted = topoSort(rules)
  compiled = sorted.map(compileRule)
  merge(compiled...).pipe(
    tap (cursor) -> emitRow(cursor._ruleName, cursor.captures)
  )

# daemon mode: rule streams never complete
# file change → fs_op re-emits → downstream reacts
# demand scan → repo_op re-emits → downstream reacts
# codegen → subscribes to rule streams → re-renders on change
```

**Key reactive properties:**
- Cross-rule refs are `mergeWith` + `scan` on upstream rule's output stream, not table queries
- Each rule is a `ReplaySubject` -- late subscribers get all prior emissions
- Daemon/watcher: streams stay hot, file changes push new cursors, everything downstream reacts
- Three-phase scan maps to: phase 1 streams complete → phase 2 (sh) subscribes → phase 3 (check/codegen) subscribes
- No distinction between "batch mode" and "live mode" -- batch is just "all streams complete"

### Feature 6: self-rendering codegen via comment scopes

Codegen can target comment scopes in any file. Combined with rules that extract errors/violations/state, a .sprf file can declare "this region of this file is rendered from this data" and the daemon keeps it live.

**Error dashboard:**

```sprf
rule(all_violations) {
  check_violations(check: $CHECK, data: $DATA)
};

rule(pending_sh) {
  pending_cuts(cmd: $CMD, rule: $RULE, inputs: $INPUTS)
};

rule(parse_errors) {
  parse_diagnostics(file: $FILE, line: $LINE, msg: $MSG)
};

codegen(error_report) {
  source: all_violations | pending_sh | parse_errors
  target: fs(STATUS.md) > comment("sprf:codegen error_report")
  template: "- [$CHECK]($FILE:$LINE): $DATA"
};
```

Produces in STATUS.md:

```markdown
## Current Errors

<!-- sprf:codegen error_report -->
- [version_drift](values.yaml:12): tag=latest
- [missing_dep](package.json:4): lodash unpin
- ⧖ npm_versions: `npm view *` (3 pending)
- ⚠ rules/ci.sprf:7: unexpected token
<!-- sprf:codegen-end -->
```

**Other self-rendering patterns:**

```sprf
# dependency table in README
codegen(dep_table) {
  source: dep_source(dep: $D, spec: $S)
  target: fs(README.md) > comment("sprf:codegen dep_table")
  template: "| $D | $S |"
};

# API route docs from openapi
codegen(api_docs) {
  source: openapi_operations(path: $P, method: $M, op: $OP)
  target: fs(docs/api.md) > comment("sprf:codegen api_docs")
  template: "### $M $P\nHandler: `$OP`"
};

# env var inventory from k8s configmaps
codegen(env_inventory) {
  source: k8s_configmap_envs(key: $K, val: $V)
  target: fs(.env.example) > comment("sprf:codegen env")
  template: "$K=$V"
};
```

**The loop:** extract → check → codegen → file changes → re-extract. Comment scopes are write targets. Daemon mode keeps it live. `sprefa codegen --check` in CI catches drift.

---

## 2026-04-14: v2 LSP foundations landed — future features

After the arm-syntax + absolute-offset + evidence-site-filter + partial-match pass, the LSP surface finally reflects real semantics. Foundations feel sound: arm-braced fork grammar, op ownership of diagnostics/hover, reader layers (git + buffer overlay), evidence-driven scope filtering, partial walker matches. SQLite sink is abstracted behind Writer — can slot in anytime. Other ops (`sh`, `line`, `marker`, `md`, `render`) can now parallelize because the op surface has been stress-tested through this shakedown.

### 1. Hover: group matches by file/rev

Current shape for capture hover:
```
**$VER** values:
- 2.1.0
- 0.1.0
```

Target: SQLite-row-like grouping. Each row = one cursor's witness tuple. Group cursors by `(file, rev)` (file primary, rev secondary when multiple revs present), then list capture values under each.

```
**$VER** values:

### crates/sprf-lsp/package.json
- `main`: 2.1.0
- `test/1`: 2.0.0

### crates/sprf/package.json
- `main`: 0.1.0
```

This is the wide-table projection rendered as markdown. Requires hover_capture to pass cursor identity (fs + rev at minimum) alongside the capture value — today it deduplicates raw value strings only.

### 2. Hover: show the cross-ref form `${rule.$VAR}` for any capture

On hovering `$PKG_NAME` inside `rule(capture_test)`, append a copy-paste line:

```
**$PKG_NAME** values:
- foo
- bar

*reference as:* `${capture_test.$PKG_NAME}`
```

Makes forward-binding ergonomic. Enables users to discover the cross-ref sigil without reading docs.

### 3. DAG forward-binding at parse time (v1 parity)

`${rule_a.$X}` referenced in rule_b means rule_b depends on rule_a's output. Parse-time build the rule dependency DAG, topologically order rules for runtime so forward-bound captures are already populated. v1 had this; v2 hasn't reimplemented yet. Blocker for cross-rule walker semantics (the commented `derived_rule` in `kitchen_sink_v2.sprf`).

**Landed 2026-04-14 (commit f6b8008, layers 0–2.5):** parse-time crossref collection on `OpInvocation.crossrefs`, `RuleHandle.depends_on`, Kahn topo + DFS cycle recovery in `_11_dag.rs`, `xref/cycle` diag, `LoweredOp` wrapper, `ResultStore` per-rule row store with Pending/Complete gating, `expand_xrefs` adapter spliced into `Pipeline::run_with_step`, walker constrain-when-prebound on Leaf so seeded captures filter downstream. Runner write path (cursor → `ResultStore.append`, stream end → `mark_complete`, level-barrier scheduling) is **Layer 5, still pending** — adapter exercises through unit tests with mock stores. End-to-end execution of `derived_rule` blocked on Layer 5.

### 4. `$$repo` / `$$rev` provenance (with `.norm` variants)

Principal feature for high-scale gitops. Captures a cursor's originating repo/rev tagged alongside every walker match, so a `json({ ** : { image: $I, $$repo($R), $$rev($V) } })` binds the full provenance chain at scan time, independent of whether the pipeline has an explicit `repo()`/`rev()`. `.norm` variants normalize semver tags or slug forms (v1 had this).

This is how reports tie back to "which commit in which repo produced this match" — the point of the whole system at scale.

**Implementation hook (post-2026-04-14):** reuse the Layer 2 `expand_xrefs` adapter pattern in `_5_op.rs`. Provenance values come from `cursor.repo` / `cursor.rev` instead of `ResultStore`. Same pre-seed semantics, same constrain-when-prebound walker behavior. Decision still open: if N input cursors have M distinct repos, does an op with `$$repo($R)` cartesian-expand or stay 1:1 with each cursor's own repo? Probably 1:1 (provenance is inherent to the cursor, not joined).

### 5. Path resolution: inside-repo vs outside-repo `.sprf` files

Currently the LSP walks up from the `.sprf` file looking for `.sprefa.toml`, else for a `.git/` ancestor (auto git root + `HEAD`). Two distinct orchestration modes want to coexist:

- **In-repo `.sprf`**: references its own repo implicitly; can also cross-ref other repos if the ancestor config declares them. `.sprefa.toml` lives in the repo.
- **Outside-repo `.sprf`** (orchestration dir): holds cross-repo rules spanning many ghcacher-managed checkouts. `.sprefa.toml` at that dir declares all relevant `[[repo]]` entries.

Both modes must coexist: a folder containing per-repo `.sprf` plus cross-repo orchestration `.sprf`. Ancestor resolution handles this — first `.sprefa.toml` wins. **Repo provenance in rules then drives ghcacher to ensure the referenced checkout exists** — ghcacher pulls/updates; sprefa reads. Division of responsibility is clean.

### 6. Parallel op expansion now unblocked

Op trait has survived the shakedown: parse/pipe/hover_self/hover_capture/hover_match/witness/capture_name plus framework-owned evidence + fork lowering. Time to grow the op surface without fearing trait churn.

Priority targets: `sh()` (sandboxed shell with allow-list from Config.shell_allow), `line()` (regex over raw text), `marker()` (comment-bounded span extraction for codegen), `md()` (markdown AST walker reusing brace-pattern language), `render()` (codegen sink).

### 7. Missing-entry diagnostics for partial walker matches

Partial semantics landed — now emit a hint-level diagnostic per cursor/file listing which object entries failed to bind. Surface in hover dump under the json match site. Gives the user a "why didn't `$PKG_NAME` bind?" answer without adding a walker debugger.
