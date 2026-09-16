# CONTRACT: golden scenarios — enum + match + scan-fold + json compose; one vs any

Lane: worktree sprefa-lab-compose, branch lab/compose-golden, base
719901f8. FIRST ACTION: `git merge --ff-only 719901f8`; else STOP.
Commits fail-first then green. No pushes, no subagents.

User words (2026-08-03, binding): "how do enums and scans and match
compose lets get some golden test scenarios integrating all 3 for the
golden e2e script with current language features, we should include
nesting types, json somewhere, a scan, one and any and show one is
necessarily different".

## Tasks
1. Extend v6/dl/fixtures/golden-flex.dl6 (the golden e2e program; its
   coverage gate at golden_coverage.pl forces new constructs to appear
   by name) with a scenario section composing, in CURRENT surface only:
   an enum decl with a tag column; a match block over that tag; a
   scan-shaped fold (the self-referential level-rule spelling of rx
   scan, since the scan keyword does not exist yet); a decode/2 json
   read feeding it; a nested/struct value type in the flow. One
   coherent story (e.g. tagged events folded into per-tag counts from
   a json source), not five islands. Every added rel typed, queried
   (`?`), and graded by the golden harness.
2. THE any/one DIVERGENCE RECEIPT: build the same-tick double-fire
   scenario twice with current features: (a) `any` = two edge arms on
   one tagged head, same tick, BOTH rows land (assert both, tags
   distinct); (b) show `one` is NOT expressible today: the honest
   attempts and why each fails — keyed head folds to a winner but
   loses the loser silently (assert the fold, note the lost tag);
   bounded log REFUSES (retention_head_conflict_risk — cite the ruling
   in rulings.pl); guard-by-negation same-tick is refused by the clock
   checker (capture the refusal term). Each attempt is a fixture or a
   refusal receipt, committed. This gap ledger is the fail-first
   record the future `one { }` construct must turn green.
3. A short COMPOSE.md at worktree root: what composed cleanly, what
   grated (exact line receipts), and which rx translation each piece
   landed on — the "best translation fit" evidence the design lane
   will consume. Plain words + snippets, every dl snippet with its rx
   lowering.

## Gates
just plunit / conformance / text-door / golden-flex all green at the
end (re-golden the flexed bytes per the established additive pattern);
paste totals. The divergence receipts must each show their assertion
or refusal output verbatim.

## Style
subscribe vocabulary never demand; banned words provenance/substrate/
load-bearing/regime; descriptive dl variable names; rx lowering beside
every snippet; comment budget constraints-only. Report = final text.

## SYNTAX ANCHORS (read these before writing a line; the coordinator's
earlier sketches used invented syntax — trust only the code)
- enum surface: rel with semicolon variant disjunction —
  plunit_tests.pl:1118 parser_retains_semicolon_enum_decl, and
  0_enum_expand.pl for what it expands to.
- match block: golden-flex.dl6:440-457 (paren block, `; guard |-> head`
  and `|+> head` arms, match_nonexhaustive coverage).
- scan the WORD is banned for file enumeration (files-hosts.dl6:5-6);
  spell the fold as the self-referential level rule and name nothing.
- decode/2 json and struct/nested values: existing golden-flex sections.
