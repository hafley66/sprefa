# Lane usedoor -- steps A1, A2, A3 of plans/2026-08-07-dynamic-loading.md

## First action, non-negotiable
`git merge --ff-only <BASE_SHA>` in your worktree. Failure or a missing tree =
STOP AND REPORT. Do not work around a blocked command (no tar, no --no-verify,
no copying). Never spawn a subagent; fan-out is the coordinator's call.

BASE_SHA: 1ca2f5ff

## What you own (nobody else touches these)
| file | state |
|---|---|
| `v6/prolog/compile/parse_dl.pl` | EDIT: add `use_item//1` only |
| `v6/prolog/compile/use_resolve.pl` | NEW: `include_roots/2`, `resolve_use_path/3`, `expand_uses/6` |
| `v6/prolog/conformance/fixtures/7_use_include.pl` | NEW: the fixtures below |
| `v6/prolog/compile/test/plunit_tests.pl` | EDIT: append your plunit block at the tail, touch no existing test |

Do NOT touch: `v6/prolog/0_dot_expand.pl` (step A4, coordinator's),
`v6/prolog/emit_ts.pl`, `v6/prolog/lower.pl`, `v6/prolog/analyze.pl`,
`v6/tsv2/**` (coordinator is mid-flight in all four).

## The receipts you are building against
- v5 ALREADY SHIPS this module system in Rust, and the DESIGN transfers whole:
  `src/frontend.rs:1-23` -- `use "path".` needs no new keyword, `expand` splices
  in file order, four include roots, canonical-path dedup, same-name-same-cols
  dedups, conflicting cols is a hard error naming both paths. Read it first.
- v6 parses a dotted path in functor position TODAY and refuses it:
  `v6/prolog/compile/parse_dl.pl:1062` `head_atom`, `:1538` `relatom_item`,
  refusal at `v6/prolog/0_dot_expand.pl:61` `refuse_rel_path_rule`. Your steps do
  NOT remove that refusal (A4 does).
- `grep -niE "\buse\b|\bimport\b|\binclude\b" v6/prolog/compile/parse_dl.pl` hits
  two prose comments (`:3`, `:36`) and ZERO grammar rules. `use` stays a plain
  ident, so the lexer gains nothing.
- **Comments are not the language.** `v6/prolog/compile/out/manifest.json` (306
  rows, `bucket` + `reason` each) is the verdict on what compiles, never a header
  and never `v6/dl/grammar/dl.langium` (a narrow MVP slice). Grep the manifest
  before you claim any construct is absent.
- Today's refusal for a module path: manifest buckets the three
  `7_module_path.pl` fixtures `unsupported`, reasons
  `module_path_unresolved([orchard,tree])`, `([orchard,fruit])`,
  `([orchard,north,tree])`. That term SURVIVES your work.

## The code to write, signatures first
```prolog
%! use_item(-Item, +S0, -S) is semidet.        % parse_dl.pl, beside the other items
%  ws0, lit_dcg(`use`), ws1, string_literal(Text), ws0, `.`  ->  Item = use(Text)

%! include_roots(+EntryPath, -Roots) is det.   % use_resolve.pl
%  v5's four, src/frontend.rs:9-19: dirname(EntryPath), $SPREFA_STD, <crate>/std,
%  <exe>/'..'; an absent env var contributes NOTHING (no empty-string root)

%! resolve_use_path(+Roots, +UseText, -AbsPath) is semidet.
%  first Root satisfying absolute_file_name(UseText, AbsPath,
%    [relative_to(Root), access(read), file_errors(fail)])
%  every root fails -> throw(use_path_unresolved(UseText, Roots))

%! expand_uses(+EntryPath, +OnStack, +Loaded0, -Loaded, -prog(Decls,Rules), -ModuleTable) is det.
%  parse EntryPath -> Items; partition use(_) from Core items
%  canonical AbsPath in OnStack -> throw(use_cycle([AbsPath | OnStack]))
%  foldl over uses: AbsPath in Loaded0 -> SKIP (a diamond parses ONCE); else
%    recurse with [EntryPath | OnStack] and splice the child's Decls/Rules
%    BEFORE the entry's own
%  merge col_type/3 by (Ref, ColumnName): equal type keeps one copy, conflict ->
%    throw(rel_col_conflict(Ref, PathA, PathB))
%  ModuleTable = [module(AbsPath, ModuleName, ModuleHash), ...] in LOAD ORDER
```
Build-vs-buy is already decided, do not re-litigate: `absolute_file_name/3` with
an EXPLICIT root list, NOT `user:file_search_path/2` (that predicate is global to
the SWI process, so a second concurrent compile would see the other's roots) and
NOT a hand-rolled `exists_file/1` walk (symlink and `..` canonicalization is the
part that decides whether a diamond dedups at all).

Lifetimes: `Loaded` and `OnStack` are born at the first `expand_uses/6` call and
die with that compile. `ModuleTable` must outlive you (A4 reads it), so return it,
never assert it.

## Fixtures, exact names, in conformance/fixtures/7_use_include.pl
| case | fixture name | required bucket / reason |
|---|---|---|
| zero imports | `use_absent_program_unchanged` | `compiled` |
| one import | `use_one_sibling_splices_in_file_order` | `compiled` |
| three-deep chain | `use_chain_three_deep_keeps_load_order` | `compiled` |
| diamond | `use_diamond_parses_each_file_once` | `compiled` |
| same rel, same cols | `use_same_rel_same_cols_dedups` | `compiled` |
| cycle | `use_cycle_refuses_naming_the_chain` | `unsupported`, `use_cycle(PathChain)` |
| self-import | `use_self_refuses` | `unsupported`, `use_cycle([Self])` |
| missing file | `use_missing_file_refuses_naming_the_roots` | `unsupported`, `use_path_unresolved(Text, Roots)` |
| same rel, conflicting cols | `use_same_rel_conflicting_cols_refuses` | `unsupported`, `rel_col_conflict(Ref, PathA, PathB)` |

COUNT RAIL, mandatory, because a naive recursive loader is exponential over a
diamond chain and end-state equality alone hides it: `use_diamond_parses_each_
file_once` asserts a per-canonical-path parse counter of EXACTLY 1, and
`use_chain_three_deep_keeps_load_order` asserts EXACTLY 4 parses for 4 files.
Additive assertions only; never replace an existing equality check.

## Style laws, inline, all mandatory
- Comment budget: comments state ONLY constraints the code cannot show, max 2
  consecutive comment lines in new code (a hook blocks the edit otherwise). No
  dates, no change-log narrative, no restating the next line. `%!` signature
  headers count toward the 2.
- Fail-first is the law: write each fixture, SEE it red with the reason you
  expect, then implement. Report the red text you saw.
- dl variable names are descriptive, never single-letter, in every snippet.
- Construct names and design discussion use ONLY rxjs, prolog or SQL words.
- Banned words, prose AND identifiers: provenance, substrate, load-bearing,
  regime, support (say refCount). No em dashes.
- One rel = one rule kind: a spliced module must not merge a source head with a
  derived head. If your splice makes that possible, REPORT it, do not paper over.

## Your gates, run all three, paste the output
```
cd v6/prolog && bash compile/scripts/text_door_receipt.sh    # expect 196/196/0 or better
cd v6 && just plunit                                          # expect 362 pass, 0 fail, PLUS your new cases
cd v6 && just conformance                                     # expect 306 pass, 0 fail, PLUS your new fixtures
```
If any leg is red on arrival, before you edit anything, REPORT THAT FIRST and stop.

## Pass 1 of 2, and the rules around the edges
- This is PASS 1 OF 2. A named second pass follows (style/dead-code/receipt
  sweep, then a coordinator design review), so FAVOR PLAIN CODE over clever
  code and do not pre-optimize anything.
- PACKAGE MANAGER IS pnpm. node_modules is ALREADY INSTALLED in your worktree.
  Never run `npm install` (it rewrites the lockfile and un-dedupes types) and
  never run `pnpm install` either; if a package is genuinely missing, STOP AND
  REPORT.
- IF REALITY DEVIATES FROM THIS BRIEF, STOP AND REPORT. Do not improvise, do
  not fix an adjacent thing you noticed, do not widen your file ownership.
  A wrong premise in this brief is the single most useful thing you can find.
- DO NOT COMMIT. Leave the work in the worktree.
- Deliverable contract: `REPORT.md` at your worktree root, in the report format
  below. Write it even if you stopped early; especially then.

## Report format
One table: fixture name, bucket reached, the red text you saw first. Then the
three gate outputs verbatim. Then anything you could not express and why. No
prose narrative.
