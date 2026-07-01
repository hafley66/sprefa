# Typed path literals + agg + LSP routing — implementation contract

Design session: chat_log/20260611.0.sprefa-v5-typed-paths-lang-plan.md. This file is the
binding spec for tasks T1-T6. Implementing agents: read this whole file, then
~/projects/sprefa/CLAUDE.md (style rules, N+1 rule), then the source files your task names.

## The model

1. Typed literal = `scheme:body`. Two body forms:
   - bare: starts immediately after `:` (no space). Ends at whitespace, `,`, or `)` at
     nesting depth 0. Internal `(` `)` `{` `}` `[` `]` tracked balanced. `\` escapes the
     next char. `${NAME}` is an interpolation hole (same as existing string interp).
   - fenced: `scheme:` + backtick + body + backtick. Only `` ` `` and `${` are special
     inside (v4 dsl_body rule). Use when the body contains a bare-form terminator.
2. Schemes v1: `fs:` (concrete path), `glob:` (path pattern). Parse-level registry is a
   static table so adding `rs:`/`dot:`/`url:` later is a row, not a parser. Unknown
   scheme = parse error with span. NOTE: `/re/` regex literals already exist; do NOT add
   `re:` now and do NOT disturb `/.../`.
3. Anchors: `anchor <name> = <fs-literal>.` at top level. `<name>` = `~` or ident.
   Literal bodies may start `~/`, `<name>/` resolves... NO — anchors are referenced as
   `fs:~/x` (tilde) only in v1; named anchors are accepted by the grammar but `fs:` body
   anchor syntax is `~` only for now (named anchor refs deferred with `rs:`).
   Default `~` = scan root. Resolution at lower time, parse-site semantics (Nix rule).
4. Brands: `type <ident> <: <parent>.` parent in {text, int, path, file, dir} or a prior
   brand. Brand lives in the relation schema metadata; runtime storage stays text.
5. Descriptors (the canonical value):
   - `Desc { text: String, kind: DescKind }`
   - `DescKind { Namespace, Type_, Term, Method, Param, Index(u64), Dir, File, Hole(HoleKind) }`
   - `HoleKind { Named(String), Star, Globstar }`
   - `fs:`/`glob:` parse to Vec<Desc> with Dir/File/Hole kinds. Canonical rendered text
     for fs = normalized repo-relative path (the existing `_file.path` domain). A path
     escaping the scan root = TypeDiag error. Zero holes = value; any hole = pattern.
6. Lowering:
   - concrete `fs:` literal in any term position lowers to its canonical text (plain SQL
     text equality — joins against existing path columns work unchanged).
   - `glob:` literal lowers exactly where quoted glob strings lower today (scan arg, `~~`
     constraint). scan("WORK", glob:src/**/*.rs, p, rev) must behave byte-identically to
     scan("WORK", "src/**/*.rs", p, rev).
7. Type checking (lower time): `check_rule_types(rule, rels) -> Vec<TypeDiag>`.
   - unify each var's type across body atoms via declared rel cols.
   - brand A vs brand B (A != B, neither ancestor of other) = ERROR brand-mismatch.
   - plain text column meeting a path/branded var = WARN coerce (grandfather everything).
   - PathLit in an int column = ERROR. Literal failing existence kind (file vs dir) is
     NOT checked at lower time (that stays extraction's job).
   - `TypeDiag { path, span: (u32,u32), severity, code, msg }`. Codes: `brand-mismatch`,
     `path-escapes-root`, `unknown-anchor`, `unknown-scheme`, `coerce-text-path`.
8. Aggregation (T4): head-position only. `fan_out(F, count(T)) <- type_edge(F, T, _).`
   - `HeadTerm::Agg(AggFn, Term)`, AggFn in {Count, Sum, Min, Max}.
   - lowers to SELECT <plain head vars>, AGG(<arg>) ... GROUP BY <plain head vars>.
   - Count/Sum output Int; Min/Max output the arg's type.
   - stratification: build the rel dependency graph (the one rebuild_derived prunes
     with); an agg or negation edge inside an SCC = TypeDiag error `not-stratified`.
9. Spine lever (T5): `refresh_spine_rels_delta(&mut self, delta: Option<&SpineDelta>)`,
   `SpineDelta { strings_added: Vec<StringId>, spans_added: Vec<WhereBytes>,
   retracted_paths: Vec<(RepoId, String)> }`. None = current wholesale body, verbatim.
   Wire None at every existing call site. Comment block on the fn: this is the
   incremental-load lever; staged per-tick vecs in insert_spine_where_bytes are the
   future Some() source; collect-then-flush (v4 saga shape), never per-row.

## Hard style rules (from repo CLAUDE.md, repeated because they gate review)
- Never a per-row write loop; collect, then one Db::insert_rows. The tick counter screams.
- Banned identifiers AND prose: provenance, substrate, load-bearing, regime.
- No em dashes in comments/docs.
- Match existing file style (engine.rs patterns, error handling idioms).

## Tasks and gates

T1 (lex.rs, parse.rs, ast.rs): scheme-literal token (bare + fenced), `anchor` stmt,
  `type X <: Y` stmt, Term::PathLit { scheme, body, span }, AST nodes for AnchorDecl,
  BrandDecl. Existing programs must parse unchanged.
  GATE: new parse-error fixtures under --check; full suite green.

T2 (new src/desc.rs, lower.rs, engine.rs as needed): SchemeSpec table + one generic
  segment lexer; resolve fs/glob literal -> Vec<Desc> -> canonical text or pattern;
  anchor resolution; check_rule_types; TypeDiags rendered through the --check path
  (compiler-style, like diag rows render today) and non-zero exit on error severity.
  GATE: tests/path_types.rs covering: fs literal joins against scanned path; ~ anchor;
  path-escapes-root error; brand mismatch error; coerce warn compiles; glob: in scan
  byte-identical to quoted form. Every file in examples/ still runs green.

T3 (engine.rs, lsp.rs): TypeDiags AND extraction type-drops become diag-shaped rows
  published over LSP (same path as the diag relation; severity from TypeDiag). The
  [checked-type] stderr counter stays but each dropped row also lands a diag at the
  extraction site when a span is known, else file-level line 1.
  GATE: tests/lsp_protocol.rs - port /tmp/lsp_smoke.py to Rust (spawn dl --lsp, LSP
  framing over stdio, assert publishDiagnostics for a brand-violating fixture program
  AND for examples/lint-imports.dl baseline).

T4 (ast.rs, parse.rs, lower.rs): HeadTerm::Agg, GROUP BY lowering, stratify().
  GATE: tests/agg.rs - fan_out over v5's own type_edge returns Tok=21, BodyItem=9,
  Engine=9 (counts verified 2026-06-06); not-stratified fixture errors; agg+negation
  combo errors when cyclic.

T5 (engine.rs): the spine lever seam, None-wired, zero behavior change.
  GATE: full suite green, no new test needed beyond compile + existing spine tests.

T6 (lsp.rs): definition/references handlers over ref spine + seeded closures. Deferred
  until T3 lands the protocol test harness.

## Deferred (do not build)
rs:/dot:/url: schemes, xlate, at(), access-path extraction, DslBodyLsp port, named-anchor
refs in bodies, segment materialization table, tick transaction.
