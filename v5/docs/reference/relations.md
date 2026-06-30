# Built-in relations

Generated from the engine's `rel_catalog` by examples/gen-reference.dl. Do not hand-edit.

| relation | group | columns | summary |
|---|---|---|---|
| `agent_edit` | agent | `(harness, session, idx, path)` | every file edit in the latest agent turn, tagged harness+session+turn idx (from the at-rest harness store) |
| `agent_touch` | agent | `(harness, session, path)` | the latest agent turn's edited files (harness, session, path) |
| `allocates` | dataflow | `(fn)` | one row per fn whose body builds a collection (Vec/HashMap/String ctor, .collect/.clone/.to_string) |
| `call_def` | call | `(repo, sym, kind, file, line, end)` | every callable; sym is file::kind::name |
| `call_edge` | call | `(caller, callee, kind)` | resolved caller-sym to callee-sym edge (single-def or SCIP override) |
| `call_edge_rev` | call | `(caller, callee, kind, rev)` | rev-aware call_edge |
| `call_kind` | call | `(fn, kind)` | per-fn read/write classification from the bare callee name (execute* -> write, query*/prepare -> read); rusqlite-shaped, collection names dropped to avoid false positives |
| `call_name` | call | `(sym, name)` | def sym to bare callable name; resolves a call_site callee to candidate def syms |
| `call_site` | call | `(repo, caller, callee, file, line)` | each call occurrence; caller is the resolved fn sym, callee the bare text; changed_line joins here for line-scoped rails |
| `changed` | changed | `(path)` | git status --porcelain -uall vs HEAD (modified/added/renamed/untracked); empty outside git; the rails join |
| `changed_line` | changed | `(path, line)` | new-side lines of git diff -U0 HEAD hunks plus every line of untracked files; pure-deletion hunks emit nothing; line-scoped rails precision |
| `child` | node | `(parent, child)` | CST parent-child edges (exactly 2 cols, so closure(child) gives ancestry) |
| `clock` | clock | `(secs, bucket)` | the current time bucket now/secs per named period, present EVERY tick (not edge-triggered like every); clock(300,b) binds b to a monotone int advancing once per 300s — join it to vary a digest or gate on cadence, no @next counter |
| `content` | core | `(id, hash)` | content addresses |
| `crate_edge` | module | `(src, dst, kind, rev)` | workspace-internal Cargo dependency edges |
| `created` | created | `(path, name, email, ts)` | files added since their first appearance, with author name/email/timestamp |
| `df_edge` | dataflow | `(from, to)` | intra-procedural dataflow dependency edge |
| `df_node` | dataflow | `(id, kind, var, fn, file, line)` | intra-procedural dataflow node (call_res/assign/...); id is file::line::kind |
| `df_param` | dataflow | `(id, pos)` | (param df_node id, positional index); index counts typed params only (self skipped) so it aligns with type_sig.pos for node-level type joins |
| `dl_diag` | meta | `(path, line, col, end_line, end_col, severity, code, msg)` | parse/type diagnostics for each scanned `.dl` file (path, line, col, end_line, end_col, severity, code, msg); the engine's own lexer/parser/typechecker run over `file` rows ending in `.dl`, byte spans mapped to 1-based line / 0-based col — join agent_changed for lint-on-edit |
| `doc_comment` | type | `(repo, sym, line, text)` | doc comment per type_entity sym: (repo, sym, line, text); AST-located per language (Rust #[doc] attrs, Kotlin KDoc sibling, TS leading /** */) |
| `doc_node` | doc | `(repo, file, line, kind, name, parent)` | structural nodes from non-source text (markdown headings + code blocks via tree-sitter-md: ATX/setext headings, fenced/indented blocks); parent is the enclosing heading |
| `doc_ref` | doc | `(repo, file, line, sym, kind, matched_name)` | doc-to-code bridge: name-matches doc_node headings to type_entity symbols (exact + normalized) and scans code blocks for identifier mentions; empty unless the program also uses type relations |
| `doc_tag` | type | `(repo, sym, tag, arg, text)` | structured doc tags per sym: (repo, sym, tag, arg, text); @param/@returns/@deprecated for JSDoc/KDoc, # Section headings for rustdoc |
| `effect_log` | effect | `(id, kind, head, state, args, req_tx)` | the @async/@stream drain queue: one row per request (id, kind, head rel, state queued/running/done/failed, args JSON, req_tx); the dl-native call log, queryable live and parity-comparable to an external cache's call log |
| `every` | clock | `(secs)` | holds interval N only on ticks that cross an N-second boundary (and the first tick); an every(30) body atom self-throttles its rule |
| `file` | core | `(repo, rev, path, content)` | scanned files, keyed by (repo, rev, path, content) |
| `fn_catalog` | meta | `(name, arity, group, doc)` | every scalar function callable in a head or comparison with its arity, group, and one-line doc; sourced from fn_docs |
| `head` | daemon | `(repo, name, oid)` | git HEAD per repo (repo, ref name, oid) |
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
| `ref` | spine | `(id, string, file, lo, hi)` | byte span per interned string; id is the rewrite coordinate — 'where does Foo occur' is string(s, Foo, _), ref(_, s, f, lo, hi) |
| `rel_catalog` | meta | `(name, group, cols, doc)` | this table: every built-in relation with its group, columns, and one-line doc |
| `repo` | core | `(slug, root, url)` | configured + dynamically-pulled repos whose root exists; writable as a sink — a repo(...) rule clones+registers when the github org is in `org` (hard filter); see docs/dynamic-reaching.md |
| `rev` | core | `(id, repo, oid, ts)` | git revs seen by scans |
| `rev_advanced` | daemon | `(repo, name, old, new)` | daemon signal that a repo ref advanced (repo, name, old oid, new oid) |
| `scip_callee_type` | scip | `(sym, type)` | receiver type parsed from a method moniker's impl/for segment |
| `scip_def` | scip | `(symbol, file)` | symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX) |
| `scip_edge` | scip | `(src, dst)` | file-to-file SCIP dependency edges |
| `scip_fn_edge` | scip | `(caller, callee)` | function-level call edge; caller is the innermost enclosing fn def |
| `scip_impl` | scip | `(impl, iface)` | interface/supertype dispatch edge from SCIP is_implementation (impl to iface) |
| `scip_local` | scip | `(fn, name)` | local-variable + parameter declarations attributed to their enclosing fn |
| `scip_name` | scip | `(symbol, name)` | descriptor name (last identifier run) of a moniker, computed in-engine |
| `scip_ref` | scip | `(file, symbol, def_file)` | compiler-backed references (ref file, symbol, def file) |
| `similar` | embed | `(a, b, score)` | content-addressed nearest-neighbor pairs from the embedding backend, with score |
| `string` | spine | `(id, text, norm)` | interned strings (ref spine): id, text, normalized text |
| `true` | core | `()` | zero-arity singleton; the always-succeeds atom |
| `type_edge` | type | `(from, to, kind)` | type-graph edges across Rust (syn), Kotlin (tree-sitter), TS (oxc); kind is field/variant/impl/generic — Kotlin interface supertypes are generic, class/object impl, val/var ctor params + body properties field, enum entries variant |
| `type_edge_rev` | type | `(from, to, kind, rev)` | rev-aware type_edge (WORK-vs-HEAD type diff) |
| `type_entity` | type | `(repo, sym, name, kind, parent, file, line)` | every declared type; sym is file::kind::name, the cross-graph join key; scip_ref overrides name resolution when a SCIP index is present |
| `type_lgg` | type-shape | `(a, b, vars)` | least-general generalization of two type shapes (shape-iso experiment) |
| `type_link` | type | `(src, dst, kind)` | cross-type links not carried by type_edge (SCIP-resolved sym to sym) |
| `type_shape` | type-shape | `(name, hash)` | structural type-shape fingerprint per type (shape-iso experiment) |
| `type_sig` | type | `(sym, slot, pos, ref)` | type signature slots (params, fields) per sym |
