# Syntax & semantics

Source ops (extract facts from files), body constructs (derived rules), and sinks. The `kind` column groups them; `example` shows one program that uses the op (`dl examples --show <name>` prints it from the binary). Generated from the engine's `op_catalog` by examples/gen-reference.dl. Do not hand-edit.

| op | kind | syntax | semantics | example |
|---|---|---|---|---|
| `ast` | source | `ast(path, rev, :lang, "(query) @cap", line[, end])` | tree-sitter query; @cap captures bind same-named vars; :lang ∈ rust/c/kotlin/... | `dl examples --show callgraph-ast.dl` |
| `ast_yaml` | source | `ast_yaml(path, rev, :lang, "rule yaml", line, ...)` | ast-grep RuleCore YAML body (inside:/has: relational rule) instead of a pattern string; span outputs share the sg form | `dl examples --show lint-unwrap.dl` |
| `closure` | body | `closure(edge)` | transitive closure of a 2-col relation as the entire body (SCC-condensed); pin an endpoint for a point query; mixed-body closure is literal-seeded only | `dl examples --show callgraph.dl` |
| `comment` | source | `comment(path, rev, /open/[, /close/], l0, l1, label)` | comment-marker regions in any file type; one regex = sequential dividers, two = paired BEGIN/END with LIFO nesting; l0/l1 are 1-based marker lines; pairs with gen splice | `dl examples --show gen-type-table.dl` |
| `diag` | sink | `rel diag(path, line, col, ..., severity, msg).` | declare a rel named diag; the engine maps columns BY NAME into editor diagnostics (--lsp) or check output (--check); required path/line/msg | `dl examples --show agent-live.dl` |
| `gen` | sink | `gen([:mode,] path, [l0, l1,] "{var} template")` | codegen; file form renders body rows through a path+row template, splice form replaces lines between comment marker pairs; convergent (skips write when bytes match); never runs under --check/--lsp | `dl examples --show anim-deck.dl` |
| `json` | source | `json(path, rev, q:{ $k: $v })` | declarative brace pattern over json/yaml/toml; each match binds named key AND value captures as dl vars; supports **: recursion, [...$x] spread, re:/glob keys | `dl examples --show gh-cache-batch.dl` |
| `jsonp` | source | `jsonp(path, rev, "a.*.b", out)` | dotted path over json/yaml/toml (* = any key/element); the value is located; the string form of json | `dl examples --show gh-cache-config.dl` |
| `match` | source | `match(path, rev, /re/, line[, id][, col, end_col])` | regex over file content, one row per match line; (?<cap>..) named groups bind dl vars; $cap is sugar for a lazy named group; trailing id/col bind the whole-match span | `dl examples --show anim-deck.dl` |
| `node2vec` | body | `head(a, b, score) <- node2vec(edge)` | structural graph embedding of a 2-col relation as the entire body; binds node pairs with a similarity score (the graph-position sibling of the text `similar` rel); evaluated outside SQL | `dl examples --show node2vec-callgraph.dl` |
| `scan` | source | `scan([repo,][rev,] glob, path[, rev_out])` | select files; 2-ary omits rev_out, 5-ary names a repo coordinate; outputs path/rev_out take the _ or name: form (rev_out _ or omitted = rev not bound); repo defaults ".", rev "WORK" (WORK/HEAD/any git rev) | `dl examples --show anim-deck.dl` |
| `scc` | body | `head(rep, member) <- scc(edge)` | strongly-connected-component condensation of a 2-col relation as the entire body; binds (representative, member) per node; mirrors closure, evaluated outside SQL | `dl examples --show context-object.dl` |
| `sg` | source | `sg(path, rev, :lang, "$X.unwrap()", line[, col, end_line, end_col][, id])` | ast-grep pattern; metavar $X binds dl var X (matched text); trailing id binds the whole-match span for structural rewrite via gen(:replace) | `dl examples --show ban.dl` |
| `aggregation` | body | `count sum min max` | head-position-only aggregation; non-aggregate head terms are the grouping key; count/sum produce int, min/max carry the arg type; count in body is a parse error | |
| `arith` | body | `+ - * / %` | int arithmetic in rule heads and comparison sides (rank(p, line+1)); usual precedence, parens OK; never in a binding atom | |
| `atom` | body | `edge(f, t)` | positive atom; binds its vars from the named relation | |
| `cmd` | source | `cmd(path, rev, "tool {file}", line, out)` | shell out per matched file, one row per stdout line; cached by (file hash, rule text); nonzero exit + stdout = findings, nonzero + empty = error | |
| `comparison` | body | `= != < <= > >=` | scalar comparison on bound vars or literals (n >= 4, p != fs:src/db.rs) | |
| `glob` | body | `p ~~ "src/*"` | glob constraint (SQLite GLOB) | |
| `negation` | body | `!round(t, _)` | negation / anti-join; the row must NOT exist in the relation | |
| `query` | sink | `? rel(a, b).` | print a TSV block (or JSON-lines with --query-json); a literal in any position filters; no where clause | |
| `regex` | body | `f =~ /^[A-Za-z]+$/` | regex constraint (SQLite REGEXP); the /.../ unified regex literal, same form match/comment/sg use | |
| `strfn` | body | `split(text, sep, idx) / replace(text, from, to)` | string functions in heads and comparison sides; idx 0-based, negative counts from the end; a computed binding (ext = split(p, ".", -1)) binds for later use in the same body | |
