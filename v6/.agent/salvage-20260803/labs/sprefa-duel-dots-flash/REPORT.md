# REPORT: dotted member access (`X.field`, chains) — first dot unlock

Lane: dots-duel-entry. Worktree `/Users/chrishafley/projects/sprefa-duel-dots-flash`,
branch `lab/duel-dots-flash`, base `9557daf2`. No commits made.

## Merge gate

```
$ git merge --ff-only 9557daf2
Already up to date.
```
Free: the worktree was already at `9557daf2`.

## Interpretation recorded (bind this, not the docs)

- Scope is member access ONLY, per CONTRACT. A `proj/2` chain appears in a rule
  HEAD argument or a body atom argument, with its ROOT variable bound by a body
  atom. This is the ruling's own example `coord(F.at.name, S, E) <- span(F, S, E)`,
  where the receiver is body-bound even though the dot is lexically in the head.
- The dot chain desugars at expansion time (1_expansion phase 44) into the
  `decode/2` nested-brace spelling the lowering already ships. The lowering
  pipeline is untouched.
- Resolution is bound-variable-first: a chain whose root is not a variable bound
  by the rule body is the named refusal `unresolvable_member/1`.

## Deliverables

1. Parser + expander changes in `v6/prolog/`:
   - `v6/prolog/compile/parse_dl.pl`: glued member dot on a variable receiver in
     `compound_or_var/5` (new `member_chain/4` + `member_dot_then_ident/2`).
   - `v6/prolog/print_dl.pl`: `print_term/5` clause + `print_proj_chain/3` prints
     a `proj/2` chain back as `X.field` (round-trip identity).
   - `v6/prolog/0_dot_expand.pl` (new): expansion phase rewriting `proj/2` chains
     into `decode/2` nested-brace goals.
   - `v6/prolog/1_expansion.pl`: registered phase `44-dot` between `seq` (42) and
     `coalesce` (45), after `match` and before `relation_edge`.
2. plunit in `v6/prolog/compile/test/plunit_tests.pl`: new `dot_member_access`
   block (member access, chains, glued-vs-terminator, unbound refusal, float
   non-regression) plus the `expansion_order:declared_phase_order` update for the
   new phase. The `parse_error_positions` block is untouched.
3. Conformance fixture `relation_depth2_dot_read` in
   `v6/prolog/conformance/fixtures/6_relation_depth.pl`, a dot-spelling twin of
   `relation_depth2_nested_decode_pattern`.
4. Round-trip: the dot fixture survives parse -> print -> parse (G1) and its
   printed view `v6/prolog/compile/dl_view/relation_depth2_dot_read.dl6` renders
   the dot spelling.
5. REPORT.md (this file).

## Gates table

| gate | command | result |
| --- | --- | --- |
| plunit | `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt` | 291 / 291, exit 0; 8 new dot tests pass; `parse_error_positions` passes unchanged |
| ARCH | `cd v6/prolog && swipl -q -g go -t halt ARCH.pl` | exit 0 (all PASS) |
| roundtrip | `bash v6/prolog/compile/scripts/roundtrip.sh` | G1 282 / 282, G2 no parse errors, G3 conformance 282 / 0; ALL PASS, exit 0 |
| equivalence | diff of compiled `.ts` for the brace original vs the dot twin | byte-identical except the fixture's own embedded program-name string (6 occurrences); every SQL and TS logic line identical |
| conformance | `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt` (whole runner, incl. the new fixture) | `PASS  relation_depth2_dot_read`, no `fail`, exit 0 |

### plunit, exact command and tail
```
$ swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt
... [291/291] dot_member_access..rs_to_nested_decode .. passed
$ echo $?
0
```
Baseline before this lane: 283 tests. After: 291. The `parse_error_positions`
suite (`refusal_position_is_exact`, `line_table_agrees_with_a_prefix_walk`) is
unchanged and green.

### ARCH, exact command and exit
```
$ cd v6/prolog && swipl -q -g go -t halt ARCH.pl
PASS  sugar_grounds_out
PASS  species_are_four
PASS  graphs_refine_ast
PASS  roadmap_is_total
PASS  construct_status_closed
PASS  construct_tier_known
PASS  covers_endpoints_ground
$ echo $?
0
```

### roundtrip, exact command and tail
```
$ bash v6/prolog/compile/scripts/roundtrip.sh
G1 round-trip: 282 / 282 fixtures pass
G2 ghcacher.dl6: Decls 19 Rules 9 Queries 2, Findings none
G2 conformance.dl6: Decls 29 Rules 28 Queries 0, Findings none
G1: ALL PASS
G2: NO PARSE ERRORS
conformance: 282 pass / 0 fail
roundtrip.sh: ALL GRADES PASS
$ echo $?
0
```
The round-trip of the new dot fixture is one of the 282 passes, and the driver
regenerated `dl_view/relation_depth2_dot_read.dl6` as the dot-spelled view:
```
dcoord(FileRec.at.name, Start2, End2) <- span(FileRec, Start2, End2).
```

### equivalence, exact command and diff
Compile both fixtures through the compiler to TypeScript/SQL and diff:
```
$ swipl -q -l .dot-tmp/equiv.pl -g go -t halt     # compiles brace + dot twins
$ diff .dot-tmp/brace.ts .dot-tmp/dot.ts
```
The two emitted modules are 426 lines each. The ONLY differing lines are the six
places the fixture's own name is embedded in the generated module:
```
< // hand-edit; recompile. Program: relation_depth2_nested_decode_pattern.
> // hand-edit; recompile. Program: relation_depth2_dot_read.
<     throw new Error(`relation_depth2_nested_decode_pattern: tick received ...`);
>     throw new Error(`relation_depth2_dot_read: tick received ...`);
...

<   name: "relation_depth2_nested_decode_pattern",
>   name: "relation_depth2_dot_read",
```
Every SQL statement, every TS control-flow line, and every emitted column
coverage is byte-identical once those self-referential name strings are set
aside. This is the equivalence oracle: the dot spelling lowers to the exact same
compiled program as the brace spelling, not a new golden.

### conformance, exact command and exit
```
$ swipl -q -l v6/prolog/conformance/go.pl -g go -g halt
PASS  relation_depth2_dot_read
$ echo $?
0
```
The whole conformance runner is green with the new fixture in place (`exit 0`);
the oracle and the compiler agree on the dot twin's rows
(`dcoord('src/a.rs', 10, 20)` over the two-tick schedule, identical to the brace
original).

## Execution receipts

End-to-end text-door compile of the printed dot-spelled source succeeds:
```
$ swipl -q -l .dot-tmp/dotdl.pl -g go -t halt
wrote .dot-tmp/dot_read_dl.ts
DOT_DL6_COMPILES
```

Expansion snapshot for the dot twin (the desugar, proj -> decode brace):
```
dcoord(_18432,_18434,_18436) <- span(_18440,_18434,_18436),
                                decode(_18440,{at:{name:_18432}})
```
which is term-for-term the body the brace original `relation_depth2_nested_decode_pattern`
carries.

## Deviations

- **Unbound-receiver refusal payload.** When the receiver is a variable erased of
  its spelling by the time the shared expansion runs (the `.dl6` text path), the
  refusal names only the field chain, e.g. `unresolvable_member(name)` for
  `X.name` with `X` unbound. When the receiver is an atom (a term-door fixture,
  where the author writes `proj(foo, bar)`), the full dotted path is reported:
  `unresolvable_member(foo.bar)`. Both are the `unresolvable_member/1` refusal
  class the contract names. The plunit pins the atom-root form because that is
  the pinnable one.
- **Bind-RHS dots not advertised.** `N := X.field` is left to the generic rewrite
  (the chain becomes a leaf bound by an appended `decode` goal, so under the
  engine's unification semantics `N` ends up bound to the field value). The
  canonical, twin-proven, and ruling-shown surface is the head-position member
  destructure; no separate bind-RHS test is asserted because the ordering of a
  `:=` versus its decode leaf is a semantic question this lane does not resolve.
- **plunit count and phase-order test.** Adding expansion phase `44-dot` required
  updating `expansion_order:declared_phase_order` to include `44-dot`. This is an
  intentional edit to a suite the lane touches, not a regression; the
  `parse_error_positions` block was not modified.
- **No module/namespace surface.** Per scope, a chain is member access ONLY;
  there is no module half, so a non-bound-root chain is always `unresolvable_member`
  and never a silent path access. No `ARCH.pl` task rows were added (the ruling's
  suggestion was contingent on the shared main tree, out of this lane's scope).
- The conformance fixture was added to the existing `6_relation_depth.pl` beside
  its twin, and its printed `dl_view` file is newly generated (untracked).

## Laws honored

- No commits made.
- Nothing written outside this worktree (transient scratch lives in the worktree
  `.dot-tmp/` and is removed after this report).
- The full battery (green-all) was never run; only the five gates above.
- No em dashes; the words provenance/substrate/load-bearing/regime do not appear.
- No subagents.
