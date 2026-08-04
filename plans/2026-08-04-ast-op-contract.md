# ast() in dl6 — the two-lane contract (user directive 2026-08-04)

User directive: reverse spine_residency's surface consequence. dl6 gets
`ast()` with OR-of-syntax and regex filtering in one query. The executor
stays the bought one (sprefa-extract; tree-sitter + ast-grep crates already
linked). Two lanes, disjoint ownership, this contract is the seam.

## The wire contract (BOTH lanes build to this, neither may change it)

```
extract query --lang <rust|ts|tsx|js|go|kotlin> --query '<tree-sitter query>' <path>
```

- The query is FULL tree-sitter query syntax: multiple top-level patterns,
  `[ ... ]` alternation, and predicates `#match?` / `#not-match?` (Rust
  regex crate semantics, same flavor as the landed regexp/2) and `#eq?`.
- stdout: one flat JSONL object per match: every NAMED capture as a
  top-level key (capture text), plus `"line"` and `"end_line"` (1-based,
  of the whole match). No nested objects (the decode_arc refusal stands).
- Unknown lang or invalid query: exit 2, one-line reason on stderr.

## The dl6 surface (lane B)

```
def(function_name, line) <-
  file(path, digest),
  ast(path, digest, 'rust',
      "[ (function_item name: (identifier) @function_name)
         (macro_definition name: (identifier) @function_name) ]
       (#match? @function_name \"^handle_\")").
```

- v5's binding law verbatim (README.md:313): `@cap` captures bind
  SAME-NAMED variables in the rule; `line` / `end_line` bind when used.
- Desugar, never kernel: a shared expansion (0_coalesce_expand.pl is the
  module-placement precedent) mints one `sh __ast_q<n>` host per distinct
  (lang, query) with columns = captures + line + end_line, command =
  `: {digest}; "$DL_EXTRACT_BIN" query --lang <lang> --query '<query>' {path}`,
  and rewrites the ast atom to that host atom. Both doors share it.
- Refusals, named, load-time, both doors: `ast_query_not_literal`,
  `ast_lang_unknown`, `ast_query_single_quote` (v1: a `'` inside the query
  refuses rather than escaping), `ast_no_named_capture`.
- OR beyond one query needs no construct: multiple rules on one head union
  (shipped), and `regexp/2` post-filters host output (landed today).

## Receipts already verified by the coordinator
- v5 tree-sitter query op lives at src/ast.rs (graft source, same repo).
- extract CLI is clap, streams flat JSONL (src/bin/extract.rs:1-3), families
  in src/family.rs; grammars: ast-grep-language 0.38 + tree-sitter 0.25 +
  go/kotlin (Cargo.toml:18-19,45-47).
- regexp/2 landed on main (merge before 7462b380): follow its refusal
  pattern in 0_program_check.pl.

## Gates
Lane A: `cargo build --offline --release --features cli --bin extract` +
`cargo test --offline` in v6/sprefa-extract; JSONL receipts for a plain
query, an alternation, and a #match? on real sample files pasted in
REPORT.md. Lane B: `cd v6 && just conformance && just text-door &&
just plunit` (expansion-level fixtures + refusal tests; the LIVE handshake
golden is a third arc after both land). Style laws as posted in each lane's
BRIEF preamble; NO-COMMIT flow; REPORT.md the deliverable.
