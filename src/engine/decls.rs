use super::*;

pub(crate) fn builtin_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "true".into(), cols: vec![], group: "core",
            doc: "zero-arity singleton; the always-succeeds atom", ..Default::default() },
        RelDecl { name: "repo".into(), cols: vec![c("slug", Type::Text), c("root", Type::Path), c("url", Type::Text)], group: "core",
            doc: "configured + dynamically-pulled repos whose root exists; writable as a sink — a repo(...) rule clones+registers when the github org is in `org` (hard filter); see docs/dynamic-reaching.md", ..Default::default() },
        // `id` and `oid` both hold the same resolved rev text (declare.rs
        // fills both from the one `rev` value carried off `_file`) — id is a
        // legacy duplicate key, not a distinct identifier, so both retype.
        RelDecl { name: "rev".into(), cols: vec![c("id", Type::Rev), c("repo", Type::Text), c("oid", Type::Rev), c("ts", Type::Int)], group: "core",
            doc: "git revs seen by scans", ..Default::default() },
        RelDecl { name: "content".into(), cols: vec![c("id", Type::Text), c("hash", Type::Text)], group: "core",
            doc: "content addresses", ..Default::default() },
        RelDecl { name: "file".into(), cols: vec![c("repo", Type::Text), c("rev", Type::Rev), c("path", Type::Path), c("content", Type::Text)], group: "core",
            doc: "scanned files, keyed by (repo, rev, path, content)", ..Default::default() },
    ]
}

/// Every engine-emitted (built-in) relation declaration, in documentation order.
/// One list so `builtin_rel_names`, the self-describing `rel_catalog`, and the
/// doc-completeness check all read the SAME source. Keep in sync with
/// `declare_builtins` (which declares the same set).
pub fn all_builtin_decls() -> Vec<RelDecl> {
    builtin_rel_decls()
        .into_iter()
        .chain(module_rel_decls())
        .chain(type_rel_decls())
        .chain(doc_text_rel_decls())
        .chain(const_value_rel_decls())
        .chain(comment_rel_decls())
        .chain(template_rel_decls())
        .chain(unresolved_rel_decls())
        .chain(call_rel_decls())
        .chain(dataflow_rel_decls())
        .chain(doc_rel_decls())
        .chain(spine_rel_decls())
        .chain(node_rel_decls())
        .chain(crate::rels::rel_kind_decls())
        .chain(daemon_rel_decls())
        .chain(every_rel_decls())
        .chain(clock_rel_decls())
        .chain(effect_rel_decls())
        .chain(hook_rel_decls())
        .chain(diag_rel_decls())
        .chain(diag_stage_rel_decls())
        .chain(hover_note_rel_decls())
        .chain(graph_rel_decls())
        .chain(diag_mute_rel_decls())
        .chain(demand_rel_decls())
        .chain(checkout_out_rel_decls())
        .chain(type_decl_rel_decls())
        .collect()
}

/// Names of every engine-emitted (built-in) relation. The daemon's `schema` RPC
/// flags relations against this so the count and the per-rel "emitted by the
/// engine" label agree (a user `.dl` rule's relation is not in this set).
pub fn builtin_rel_names() -> std::collections::HashSet<String> {
    all_builtin_decls().into_iter().map(|d| d.name).collect()
}

/// Ambient enum brands carried by BUILTIN relation columns: brand name -> the
/// closed literal vocabulary. Injected into `typecheck::Brands` without any user
/// `type` decl, so a literal pin against e.g. `type_edge.kind` outside the set is
/// an `enum-variant-unknown` error (the documented #1 agent failure mode: pin
/// `kind = "fields"`, get silent zero rows). Each set mirrors its extractor's
/// literal emit sites — adding a kind string there means adding it HERE (the
/// enum check turns a missed entry into loud false errors, never silent rows).
/// A user `type` decl reusing one of these names is a load error.
pub fn builtin_enum_brands() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        // typegraph.rs edge emitters (push/bound_edge literal kind args, all 3 langs).
        (
            "type_edge_kind",
            &[
                "field", "variant", "impl", "generic", "param", "returns", "uses",
            ],
        ),
        // typegraph.rs EntityKind::tag.
        (
            "type_entity_kind",
            &[
                "struct",
                "enum",
                "trait",
                "class",
                "interface",
                "alias",
                "function",
                "method",
                "const",
            ],
        ),
        // typegraph.rs push_node + ts_push literal kind args (union across the
        // Rust/Kotlin/TS lifts; the two variable-kind call sites resolve to
        // new/call_res).
        (
            "df_node_kind",
            &[
                "binop",
                "block",
                "borrow",
                "break",
                "call_res",
                "closure",
                "concat",
                "cond",
                "expr",
                "if",
                "let_bind",
                "lit",
                "logic",
                "loop",
                "match",
                "member",
                "new",
                "param",
                "ret",
                "template",
                "unop",
                "var_read",
                "var_write",
            ],
        ),
        // typegraph.rs ConstValueFact.kind literals (ts_collect_const_values /
        // ts_enum_const_values / rust_const_values_from).
        ("const_value_kind", &["lit", "template"]),
        // CheckoutOutcome.action literals (checkout_one / the throttle skip row).
        ("checkout_action", &["ff", "branch-f", "skip"]),
    ]
}

/// The variant set of one ambient builtin enum brand, or `None` for a user brand
/// (or any unknown name).
pub fn builtin_enum_variants(brand: &str) -> Option<&'static [&'static str]> {
    builtin_enum_brands()
        .iter()
        .find(|(name, _)| *name == brand)
        .map(|(_, vs)| *vs)
}

/// Built-in relation names whose decl carries an empty `doc`. The
/// doc-completeness invariant: this must be empty. A test asserts it, so adding
/// a built-in without documenting it on its `RelDecl` fails CI rather than
/// silently shipping a blank row in the generated table.
pub fn undocumented_builtins() -> Vec<String> {
    let mut missing: Vec<String> = all_builtin_decls()
        .into_iter()
        .filter(|d| d.doc.is_empty())
        .map(|d| d.name)
        .collect();
    missing.sort();
    missing
}

/// One-line documentation for every scalar function callable in a rule head or
/// comparison: `(name, arity, group, doc)`. THIS is the single source of function
/// docs — `fn_catalog` projects it and `examples/fn-catalog.dl` renders it into
/// the README, mirroring the per-decl doc/group + `rel_catalog` for relations. A new
/// `STR_FNS` entry is forced to appear here by `undocumented_fns` (a test fails
/// until it does). Docs avoid `|` so they render inside a markdown table cell.
pub fn fn_docs() -> &'static [(&'static str, usize, &'static str, &'static str)] {
    &[
        // native: lowered to SQLite or a hand-coded arm in lower.rs.
        ("split", 3, "string", "split text on a separator; idx 0-based, negative counts from the end (-1 = last); out-of-range drops the row (NULL filter); the sprf_split UDF"),
        ("replace", 3, "string", "replace ALL occurrences of `from` with `to`; SQLite-native"),
        ("sym", 1, "string", "identity compatibility builtin; text columns are interned automatically"),
        ("int", 1, "cast", "text->int coercion (leading-int prefix, else 0); fills an int column or compares numerically; SQLite CAST"),
        // json constructors: variadic (arity shown is the minimum). Build a JSON
        // string in a head or comparison; SQLite-native. Pair with the json_group_*
        // head aggregates for nested output.
        ("json_object", 2, "json", "build a JSON object from (key, value, ...) pairs; even arity >= 2; values keep their type (int -> number, text -> string); SQLite-native json_object"),
        ("json_array", 1, "json", "build a JSON array from the arg values; arity >= 1; SQLite-native json_array"),
        ("json", 1, "json", "validate and minify a JSON string (passthrough); SQLite-native json()"),
        // pass-through string builtins (STR_FNS in lower.rs -> sprf_* UDFs).
        ("lower", 1, "string", "lowercase (Unicode-aware)"),
        ("upper", 1, "string", "uppercase (Unicode-aware)"),
        ("lcfirst", 1, "string", "first char lowercased, the rest unchanged"),
        ("ucfirst", 1, "string", "first char uppercased, the rest unchanged"),
        ("trim", 1, "string", "strip leading and trailing whitespace"),
        ("strip_prefix", 2, "string", "drop a leading affix if present, else return the input unchanged (idempotent cleanup, not a filter — pair with =~ /^p/ for drop-on-miss)"),
        ("strip_suffix", 2, "string", "drop a trailing affix if present, else return the input unchanged"),
        ("replace_re", 3, "string", "regex replace-all with $1 group refs; the pattern shares the process-wide compile cache"),
        ("norm", 1, "string", "normalize for comparison: keep ASCII alphanumerics, lowercase, drop the rest — the same fold as the `string(id,text,norm)` rel's norm column, so `norm(a) = norm(b)` is a punctuation/case-blind compare and text joins against `string.norm`"),
    ]
}

/// Lowering names (STR_FNS plus the hand-coded native arms) that have no matching
/// `fn_docs` entry. The doc-completeness invariant: this must be empty. A test
/// asserts it, so adding a string builtin without documenting it fails CI.
pub fn undocumented_fns() -> Vec<String> {
    let documented: std::collections::HashSet<(&str, usize)> =
        fn_docs().iter().map(|(n, a, _, _)| (*n, *a)).collect();
    let native: [(&str, usize); 6] = [
        ("split", 3),
        ("replace", 3),
        ("int", 1),
        ("json_object", 2),
        ("json_array", 1),
        ("json", 1),
    ];
    let mut missing: Vec<String> = crate::lower::STR_FNS
        .iter()
        .map(|(n, _, a)| (*n, *a))
        .chain(native)
        .filter(|(n, a)| !documented.contains(&(n, *a)))
        .map(|(n, a)| format!("{n}/{a}"))
        .collect();
    missing.sort();
    missing
}

/// One-line documentation for every body/sink OP: `(op, kind, syntax, semantics)`.
/// `kind` ∈ source / body / sink. THIS is the single source of op docs — `op_catalog`
/// projects it and `gen-reference.dl` renders `docs/reference/syntax.md` plus the
/// README splice, mirroring `fn_docs`/`fn_catalog` for functions. Docs avoid `|`
/// so they render inside a markdown table cell (use `/` for alternatives).
pub fn op_docs() -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    &[
        // source ops: body position, extract facts from files. Cannot join derived rels.
        ("scan", "source", "scan([repo,][rev,] glob, path[, rev_out])", "select files; 2-ary omits rev_out, 5-ary names a repo coordinate; outputs path/rev_out take the _ or name: form (rev_out _ or omitted = rev not bound); repo defaults \".\", rev \"WORK\" (WORK/HEAD/any git rev)"),
        ("match_line", "source", "match_line(path, rev, /re/, line[, id][, col, end_col])", "LINE REGEX over file content — for FLAT TEXT (ini/env/log/csv) only, never structured source code (a construct spanning more than one line will not match; use match_ast for source). one row per match line; (?<name>..) named groups bind captured text as dl vars; $cap is sugar for a lazy named group; trailing id is a match ID for the whole-match span, not captured text; trailing col/end_col are its coordinates"),
        ("match", "source", "match(...) — DEPRECATED alias for match_line(...)", "deprecated pre-rename spelling; parses identically to match_line and still runs, but emits a deprecated-op-name warning naming match_line (and match_ast for source code) — see match_line"),
        ("ast", "source", "ast(path, rev, :lang, \"(query) @cap\", line[, end])", "tree-sitter query; @cap captures bind same-named vars; :lang ∈ rust/c/kotlin/..."),
        ("match_ast", "source", "match_ast(path, rev, :lang, \"$X.unwrap()\", line[, col, end_line, end_col][, id])", "ast-grep structural pattern — the correct tool for SOURCE CODE (sees multi-line and AST-shaped constructs a line regex cannot); metavar $X binds dl var X (matched text); trailing id binds the whole-match span for structural rewrite via gen(:replace). TERM form match_ast(:lang, str, \"pat\"[, line, col, end_line, end_col]) parses a STRING bound earlier in the rule (an embedded language body — styled-components css, a code fence) with the ast-grep grammar; spans are RELATIVE to that string, no file, no id (the string form of match_ast, runs in the join+extract pass like term-form json/jsonp)"),
        ("sg", "source", "sg(...) — DEPRECATED alias for match_ast(...)", "deprecated pre-rename spelling; parses identically to match_ast (file and term form alike) and still runs, but emits a deprecated-op-name warning naming match_ast — see match_ast"),
        ("ast_yaml", "source", "ast_yaml(path, rev, :lang, \"rule yaml\", line, ...)", "ast-grep RuleCore YAML body (inside:/has: relational rule) instead of a pattern string; inside: matches the immediate parent only; there is no field: selector — use kind + inside; span outputs share the match_ast form"),
        ("json", "source", "json(path, rev, q:{ $k: $v })", "declarative brace pattern over json/yaml/toml; each match binds named key AND value captures as dl vars; supports **: recursion, [...$x] spread, re:/glob keys"),
        ("jsonp", "source", "jsonp(path, rev, \"a.*.b\", out)", "dotted path over json/yaml/toml (* = any key/element); the value is located; the string form of json"),
        ("cmd", "source", "cmd(path, rev, \"tool {file}\", line, out)", "shell out per matched file, one row per stdout line; cached by (file hash, rule text); nonzero exit + stdout = findings, nonzero + empty = error"),
        ("comment", "source", "comment(path, rev, /open/[, /close/], l0, l1, label)", "comment-marker regions in any file type; one regex = sequential dividers, two = paired BEGIN/END with LIFO nesting; l0/l1 are 1-based marker lines; pairs with gen splice"),
        // body constructs: derived rules.
        ("atom", "body", "edge(from, to) / edge(to: dst) / edge(\"x\", 1, kind: edge_kind)", "positive atom; binds its vars from the relation. Positional by slot (edge(from, to)) OR named mode once any `col: term` appears: then a term that carries a name binds by name (a bare var `from` puns to its own column `from: from`, the JS/Rust struct shorthand, in any order), and a nameless literal fills the next column left open (Python-style positional prefix); an unmentioned column is a don't-care, so you name only the columns you use instead of counting positional `_`"),
        ("negation", "body", "!edge(from, _)", "negation / anti-join; the row must NOT exist in the relation"),
        ("comparison", "body", "= != < <= > >=", "scalar comparison on bound vars or literals (n >= 4, path != fs:src/db.rs)"),
        ("regex", "body", "name =~ /^[A-Za-z]+$/", "regex constraint (SQLite REGEXP); the /.../ unified regex literal, same form match/comment/sg use; escape // as \\/\\/ because // starts a comment; (?i) folds character classes too, so uppercase-boundary checks need case-exact branches; named captures bind only in match, not in a plain =~ constraint (parse accepts it, but run errors on the unbound variable)"),
        ("glob", "body", "path ~~ \"src/*\"", "glob constraint (SQLite GLOB)"),
        ("closure", "body", "closure(edge)", "transitive closure of a 2-col relation as the entire body (SCC-condensed); pin an endpoint for a point query; mixed-body closure is literal-seeded only"),
        ("scc", "body", "head(rep, member) <- scc(edge)", "strongly-connected-component condensation of a 2-col relation as the entire body; binds (representative, member) per node; mirrors closure, evaluated outside SQL"),
        ("node2vec", "body", "head(node_a, node_b, score) <- node2vec(edge)", "structural graph embedding of a 2-col relation as the entire body; binds node pairs with a similarity score (the graph-position sibling of the text `similar` rel); evaluated outside SQL"),
        ("arith", "body", "+ - * / %", "arithmetic in rule heads and comparison sides (rank(path, line+1)); `+` is overloaded — int + int adds, text + text concatenates (url = \"https://\" + host), mixed int/text is a typecheck error (interpolate or int(..)); - * / % stay int-only; usual precedence, parens OK; never in a binding atom"),
        ("strfn", "body", "split(text, sep, idx) / replace(text, from, to)", "string functions in heads and comparison sides; idx 0-based, negative counts from the end; a computed binding (ext = split(path, \".\", -1)) binds for later use in the same body — later joins, negations, and the head all see it (derived rules only; a source rule inlines into the head)"),
        ("aggregation", "body", "count sum min max json_group_array json_group_object", "head-position aggregation, in a rule head OR a `?` query head; non-aggregate head terms are the grouping key; count/sum produce int, min/max carry the arg type; json_group_array(x) / json_group_object(k, v) build a JSON array/object per group (deterministic, ORDER BY inside the agg); in a query head json_group_object(key, value) consumes two columns: place it at the key column and put `_` at the value column; count in body is a parse error"),
        // sinks.
        ("query", "sink", "? rel(from, to). / ? rel(col: value). / ? rel(key, count(n)).", "query grammar is bare `? rel(vars).` only: no `:-` and no `==` (equality is shared variables); print a TSV block (data rows start at column 0, no indent), or JSON-lines with --query-json ({query,columns,rows,count} per query), or one JSON array of {col: value} row-objects per query with --format json; a literal in any position filters; args may be named by column (`col: value`), unmentioned columns are don't-cares; an aggregate call (count/sum/min/max/json_group_array/json_group_object) in a query head groups over the plain-var columns (whole-rel aggregate when none), the agg arg names the output column"),
        ("diag", "sink", "diag(path: hit_path, line: hit_line, msg: message[, col: , end_line: , end_col: , severity: , code: , hint: ]) <- ...", "head the built-in diagnostic sink; fixed 9-col schema (path/line/col/end_line/end_col/severity/code/msg/hint), name only the columns you use, the rest default (severity warn, end_line=line); feeds editor diagnostics (--lsp) and check output (--check)"),
        ("gen", "sink", "gen([:mode,] path, [l0, l1,] \"{var} template\")", "codegen; file form renders body rows in output-text order (there is no order-by column) through a path+row template; interpolation is `{var}`, not `${var}`; :zone replaces content between a NAMED `BEGIN:/END:` marker pair (markers stay, immune to surrounding edits); splice form replaces between two line numbers; raw `r\"...\"` / `r#\"...\"#` keep dollar-brace verbatim; `{{x}}` in a template emits literal `{x}`; convergent (skips write when bytes match); never runs under --check/--lsp; one-shot needs --apply"),
        ("graph_node", "sink", "graph_node(id: node_id, label: label, kind: kind[, file: , line: , parent: ]) <- ...", "head the built-in drawable-graph vertex sink; fixed 6-col schema (id/label/kind/file/line/parent), name only the columns you use; the flow panel's always-available Graph preset draws graph_node/graph_edge with no bespoke SQL"),
        ("graph_edge", "sink", "graph_edge(src: src_id, dst: dst_id, kind: kind) <- ...", "head the built-in drawable-graph edge sink; connects two graph_node ids, kind is the wire label; read by the Graph preset alongside graph_node"),
        ("hover_note", "sink", "hover_note(path: hit_path, line: hit_line, end_line: hit_line, end_col: hit_end_col, md: note_text[, col: ]) <- ...", "head the built-in hover-note sink; fixed 6-col schema (path/line/col/end_line/end_col/md), name only the columns you use; the LSP hover path appends md to the hover shown at any position inside [line,col]..[end_line,end_col], 0-based like diag"),
    ]
}

/// The diagnostic sink relation (see `DIAG_RELS`). Fixed schema, `path` is TEXT
/// (not the `file` checked type) so a synthetic origin — `"(engine)"`,
/// `"(checked-notes)"` — is not row-dropped by the file check.
pub(crate) fn diag_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "diag".into(), cols: vec![
            c("path", Type::Text), c("line", Type::Int), c("col", Type::Int),
            c("end_line", Type::Int), c("end_col", Type::Int),
            c("severity", Type::Text), c("code", Type::Text),
            c("msg", Type::Text), c("hint", Type::Text)],
            group: "diag",
            doc: "diagnostic sink; head it from a rule to emit an editor squiggle (--lsp), a --check finding, or a daemon-hook message. Fixed 9-col schema — write only the cols you need via named args (diag(path: p, line: l, msg: m)); the rest are NULL and default (severity warn, end_line=line, ints 0). path is TEXT so a synthetic origin isn't file-checked away; line/col: 0-based",
            ..Default::default() },
    ]
}

/// The diag-stage routing sink (see `DIAG_STAGE_RELS`). Fixed 2-col schema
/// (code, stage). Like `diag`, engine-declared but USER-WRITTEN: a rail heads
/// `diag_stage(code, stage)` beside its `diag(...)` rule to route a diagnostic
/// code to one or more surfaces (live / commit / agent-turn / agent-session).
/// A code with no `diag_stage` row routes by severity default (error ->
/// everywhere, warning -> commit only). Staging is presentation-time only; the
/// db keeps every `diag` row. See plans/2026-07-17-diag-stage-routing.md (R7).
pub(crate) fn diag_stage_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "diag_stage".into(), cols: vec![
            c("code", Type::Text), c("stage", Type::Text)],
            group: "diag",
            doc: "diag routing sink; head diag_stage(code, stage) from a rule to route a diagnostic code to a surface — stage is one of live / commit / agent-turn / agent-session. A code may carry several rows (one per stage it opts into). A code with NO diag_stage row routes by severity default: error -> every stage, warning -> commit only. Presentation-time filtering only; the db keeps every diag (`? diag(...)` stays complete)",
            ..Default::default() },
    ]
}

/// The hover-note sink relation (see `HOVER_RELS`). Fixed 6-col schema, same
/// span convention as `diag` (0-based line/col, end_line/end_col inclusive).
pub(crate) fn hover_note_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "hover_note".into(), cols: vec![
            c("path", Type::File), c("line", Type::Int), c("col", Type::Int),
            c("end_line", Type::Int), c("end_col", Type::Int), c("md", Type::Text)],
            group: "diag",
            doc: "markdown hover note attached to a source span; head it from a rule, shown by the LSP on hover. Positions are 0-based, same convention as diag (end_line/end_col inclusive); several notes on one span all show, appended after the synthesized entity hover; line/col: 0-based",
            ..Default::default() },
    ]
}

/// The drawable-graph sink relations (see `GRAPH_RELS`). Fixed schema so every
/// program's graph unions into the same two tables. `id` columns are TEXT (a
/// node id is any string — a sym, a path:line, a synthetic label); `file` is
/// TEXT (not the checked `file` type) so a non-path grouping key or a NULL is
/// not row-dropped, mirroring `diag.path`.
pub(crate) fn graph_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "graph_node".into(), cols: vec![
            c("id", Type::Text), c("label", Type::Text), c("kind", Type::Text),
            c("file", Type::Text), c("line", Type::Int), c("parent", Type::Text)],
            group: "graph",
            doc: "drawable-graph vertex sink: head graph_node(id, label, kind[, file, line, parent]) from a rule and the flow panel's always-available Graph preset draws it — no bespoke node SQL. Fixed 6-col schema; write only the cols you need via named args (graph_node(id: text, label: name, kind: k)); file/line place the node in the fs-tree + jump target, parent nests it in list view, all NULL by default; line: writer-defined",
            ..Default::default() },
        RelDecl { name: "graph_edge".into(), cols: vec![
            c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text)],
            group: "graph",
            doc: "drawable-graph edge sink: head graph_edge(src, dst, kind) from a rule to connect two graph_node ids; kind is the wire label/style. Read by the Graph preset alongside graph_node",
            ..Default::default() },
    ]
}

/// The diagnostic-mute set relation (see `MUTE_RELS`). One TEXT column, `code`.
/// Written only via `toggle_diag_mute`; the LSP publish path reads it to filter
/// `diag` rows.
pub(crate) fn diag_mute_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "diag_mute".into(), cols: vec![c("code", Type::Text)],
            group: "diag",
            doc: "diagnostic-mute set: one row per diag code silenced in the editor session (via the LSP `dl.toggleDiagCode` command). The --lsp publish path drops `diag` rows whose code is muted; --check/--parse-only read `diag` directly and ignore this set. Written only by the toggle command, never a rule head",
            ..Default::default() },
    ]
}

/// The demand / overlay sink relations (see `DEMAND_RELS`). Pre-declared
/// builtins a user heads from a rule; deriving rows drives the bound engine
/// behavior. Catalogued (group "demand") so the binding is visible; reserved
/// against a `rel` re-declaration.
pub(crate) fn demand_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "scip_want".into(), cols: vec![c("repo", Type::Text)],
            group: "demand",
            doc: "SCIP index demand sink: head scip_want(repo) to make the importer ensure + load that repo's index.scip (runs installed indexers when missing, merges, loads into scip_def/scip_ref/scip_edge); one-tick latency, shallow clones skip loudly",
            ..Default::default() },
        RelDecl { name: "rev_cmp_want".into(), cols: vec![
            c("repo", Type::Text), c("refname", Type::Text), c("upstream", Type::Text)],
            group: "demand",
            doc: "git ancestry demand sink: head rev_cmp_want(repo, refname, upstream) and each wanted triple runs git rev-list, filling rev_behind(repo, refname, upstream, behind, ahead); unresolvable refs and shallow clones skip loudly",
            ..Default::default() },
        RelDecl { name: "def_target".into(), cols: vec![
            c("name", Type::Text), c("file", Type::Path), c("line", Type::Int), c("kind", Type::Text)],
            group: "demand",
            doc: "LSP go-to-definition sink: head def_target(name, file, line, kind) and textDocument/definition resolves a symbol reference to (file, line) by name; falls back to the module-edge specifier match when empty. Read by column name, so a subset written via named args works; line: writer-defined",
            ..Default::default() },
        RelDecl { name: "effect_cmd".into(), cols: vec![
            c("kind", Type::Text), c("template", Type::Text)],
            group: "demand",
            doc: "effect-template overlay sink: head effect_cmd(kind, template) to override the shell command for an effect kind at drain time (dynamic per-kind template), read as the effect executor is built",
            ..Default::default() },
        RelDecl { name: "checkout".into(), cols: vec![
            Col::raw("repo", Type::Text), Col::raw("branch", Type::Text), Col::raw("pr_heads", Type::Text)],
            group: "demand",
            doc: "git checkout demand sink (the ghcacher keep-current half): head checkout(repo, branch, pr_heads) and each row clones a missing config repo, fetches origin, then NON-DESTRUCTIVELY keeps `branch` current — `merge --ff-only origin/<branch>` when that IS the current branch + the working tree is clean (skip on dirty or diverged; never stash, never reset), else `git branch -f` the ref without touching HEAD or the working tree. branch empty = discover origin/HEAD; pr_heads \"1\"/\"true\" also mirrors +refs/pull/*/head. DL_NO_FETCH skips the network (re-points to already-fetched refs only). DL_CHECKOUT_DRY_RUN=1 previews the plan without mutating. The sink drains on the daemon poll loop / --watch / --settle / one-shot --apply (not on a bare `?` read). Repos sweep in parallel on a narrow pool; failures skip loudly",
            ..Default::default() },
    ]
}

/// `checkout_done` — the OUTCOME rel the `checkout` sweep writes (read-only,
/// like `rev_behind` is the output of `rev_cmp_want`). One row per swept repo; a
/// program reads it to confirm the sweep fired and diag failures. Reserved:
/// engine-written, never a rule head.
///
/// `checkout_plan` — the dry-run twin (same schema): `DL_CHECKOUT_DRY_RUN=1`
/// emits the planned action per repo (ff/branch-f/skip) WITHOUT running
/// `merge --ff-only` or `git branch -f`, so a program/CLI can preview a sweep.
pub(crate) const CHECKOUT_OUT_RELS: [&str; 2] = ["checkout_done", "checkout_plan"];

pub(crate) fn checkout_out_rel_decls() -> Vec<RelDecl> {
    let cols = || {
        vec![
            Col::raw("repo", Type::Text),
            Col::raw("branch", Type::Text),
            Col {
                name: "action".into(),
                ty: Type::Text,
                brand: Some("checkout_action".into()),
                raw: true,
            },
            Col::plain("ok".into(), Type::Int),
            Col::raw("detail", Type::Text),
        ]
    };
    vec![
        RelDecl { name: "checkout_done".into(), cols: cols(), group: "demand",
            doc: "checkout-sweep outcome (written by the `checkout` sink, read-only): one row per swept repo — action is ff/branch-f/skip, ok is 1/0, detail is the git result. Confirms the sweep fired from a live daemon (stderr goes to daemon.log) and lets a program diag failures (ok=0); one-tick latency like other demand outputs", ..Default::default() },
        RelDecl { name: "checkout_plan".into(), cols: cols(), group: "demand",
            doc: "checkout-sweep PREVIEW (written when DL_CHECKOUT_DRY_RUN=1, read-only): same shape as checkout_done, but the sink computes the action without running `merge --ff-only` or `git branch -f` — nothing in any checkout is mutated. Use to preview what `checkout` would do before opting in via --apply / DL_APPLY_SINKS=1", ..Default::default() },
    ]
}

/// The derived-shape sink relation (see `TYPE_DECL_RELS`). Fixed 4-col schema.
/// `shape` is TEXT (a shape name; any string, computed via concat is fine),
/// `pos` is the 0-based column position, `col` the column name, `type` the base
/// type keyword (text/int/path/...) OR a declared brand name.
pub(crate) fn type_decl_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "type_decl_row".into(), cols: vec![
            c("shape", Type::Text), c("pos", Type::Int), c("col", Type::Text), c("type", Type::Text)],
            group: "types",
            doc: "derived-shape sink: head type_decl_row(shape, pos, col, type) from a derived rule to compute a relation schema from data. At end of tick its rows persist; on the next tick a `rel name: shape.` decl with no syntax `type name(...)` resolves its columns from them (shape-pending info diag until then, shape-shadowed warn if a syntax shape shares the name). the type column is a base type keyword or a declared brand; an unknown type keeps that shape pending. Derived-only (route a jsonp/json extract through its own rel first)",
            ..Default::default() },
    ]
}

/// True when the program HEADS `type_decl_row` from any rule — gates the
/// end-of-tick persist and tells `expand_shapes` to DEFER (not error) an
/// unresolved `rel name: shape.` ref (the shape derives next tick).
pub fn type_decl_row_used(prog: &Program) -> bool {
    prog.items
        .iter()
        .any(|it| matches!(it, Item::Rule(r) if r.head.rel == "type_decl_row"))
}

pub(crate) fn effect_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "effect_log".into(), cols: vec![
            c("id", Type::Text), c("kind", Type::Text), c("head", Type::Text),
            c("state", Type::Text), Col::raw("args", Type::Text), c("req_tx", Type::Int)], group: "effect",
            doc: "the @async/@stream drain queue: one row per request (id, kind, head rel, state queued/running/done/failed, args JSON, req_tx); the dl-native call log, queryable live and parity-comparable to an external cache's call log", ..Default::default() },
    ]
}

pub(crate) fn effect_rels_used(prog: &Program) -> bool {
    rels_used(prog, &EFFECT_RELS)
}

/// The harness-hook event log (see `HOOK_RELS`). Accumulating facts, one row per
/// `dl --hook` invocation; the raw event JSON rides the `json` column so all
/// field extraction is the program's job (term-form json/jsonp), no per-event
/// column in the engine.
pub(crate) fn hook_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "hook_event".into(), cols: vec![
            c("kind", Type::Text), c("session", Type::Text),
            c("seq", Type::Int), Col::raw("json", Type::Text)],
            group: "hook",
            doc: "harness-hook event log: one accumulating row per `dl --hook` invocation (kind = the event name UserPromptSubmit/PostToolUse/..., session = the event session id, seq = an ingest-time monotone millis stamp ordering events within a session, json = the raw event JSON). Written by the hook feed, never a refresh; extract fields with term-form json/jsonp",
            ..Default::default() },
    ]
}

pub(crate) fn hook_rels_used(prog: &Program) -> bool {
    rels_used(prog, &HOOK_RELS)
}

pub(crate) fn module_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "module_import".into(), cols: vec![
            c("file", Type::Path), c("rev", Type::Rev), c("specifier", Type::Text), c("kind", Type::Text), c("line", Type::Int)], group: "module",
            doc: "import statements (Rust + TS + Kotlin); Kotlin adds kind=same-package rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files; line: 1-based", ..Default::default() },
        RelDecl { name: "module_edge".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path)], group: "module",
            doc: "resolved file-to-file import graph (rev-deduped union)", ..Default::default() },
        RelDecl { name: "module_edge_rev".into(), cols: vec![c("src", Type::Path), c("dst", Type::Path), c("rev", Type::Rev)], group: "module",
            doc: "rev-aware module_edge", ..Default::default() },
        RelDecl { name: "module_unresolved".into(), cols: vec![
            c("file", Type::Path), c("specifier", Type::Text), c("reason", Type::Text), c("line", Type::Int)], group: "module",
            doc: "broken imports: a reference that resolved to no project file (the linter question); line: 1-based", ..Default::default() },
        RelDecl { name: "module_unresolved_rev".into(), cols: vec![
            c("file", Type::Path), c("rev", Type::Rev), c("specifier", Type::Text), c("reason", Type::Text), c("line", Type::Int)], group: "module",
            doc: "rev-aware module_unresolved; line: 1-based", ..Default::default() },
        RelDecl { name: "crate_edge".into(), cols: vec![c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text), c("rev", Type::Rev)], group: "module",
            doc: "workspace-internal Cargo dependency edges", ..Default::default() },
        RelDecl { name: "module_binding_resolved_rev".into(), cols: vec![
            c("file", Type::Path), c("local", Type::Text), c("source", Type::Text), c("dst", Type::Path), c("rev", Type::Rev)], group: "module",
            doc: "the resolved subset (dst = resolved file, alias-only today) of module_binding: aliased-import local bindings from the module resolvers' own parse (Rust use..as, TS import{a as b}/default, Kotlin import..as) — the index-free equivalent of scip_binding; local is the binding name in scope at file, source is the exported name at dst (\"default\" for a default import)", ..Default::default() },
        RelDecl { name: "module_binding_resolved".into(), cols: vec![
            c("file", Type::Path), c("local", Type::Text), c("source", Type::Text), c("dst", Type::Path)], group: "module",
            doc: "rev-deduped union of module_binding_resolved_rev", ..Default::default() },
        RelDecl { name: "module_binding_rev".into(), cols: vec![
            c("file", Type::Path), c("local_name", Type::Text), c("source_module", Type::Text), c("imported_name", Type::Text), c("kind", Type::Text), c("rev", Type::Rev)], group: "module",
            doc: "every local binding an import introduces, parsed off the import AST so aliased/library symbols resolve without scip — unlike module_binding_resolved_rev (alias-hop only, resolved-file-only), this fires for EVERY resolution incl. External (library) and Unresolved, and covers plain named/namespace/default/side-effect bindings too, not just aliased ones; source_module is the specifier as written (module_import's specifier text), imported_name the canonical exported name at the import site (\"default\"/\"*\"/\"\" for default/namespace/side-effect), kind = named/default/namespace/side_effect/reexport (Rust pub use). Two-line join for \"which library does this local name come from\": binds_lib(local_name, source_module) <- module_binding(file, local_name, source_module, _, _). then query ? binds_lib(\"myAlias\", lib)", ..Default::default() },
        RelDecl { name: "module_binding".into(), cols: vec![
            c("file", Type::Path), c("local_name", Type::Text), c("source_module", Type::Text), c("imported_name", Type::Text), c("kind", Type::Text)], group: "module",
            doc: "rev-deduped union of module_binding_rev", ..Default::default() },
    ]
}

pub(crate) fn type_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "type_edge".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), Col::branded("kind", "type_edge_kind"), c("repo", Type::Text)], group: "type",
            doc: "type-graph edges across Rust (syn), Kotlin (tree-sitter), TS (oxc); kind is field/variant/impl/generic — Kotlin interface supertypes are generic, class/object impl, val/var ctor params + body properties field, enum entries variant; trailing repo column so two trees scanned together don't collapse same-named types into one node (closure/scc still walk cols 0/1, unaffected)", ..Default::default() },
        RelDecl { name: "type_edge_rev".into(), cols: vec![c("from", Type::Text), c("to", Type::Text), Col::branded("kind", "type_edge_kind"), c("rev", Type::Rev), c("repo", Type::Text)], group: "type",
            doc: "rev-aware type_edge (WORK-vs-HEAD type diff)", ..Default::default() },
        RelDecl { name: "type_entity".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("name", Type::Text), Col::branded("kind", "type_entity_kind"),
            c("parent", Type::Text), c("file", Type::Path), c("line", Type::Int)], group: "type",
            doc: "every declared type; sym is file::kind::name, the cross-graph join key; scip_ref overrides name resolution when a SCIP index is present; line: 1-based", ..Default::default() },
        RelDecl { name: "type_entity_rev".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("name", Type::Text), Col::branded("kind", "type_entity_kind"),
            c("parent", Type::Text), c("file", Type::Path), c("line", Type::Int), c("rev", Type::Rev)], group: "type",
            doc: "rev-aware type_entity (rev is a column, never folded into the sym, so a diff compares the same sym across revs); legacy type_entity is the rev-deduped union; line: 1-based", ..Default::default() },
        RelDecl { name: "type_sig".into(), cols: vec![
            c("sym", Type::Text), c("slot", Type::Text), c("pos", Type::Int), c("ref", Type::Text)], group: "type",
            doc: "type signature slots (params, fields) per sym", ..Default::default() },
        RelDecl { name: "type_link".into(), cols: vec![c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text)], group: "type",
            doc: "cross-type links not carried by type_edge (SCIP-resolved sym to sym); src/dst are already repo-prefixed via type_entity's sym, so no separate repo column is needed", ..Default::default() },
        RelDecl { name: "type_link_rev".into(), cols: vec![c("src", Type::Text), c("dst", Type::Text), c("kind", Type::Text), c("rev", Type::Rev)], group: "type",
            doc: "rev-aware type_link (SCIP-resolved sym-to-sym per rev); legacy type_link is the rev-deduped union", ..Default::default() },
    ]
}

pub(crate) fn doc_text_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "doc_comment".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("line", Type::Int), Col::raw("text", Type::Text)], group: "type",
            doc: "doc comment per type_entity sym: (repo, sym, line, text); AST-located per language (Rust #[doc] attrs, Kotlin KDoc sibling, TS leading /** */); line: 1-based", ..Default::default() },
        RelDecl { name: "doc_tag".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("tag", Type::Text),
            c("arg", Type::Text), Col::raw("text", Type::Text)], group: "type",
            doc: "structured doc tags per sym: (repo, sym, tag, arg, text); @param/@returns/@deprecated for JSDoc/KDoc, # Section headings for rustdoc", ..Default::default() },
    ]
}

pub(crate) fn comment_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "comment_node".into(), cols: vec![
            c("path", Type::Path), c("line", Type::Int), c("col", Type::Int),
            c("end_line", Type::Int), c("end_col", Type::Int),
            Col::raw("text", Type::Text), c("kind", Type::Text)], group: "comment",
            doc: "every comment in every parsed file: (path, line, col, end_line, end_col, text, kind is line/block/doc); grammar-backed (oxc for TS/TSX, tree-sitter for Rust, Kotlin, Python, Go, C, ...), so a comment marker inside a string is never a row; text has the comment tokens stripped; std/suppress.dl parses it into the eslint/biome disable grammar; line: 1-based; col: 0-based", ..Default::default() },
    ]
}

pub(crate) fn template_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "template_parts".into(), cols: vec![
            c("file", Type::Path), c("line", Type::Int), c("node", Type::Text),
            c("idx", Type::Int), c("kind", Type::Text), Col::raw("text", Type::Text)], group: "template",
            doc: "every template literal's ordered static/interpolated pieces: (file, line, node, idx, kind is static/expr, text); TS/TSX/JS/JSX/MJS/CJS only (oxc), one line per file's occurrence group via node = the df_node/df_lit id for the SAME template occurrence (join key: node = df_lit.id, node = df_edge.to for whatever flows in); text is verbatim (raw static chunk or the interpolated expression's exact source); template-built import paths/URLs/keys become joinable; line: 1-based", ..Default::default() },
    ]
}

pub(crate) fn unresolved_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "unresolved".into(), cols: vec![
            c("file", Type::Path), c("line", Type::Int), c("reason", Type::Text), Col::raw("detail", Type::Text)], group: "unresolved",
            doc: "an edge that could exist but whose target is computed at runtime (as opposed to module_unresolved's no-edge-at-all case); (file, line, reason, detail is the computed thing's exact source text); TS/TSX/JS/JSX/MJS/CJS only (oxc) in v1; reason is a closed vocabulary: dynamic-import (import(expr)/require(expr) with a non-literal argument), computed-member-call (obj[key]() callee), spread-call-args (f(...args)); Python star-imports/sys.path mutation stay out of v1 (already surfaced via module_unresolved / an eprintln); line: 1-based", ..Default::default() },
    ]
}

pub(crate) fn call_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "call_def".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("kind", Type::Text),
            c("file", Type::Path), c("line", Type::Int), c("end", Type::Int)], group: "call",
            doc: "every callable; sym is repo-qualified repo::file::kind::name; line/end: 1-based", ..Default::default() },
        RelDecl { name: "call_def_rev".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("kind", Type::Text),
            c("file", Type::Path), c("line", Type::Int), c("end", Type::Int), c("rev", Type::Rev)], group: "call",
            doc: "rev-aware call_def (rev is a column, never folded into the sym); legacy call_def is the rev-deduped union; line/end: 1-based", ..Default::default() },
        RelDecl { name: "call_site".into(), cols: vec![
            c("repo", Type::Text), c("caller", Type::Text), c("callee", Type::Text),
            c("file", Type::Path), c("line", Type::Int)], group: "call",
            doc: "each call occurrence; caller is the resolved fn sym, callee the bare text; changed_line joins here for line-scoped rails; line: 1-based", ..Default::default() },
        RelDecl { name: "call_edge".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text), c("kind", Type::Text)], group: "call",
            doc: "resolved caller-sym to callee-sym edge (single-def or SCIP override)", ..Default::default() },
        RelDecl { name: "call_edge_rev".into(), cols: vec![
            c("caller", Type::Text), c("callee", Type::Text),
            c("kind", Type::Text), c("rev", Type::Rev)], group: "call",
            doc: "rev-aware call_edge", ..Default::default() },
        // def sym -> bare callable name, so rules can resolve a call_site's
        // callee text to the set of candidate def syms (then filter, e.g. by
        // allocates). One row per def; a bare name may map to several syms.
        RelDecl { name: "call_name".into(), cols: vec![c("sym", Type::Text), c("name", Type::Text)], group: "call",
            doc: "def sym to bare callable name; resolves a call_site callee to candidate def syms", ..Default::default() },
        // Per-fn read/write classification of the fn's call sites. `fn` is the
        // caller sym (same shape as call_site.caller); `kind` is `read` or
        // `write`, classified from the bare callee name (execute/
        // execute_batch -> write; prepare/query_row/query_map -> read). Lets a
        // rail ask "does this fn contain any write?" via `call_kind(fn, "write")`
        // without re-declaring the method-name table per program.
        RelDecl { name: "call_kind".into(), cols: vec![c("fn", Type::Text), c("kind", Type::Text)], group: "call",
            doc: "per-fn read/write classification from the bare callee name (execute* -> write, query*/prepare -> read); rusqlite-shaped, collection names dropped to avoid false positives", ..Default::default() },
    ]
}

pub(crate) fn dataflow_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "df_node".into(), cols: vec![
            c("id", Type::Text), Col::branded("kind", "df_node_kind"), c("var", Type::Text),
            c("fn", Type::Text), c("file", Type::Path), c("line", Type::Int)], group: "dataflow",
            doc: "intra-procedural dataflow node (call_res/let_bind/param/ret/new/member/...); id is an interned StringId over file::line::kind (sym — BREAKING as of the intern-key arc) — the full kind vocabulary is rel_col's variants for this column; line: 1-based", ..Default::default() },
        // rev-aware df_node: id is the SAME interned id df_node uses (never
        // folded with rev) — rev is a real trailing column and `(id, rev)` is
        // the primary key, so two revs' `file:line:col` ids stay disjoint AS
        // ROWS (a byte-identical node at two revs gets two rows, one per rev)
        // while the id itself joins cleanly against legacy df_node and every
        // other df_*_rev twin. legacy df_node keeps one row per raw id, deduped
        // across the whole corpus (first-seen wins, no rev column); the
        // member-edge diff is name-joined, never raw-id-joined.
        // pk_never_null is NOT set here: the explicit `key(id, rev)` already
        // narrows the PRIMARY KEY off the full 7-column row, and 7 columns is
        // outside `wants_without_rowid`'s 2..=4 range regardless — no WITHOUT
        // ROWID vouch is needed or possible for this shape.
        RelDecl { name: "df_node_rev".into(), cols: vec![
            c("id", Type::Text), Col::branded("kind", "df_node_kind"), c("var", Type::Text),
            c("fn", Type::Text), c("file", Type::Path), c("line", Type::Int), c("rev", Type::Rev)],
            key: Some(vec!["id".into(), "rev".into()]), group: "dataflow",
            doc: "rev-aware df_node; id is the SAME interned id as df_node.id (never rev-folded); PRIMARY KEY (id, rev) keeps two revs' rows disjoint; legacy df_node keeps one deduped row per raw id; line: 1-based", ..Default::default() },
        // (df_node id, repo) — the repo (nearest `.git` basename) the node's file
        // was read from. df_node ids are path-keyed (file:line:col, no repo), so
        // this side table is the repo handle a cross-repo query needs to scope a
        // fill/param to its own folder instead of fanning across every repo that
        // shares the constructed type's NAME. First-seen wins (same dedup as
        // df_node). 1:1 with df_node.
        // pk_never_null: dataflow.rs pushes `vec![sym(&n.id), t(repo)]` per row
        // (extract/dataflow.rs) — a fixed 2-element literal from plain &str
        // fields, never Option, so no row can carry a NULL id/repo.
        RelDecl { name: "df_node_repo".into(), cols: vec![c("id", Type::Text), c("repo", Type::Text)], group: "dataflow",
            doc: "(df_node id, repo) — the repo (nearest .git basename) each node's file was read from; scopes df joins per-repo (df_node ids are path-keyed)", pk_never_null: true, ..Default::default() },
        // rev-aware df_node_repo: id is the SAME raw id as df_node_rev.id (never
        // salted), rev as its own column. Repo attribution stays orthogonal to
        // rev — a multi-repo PR diff wants both axes. The full 3-column row
        // (id, repo, rev) is already the natural dedup key (seen_node_repo_rev),
        // so the default full-row PRIMARY KEY needs no explicit `key(...)`
        // narrowing here.
        // pk_never_null: same push site as df_node_repo, `vec![sym(&n.id),
        // t(repo), t(rev)]` — fixed 3-element literal, never Option.
        RelDecl { name: "df_node_repo_rev".into(), cols: vec![c("id", Type::Text), c("repo", Type::Text), c("rev", Type::Rev)], group: "dataflow",
            doc: "rev-aware df_node_repo; id is the SAME interned id as df_node_rev.id (never rev-folded); legacy df_node_repo keeps the raw id", pk_never_null: true, ..Default::default() },
        // pk_never_null: `edge_rows.push(vec![sym(&e.from), sym(&e.to)])`
        // (extract/dataflow.rs) — both from a `TypeFacts` edge struct's plain
        // String fields, never Option; no row can carry a NULL endpoint.
        RelDecl { name: "df_edge".into(), cols: vec![c("from", Type::Text), c("to", Type::Text)], group: "dataflow",
            doc: "intra-procedural dataflow dependency edge", pk_never_null: true, ..Default::default() },
        // one row per loop, with its source span + loop variable. The flag rule
        // joins this against df_node/df_edge to find loop-invariant calls: a
        // call whose line falls in [start,end] taking an argument that is a
        // function param (not the loop variable).
        RelDecl { name: "loop_over".into(), cols: vec![
            c("file", Type::Path), c("start", Type::Int), c("end", Type::Int),
            c("var", Type::Text), c("collection", Type::Text), c("fn", Type::Text)], group: "dataflow",
            doc: "one row per loop with its span, iter var, and collection; start/end: 1-based", ..Default::default() },
        // one row per fn whose body builds a collection (Vec/HashMap/String ctor
        // or .collect/.clone/.to_string). The cost signal that cuts the
        // loop-invariant-call suspect list down to recomputation candidates.
        RelDecl { name: "allocates".into(), cols: vec![c("fn", Type::Text)], group: "dataflow",
            doc: "one row per fn whose body builds a collection (Vec/HashMap/String ctor, .collect/.clone/.to_string)", ..Default::default() },
        // one row per (call, enclosing loop) pair: `call_id` is the call_res
        // node, `loop_id` joins back to loop_over via "{file}:{start}", `depth`
        // is the loop's nesting rank (1 = outermost), `collection` is the inner
        // loop's iterated collection text ("" until extractors fill it). The
        // raw material for symbolic Big-O composed over call_edge.
        // pk_never_null: `nest_rows.push(vec![t(&ns.call_id), t(&ns.loop_id),
        // i(ns.depth), t(&ns.collection)])` — fixed 4-element literal from a
        // plain struct's fields (collection defaults to "" upstream, never
        // Option), never Option.
        RelDecl { name: "nest".into(), cols: vec![
            c("call_id", Type::Text), c("loop_id", Type::Text),
            c("depth", Type::Int), c("collection", Type::Text)], group: "dataflow",
            doc: "one row per (call, enclosing loop); depth is nesting rank (1=outermost); raw material for symbolic Big-O over call_edge", pk_never_null: true, ..Default::default() },
        // (param df_node id, positional index) — the index counts only typed
        // params (the Rust receiver `self` is skipped), so it aligns with
        // type_sig's `pos`. Lets a query bind a specific param node to its
        // declared type at node granularity, not just per-fn.
        // pk_never_null: `param_rows.push(vec![sym(id), i(*pos)])` — fixed
        // 2-element literal off a `(String, i64)` tuple iteration, never
        // Option.
        RelDecl { name: "df_param".into(), cols: vec![c("id", Type::Text), c("pos", Type::Int)], group: "dataflow",
            doc: "(param df_node id, positional index); index counts typed params only (self skipped) so it aligns with type_sig.pos for node-level type joins", pk_never_null: true, ..Default::default() },
        // (call/new node id, position, arg node id) — which argument slot a
        // value feeds. 0-based, method receivers at -1 (mirroring the skipped
        // `self` in df_param), so joining df_arg.pos = df_param.pos makes the
        // interprocedural arg -> param hop positional instead of blanket.
        // pk_never_null: `arg_rows.push(vec![sym(call), Value::Int(*pos),
        // sym(arg)])` — fixed 3-element literal off a `(String, i64, String)`
        // tuple iteration, never Option.
        RelDecl { name: "df_arg".into(), cols: vec![
            c("call", Type::Text), c("pos", Type::Int), c("arg", Type::Text)], group: "dataflow",
            doc: "(call/new df_node id, slot, arg df_node id); 0-based, receiver at -1; aligns with df_param.pos for the positional arg->param hop", pk_never_null: true, ..Default::default() },
        // rev-aware df_arg: both id columns (call, arg) are the SAME raw ids
        // df_node_rev.id uses (never salted) — a join against df_node_rev needs
        // an explicit `AND rev = rev` to stay scoped to one rev now that the id
        // alone no longer encodes it. legacy df_arg keeps raw ids, no rev
        // column. The full 4-column row (call, pos, arg, rev) is already the
        // natural dedup key (seen_arg_rev), so no explicit `key(...)` needed.
        // pk_never_null: same loop as df_arg, `arg_rev_rows.push(vec![
        // sym(call), Value::Int(*pos), sym(arg), t(rev)])` — fixed 4-element
        // literal, never Option.
        RelDecl { name: "df_arg_rev".into(), cols: vec![
            c("call", Type::Text), c("pos", Type::Int), c("arg", Type::Text), c("rev", Type::Rev)], group: "dataflow",
            doc: "rev-aware df_arg; call and arg are the SAME interned ids as df_node_rev.id (never rev-folded); legacy df_arg keeps raw ids", pk_never_null: true, ..Default::default() },
        // (new/call node id, field name, value node id) — named value flow
        // into a composite: Rust struct-literal fields (`..base` under the
        // pseudo-field ".."), TS object-literal properties (spread likewise),
        // Kotlin named arguments. Matching a df_field write against a member
        // read of the same name (df_node kind=member, var=name) gives
        // field-sensitive flow.
        // pk_never_null: `field_rows.push(vec![sym(id), t(field), sym(value)])`
        // — fixed 3-element literal off a `(String, String, String)` tuple
        // iteration, never Option.
        RelDecl { name: "df_field".into(), cols: vec![
            c("id", Type::Text), c("field", Type::Text), c("value", Type::Text)], group: "dataflow",
            doc: "(new/call df_node id, field name, value df_node id); struct-literal fields, object-literal properties, Kotlin named args; \"..\" for spread/functional-update bases", pk_never_null: true, ..Default::default() },
        // rev-aware df_field: both id columns (id, value) are the SAME raw ids
        // df_node_rev.id uses — value is always a value df_node id (never a
        // literal), so it matches df_node_rev.id the same way id does. legacy
        // df_field keeps raw ids, no rev column. The full 4-column row (id,
        // field, value, rev) is already the natural dedup key
        // (seen_field_rev), so no explicit `key(...)` needed.
        // pk_never_null: same loop as df_field, `field_rev_rows.push(vec![
        // sym(id), t(field), sym(value), t(rev)])` — fixed 4-element literal,
        // never Option.
        RelDecl { name: "df_field_rev".into(), cols: vec![
            c("id", Type::Text), c("field", Type::Text), c("value", Type::Text), c("rev", Type::Rev)], group: "dataflow",
            doc: "rev-aware df_field; id and value are the SAME interned ids as df_node_rev.id (never rev-folded); legacy df_field keeps raw ids", pk_never_null: true, ..Default::default() },
        // (df_node id, text, kind) — one row per STRING-carrying value node
        // (string-values arc item 1). `kind` is lit/template/concat: `lit` is
        // the cooked string literal value; `template`/`concat` carry the RAW
        // source slice (`${}` holes intact for a template, the written
        // operands for a `+` concat). TS/TSX/JS populate template/concat;
        // Rust populates lit only (Kotlin/Go/Python ledgered as follow-up).
        RelDecl { name: "df_lit".into(), cols: vec![
            c("id", Type::Text), Col::raw("text", Type::Text), Col::branded("kind", "const_value_kind")], group: "dataflow",
            doc: "(df_node id, text, kind); lit=cooked string literal, template/concat=raw source slice with holes intact; TS/TSX/JS + Rust lit today", ..Default::default() },
        // rev-aware df_lit: id is the SAME raw id df_node_rev.id uses (never
        // rev-folded) — same shape as df_field_rev (D5 pattern). The dedup key
        // (seen_lit_rev) is only (id, rev); text/kind are functionally
        // determined by (id, rev) (deterministic parse), so the wider full-row
        // default PRIMARY KEY is still exactly as unique, no explicit
        // `key(...)` needed.
        RelDecl { name: "df_lit_rev".into(), cols: vec![
            c("id", Type::Text), Col::raw("text", Type::Text), Col::branded("kind", "const_value_kind"), c("rev", Type::Rev)], group: "dataflow",
            doc: "rev-aware df_lit; id is the SAME interned id as df_node_rev.id (never rev-folded); legacy df_lit keeps the raw id", ..Default::default() },
    ]
}

/// String values folded from `const`/`as const` bindings (string-values arc,
/// item 3). Rides the same `TypeFacts` parse `doc_comment` does, so it lives
/// beside `doc_text_rel_decls` rather than inside `type_rel_decls` (same
/// "own function, shared group" shape doc_comment already established).
pub(crate) fn const_value_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "const_value".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("field", Type::Text), Col::raw("text", Type::Text),
            Col::branded("kind", "const_value_kind"), c("file", Type::Path), c("line", Type::Int)], group: "type",
            doc: "string value folded from a const (or as const) binding; sym is the owning type_entity (the const itself, or the enum for a string member), field is \"\" for a bare const or a dotted key path (\"home\", \"nested.a\") for an object literal; a let/var string initializer is never emitted (soundness rule); line: 1-based", ..Default::default() },
        RelDecl { name: "const_value_rev".into(), cols: vec![
            c("repo", Type::Text), c("sym", Type::Text), c("field", Type::Text), Col::raw("text", Type::Text),
            Col::branded("kind", "const_value_kind"), c("file", Type::Path), c("line", Type::Int), c("rev", Type::Rev)], group: "type",
            doc: "rev-aware const_value (rev is a plain trailing column, like type_entity_rev — sym never collides across revs); legacy const_value is the rev-deduped union; line: 1-based", ..Default::default() },
    ]
}

pub(crate) fn doc_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        // one row per document structural node (heading / code block / section).
        // `parent` is the enclosing heading text ("" at top level), so a rule can
        // walk the section tree. See `ingest::IngestLang`.
        RelDecl { name: "doc_node".into(), cols: vec![
            c("repo", Type::Text), c("file", Type::Path), c("line", Type::Int),
            c("kind", Type::Text), Col::raw("name", Type::Text), Col::raw("parent", Type::Text)], group: "doc",
            doc: "structural nodes from non-source text (markdown headings + code blocks via tree-sitter-md: ATX/setext headings, fenced/indented blocks); parent is the enclosing heading; line: 1-based", ..Default::default() },
        // doc→code bridge: (file, line, sym, kind, matched_name). For each row,
        // `kind` is the doc_node kind that produced it ("heading" or
        // "code_block") and `matched_name` is the doc-side string that matched a
        // type_entity name (the heading text for `heading`, the matching
        // identifier token for `code_block`). Heading names are normalized
        // before the join (articles + trailing kind words stripped) so "The
        // Engine struct" bridges to `Engine`. Empty unless the program also uses
        // type relations.
        RelDecl { name: "doc_ref".into(), cols: vec![
            c("repo", Type::Text), c("file", Type::Path), c("line", Type::Int), c("sym", Type::Text),
            c("kind", Type::Text), Col::raw("matched_name", Type::Text)], group: "doc",
            doc: "doc-to-code bridge: name-matches doc_node headings to type_entity symbols (exact + normalized) and scans code blocks for identifier mentions; empty unless the program also uses type relations; line: 1-based", ..Default::default() },
    ]
}

pub(crate) fn spine_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl { name: "string".into(), cols: vec![c("id", Type::Int), c("text", Type::Text), c("norm", Type::Text)], group: "spine",
            doc: "interned strings (ref spine): id (StringId::sqlite(), an INTEGER — BREAKING as of the intern-key arc, was decimal TEXT), text, normalized text", ..Default::default() },
        RelDecl { name: "ref".into(), cols: vec![
            c("id", Type::Text), c("string", Type::Int), c("file", Type::Text), c("lo", Type::Int), c("hi", Type::Int)], group: "spine",
            doc: "byte span per interned string; id is the rewrite coordinate — 'where does Foo occur' is string(s, Foo, _), ref(_, s, f, lo, hi)", ..Default::default() },
    ]
}

pub(crate) fn node_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl {
            name: "node".into(),
            cols: vec![
                c("id", Type::Text),
                c("kind", Type::Text),
                c("file", Type::Text),
                c("lo", Type::Int),
                c("hi", Type::Int),
                c("parent", Type::Text),
            ],
            group: "node",
            doc: "CST nodes (nested-set spans): id, kind, file, lo, hi, parent",
            ..Default::default()
        },
        // EXACTLY 2 cols: `declare_closure` requires it, so `anc(a,b) <-
        // closure(child).` works with zero new recursion code.
        RelDecl {
            name: "child".into(),
            cols: vec![c("parent", Type::Text), c("child", Type::Text)],
            group: "node",
            doc: "CST parent-child edges (exactly 2 cols, so closure(child) gives ancestry)",
            ..Default::default()
        },
    ]
}

pub(crate) fn daemon_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![
        RelDecl {
            name: "program".into(),
            cols: vec![
                c("path", Type::Path),
                c("hash", Type::Text),
                c("mtime", Type::Int),
            ],
            group: "daemon",
            doc: "dl programs the daemon tracks (path, content hash, mtime)",
            ..Default::default()
        },
        RelDecl {
            name: "head".into(),
            cols: vec![
                c("repo", Type::Text),
                c("name", Type::Text),
                c("oid", Type::Text),
            ],
            group: "daemon",
            doc: "git HEAD per repo (repo, ref name, oid)",
            ..Default::default()
        },
        RelDecl {
            name: "rev_advanced".into(),
            cols: vec![
                c("repo", Type::Text),
                c("name", Type::Text),
                c("old", Type::Text),
                c("new", Type::Text),
            ],
            group: "daemon",
            doc: "daemon signal that a repo ref advanced (repo, name, old oid, new oid)",
            ..Default::default()
        },
    ]
}

pub(crate) fn every_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![RelDecl { name: "every".into(), cols: vec![c("secs", Type::Int)], group: "clock",
        doc: "holds interval N only on ticks that cross an N-second boundary (and the first tick); an every(30) body atom self-throttles its rule", ..Default::default() }]
}

/// The distinct `every(N)` interval literals used as body atoms in the program.
/// A non-literal arg (`every(N)` with `N` a variable) is ignored: the clock fires
/// per concrete interval the program names. Each becomes a row candidate in
/// `refresh_every`.
pub(crate) fn every_intervals(prog: &Program) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    let mut atoms = |body: &[BodyItem]| {
        for b in body {
            if let BodyItem::Pos(a) = b {
                if a.rel == "every" {
                    if let [Term::Int(n)] = a.terms.as_slice() {
                        if *n > 0 && !out.contains(n) {
                            out.push(*n);
                        }
                    }
                }
            }
        }
    };
    for item in &prog.items {
        match item {
            Item::Rule(r) => atoms(&r.body),
            Item::Gen(g) => atoms(&g.body),
            _ => {}
        }
    }
    out
}

pub(crate) fn every_rels_used(prog: &Program) -> bool {
    rels_used(prog, &EVERY_RELS)
}

pub(crate) fn clock_rel_decls() -> Vec<RelDecl> {
    let c = |n: &str, t: Type| Col::plain(n.to_string(), t);
    vec![RelDecl { name: "clock".into(), cols: vec![c("secs", Type::Int), c("bucket", Type::Int)], group: "clock",
        doc: "the current time bucket now/secs per named period, present EVERY tick (not edge-triggered like every); clock(300,b) binds b to a monotone int advancing once per 300s — join it to vary a digest or gate on cadence, no @next counter; in an @async/@stream body, clock(300, _) (wildcard or unused-var bucket) salts the REQUEST digest with the current bucket while binding nothing — the request re-fires once per bucket and no row ever stores the bucket, so a flip invalidates zero derived rows", ..Default::default() }]
}

/// The distinct `secs` periods the program names in a `clock(secs, bucket)` body
/// atom (first arg an int literal). Mirrors `every_intervals`; each becomes one
/// row in `refresh_clock`.
pub(crate) fn clock_periods(prog: &Program) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    let mut atoms = |body: &[BodyItem]| {
        for b in body {
            if let BodyItem::Pos(a) = b {
                if a.rel == "clock" {
                    if let [Term::Int(n), _] = a.terms.as_slice() {
                        if *n > 0 && !out.contains(n) {
                            out.push(*n);
                        }
                    }
                }
            }
        }
    };
    for item in &prog.items {
        match item {
            Item::Rule(r) => atoms(&r.body),
            Item::Gen(g) => atoms(&g.body),
            _ => {}
        }
    }
    out
}

pub(crate) fn clock_rels_used(prog: &Program) -> bool {
    rels_used(prog, &CLOCK_RELS)
}

/// Does the program reference any relation in `rels` (body atom, closure edge,
/// or query head)? Gates lazy built-in indexers so unrelated programs pay nothing.
pub(crate) fn rels_used(prog: &Program, rels: &[&str]) -> bool {
    let hit = |r: &str| rels.contains(&r);
    for item in &prog.items {
        match item {
            Item::Rule(r) => {
                for b in &r.body {
                    match b {
                        BodyItem::Pos(a) | BodyItem::Neg(a) => {
                            if hit(&a.rel) {
                                return true;
                            }
                        }
                        BodyItem::Closure { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        BodyItem::Scc { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        BodyItem::Node2vec { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Item::Query(q) => {
                if hit(&q.head.rel) {
                    return true;
                }
            }
            Item::Gen(g) => {
                for b in &g.body {
                    match b {
                        BodyItem::Pos(a) | BodyItem::Neg(a) => {
                            if hit(&a.rel) {
                                return true;
                            }
                        }
                        BodyItem::Closure { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        BodyItem::Scc { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        BodyItem::Node2vec { rel } => {
                            if hit(rel) {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Shapes are expanded to plain RelDecls at load, so none reach here.
            Item::Rel(_) | Item::Anchor(_) | Item::Brand(_) | Item::Shape(_) | Item::Shell(_) => {}
        }
    }
    false
}

/// Classify a call site's bare callee name as `read` or `write`. Heuristic by
/// method name only (no receiver type); rail-side joins (`conn_fn` for the
/// conn() ratchet) narrow to db-shaped sites, so a `HashMap::insert` false
/// positive in an unrelated fn never reaches a diag because that fn has no
/// Db::conn site. The table is deliberately rusqlite-shaped: `insert`/
/// `update`/`delete`/`replace`/`commit` are dropped because they collide with
/// collection methods (`HashMap::insert`) and would pollute the table for
/// little gain — the rail's `conn_fn` join already gates on the conn method,
/// so the only writes that matter are the ones chained off a `Db::conn` or
/// `Db::` call, which in rusqlite are `execute`/`execute_batch`/`execute_returning`.
/// `None` for anything not clearly a db read or write.
pub(crate) fn classify_call_kind(callee: &str) -> Option<&'static str> {
    Some(match callee {
        "execute" | "execute_batch" | "execute_returning" => "write",
        "prepare" | "prepare_cached" | "query_row" | "query_map" | "query_and_then"
        | "query_named" => "read",
        _ => return None,
    })
}

pub(crate) fn module_rels_used(prog: &Program) -> bool {
    rels_used(prog, &MODULE_RELS)
}

/// Whether the module family must run THIS tick: either the program
/// directly references a module_* relation, or it references type_link/
/// call_edge (or their dependent analyses type_shape/type_lgg, or the
/// doc_comment/doc_tag pair riding the same parse). Win D's import-scoped
/// ambiguity narrowing in `refresh_type_rels`/`refresh_call_rels` reads
/// `module_edge_rev`, so those families need a FRESH module graph even when
/// the program never asks for a module_* relation itself. Without this, a
/// program that only queries `type_link`/`call_edge` would silently never
/// populate `module_edge_rev`, and every ambiguous name would stay bare
/// forever (the narrowing looks like a no-op, not an error). Used by both
/// `ModuleFamily::used` (the full tick's per-family loop) and `tick_paths`'
/// `wants_module_rels` (the incremental path, which reads this directly
/// rather than through the trait).
pub(crate) fn module_rels_needed(prog: &Program) -> bool {
    module_rels_used(prog)
        || type_rels_used(prog)
        || rels_used(prog, &["type_shape", "type_lgg"])
        || doc_text_rels_used(prog)
        || const_value_rels_used(prog)
        || call_rels_used(prog)
}

pub(crate) fn type_rels_used(prog: &Program) -> bool {
    rels_used(prog, &TYPE_RELS)
}

pub(crate) fn doc_text_rels_used(prog: &Program) -> bool {
    rels_used(prog, &DOC_TEXT_RELS)
}

pub(crate) fn const_value_rels_used(prog: &Program) -> bool {
    rels_used(prog, &CONST_VALUE_RELS)
}
pub(crate) fn comment_rels_used(prog: &Program) -> bool {
    rels_used(prog, &COMMENT_RELS)
}

pub(crate) fn template_rels_used(prog: &Program) -> bool {
    rels_used(prog, &TEMPLATE_RELS)
}
pub(crate) fn unresolved_rels_used(prog: &Program) -> bool {
    rels_used(prog, &UNRESOLVED_RELS)
}

pub(crate) fn call_rels_used(prog: &Program) -> bool {
    rels_used(prog, &CALL_RELS)
}

pub(crate) fn dataflow_rels_used(prog: &Program) -> bool {
    rels_used(prog, &DATAFLOW_RELS)
}

pub(crate) fn doc_rels_used(prog: &Program) -> bool {
    rels_used(prog, &DOC_RELS)
}

pub(crate) fn daemon_rels_used(prog: &Program) -> bool {
    rels_used(prog, &DAEMON_RELS)
}

pub(crate) fn spine_rels_used(prog: &Program) -> bool {
    rels_used(prog, &SPINE_RELS)
}

pub(crate) fn node_rels_used(prog: &Program) -> bool {
    rels_used(prog, &NODE_RELS)
}
