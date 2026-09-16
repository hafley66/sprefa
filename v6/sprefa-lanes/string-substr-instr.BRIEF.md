# BRIEF: string free tier — substr, instr, length

## Base
`git merge --ff-only 23af7ef5bc5dc1e172b907fc81f9fa516a813921`. Failure = STOP
AND REPORT. If a procedural line here seems to forbid otherwise-correct work,
the work wins: note the conflict in your report and keep going.

## One sentence
dl6's text scalars (`upper`, `lower`, `trim`, `replace`, ...) lack `substr`,
`instr`, `length`; all three are native SQLite scalars, so each is a registry
row + oracle clause + fixture, exactly the shape PR #238 landed.

## The template to copy: PR #238 (merge 0755e6bc)
Its entire source footprint was THREE files. Yours is the same three:

| file | what #238 did there | what you do |
|---|---|---|
| `v6/prolog/compile/registry.pl:258-275` | one `expression(Name/Arity, text_scalar, 3, Rendering, text_only)` row per func; Rendering == Name means "lower directly to the SQLite scalar of the same name" (`lower.pl:620-624`) | rows for `substr/2`, `substr/3`, `instr/2`, `length/1` |
| `v6/prolog/conformance/body.pl` | prolog evaluation of each scalar so the oracle grades the same semantics (grep `upper` there for the exact clause shape) | `substr` via `sub_atom/5`, `instr` via `sub_atom/5` first match (SQLite is 1-based, 0 = not found), `length` via `atom_length/2` |
| `v6/prolog/conformance/fixtures/11_string_std_builtins.pl` | one fixture file, several `fixture(...)` cases with exact expected rows | NEW file `v6/prolog/conformance/fixtures/12_string_substr_instr.pl`, same structure |

## The wrinkle, decided for you: ALL THREE need typed operands and results
The existing category cannot carry any of the three. Receipts:
- `text_only` is an OPERAND rule, "operand must be text" (`registry.pl:231`),
  and `11_string_std_builtins.pl` header states every current row is
  all-text-operand and that this is why they lower with no glue. `substr`
  takes int operands, so it does not fit.
- The result type is hardwired `Type = text` in the text_scalar branch at
  `lower.pl:551-552`. `instr`/`length` return int, so they do not fit either.

Build ONE new parallel category (suggested name `typed_scalar`) whose registry
rows carry an operand-type list and a result type, then add a parallel clause
beside every site that pattern-matches `text_scalar`. The complete site list:
- `analyze.pl:792` and `analyze.pl:832-834` (expression acceptance)
- `lower.pl:549-552` (compile branch; for int operands reuse the
  `compile_numeric_operand` machinery the arithmetic branch at
  `lower.pl:558-560` already uses, and set `Type` from the row's result type)
- `lower.pl:591-594` (`text_scalar_expr`) and `lower.pl:608-624`
  (`text_scalar_sql`; the direct rendering clause carries over unchanged)
- `conformance/body.pl:57` (`expression_for_term` category match) and the
  `text_scalar_value/3` clauses at `body.pl:86+` (oracle evaluation)
Grep for `text_scalar` yourself before starting and extend the list if the
grep finds a site this brief missed; say so in the report if it does.

## SQLite semantics to match exactly (the oracle must agree byte-for-byte)
- `substr(X, Y)` / `substr(X, Y, Z)`: 1-based start; negative Y counts from
  the end. Test both.
- `instr(X, Y)`: 1-based position of first occurrence, 0 when absent.
- `length(X)`: character count, not bytes.

## Fixture cases required (exact expected rows, not counts)
positive-start substr, negative-start substr, substr/3 with length, instr hit,
instr miss (= 0), length of ascii text, an int result used in a comparison
(e.g. `length(Text) > 3` as a guard), and at least one case composing two
funcs (e.g. `substr(x, instr(x, '_') + 1)`) IF int expressions are accepted in
that argument position; if they are not, record the error text in the report
instead of forcing it.

## HARD RULES
- Commit ONLY the three source files + the new fixture. `compile/out/**`
  artifacts regenerate; a `git add -A` that sweeps them in is a defect
  (#238's salvage did exactly that and a cleanup PR had to untrack 586 files).
- Do NOT touch `parse_dl_dcg.pl` or `print_dl.pl`; #238 needed neither
  (expressions parse generically). Needing them = STOP AND REPORT.
- FORBIDDEN: `v6/tsv2/**`, `v6/sprefa-engine-rs/**`, `v6/sprefa-extract/**`,
  `CLAUDE.md`. Both runtimes execute emitted SQL; native scalars need zero
  runtime work.

## Validation, run and paste verbatim, each three times
```bash
cd v6/prolog/conformance && swipl -g go -t halt go.pl 2>&1 | tail -5
cd v6 && just plunit 2>&1 | tail -3
```
Your new fixture's cases must appear as PASS. Pre-existing failures: check
`.github/CI-KNOWN-RED.md` before reporting anything as broken.

## Style laws, inline
- No em dashes. Banned words in prose AND identifiers: provenance, substrate,
  load-bearing, regime. The word "refusal" is banned in prose.
- Comments state only constraints the code cannot show.
- Descriptive variable names, never single-letter.

## Report format
Zero-context coworker brief, every claim `path:line`. COMMIT your work; a lane
that exits without committing has not delivered.
