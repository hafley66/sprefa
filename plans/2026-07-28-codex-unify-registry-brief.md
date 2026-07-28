# CODEX BRIEF: surface/5 construct registry (sol-class, the compiler unification)

Context: renaming three surface words touched 49 files because every
construct is hand-encoded in five places (parse_dl.pl, print_dl.pl,
analyze.pl, lower.pl's construct handling, SYNTAX.md). The fix: the
construct inventory becomes ONE fact table and the five places become
projections of it. The conformance side already lives table-driven
(rulings.pl, covers/2, ARCH sugar/2); this brings the compiler in line.

## Build

In v6/prolog/compile/ (you own the whole dir; no other agent is editing
compile/*.pl):

1. `registry.pl` (new): `surface(Functor/Arity, Axis, AnalyzeRole,
   LowerRole, Status)` rows for EVERY construct the compiler currently
   touches: latest/1, finalize/1, next/1, combine/N, zip/2,
   unsubscribe/complete/subscribe/error wrappers, not/1, pre/1, now/1,
   decode/2, comparison ops, := binds, aggregate heads -- mine the
   current clause lists in analyze.pl (body item handling,
   check_supported_subset) and parse_dl.pl for the full inventory.
   Status in {live, reserved, refused}. Axis names from the design
   record: sign, sample, join, time, guard, bind, aggregate.
2. Refusal-by-absence: check_supported_subset's per-construct refusal
   clauses collapse to one rule consulting the registry; a body item
   whose functor has no live surface/5 row throws
   unsupported_construct(Functor) automatically. Named refusals for
   reserved rows keep their specific error terms where tests pin them.
3. analyze.pl's per-construct dispatch (goal_rel_refs and friends)
   becomes one generic clause + registry lookup for AnalyzeRole; the
   roles are implemented once each (refs_of_arg, splice_bare,
   arm(Sign), no_refs, ...).
4. parse_dl.pl + print_dl.pl consult the registry for the wrapper-word
   inventory (which functors parse as body wrappers, which print back).
   FULL bidirectional single-grammar unification is a STRETCH GOAL:
   attempt it only after grades pass on the table-driven version; if the
   pure-DCG both-directions attempt costs formatting fidelity, keep two
   files consulting one table and say so.
5. SYNTAX.md's construct table section becomes GENERATED from the
   registry (a small emitter + a committed regenerated section, marked
   as generated); hand prose stays.

## Grades (byte-identity, all re-run by you)

conformance go.pl 110 PASS; sweep RUN total=31 identical=28 wrong=0
unchanged per-fixture; roundtrip.sh ALL GRADES PASS; plunit all pass;
tsv2 6/6 + import gate. PLUS the refactor receipts: before/after clause
counts for the collapsed dispatch sites (grep-countable, paste them),
and one demonstration in the final summary: adding a fake reserved
construct as ONE registry row makes it parse, print, and refuse by name
with zero other edits (then remove it).

## Laws

Descriptive prolog variables; no em dashes; banned words provenance,
substrate, load-bearing, regime. One logical step per commit, git
commit -n, no push, no merge. If a grade breaks and the fix is not
obviously mechanical, STOP and record. Final summary: registry row
count, clause-count receipts, the one-row demo receipt, all grades.
