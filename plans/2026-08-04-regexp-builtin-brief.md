# Lane brief: `regexp/2` builtin, PCRE grafted, no more host shims for string tests

User ruling (2026-08-04): stop shimming string conditions through host
subprocesses; the language gets a regex predicate. Graft, never build: the
matchers are SQLite-function-registered JS RegExp on the runtime side and SWI
`library(pcre)` on the oracle side, both already installed.

## Ground (verified by the coordinator on 6c3e928c before this brief)

| fact | receipt |
|---|---|
| runtime driver | `better-sqlite3 ^13` (v6/sprefa-store/js/package.json:15), has `db.function()` |
| oracle pcre | `swipl -g "use_module(library(pcre)), re_match('^a.c$'/i, \"AbC\")"` prints ok |
| dl6 expression surface today | arithmetic, comparison, `norm/1`; no string ops |
| scalar-function precedent | `norm/1`: follow it through parse, type plane, lower, emit, oracle |
| named-refusal precedent | ARCH row aggregate_text_refusal: shared load-time check in 0_program_check.pl so BOTH doors throw the same term |

## Design (pinned; do not redesign)

- Surface: `regexp(text_operand, "pattern")` as a positive body condition.
  REGEXP is the SQL vocabulary word. Same body positions norm/1 is legal in,
  including inside `not(...)` iff norm is legal there; mirror norm exactly.
- Pattern must be a STRING LITERAL. Non-literal pattern = named refusal
  `regexp_pattern_not_literal`. Non-text operand (declared type) = named
  refusal `regexp_operand_not_text`. Both load-time, shared, both doors.
- SQL lowering: `(<operand_sql> REGEXP '<pattern>')` with the pattern carried
  as a bound parameter if the surrounding lowering binds literals that way;
  match the file's existing literal discipline.
- Runtime: register once at database open, next to where the connection is
  created in sprefa-store js (find by symbol, e.g. the SqlRunner seam):
  `db.function("regexp", { deterministic: true }, (pattern, text) =>
  text == null ? 0 : (new RegExp(pattern).test(text) ? 1 : 0))` with a
  per-pattern compiled-RegExp cache (a Map; patterns are literals, so the
  cache is bounded by program text).
- Oracle: `re_match/2` from `library(pcre)`, no flags. A `re_match` throw on
  a pattern PCRE rejects surfaces as a compile-time refusal if detectable at
  load (try `re_compile/3` during the shared check and refuse
  `regexp_pattern_invalid` with pcre's message), so a bad pattern never
  becomes a runtime divergence.
- Flavor: JS RegExp is Perl-derived, PCRE2 is Perl-derived, corners differ
  (possessive quantifiers, some escapes). Conformance fixtures pin the shared
  subset ONLY: literals, `[]` classes, `.`, anchors, `* + ? {n,m}`,
  alternation, groups, `\d \w \s`. One LANG.md line states the subset and
  that outside it the two engines are not promised to agree.
- rx lowering (for the .dl6-snippet law, goes in LANG.md beside the entry):
  `filter(row => /pattern/.test(row.textColumn))`.

## Scope

- v6/prolog: parse_dl.pl, the expansion/type-plane module norm rides,
  0_program_check.pl (refusals), lower.pl, emit_ts.pl, oracle engine files,
  LANG.md (one entry), conformance fixtures (positive match, non-match,
  retraction flip on a derived rel guarded by regexp, both refusals, a
  pattern-invalid refusal), plunit tests.
- v6/sprefa-store/js: the single registration site + one unit test proving
  `SELECT 'abc' REGEXP 'b'` = 1 through the runner.
- NOTHING else. The comment rail stays as-is; migrating its host flags is a
  separate later ruling.

## Gates (run in-lane; sandbox has no network and cannot bind ports)

```bash
cd v6 && just conformance && just text-door && just plunit
```
plus the store unit test if its suite binds no port; if it binds a port,
record the pre-edit pass/fail fingerprint and require it unchanged, and the
coordinator runs the real suite at review.

## Flow

NO-COMMIT lane (coordinator-cut worktree; git metadata is outside your
sandbox): verify base with `git rev-parse HEAD` == the sha the dispatch
states, STOP if it differs, leave the tree dirty, write REPORT.md at the
worktree root (changes file:line, fail-first receipts for each refusal,
gate outputs verbatim, deviations). Style laws: max 2 consecutive comment
lines, no banned words (provenance/substrate/load-bearing/regime/support),
descriptive dl variable names, follow each file's existing style.
