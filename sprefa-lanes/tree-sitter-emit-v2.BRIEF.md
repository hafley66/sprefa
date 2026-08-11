# lab/tree-sitter-emit-v2: the emitter measured ITSELF, not the question

## Why round 2 exists
Round 1 built a structural translator (seq / choice / literal / call /
blank) and reported `clauses=103 translatable=32`, overlay ratio 2.96,
verdict "the DCG cannot be the single description". That verdict is an
artifact of ONE naive generator. Three bounded techniques were never
attempted, and the coordinator verified the receipts for all three
against the code today. Your job is to attempt them and REMEASURE. The
answer may still be no; it must be no for a reason the code supports.

## The three untried techniques, with receipts

### 1. The precedence table is DATA, not runtime magic
Round 1's report says "arithmetic tiers come from runtime registry data"
and treats that as non-derivable. It is a `findall` over facts you can
load and query:

- `parse_dl_dcg.pl:967-972` — `infix_op(Axis, Op)` runs
  `findall(C, surface(C/2, Axis, no_refs, infix(_), _), Cands)`.
- `parse_dl_dcg.pl:1020` — `tier_operators(Prec, Ops)` runs
  `findall(Op, expression(Op/2, arithmetic, Prec, _, _), Ops0)`.
- `v6/prolog/compile/registry.pl` holds 76 `surface/5` + `expression/5`
  facts.

So: load registry.pl into the emitter, run the SAME `findall` goals, and
emit `prec.left(N, ...)` / `prec.right(N, ...)` declarations plus the
operator literal sets mechanically. Compare what you emit against the
six precedence levels the hand grammar declares (REPORT2.md Phase A).
If they match, precedence moves from HAND-ONLY to EMITTED.

### 2. Character predicates are a FOUR-row table, written once
The parser uses exactly four `code_type/2` classes:
`space` (:243), `alnum` (:269), `alpha` (:279), `digit` (:290).
A fixed mapping (`space` -> `/\s/`, `digit` -> `/[0-9]/`, etc.) is
written ONCE and never grows per construct. Round 1 counted every
regex-shaped token as HAND-ONLY overlay; with the table they are
generated. Confirm the class inventory yourself (`grep -n code_type`)
and report if it is larger than four.

### 3. Parameterized nonterminals can be SPECIALIZED
`sep(P, Xs)` and `args(P, Xs)` are the combinators round 1 declared
fatal. Their call sites bind P to CONCRETE parsers; the coordinator
counted 24 call sites and these distinct arguments: `atom_arg`, `body`,
`decl_a_column`, `enum_field`, `expr`, `head`, `typed_col`, `int_lit`,
`rel_atom_term`. Standard partial evaluation: for each distinct binding,
emit one specialized grammar rule (`sep_expr`, `sep_int_lit`, ...). This
is mechanical and bounded by the number of call sites, not by language
size. Handle the `args(_ ...)` unbound case by reporting it separately.

## What to deliver
1. `emit_grammar.pl` v2 implementing all three, plus whatever else you
   find that is mechanical. It may load registry.pl and any other
   compiler data file; state every input in the report.
2. Re-run the classification of all 44 hand rules:
   EMITTED-IDENTICAL / EMITTED-NEEDS-OVERLAY(reason) / HAND-ONLY(reason).
3. Re-measure the overlay ratio the same way round 1 did (non-ws chars
   of remaining hand overlay vs emitted), so the two numbers compare.
4. `REPORT3.md`: per-technique verdict (WORKED, -N chars of overlay /
   FAILED, why), the new classification table, the new ratio, and a
   VERDICT line: is the DCG-plus-registry a sufficient single
   description, yes or no, and what specifically still is not derivable.

## The bar
Round 1's ratio was 2.96 hand-written chars per generated char. Beat it
or explain precisely why each technique could not. A ratio under 1.0
flips the whole arc's answer, so measure carefully and do not round in
your own favor.

## Rails
- The emitted grammar must still be a REAL grammar: after emitting, run
  `npx tree-sitter generate` on it and report whether it builds. A
  smaller emitted file that does not generate is worth nothing.
- You own `v6/labs/tree-sitter-door/**` ONLY. Never edit
  `parse_dl_dcg.pl`, `registry.pl`, `print_dl.pl`, or anything outside
  the lab: reading them is the whole point, changing them is cheating.
- Keep the Phase A hand grammar and its gate passing: `./run-tests.sh`
  must stay rc=0.
- Setup first: `cd <worktree>/v6 && just text-door` generates the
  266-file corpus the gate needs.
- Blocked: `FAILURE-REPORT-EMIT2.md` with exact command + output, exit
  nonzero. A technique that genuinely fails is a RESULT; report it with
  the receipt. Do not improvise past the brief.
- NEVER git merge/pull/rebase. NEVER --no-verify. Up to 6 commits,
  prefix `lab:`. No push, no PR; coordinator judges. Lanes never spawn
  subagents.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Package manager is pnpm, never npm.
