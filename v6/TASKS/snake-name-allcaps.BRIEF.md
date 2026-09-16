# snake-name-allcaps (issue: snake-name-allcaps-mangling, size:small)

FIRST ACTION: `git merge --ff-only 046cbc510804671d2441aca36536bbd310eef485`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Issue:
/Users/chrishafley/projects/sprefa/issues/snake-name-allcaps-mangling/item.md

BUG (traced): `snake_codes/2` at v6/prolog/analyze.pl:370-376 emits `_<lower>`
for EVERY uppercase code, so `VAR_CAPS_0` becomes `v_a_r__c_a_p_s_0` and `URL`
becomes `u_r_l`. Renaming a variable to an ALLCAPS shape changes its inferred
column name and flips 5 corpus fixtures from compiling to
`join_column_type_mismatch`.

THE FIX (exact, implement this and nothing else): rewrite `snake_codes/2` so
an underscore is inserted ONLY at a word boundary, and uppercase RUNS collapse
to one word:
- boundary 1: a lowercase letter or digit followed by an uppercase letter
  (`fooBar` -> `foo_bar`, `a1B` -> `a1_b`)
- boundary 2: an uppercase letter followed by uppercase-then-lowercase — the
  last upper of a run starts the next word (`HTTPServer` -> `http_server`)
- inside an uppercase run, NO underscore (`URL` -> `url`)
- an underscore already in the name passes through; never emit two consecutive
  underscores (`VAR_CAPS_0` -> `var_caps_0`); the existing leading-underscore
  strip in `snake_name/2` (analyze.pl:364-368) stays as is.

PINNING TABLE (put exactly these in a plunit test, fail-first — run the test
BEFORE the fix, paste the failures, then after):
| input | output |
|---|---|
| `'G'` | `g` |
| `'FooBar'` | `foo_bar` |
| `'fooBar'` | `foo_bar` |
| `'URL'` | `url` |
| `'HTTPServer'` | `http_server` |
| `'VAR_CAPS_0'` | `var_caps_0` |
| `'already_snake'` | `already_snake` |

FILES YOU OWN: v6/prolog/analyze.pl (ONLY snake_codes/2 and any helper it
needs), v6/prolog/compile/test/plunit_tests.pl (additive test block), plus
regenerated v6/prolog/compile/out/** IF the sweep changes any module (existing
CamelCase variables produce identical names under the new rules, so expect
zero drift; nonzero drift on a name NOT in the ALLCAPS class = STOP AND
REPORT).
FORBIDDEN: lower.pl, emitters, conformance/**, fixtures, everything else.

VALIDATION (paste outputs, run each leg twice):
1. plunit: `cd v6 && just plunit` — your new tests pass, prior red set
   unchanged (known-red five).
2. `cd v6/prolog/conformance && swipl -g go -t halt go.pl` — same PASS count
   as base, 0 FAIL.
3. `cd v6/tsv2 && bash scripts/sweep.sh` — identical/wrong counts unchanged
   from base, manifest reason-diff all zeros.
4. The metamorphic repro from the issue: fixture `enum_name_is_a_column_type`
   with variable `G` renamed to `VAR_CAPS_0` now compiles (one-off term-level
   check is fine; paste it).

COMMIT plain, COMMENT_RAIL_IDLE_MS=3000, never pipe a commit, commit ONLY in
your worktree on your branch (`pwd` before every git commit).
Close: `issuectl --json close snake-name-allcaps-mangling --commit <sha>:<summary>`.
Report: the 7 table rows passing, the 4 gate numbers, out/ drift (expected zero).
