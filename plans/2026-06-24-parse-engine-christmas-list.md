# parse-engine christmas list

Everything that bit us or that we routed around. Tagged: 🎁 engine wish, 🔧 papercut, 🌐 external (not an engine ask).

Source: 6 photos of `Untitled-1` taken 2026-06-24, transcribed verbatim. This list came out of a project that dynamically crawled repos/revs into a mermaid diagram and hit generic friction the whole way.

## Top of the tree (unlock the most)

1. 🎁 Data-driven scan repo/rev. Coordinates are literal-only, so a derived row can't drive a scan. "Verify every derived pin at its own rev in one pass" is impossible; fell back to codegen or a shell-loop. This forced an "enumerate everything in shell, join in datalog" shape instead of "compute for the one row we care about."

2. 🎁 Structured-config key capture. The dotted-path config op binds values, not keys. Iterating `parent.*` can't bind the child name, which forced a nearest-header proximity join (header line + value line + max aggregate) for every nested config block. A prior engine generation already had `{ $K: $V }` key capture and recursive path descent; porting that walker retires the most regex.

3. 🎁 CST as a relation (`node(id, kind, file, lo, hi, parent)` + `child`). No parent/child/ancestor relation; structure is reachable only through ast-grep `inside`/`has`. This is the codemod foundation: anchor-finding, innermost-containment, and scope all become joins.

4. 🎁 Edit algebra. The edit/fix sink is replace-one-span only. No `insert_before`/`insert_after`/`delete`/`wrap`, no multi-edit transaction with overlap detection.

## Parsers

5. 🎁 Go-template grammar. Not wired into the structural backends; `{{ }}` holes also break the config-value parser, so template manifests are regex-only. An upstream tree-sitter grammar plus one registry arm.

6. 🎁 Structural YAML as a first-class structural lang (values reach through the config op today, but not shape queries). Overlaps with #2.

7. 🎁 HCL/Terraform and Dockerfile grammars absent.

8. 🌐 Registry-URL refs are not a parser gap. They are strings for regex; no tool needed.

## Structural reads (codemod foundation)

9. 🎁 Whole-match span in the located spine. The structural ops locate only individual captures; the whole-match span exists solely as positional outputs, not as a first-class located id.

10. 🎁 Scope-as-interval + binding/visibility. No way to ask "is name X in scope at coord C" or "what does this byte range reference but not bind" (free-variable analysis). Precise scope needs #3 plus binding rules, or leaning on an external index for full name resolution.

11. 🔧 The located-ref file column is a content hash, not a path. Easy to misjoin if a path is expected.

## Edit / codemod writes

12. 🎁 Window aggregates (rank/row_number/nth). Only count/max/min exist, so fresh-name generation with -2/-3 collision suffixing and "Nth occurrence / keep-first" are not expressible. The underlying store already has these.

13. 🎁 File-level sinks (create / rename / delete a file) beyond what the move command hardcodes.

14. 🎁 Verify-rollback harness for the fix path (apply to a scratch worktree, run a checker via the shell op, keep only if it passes).

## Shell-op ergonomics

15. 🎁 Shell op can't take derived data as args (same root as #1). Forces enumerate-then-join.

16. 🔧 Shell-op cache keying. Cached by (trigger-file hash, rule text), so results that depend on external state (fetched git refs) refresh only when the trigger file changes; needs a force flag after a fetch. A cache that keys on declared external inputs, or a TTL, fixes it.

17. 🔧 Shell quoting inside program strings. Double-quotes need escaping and `\t`/`\n` are ambiguous through the string lexer, so a script had to avoid double-quotes entirely and use `echo` instead of `printf`. Raw string literals for shell/regex bodies would remove this.

18. 🔧 Shell-op budget surprises. Broad trigger globs blew the default by pulling monorepo-nested manifests; narrowing the trigger set fixed it. A per-rule budget or a dry-run count would help.

## Memory / perf / scale

19. 🎁 Flush-when-tick-large. A cold full-tree scan held large RSS and an unbounded WAL because each relation fully materializes before its single batched insert and the WAL never checkpoints mid-tick. Chunked flush plus periodic checkpoint.

20. 🎁 In-walker max-filesize. The walker crate supports it; the engine reads the size but never caps. Worked around with a generated ignore-block. Roughly a one-line change.

21. 🎁 Trigram full-text index for batch substring. A single substring lookup is fine, but a batch substring join can't use a B-tree index. One virtual-table statement over the string-literal relation.

22. 🔧 Orphan string-intern GC. Interns linger after their last ref retracts. Harmless (content-addressed).

## LSP

23. 🎁 Live (keystroke) diagnostics. The server is save-driven / disk-truth; unsaved-buffer support is deferred, so squiggles fire on save.

24. 🔧 Discovery mode merges all programs in the program dir, and a relation headed by both a source and a derived rule makes the tick bail. Both by-design, both sharp edges.

## DSL papercuts

25. 🔧 Source rules can't join derived/builtin relations ("head var unbound in source rule"); the fix is always to split into a source rule plus a derived rule.

26. 🔧 Wildcard `_` rejected in line/out slots of the line-oriented ops; a throwaway named variable is required.

27. 🔧 Two file-emit rules to the same path don't concatenate (last wins); must union into one relation and emit once.

28. 🔧 Rev is literal, so each branch is its own rule. The branch-drift program repeats the same extractor once per branch. A scan that fans over a rev set would collapse it.

29. 🔧 String-split returns text, no clean text-to-int, so a numeric shell output was kept as text and compared to "0". A documented coercion or a cast builtin.

30. 🔧 Duplicate regex group name across rules errors at the regex layer; had to split into two rules.

31. 🔧 Coercion log noise on the path-to-text coercion; filtered on every run.

32. 🎁 Nested report output. The JSON diagnostic mode is flat rows; a grouped report shape needs manual flattening or a JSON emit.

---

# Sequenced roadmap (all 32)

Status key: **done** (already in tree), **noted** (documented limitation / deferred), **open**.
Effort: XS (~1 line) · S · M · L.

## Already resolved / not an ask — close out

| # | Item | Disposition |
|---|------|-------------|
| 24 | mixed source+derived tick bail | **done** ba97aa4 — documents existing behavior |
| 11 | located-ref file col = content hash | **noted** — `ref.file` is content FileId by design; add doc-comment + a `ref_path` convenience join if it keeps biting |
| 21 | trigram FTS5 | **noted/deferred** — ref-spine C "if needed"; revisit only when a batch substring join shows up |
| 22 | orphan intern GC | **noted/deferred** — harmless, content-addressed |
| 8 | registry-URL refs | not an engine ask |

## Phase 0 — quick-win batch (this session)

Independent, no cross-deps. Order = ascending effort.

| # | Item | Effort | Status |
|---|------|--------|--------|
| 31 | suppress `coerce-text-path` warn by default + dedupe; `DL_COERCE_WARN=1` opts back in | XS | **done** — `lib.rs::render_type_diags_eprintln` |
| 20 | cap walker by file size via `DL_MAX_FILESIZE` (WORK walk + git ls-tree) | XS | **done** — `engine.rs::max_filesize` |
| 26 | accept `_` in line/out slots of `match`/`ast`/`cmd`/`json` ops | S | **done** — `var_of`→`opt_var` at the op handlers |
| 29 | `int(text)` cast builtin (CAST-style, SQL + `val_of` + typecheck) | S | **done** — `lower.rs`/`engine.rs::cast_int`/`typecheck.rs` |
| 27 | two emits to one path: auto-union OR loud error (not silent last-wins) | S | **done** — `run_gens` claimed-path set bails loudly (gen_op tests) |
| 17 | raw-string body literal for shell/regex (no double-quote escaping) | S | **done** — already mechanized by the backtick fenced string (`Tok::Str`); flows into `cmd`; regex already raw via `/.../`; regression test added |
| 30 | dedupe regex group names across rules (auto-namespace) | S | **done** — `desugar_regex_holes` dedupes a repeated `$NAME` (first captures, repeats → `(?:.*?)`); was a within-pattern "duplicate capture group name" error, NOT cross-rule |

Done in the daemon-stateful-revs session (commit d30e39a): #31, #20, #26, #29.
Done in the `codex/christmas-phase0-finish` worktree (branch off d30e39a):
- **#27** (f4358de): two gen File rules to one path bail loudly. `run_gens` threads a
  claimed-path set; cross-rule collision bails (mirrors the mixed-source/derived bail),
  within-rule rows still concat. Tests: `gen_op::two_file_rules_same_path_bail_loudly`,
  `two_file_rules_disjoint_paths_ok`.
- **#17** (2fb08ad): the backtick fenced string already lexes to a raw multiline
  `Tok::Str` that `cmd()` accepts, so a shell body with embedded `"` + printf `\t`/`\n`
  needs no escaping; regex bodies are already raw via the `/.../` form. No engine change;
  regression test `cmd_op::backtick_shell_body_skips_quote_escaping`.
- **#30** (825b7c6): a `$NAME` hole repeated in one `/regex/` desugared to two
  identical `(?P<name>...)` groups → regex-crate "duplicate capture group name" error.
  `desugar_regex_holes` now dedupes: first occurrence captures, repeats → `(?:.*?)`.
  (The plan's "across rules" framing was imprecise — two separate rules with the same
  hole name already worked.) Tests: `parse::repeated_hole_dedupes_to_noncapturing`,
  `regex_sugar::repeated_hole_compiles_and_binds_first`.

Phase 0 is now COMPLETE. Remaining DSL papercuts (#25) and the bigger phases (B/C/D/A)
are untouched.

Note (unrelated): the working tree had in-flight `daemon.rs`/`main.rs`/grammar changes that leave
~16 suites red at HEAD (call_rels, doc_ref/doc_node, type_entity_xlang, type_graph_ts, lsp hover/
definition, `rpc::extra_headers_ignored` — the last has a self-evidently broken fixture: body 11
bytes, `Content-Length: 13`). Proven pre-existing by reverting the Phase-0 edits and re-running.

## Phase 1 — structured-config walker (B)

| # | Item | Effort | Dep |
|---|------|--------|-----|
| 2 | `{$K:$V}` key capture + recursive path descent (port v3/v4 walker) | M | — |
| 6 | structural YAML as first-class lang (shape queries, not just value reach) | M | rides #2 |

"Retires the most regex." Self-contained, no engine-core surgery.

## Phase 2 — CST-as-relation foundation (C)

| # | Item | Effort | Dep | Status |
|---|------|--------|-----|--------|
| 3 | `node(id,kind,file,lo,hi,parent)` + `child` relation | L | — | **done** 110f2ea/28c2e1b/ce7adf5 |
| 9 | whole-match span as first-class located id | S | feeds #3/#4 | **done** 3d77f47 (ast) + 2adf8dc (sg/json) |
| 10 | scope-as-interval + binding/visibility (free-var analysis) | L | rides #3 | **partial** — interval predicate + point index landed; binding/free-var open |

Codemod foundation: anchor-finding, innermost-containment, scope all become joins.

Done this arc (CST-as-relation, merged to main):
- **#3** (110f2ea, 28c2e1b): lazy built-in query rels `node(id,kind,file,lo,hi,parent)`
  + `child(parent,child)`, gated on use, built by `refresh_node_rels` walking every
  named tree-sitter node across all 11 `ts_lang` grammars; ids are `_where_bytes`
  edit coordinates that round-trip through `ref` AND `string` (CST step 1, 2e6e02b).
  `refresh_node_rels_delta` (28c2e1b) makes the `--changed` tick path-scoped — only
  the edited file re-walks (`_node_path(id,path)` side table for the prune, structural
  guard test `last_node_files_walked==1`). Cold ~5.8s / incremental ~0.82s, no N+1.
- **#9** (3d77f47 ast, 2adf8dc sg/json): trailing optional located `id` arg on
  `ast`/`sg`/`json`, so the whole-match span is a first-class located id (not just
  positional byte outputs).
- **#10 (partial)** (ce7adf5): the nested-set insight — a CST is a forest with
  properly-nested spans, so ancestry/containment is a byte-span range join, not a
  closure. Point/containment ("innermost node covering byte C in file F",
  `node(_,_,F,lo,hi,_), lo<=C, C<hi`) is now a range scan via the optional
  `node_file_span_idx ON rel_node(file,lo,hi)`. MEASURED: `closure(child)` still
  WINS full-ancestry materialization (91ms vs 484ms unindexed range self-join at
  1721 nodes), so it is NOT retired — the interval predicate wins only the
  point/containment query (the LSP-common one). Binding/visibility + free-var
  analysis (the rest of #10) remains open. See `reference_cst_ancestry_nested_set`.

## Phase 3 — edit algebra + sinks (D)

| # | Item | Effort | Dep |
|---|------|--------|-----|
| 4 | insert_before/after/delete/wrap + multi-edit txn w/ overlap detection | M | #9 |
| 12 | window aggregates (rank/row_number/nth) — store already has them | S | — |
| 13 | file-level sinks (create/rename/delete) beyond move hardcode | M | — |
| 14 | verify-rollback harness (scratch worktree + checker + keep-if-pass) | M | #4, shell op |

## Phase 4 — coordinate dataflow (A)

| # | Item | Effort | Dep |
|---|------|--------|-----|
| 1 | derived row drives a scan (coordinates beyond literal) | L | core |
| 15 | shell op takes derived data as args (same root as #1) | M | #1 |
| 28 | scan fans over a rev set (collapse per-branch rule repetition) | M | #1 |

Most central, hardest. The "functional, no statements" core. Defer until B/C land.

## Phase 5 — grammars (E)

| # | Item | Effort | Status |
|---|------|--------|--------|
| 5 | Go-template grammar + registry arm (also fixes config-value parser on `{{ }}`) | M | **grammar done** d04a711 (`gotmpl` in `ast`); config-value `{{ }}` parser fix open |
| 7 | HCL/Terraform + Dockerfile grammars | M each | **done** d04a711 (`hcl`/`terraform`/`tf`, `dockerfile` in `ast`) |

d04a711 wired 8 grammars into the raw `ast` backend's `ts_lang` dispatch —
`python`/`bash`/`go`/`hcl`/`starlark`/`jsonnet` (crates.io) + `gotmpl`/`dockerfile`
(vendored C via `cc` build.rs to dodge the tree-sitter 0.20 ABI conflict). All 11
`ts_lang` grammars (these + rust/c/kotlin) also feed CST `node`/`child` (#3). The
residual on #5 is the structured-config value parser choking on `{{ }}` holes, not
the grammar itself.

## Phase 6 — shell-op ergonomics + perf + LSP

| # | Item | Effort | Cluster |
|---|------|--------|---------|
| 16 | cache key on declared external inputs / TTL (force flag after fetch) | S | shell |
| 18 | per-rule budget / dry-run count | S | shell |
| 19 | chunked flush + periodic WAL checkpoint mid-tick | M | perf |
| 23 | live keystroke diagnostics (unsaved-buffer) | M | LSP |
| 32 | nested/grouped report (JSON emit) | S | DSL |
| 25 | (accept-as-is) split source+derived — or sugar that auto-splits | S | DSL |
| 30 | dedupe regex group names across rules (auto-namespace) | S | DSL |

Recommended landing order: **Phase 0 → B → C → D → A**, with Phase 6 papercuts picked off opportunistically.
