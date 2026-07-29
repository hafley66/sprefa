# REL SPREADING LAB (planner contract; user go 2026-07-29 "opus lab on it")

Question: can dl6 allow type/rel spreading (`rel b(...a, extra: int)`,
row spread in heads) and what are the exact semantics per case. The
coordinator's draft case table below IS the contract; the lab proves,
refutes, or amends each row with executable checks and captures the
cross-language ground truth with REAL compiler probes, not recall.

Lab home: v6/prolog/labs/rel_spreading/ + ONE verdict doc
plans/2026-07-29-rel-spreading-verdict.md. TOUCH NOTHING ELSE (a
concurrent arc owns compile/* and engine.pl; the lab is standalone .pl
model + probe scripts, the hosts-lab pattern). Labs die on landing.

## Cross-language ground truth (Q0, receipts first)

For each case below, write the minimal snippet in TypeScript (check
with the repo's tsgo/tsc), Rust (rustc --edition 2021), and Go
(go build; if no go toolchain on this machine, N/A-with-reason, never
fake it), capture accept / refuse / silent-behavior VERBATIM
(compiler message or runtime value) into a receipts file. The verdict
table cites these receipts, never memory.

## Cases (each = model check + cross-language receipt + verdict row)

C1 decl spread `rel b(...a, extra: int)`: compile-time column splice.
   Prototype expand_spread_program/2 in the lab model (the
   enum/match expand-module precedent): splice a's columns
   positionally at the spread point, source order preserved.
C2 column collision: `rel c(...a, ...b)` where a and b share a
   column name. Draft call: named refusal (enum collision
   precedent). Grade the TS last-wins hazard as the negative example.
C3 row spread in a head: `c(...a_row, 5) <- a(...a_row), ...` --
   define what the body-side spread binds (a fresh variable per
   spliced column) and check head arity totality.
C4 width subtyping: wider row where narrower wanted. Draft call:
   REFUSE, rels stay nominal, spread is splice never subtyping.
   Show the check that catches it.
C5 plane/key inheritance: spread carries COLUMNS ONLY, never
   key()/keep/log. Check: spreading a keyed rel into an unkeyed decl
   yields unkeyed; the spread-imports-a-key hazard is a graded
   negative.
C6 spreading a derived rel: BLOCKED on resolved derived decls (the
   type-pass dependency). The lab documents the failure shape today
   (what inference hands you for a derived rel) and names the slot;
   it does NOT design around it.
C7 spread in host decls `sh f(...common, ep: text) -> (...)`: falls
   out of C1 if spread is decl-time splice; check it composes with
   the sh_decl input/output split (term forms in
   plans/2026-07-29-hosts-extraction-verdict.md).
C8 rest/partial beyond kwargs: OUT OF SCOPE, record the boundary
   (kwargs partial application is landing in a concurrent lane;
   cite it, do not duplicate it).

## Spelling pricing (no fiat)

At least two spellings priced with criteria visible: `...name` spread
vs an `include name` decl clause vs prolog-side term form only. Every
selected spelling carries its rx lowering row (house law) -- for
spread this is trivial (splice happens before lowering) but SAY that
explicitly with one worked example lowered.

## Grades

Lab suite: swipl -q -l labs/rel_spreading/lab.pl -g go -g halt, exit
0, PASS-only stdout, run twice. Cross-language receipts file present
and cited per verdict row. No-drift: conformance go.pl and
roundtrip.sh untouched and green (run them, do not modify them).

## Laws

Worktree agent: FIRST ACTION `git merge --ff-only <base sha stated at
dispatch>`; on failure or missing v6/, STOP AND REPORT. Commit per
logical step with git commit -n; do NOT merge. Descriptive
identifiers; no em dashes; banned words provenance, substrate,
load-bearing, regime. Verdict doc: per-case table with criteria
visible, receipts cited, named slots for everything unresolved.
