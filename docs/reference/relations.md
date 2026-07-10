# Built-in relations

Generated from the engine's `rel_catalog` by examples/gen-reference.dl. Do not hand-edit.

| relation | group | columns | summary |
|---|---|---|---|
| `agent_edit` | agent | `(harness, session, idx, path)` | every file edit in the latest agent turn, tagged harness+session+turn idx (from the at-rest harness store) |
| `agent_touch` | agent | `(harness, session, path)` | the latest agent turn's edited files (harness, session, path) |
| `allocates` | dataflow | `(fn)` | one row per fn whose body builds a collection (Vec/HashMap/String ctor, .collect/.clone/.to_string) |
| `call_def` | call | `(repo, sym, kind, file, line, end)` | every callable; sym is file::kind::name |
| `call_def_rev` | call | `(repo, sym, kind, file, line, end, rev)` | rev-aware call_def (rev is a column, never folded into the sym); legacy call_def is the rev-deduped union |
| `call_edge` | call | `(caller, callee, kind)` | resolved caller-sym to callee-sym edge (single-def or SCIP override) |
| `call_edge_rev` | call | `(caller, callee, kind, rev)` | rev-aware call_edge |
| `call_kind` | call | `(fn, kind)` | per-fn read/write classification from the bare callee name (execute* -> write, query*/prepare -> read); rusqlite-shaped, collection names dropped to avoid false positives |
| `call_name` | call | `(sym, name)` | def sym to bare callable name; resolves a call_site callee to candidate def syms |
| `call_site` | call | `(repo, caller, callee, file, line)` | each call occurrence; caller is the resolved fn sym, callee the bare text; changed_line joins here for line-scoped rails |
| `changed` | changed | `(path)` | git status --porcelain -uall vs HEAD (modified/added/renamed/untracked); empty outside git; the rails join |
| `changed_line` | changed | `(path, line)` | new-side lines of git diff -U0 HEAD hunks plus every line of untracked files; pure-deletion hunks emit nothing; line-scoped rails precision |
| `checkout` | demand | `(repo, branch, pr_heads)` | git checkout demand sink (the ghcacher keep-current half): head checkout(repo, branch, pr_heads) and each row clones a missing config repo, fetches origin, then NON-DESTRUCTIVELY keeps `branch` current — `merge --ff-only origin/<branch>` when that IS the current branch + the working tree is clean (skip on dirty or diverged; never stash, never reset), else `git branch -f` the ref without touching HEAD or the working tree. branch empty = discover origin/HEAD; pr_heads "1"/"true" also mirrors +refs/pull/*/head. DL_NO_FETCH skips the network (re-points to already-fetched refs only). DL_CHECKOUT_DRY_RUN=1 previews the plan without mutating. The sink drains on the daemon poll loop / --watch / --settle / one-shot --apply (not on a bare `?` read). Repos sweep in parallel on a narrow pool; failures skip loudly |
| `checkout_done` | demand | `(repo, branch, action, ok, detail)` | checkout-sweep outcome (written by the `checkout` sink, read-only): one row per swept repo — action is ff/branch-f/skip, ok is 1/0, detail is the git result. Confirms the sweep fired from a live daemon (stderr goes to daemon.log) and lets a program diag failures (ok=0); one-tick latency like other demand outputs |
| `checkout_plan` | demand | `(repo, branch, action, ok, detail)` | checkout-sweep PREVIEW (written when DL_CHECKOUT_DRY_RUN=1, read-only): same shape as checkout_done, but the sink computes the action without running `merge --ff-only` or `git branch -f` — nothing in any checkout is mutated. Use to preview what `checkout` would do before opting in via --apply / DL_APPLY_SINKS=1 |
| `child` | node | `(parent, child)` | CST parent-child edges (exactly 2 cols, so closure(child) gives ancestry) |
| `clock` | clock | `(secs, bucket)` | the current time bucket now/secs per named period, present EVERY tick (not edge-triggered like every); clock(300,b) binds b to a monotone int advancing once per 300s — join it to vary a digest or gate on cadence, no @next counter |
| `comment_node` | comment | `(path, line, col, end_line, end_col, text, kind)` | every comment in every parsed file: (path, line, col, end_line, end_col, text, kind is line/block/doc); grammar-backed (oxc for TS/TSX, tree-sitter for Rust, Kotlin, Python, Go, C, ...), so a comment marker inside a string is never a row; text has the comment tokens stripped; std/suppress.dl parses it into the eslint/biome disable grammar |
| `content` | core | `(id, hash)` | content addresses |
| `crate_edge` | module | `(src, dst, kind, rev)` | workspace-internal Cargo dependency edges |
| `created` | created | `(path, name, email, ts)` | files added since their first appearance, with author name/email/timestamp |
| `def_target` | demand | `(name, file, line, kind)` | LSP go-to-definition sink: head def_target(name, file, line, kind) and textDocument/definition resolves a symbol reference to (file, line) by name; falls back to the module-edge specifier match when empty. Read by column name, so a subset written via named args works |
| `df_arg` | dataflow | `(call, pos, arg)` | (call/new df_node id, slot, arg df_node id); 0-based, receiver at -1; aligns with df_param.pos for the positional arg->param hop |
| `df_arg_rev` | dataflow | `(call, pos, arg, rev)` | rev-aware df_arg; call and arg are salt_rev(raw id, rev), matching df_node_rev.id; legacy df_arg keeps raw ids |
| `df_edge` | dataflow | `(from, to)` | intra-procedural dataflow dependency edge |
| `df_field` | dataflow | `(id, field, value)` | (new/call df_node id, field name, value df_node id); struct-literal fields, object-literal properties, Kotlin named args; ".." for spread/functional-update bases |
| `df_field_rev` | dataflow | `(id, field, value, rev)` | rev-aware df_field; id and value are salt_rev(raw id, rev), matching df_node_rev.id; legacy df_field keeps raw ids |
| `df_node` | dataflow | `(id, kind, var, fn, file, line)` | intra-procedural dataflow node (call_res/let_bind/param/ret/new/member/...); id is file::line::kind — the full kind vocabulary is rel_col's variants for this column |
| `df_node_repo` | dataflow | `(id, repo)` | (df_node id, repo) — the repo (nearest .git basename) each node's file was read from; scopes df joins per-repo (df_node ids are path-keyed) |
| `df_node_repo_rev` | dataflow | `(id, repo, rev)` | rev-aware df_node_repo; id is salt_rev(raw id, rev), matching df_node_rev.id; legacy df_node_repo keeps the raw id |
| `df_node_rev` | dataflow | `(id, kind, var, fn, file, line, rev)` | rev-aware df_node; id is salt_rev(raw id, rev) so revs stay disjoint; legacy df_node keeps the raw id |
| `df_param` | dataflow | `(id, pos)` | (param df_node id, positional index); index counts typed params only (self skipped) so it aligns with type_sig.pos for node-level type joins |
| `diag` | diag | `(path, line, col, end_line, end_col, severity, code, msg, hint)` | diagnostic sink; head it from a rule to emit an editor squiggle (--lsp), a --check finding, or a daemon-hook message. Fixed 9-col schema — write only the cols you need via named args (diag(path: p, line: l, msg: m)); the rest are NULL and default (severity warn, end_line=line, ints 0). path is TEXT so a synthetic origin isn't file-checked away |
| `diag_mute` | diag | `(code)` | diagnostic-mute set: one row per diag code silenced in the editor session (via the LSP `dl.toggleDiagCode` command). The --lsp publish path drops `diag` rows whose code is muted; --check/--parse-only read `diag` directly and ignore this set. Written only by the toggle command, never a rule head |
| `dl_diag` | meta | `(path, line, col, end_line, end_col, severity, code, msg)` | parse/type diagnostics for each scanned `.dl` file (path, line, col, end_line, end_col, severity, code, msg); the engine's own lexer/parser/typechecker run over `file` rows ending in `.dl`, byte spans mapped to 1-based line / 0-based col — join agent_changed for lint-on-edit |
| `doc_comment` | type | `(repo, sym, line, text)` | doc comment per type_entity sym: (repo, sym, line, text); AST-located per language (Rust #[doc] attrs, Kotlin KDoc sibling, TS leading /** */) |
| `doc_node` | doc | `(repo, file, line, kind, name, parent)` | structural nodes from non-source text (markdown headings + code blocks via tree-sitter-md: ATX/setext headings, fenced/indented blocks); parent is the enclosing heading |
| `doc_ref` | doc | `(repo, file, line, sym, kind, matched_name)` | doc-to-code bridge: name-matches doc_node headings to type_entity symbols (exact + normalized) and scans code blocks for identifier mentions; empty unless the program also uses type relations |
| `doc_tag` | type | `(repo, sym, tag, arg, text)` | structured doc tags per sym: (repo, sym, tag, arg, text); @param/@returns/@deprecated for JSDoc/KDoc, # Section headings for rustdoc |
| `effect_cmd` | demand | `(kind, template)` | effect-template overlay sink: head effect_cmd(kind, template) to override the shell command for an effect kind at drain time (dynamic per-kind template), read as the effect executor is built |
| `effect_log` | effect | `(id, kind, head, state, args, req_tx)` | the @async/@stream drain queue: one row per request (id, kind, head rel, state queued/running/done/failed, args JSON, req_tx); the dl-native call log, queryable live and parity-comparable to an external cache's call log |
| `every` | clock | `(secs)` | holds interval N only on ticks that cross an N-second boundary (and the first tick); an every(30) body atom self-throttles its rule |
| `file` | core | `(repo, rev, path, content)` | scanned files, keyed by (repo, rev, path, content) |
| `fn_catalog` | meta | `(name, arity, group, doc)` | every scalar function callable in a head or comparison with its arity, group, and one-line doc; sourced from fn_docs |
| `git_ref` | git-ref | `(repo, refname, kind, sha)` | every branch/tag/remote ref plus HEAD across self + config repos (repo, refname, kind, sha); annotated tags peeled to the commit |
| `graph_edge` | graph | `(src, dst, kind)` | drawable-graph edge sink: head graph_edge(src, dst, kind) from a rule to connect two graph_node ids; kind is the wire label/style. Read by the Graph preset alongside graph_node |
| `graph_node` | graph | `(id, label, kind, file, line, parent)` | drawable-graph vertex sink: head graph_node(id, label, kind[, file, line, parent]) from a rule and the flow panel's always-available Graph preset draws it — no bespoke node SQL. Fixed 6-col schema; write only the cols you need via named args (graph_node(id: sym, label: name, kind: k)); file/line place the node in the fs-tree + jump target, parent nests it in list view, all NULL by default |
| `head` | daemon | `(repo, name, oid)` | git HEAD per repo (repo, ref name, oid) |
| `hook_event` | hook | `(kind, session, seq, json)` | harness-hook event log: one accumulating row per `dl --hook` invocation (kind = the event name UserPromptSubmit/PostToolUse/..., session = the event session id, seq = an ingest-time monotone millis stamp ordering events within a session, json = the raw event JSON). Written by the hook feed, never a refresh; extract fields with term-form json/jsonp |
| `loop_over` | dataflow | `(file, start, end, var, collection, fn)` | one row per loop with its span, iter var, and collection |
| `module_edge` | module | `(src, dst)` | resolved file-to-file import graph (rev-deduped union) |
| `module_edge_rev` | module | `(src, dst, rev)` | rev-aware module_edge |
| `module_import` | module | `(file, rev, specifier, kind, line)` | import statements (Rust + TS + Kotlin); Kotlin adds kind=same-package rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files |
| `module_unresolved` | module | `(file, specifier, reason, line)` | broken imports: a reference that resolved to no project file (the linter question) |
| `module_unresolved_rev` | module | `(file, rev, specifier, reason, line)` | rev-aware module_unresolved |
| `nest` | dataflow | `(call_id, loop_id, depth, collection)` | one row per (call, enclosing loop); depth is nesting rank (1=outermost); raw material for symbolic Big-O over call_edge |
| `node` | node | `(id, kind, file, lo, hi, parent)` | CST nodes (nested-set spans): id, kind, file, lo, hi, parent |
| `op_catalog` | meta | `(op, kind, syntax, doc)` | every body/sink op (source ops, derived constructs, sinks) with its syntax sketch and one-line semantics; sourced from op_docs |
| `program` | daemon | `(path, hash, mtime)` | dl programs the daemon tracks (path, content hash, mtime) |
| `propose_clone` | propose | `(kernel, path, lo, hi, param)` | proposed clone/near-duplicate groups keyed by a shared kernel |
| `propose_extract` | propose | `(path, lo, hi, param)` | proposed extract-function refactor spans (path, lo, hi, param) |
| `query_log` | daemon | `(ts, source, method, body, params)` | history of server query requests: one row per daemon `query`/`query_sql` RPC and LSP `dl/query` request (ts = ISO-8601 UTC, source in {daemon,lsp}, method = RPC name, body = SQL text or empty, params = JSON array text); append-only, no retention — a polling client (the flow panel) accumulates its own rows too, by design |
| `ref` | spine | `(id, string, file, lo, hi)` | byte span per interned string; id is the rewrite coordinate — 'where does Foo occur' is string(s, Foo, _), ref(_, s, f, lo, hi) |
| `rel_catalog` | meta | `(name, group, cols, doc)` | this table: every built-in relation with its group, columns, and one-line doc |
| `rel_col` | meta | `(rel, pos, col, type, variants)` | one row per built-in relation column: (rel, 0-based pos, col name, type keyword, variants); variants is the JSON array of allowed values for an enum-vocabulary column (e.g. type_edge.kind), empty for an open column — query it instead of guessing a kind literal |
| `rel_count` | perf | `(rel, rows)` | row count per declared relation at refresh time; derived rels report the previous tick's counts (source-phase refresh, one-tick lag) — the cardinality-blowup rail joins here |
| `repo` | core | `(slug, root, url)` | configured + dynamically-pulled repos whose root exists; writable as a sink — a repo(...) rule clones+registers when the github org is in `org` (hard filter); see docs/dynamic-reaching.md |
| `rev` | core | `(id, repo, oid, ts)` | git revs seen by scans |
| `rev_advanced` | daemon | `(repo, name, old, new)` | daemon signal that a repo ref advanced (repo, name, old oid, new oid) |
| `rev_behind` | git-ref | `(repo, refname, upstream, behind, ahead)` | demand-driven ancestry counts: derive rev_cmp_want(repo, refname, upstream) and each wanted pair yields behind/ahead commit counts (ahead>0 = the ref diverged from upstream); one-tick latency like a data-driven scan; unresolvable refs and shallow clones skip loudly |
| `rev_cmp_want` | demand | `(repo, refname, upstream)` | git ancestry demand sink: head rev_cmp_want(repo, refname, upstream) and each wanted triple runs git rev-list, filling rev_behind(repo, refname, upstream, behind, ahead); unresolvable refs and shallow clones skip loudly |
| `scip_binding` | scip | `(file, symbol, local_name, line, col, repo)` | an occurrence's LOCAL binding text (source slice at its range) joined to the canonical symbol — resolves an alias/default import (import { foo as bar }) that scip_name's canonical-only name drops; WORK content slice, 0-based line/col |
| `scip_callee_type` | scip | `(sym, type)` | receiver type parsed from a method moniker's impl/for segment |
| `scip_def` | scip | `(symbol, file, repo)` | symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX); repo = origin index |
| `scip_edge` | scip | `(src, dst, repo)` | file-to-file SCIP dependency edges (with origin repo) |
| `scip_fn_edge` | scip | `(caller, callee)` | function-level call edge; caller is the innermost enclosing fn def |
| `scip_impl` | scip | `(impl, iface)` | interface/supertype dispatch edge from SCIP is_implementation (impl to iface) |
| `scip_local` | scip | `(fn, name)` | local-variable + parameter declarations attributed to their enclosing fn |
| `scip_name` | scip | `(symbol, name)` | descriptor name (last identifier run) of a moniker, computed in-engine |
| `scip_occurrence` | scip | `(file, symbol, line, col, end_line, end_col, role, repo)` | every SCIP occurrence with its 0-based line/col span, role (definition or reference), and origin repo — the position handle scip_ref lacked |
| `scip_ref` | scip | `(file, symbol, def_file, repo)` | compiler-backed references (ref file, symbol, def file, origin repo) |
| `scip_want` | demand | `(repo)` | SCIP index demand sink: head scip_want(repo) to make the importer ensure + load that repo's index.scip (runs installed indexers when missing, merges, loads into scip_def/scip_ref/scip_edge); one-tick latency, shallow clones skip loudly |
| `similar` | embed | `(a, b, score)` | content-addressed nearest-neighbor pairs from the embedding backend, with score |
| `skill_loaded` | agent | `(harness, session, name)` | skills loaded in the newest agent session (harness, session, name): explicit Skill tool calls + dl's own prior `dl --hook` injections — negate it for a declarative load-once guard |
| `stmt_ms` | perf | `(rel, ms)` | wall ms of each derived rel's INSERT statements from its most recent rebuild (max across rules/passes); empty until a rebuild has landed in this db, so a one-shot CLI run reports on the second invocation — the slow-rule rail joins here |
| `string` | spine | `(id, text, norm)` | interned strings (ref spine): id, text, normalized text |
| `true` | core | `()` | zero-arity singleton; the always-succeeds atom |
| `type_decl_row` | types | `(shape, pos, col, type)` | derived-shape sink: head type_decl_row(shape, pos, col, type) from a derived rule to compute a relation schema from data. At end of tick its rows persist; on the next tick a `rel name: shape.` decl with no syntax `type name(...)` resolves its columns from them (shape-pending info diag until then, shape-shadowed warn if a syntax shape shares the name). the type column is a base type keyword or a declared brand; an unknown type keeps that shape pending. Derived-only (route a jsonp/json extract through its own rel first) |
| `type_edge` | type | `(from, to, kind, repo)` | type-graph edges across Rust (syn), Kotlin (tree-sitter), TS (oxc); kind is field/variant/impl/generic — Kotlin interface supertypes are generic, class/object impl, val/var ctor params + body properties field, enum entries variant; trailing repo column so two trees scanned together don't collapse same-named types into one node (closure/scc still walk cols 0/1, unaffected) |
| `type_edge_rev` | type | `(from, to, kind, rev, repo)` | rev-aware type_edge (WORK-vs-HEAD type diff) |
| `type_entity` | type | `(repo, sym, name, kind, parent, file, line)` | every declared type; sym is file::kind::name, the cross-graph join key; scip_ref overrides name resolution when a SCIP index is present |
| `type_entity_rev` | type | `(repo, sym, name, kind, parent, file, line, rev)` | rev-aware type_entity (rev is a column, never folded into the sym, so a diff compares the same sym across revs); legacy type_entity is the rev-deduped union |
| `type_lgg` | type-shape | `(a, b, vars)` | least-general generalization of two type shapes (shape-iso experiment) |
| `type_link` | type | `(src, dst, kind)` | cross-type links not carried by type_edge (SCIP-resolved sym to sym); src/dst are already repo-prefixed via type_entity's sym, so no separate repo column is needed |
| `type_link_rev` | type | `(src, dst, kind, rev)` | rev-aware type_link (SCIP-resolved sym-to-sym per rev); legacy type_link is the rev-deduped union |
| `type_shape` | type-shape | `(name, hash)` | structural type-shape fingerprint per type (shape-iso experiment) |
| `type_sig` | type | `(sym, slot, pos, ref)` | type signature slots (params, fields) per sym |
