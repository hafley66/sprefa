# CONTRACT: dotted member access (`X.field`, chains) — first dot unlock

Scope: MEMBER ACCESS ONLY. `X.field` and chains `X.field.sub` in rule
bodies, where X is a variable bound by an atom in the same body. NO path
access, NO namespaces, NO dotted heads, NO modules (they need the catalog;
out of scope). Grounding docs at this worktree root: DOTPLAN.md (the
dot-access recon; its R2/R4 sections locate every parser/printer/lowering
site) and 2026-08-03-module-catalog-ruling.md (stance 9 = the ruling you are
implementing).

## The semantics (ruled, not yours to redesign)

- `X.field` is a THIRD SPELLING of what the decode-brace / relation-pattern
  spellings already do (DOTPLAN shows both live in
  `dl_view/relation_depth2_*.dl6`). It must DESUGAR to the same terms the
  existing brace spelling produces — the desugar happens at parse/expand
  time; the lowering pipeline sees nothing new.
- Resolution: bound-variable-first. X must be bound in the body; an
  unresolvable `x.y` (x unbound) is a refusal (`unresolvable_member(x.y)`),
  never a silent parse as something else.
- Chains: `X.a.b` = nested destructure, one hop per dot, same as nesting
  the brace shapes.

## Parser rules

- Member dot is GLUED: `X.field` with no whitespace around the dot, dot
  followed by identifier-start. The statement-terminator dot keeps working:
  followed by whitespace/EOF. `x . y` (spaced) stays a syntax error.
- Float literals (`1.5`) must be unaffected.
- print_dl prints the dot spelling back (round-trip identity for the new
  form), and existing corpus round-trip stays 100%.

## Deliverables

1. Parser + expander changes in v6/prolog/compile/ (DOTPLAN R2 names the
   sites; calibrate against its line estimates).
2. plunit: new cases for member access, chains, glued-vs-terminator
   disambiguation, unbound refusal, float non-regression. The 771-input
   `parse_error_positions` suite MUST pass unchanged — regressing it is a
   STOP-and-report, not a fix-the-test.
3. Conformance fixture: a dot-spelling TWIN of an existing
   relation_depth2/decode fixture. THE PROOF: the twin's compiled output is
   IDENTICAL to the brace-spelling original (same emitted SQL/ts) — the
   equivalence oracle, not a new golden.
4. Round-trip: the dot fixture survives parse -> print -> parse.
5. REPORT.md: gates table with exact commands + outputs, deviations
   (STOP-and-record, never improvise).

## Gates (all recorded)

| gate | command |
| --- | --- |
| plunit | the parse/expand suites you touched + parse_error_positions, exact commands recorded |
| ARCH | `swipl -g go -t halt ARCH.pl` from v6/prolog exits 0 |
| roundtrip | the repo's roundtrip check (find it; DOTPLAN cites it) passes 100% |
| equivalence | dot twin fixture compiles byte-identical to brace original |
| conformance | the conformance runner over your new fixture |

## Laws

No commits. Nothing outside this worktree. Never run the full battery.
Comment budget: constraints only. No em dashes. Never provenance, substrate,
load-bearing, regime. Descriptive dl variable names, never single letters.
Every .dl snippet in REPORT.md carries its rx lowering. No subagents.
