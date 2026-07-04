# Agent sharp edges (field report, 2026-07-04)

Source: an agent authoring dl rules cold against an external TSX repo (the
`instant` smoke test), asked afterward "what cost you tokens and what one doc
line would have prevented it". Logged verbatim-in-substance; the fix column
tracks what we shipped in response. Calibration from the same report: the
self-documenting catalogs, `dl examples --show`, `--no-daemon`, discovery mode,
and named-field atoms all reduced friction. These are the places it burned
tool calls or made a wrong first move.

## Sharp edges (symptom, root cause, the missing doc line)

| # | symptom | root cause | preventive line |
|---|---|---|---|
| 1 | `$$$body`/`$$$deps` matched 0 rows, no error | sg metavars must be UPPERCASE (`sg.rs` capture regex `\$+([A-Z_]...)`); lowercase is literal text | "Captures are UPPERCASE (`$X`, `$$$ARGS`); lowercase is literal text." |
| 2 | `head var BODY is not bound by any source op` | only single captures (`$X`) and span outputs bind head vars; `$$$X` is structural-only | "`$$$X` is pattern structure only; bind via `$X` or a span output." |
| 3 | `unbound var l in constraint on l = m+1` | arithmetic is head/comparison only, never a body binding | "Arithmetic runs in heads (`rank(path, line+1)`) and comparison sides only, never as a body binding." |
| 4 | regex `(?!-)` parse error | Rust regex crate: no look-around, no backrefs | "Regexes are Rust-flavor: no lookahead/lookbehind/backrefs; anchor with `$`, `\b`, char classes." |
| 5 | inline `// dl-disable-line` silently missed | RESOLVED: the built-in `comment_node` rel sees EVERY comment (line/block/doc, incl. inline trailing) grammar-backed; the regex `comment` op keeps its whole-line-by-prefix limit for no-grammar files | "`comment_node` sees all comments incl. inline (`std/suppress.dl` rides it); the regex `comment` op is whole-line-only, for files with no grammar." |
| 6 | `dl-disable` open-regex substring-matched `dl-disable-next-line` | flat alternations collide when one marker prefixes another | "Anchor disjunct markers." |
| 7 | scan arity guessing | `scan([repo,][rev,] glob, path[, rev_out])` has two optional leading args | show the with-repo and without-repo forms as two copy-paste lines |
| 8 | picked `match` for a structural pattern, then `sg`/`ast` blindly | no op-selection decision tree | "`sg`/`ast` when a grammar exists; `match` for substrings/no grammar." |
| 9 | `ast_yaml(:tsx)` assumed available, bailed | `sg` has tsx (ast-grep grammars) but `ast`/`ast_yaml` language set differs | per-op language matrix |

Costliest: #1 (silent zero-match, 3 round-trips) and #8/#9 (the
sg-has-tsx-but-ast-does-not gap is invisible until the bail).

## Missing for one-shot authoring (the report's ranking)

1. **Per-op language coverage matrix** in the skill quickref and the CLI
   discovery surface. Highest leverage; today the only signal is the bail
   message after the wrong pick.
2. **A ~20-line survival block** of imperatives (edges 1-8 above) at the TOP
   of the skill, ahead of the op list. The op-quickref says what each op is;
   the constraints that bite are buried or absent.
3. **A no-scan validate** (parse + typecheck + op resolution + metavar sanity,
   sub-second) so the authoring loop does not pay a full scan to find a parse
   error.
4. **Errors that name the fix.** "head var not bound" and "unbound var in
   constraint" are accurate and point at the wrong variable; each should end
   with the escape hatch ("to compute, put the expression in the head: ...").
5. **--help says nothing about authoring rules.** One trailing pointer from
   --help to the authoring surface closes the gap.

## Disposition

- Logged 2026-07-04. Fix arc landed same day:
  - **Survival block + language matrix** — `assets/sprefa-dl.skill.md` gains a
    "## Before you write a rule (the constraints that bite)" section (edges 1-8 +
    one-rel-one-rule-kind + closure-unpinned-read) and a per-op LANGUAGE MATRIX.
    The matrix is kept honest by `tests/it/lang_matrix.rs`, which parses the
    `sg, ast_yaml:` / `ast:` lines out of the embedded skill and asserts
    set-equality against the real grammar tables (`SG_LANG_TABLE` /
    `AST_LANG_TABLE`, exposed as `sg::sg_langs()` / `engine::ast_langs()`). A
    stale matrix fails CI. As shipped: `sg`/`ast_yaml` = rust, typescript, tsx,
    javascript, python, go, json, c, cpp, kotlin (ast-grep); `ast` = rust, c,
    kotlin, python, bash, go, hcl, starlark, jsonnet, gotmpl, dockerfile
    (tree-sitter). Note the field report's edge #9 was half-right: `ast_yaml`
    shares `sg`'s table (it HAS `tsx`); only the `ast` op lacks `tsx`.
  - **CLI pointers** — `dl --help` gains an AUTHORING RULES trailer
    (`setup --print`, `docs authoring`, `--parse-only`); `dl docs authoring`
    prints the skill body (re-exported `setup::SKILL_MD`, not re-embedded).
  - **No-scan validate** — `--parse-only`: parse + typecheck + op resolution +
    metavar sanity over the program file(s), no scan, no db writes, sub-second.
    Exit 0 clean, 1 on any error; diagnostics to stderr in the `--check` style.
    Reuses `prepare_paths` (same acceptance as a real run). `run_parse_only` in
    `src/lib.rs`; `tests/it/parse_only.rs`.
  - **Errors that name the fix** — the "head var not bound" error appends the
    `$$$NAME` structural-metavar note when the source op is `sg`/`ast_yaml` with
    a matching `$$$` (`src/engine/mod.rs`); the "unbound var in constraint" error
    appends the put-the-expression-in-the-head note. `tests/it/authoring_edges.rs`.
  - **Lowercase-metavar lint** — a warn-severity typecheck lint, code
    `lowercase-metavar`: an `sg`/`ast_yaml` pattern with `$name` (lowercase lead)
    warns "lowercase $name is literal text, not a capture; metavars are UPPERCASE
    ($NAME)". Warn, not error (literal `$` is legal); surfaced through the normal
    diag path so `--check`/`--lsp`/`--parse-only` all show it
    (`typecheck::metavar_case_diags`).
  - **Parse-only compiles regex literals + regex errors name the escape**
    (edge #4 follow-on, 2026-07-04). `--parse-only` now walks the prepared
    program and compiles every regex literal (`match`, `comment` open/close, `=~`
    body constraints) through one shared `engine::compile_dl_regex`, the SAME
    construction the scan/eval path uses, so an unsupported pattern (`/(?!-)/`)
    fails at parse-only (exit 1, all bad regexes reported) instead of surviving
    to a real scan (residual 1). Every regex compile error — parse-only AND the
    runtime `match`/`comment`/`=~` eval path — appends `engine::DL_REGEX_NOTE`
    ("note: regexes are Rust-flavor: no lookahead/lookbehind/backrefs; anchor
    with $, \b, or character classes.") after the raw regex-crate caret message
    (residual 3). `engine::regex_literal_diags` in `src/engine/mod.rs`; tests in
    `tests/it/parse_only.rs`. NOT built: residual 2 (pre-scan binding analysis so
    unbound-constraint errors fire without a scan) needs scope analysis; the
    improved "put the expression in the rule head" message still fires only on a
    real run.

Keep (per the same report): catalog-driven self-docs, `dl examples --show`,
`--no-daemon`, named-field atoms, discovery mode, `${var}` interpolation in
diag messages.
