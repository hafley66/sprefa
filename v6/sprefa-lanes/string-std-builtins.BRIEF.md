# BRIEF: the string standard library, as registry rows

## SALVAGE STATE (2026-08-13, read first)
A previous lane died mid-work. Your branch (`feature/string-std-builtins-2`)
is cut from its salvage head and already carries:
- `daa7c3b2`: the inventory plan doc (`plans/2026-08-12-string-std-builtins.md`), Deliverable 1 DONE
- `964f1338`: WIP salvage commit: `registry.pl` mid-edit rows, a started fixture
  `v6/prolog/conformance/fixtures/11_string_std_builtins.pl`, plus untracked
  `.types.*` noise in `compile/out/` you may ignore
Your FIRST action: `git merge 73a6d7f7c3513f3b9516ff3d3f25d014cb72d736` (a real
merge; the branch predates current main). Then read the salvage diff
(`git show 964f1338 -- v6/prolog/compile/registry.pl`), keep what is correct,
and continue Deliverables 2 and 3. Failure to merge = STOP AND REPORT.

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

## One sentence
`.dl6` has exactly THREE text functions; add the rest of a string standard
library as rows in the existing builtin registry, each with a fixture proving
it compiles and answers correctly.

## What exists, measured. Verify each line before you build on it.

| fact | evidence |
|---|---|
| the builtin registry is one table, one row per function | `v6/prolog/compile/registry.pl:237-264` |
| exactly 3 text functions exist today | `registry.pl:258` `norm/1`, `:263` `rtrim/2`, `:264` `replace/3` |
| the row shape is `expression(Name/Arity, Family, Precedence, Rendering, TypeRule)` | same file |
| a Rendering equal to the function name lowers straight to the SQLite scalar of that name, no glue | `v6/prolog/lower.pl:620-624` |
| a Rendering that is NOT the name gets a hand-written SQL expression | `lower.pl:614-618`, `norm/1` as `WITH RECURSIVE` over characters |
| **the seam has NO scalar-function registration, so user-defined SQL functions are NOT available** | `lower.pl:612-613`, comment on the `@libsql` seam |
| `concat([...])` lowers to SQL `\|\|` | `lower.pl:544-546` |
| `group_concat(Value, Sep, Ordinal)` exists as an aggregate | `lower.pl:5159`; fixture `ordered_fragment_line_assembly.dl6` |
| `group_concat`'s ordinal must be `int` | probed 2026-08-12, ordering by a text column throws `aggregate_ordinal_not_int` |

That last "no UDF" line is the binding constraint of this whole lane. Every
function you add must be expressible as either (a) a SQLite built-in scalar or
(b) a SQL expression built from built-in scalars, exactly as `norm/1` is. If a
function needs neither, it does not land in this lane; it becomes a reported
fork.

## Deliverable 1: the inventory, before any code

Write `plans/2026-08-12-string-std-builtins.md` opening with a table of contents,
then a table with one row per candidate function:

| function | signature | SQLite expressible? | how | verdict |

Cover at minimum: `upper`, `lower`, `trim`, `ltrim`, `substr`, `length`,
`instr`/`index_of`, `starts_with`, `ends_with`, `contains`, `pad_left`,
`pad_right`, `repeat`, `reverse`, `char_at`, `split`, `join`, `snake_to_pascal`,
`snake_to_camel`, `pascal_to_snake`, `format`/`printf`.

Consult SQLite's actual scalar function documentation for what exists in the
pinned version rather than assuming. State the version you checked. Note that
SQLite has `instr`, `substr`, `replace`, `trim`/`ltrim`/`rtrim`, `upper`,
`lower`, `length`, `printf`, `char`, `unicode`, `hex`, `quote`, `format` and
does NOT have a split.

`split` is the one the user has been blocked on and it is named as an open item
in CLAUDE.md. Price it honestly: a split producing MULTIPLE ROWS is a different
shape from a scalar (it is a table-valued function, and the existing
`json_each`-style fan-out may be the real door). If `split` cannot be a
`text_scalar`, say so with the reason and propose the shape it should take.
Do not force it into the wrong family to make the row count look better.

## Deliverable 2: the functions that ARE expressible

For each one your inventory marks green:
- one row in `registry.pl`
- a rendering in `lower.pl` if the name does not match a SQLite scalar
- one conformance fixture proving the answer, not just that it compiles
- the fixture asserts a VALUE, with at least one edge case (empty string,
  a multi-byte character, an out-of-range index)

Commit per function or per small group. Do not land twenty functions in one
commit.

## Deliverable 3: the per-target lowering question, as a written fork

The user asked, and this is a QUESTION not an instruction:

> "it all should be host rels, we supply it from runtime? yea, like the emitter
> decides if it lowers to sqlite or lang?"

Do NOT implement this. Write it up as a fork in the plan doc, with citations:

- today `Rendering` is ONE value per function, and `lower.pl` turns it into SQL.
  Cite the exact line where that assumption is baked in.
- a per-target rendering means the column becomes a set: SQLite text, TypeScript
  expression, Rust expression. Say what would have to change and where.
- price the alternative: keep ONE SQL lowering and let the Rust and TypeScript
  backends both go through SQLite, which is what they do today.
- say which functions would actually BENEFIT from a native lowering, with a
  reason. A function that SQLite already does well gains nothing from three
  spellings and costs three places to be wrong.
- the `sh` host escape hatch already exists (`sh name(cols) -> (cols) = \`tmpl\`.`,
  see `duplicate_host_name_is_refused.dl6:1`) and it forks a shell per call.
  State plainly why that is the wrong mechanism for a string function.

The user rules on this fork. Your job is to present it priced, not to pick.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| a fixture that only proves it compiles | compiling is not answering; assert the value |
| adding a function with no fixture | an untested builtin is a future incident |
| claiming a function is "not possible" without naming the throw site | a refusal is a hypothesis; cite the line |
| registering a name SQLite lacks and hoping | there is no UDF registration; it will fail at runtime, not compile time |
| implementing the per-target fork because it seemed obvious | it is a language design call and it is the user's |

## File ownership. Yours alone:
- `v6/prolog/compile/registry.pl`
- `v6/prolog/lower.pl` (text scalar rendering region only, `lower.pl:590-630`)
- new fixtures under `v6/prolog/conformance/fixtures/`
- `plans/2026-08-12-string-std-builtins.md`

## Forbidden, other lanes own these:
- `v6/prolog/emit_rust.pl` and `v6/sprefa-engine-rs/**`
- `v6/boop/**`
- `v6/prolog/compile/6_isolated_compiler_dd.pl`
- `v6/prolog/compile/7_emit_ts_types.pl`, `8_emit_rust_types.pl`

If you need a change in a forbidden file, STOP and report the exact line and
reason. Do not work around it.

## Validation, run each three times, never once
- `cd v6/tsv2 && bash scripts/sweep.sh` — report the `SWEEP total= compiled=` line
  and `MANIFEST_REASON_DIFF`, which must stay all zero for existing fixtures
- conformance suite — report PASS/FAIL counts, baseline is 392/0
- `cd v6/prolog && swipl -g go -t halt ARCH.pl` — must stay all PASS

`just green-all` is RED and has been for days. `.github/CI-KNOWN-RED.md`
allowlists the failing legs. Read it before reporting any leg as broken. A leg
that fails and is NOT allowlisted is the real signal.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; a compiler error for an unbuilt
  construct is "TODO" or "not built yet". The word stays only in literal code
  identifiers and existing filenames.
- Comments state ONLY constraints the code cannot show. No change-log narrative,
  no dates, no arc references in source.
- dl variable names are descriptive, never single-letter, in every snippet.
- Construct names use ONLY rxjs, prolog, or SQL words.
- The 10-second law: any operation over 10s is a defect to investigate.
- Docs open with a table of contents; output is tables and lists, not prose.

## Worktree setup, before your first commit
The extractor binary and two pnpm installs are absent in a fresh worktree. Run
the repo's prescribed setup before committing; the pre-commit hook needs them.
