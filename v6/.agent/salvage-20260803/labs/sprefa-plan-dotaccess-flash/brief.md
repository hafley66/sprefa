# Lane: dot access / namespacing recon + plan (dl6)

Worktree /Users/chrishafley/projects/sprefa-plan-dotaccess-flash, branch
lab/dotaccess-flash, base 2eceb836. FIRST action: `git merge --ff-only 2eceb836` —
failure = STOP and write PLAN.md saying so. Research + planning ONLY: you
write exactly two files, both at the worktree root: PLAN.md (receipts,
every claim cites path:line, verified by reading before writing) and
PLAN.visual.human.unga.md (plain human words, short lines, ascii diagrams,
zero citations — the owner reads THIS one).

## The ask (owner's words, binding intent)

Dotted access (`spine.files`, `row.column`) has been circled many times and
never ruled. The owner wants: (1) full recon of every prior consideration in
this repo; (2) a plan for making dots real; (3) inference that works
"correctly and without oddities like how typespec is odd with model vs
namespace"; (4) an explicit answer to "do we need actual namespace `::`
symbols?" and (5) an explicit answer to "do we need interfaces/traits for any
specific reason" — they are type system too. Answers (4) and (5) must be
argued from what the code actually needs today, not from taste.

## Recon sweep (do all, receipts for each)

1. Prior considerations: grep plans/, v6/prolog/rulings.pl, v6/prolog/ARCH.pl,
   chat_log/ for dot/namespace/qualified-name/member-access discussions.
   Inventory every prior mention with file:line and what was concluded or left
   open. Known adjacent open items to connect: Key(Type) vs `->`
   (plans/2026-07-27-lab-consolidation.md), Q8 residual (left-of-arrow =
   demand key), the type-IR plan in the sibling worktree
   /Users/chrishafley/projects/sprefa-plan-typeir/PLAN2.md (SCIP symbol pkg
   field = today's fake namespacing; read it).
2. Current surface: the identifier rule (v6/prolog/compile/parse_dl.pl:411-416
   region, alpha/alnum/underscore only), what `.` would collide with anywhere
   in the grammar (statement end? floats? operator table), print_dl round-trip,
   the langium grammar (v6/dl/grammar/dl.langium, demoted per ARCH.pl:663 —
   state whether the demotion matters here).
3. Name resolution today: how rel names bind (registry.pl), how columns bind
   (positional? named?), what the type pass knows (v6/prolog compile type
   files; phase 5 roadmap row "type pass float/REAL+avg"), where a qualified
   name would have to resolve in each pass.
4. Lowering doors: SQL (dots collide with db.table — quoted idents vs
   dot->underscore mapping; formerly-quadratic COUNT-test law applies to
   anything you propose there), TS emit, rxjs lowering (language law: every
   construct shown must carry a pure-rxjs lowering; a dot construct whose rx
   lowering cannot be written is a design defect).
5. Typespec's model-vs-namespace oddity: characterize it PRECISELY (from the
   typespec docs you know or can fetch: namespace members vs model properties
   both spell `A.B`, different declaration kinds, different resolution rules,
   the weirdness at their boundary). Then show how each candidate design here
   avoids or reproduces it. This is the owner's named failure mode; treat it
   as the primary acceptance test of the design.

## Design space to cost (packet, does NOT pick)

- One symbol `.` for both namespace access and member access, resolution by
  inference (what the inference rules must be; where ambiguity bites; show a
  worked ambiguous example and how it resolves or refuses).
- `::` for namespaces + `.` for members (two symbols, zero ambiguity; parser
  and vocabulary cost; how it reads in dl6 surface next to prolog's own
  module `:` conventions).
- No surface dots: namespacing stays in symbol strings / underscore prefixes
  (today's answer, zero cost, the ergonomic loss stated honestly).
- For each: parse cost in lines (calibrate against parse_dl.pl precedents),
  resolution rules as prolog-style inference clauses (sketch, not code),
  SQL/TS/rx lowering per door, migration cost for the existing corpus, and
  the typespec-oddity test verdict.
- Interfaces/traits: find every place the codebase already fakes one (host
  executor contracts, lang_ext printer-ignore seam, TS `I` interfaces law,
  one-rel-one-rule-kind law) and say whether a first-class construct pays for
  itself or the fakes are already the right shape. If yes, the SMALLEST
  construct that covers the found cases; if no, say no plainly.

End PLAN.md with: ARCH-style task/3 rows (shape: task(Name, Status, Needs),
statuses unbuilt) and the blocking questions in the order they block, each
answerable in one sentence by the owner.

## Laws

- No commits. Nothing outside this worktree (reading the two sibling plan
  worktrees named above is allowed). Never run just/battery targets. No
  subagents.
- Deviations: STOP the item, record in PLAN.md.
- Style: no em dashes; never the words provenance, substrate, load-bearing,
  regime; descriptive names; construct names use only rxjs/prolog/SQL
  vocabulary; "support" is banned (use refCount).
