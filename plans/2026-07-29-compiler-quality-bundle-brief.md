# compiler quality bundle brief (codex sol): #7 + #8 + #12 + #13

Four morning-list items, one lane, because they share files
(v6/prolog/compile/{emit_ts,lower,analyze}.pl + walkers). Execute IN ORDER,
one commit-worthy unit each; each defect fix gets a fail-first receipt.

## Item 1 — emitter_groupby_literal (#7, ARCH row)
A rule head with >=2 bare integer-literal columns reaches the support-count
GROUP BY verbatim; SQLite reads a bare integer there as a POSITIONAL column
reference -> `2nd GROUP BY term out of range`. Shipped workaround: `0+0` in
v6/dl/fixtures/diag-rail.dl6. Fix in emit_ts.pl: wrap literal head columns
(or exclude constants from the GROUP BY — pick whichever leaves every
existing emitted module byte-identical; state the choice). Fail-first
fixture with two bare literal head columns (red: SQLITE_ERROR; green:
compiles + oracle-identical). Then REMOVE the 0+0 workaround from
diag-rail.dl6 and re-run `bash v6/tsv2/scripts/lsp-diags.sh` as the receipt.

## Item 2 — probe_output_guard (#8, ARCH row)
A comparison guard over a probe OUTPUT variable in a level rule is refused as
`unsupported_construct(unbound_head_var(_G))` — wrong name, no location. The
bound-variable set in lower.pl's guard compilation (find compile_guard_goals
by symbol; ARCH cites the response-atom output columns as the missing
members) must include host/probe response output columns. After the fix the
guard COMPILES (it is a legitimate WHERE over the response value). Fail-first
fixture: probe output var under a comparison guard, red with the misnamed
refusal, green oracle-identical.

## Item 3 — org_banked_findings (#12, ARCH row org_banked_findings)
Four drift sites, each already PINNED BY A TEST (find the pinning tests, they
state the wrong behavior as current): (1) trigger_items/body_atoms
misclassify next/combine/comparisons/lifecycle wrappers as relation atoms;
(2) goal_rel_refs reports next/1 + combine/2 as positive refs; (3) both
doors accept not(finalize(...)) — decide with the registry whether that is a
named refusal and make both doors agree; (4) 3 private cross-module calls in
sprefa-store/bench/v1-scale-gen.pl outside the lint gate's load set. Fix
each, flip its pinning test from drift-documenting to correct-behavior.

## Item 4 — B4 refusal messages umbrella (#13, the design review's worst
cold-author pain)
Zero prolog:message//1 clauses exist: every refusal prints as swipl's raw
`Unknown message: unsupported_construct(...)`. Add a messages module
(v6/prolog/compile/ or shared — follow the org-refactor module conventions,
prolog-lint gate must stay at baseline 1) with prolog:message//1 clauses for
EVERY named refusal reason in the registry + analyze (enumerate them from
registry.pl / 0_program_check.pl rather than hand-listing; a refusal without
a message clause should be a failing test so new refusals cannot regress).
Message shape: one line, the reason named, the offending functor/rel printed,
and FILE:LINE where the text door has position info (parse_dl may carry
positions; if it does not, location is rule-index granularity and you state
that as the residue rather than plumbing new position tracking through the
whole parser — smallest correct).

## Laws
- Files: v6/prolog/** (compile, conformance fixtures, walkers, registry,
  SYNTAX regen), v6/dl/fixtures/diag-rail.dl6, sprefa-store/bench/
  v1-scale-gen.pl. NOT v6/tsv2/scripts/flagship-flow* and NOT
  v6/dl/fixtures/flagship-flow.dl6 (another running lane owns those).
- Every emitted-module change must show zero byte movement on pre-existing
  gen modules except where an item's fix REQUIRES it (item 1 may change
  support GROUP BY text — state exactly which modules moved and why).
- Fail-first receipts recorded in fixture/test headers. No new deps.

## Validation (report exact counts)
- conformance go.pl 0 findings; plunit; sweep BOTH modes (movement only
  where item 1 predicts it); TEXT_DOOR; roundtrip; prolog-lint ratchet at
  baseline; lsp-diags receipt (item 1); dl + tsv2 suites untouched-green.

## Final summary shape
Base sha; per-item outcome + receipt lines; emitted-module movement list;
the message-clause coverage count (clauses vs refusal inventory); residues.
