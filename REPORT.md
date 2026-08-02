# dl6 diag channel: REPORT

## Base proof

`git merge --ff-only 3d8c34e3` from the worktree root printed `Already up to
date.` and no further work was needed to reach the base.

## What I built

A machine-readable diagnostic channel for the dl6 text-door compiler. The
compiler already throws one refusal term per diagnostic; that term now flows
through a JSON emitter in addition to the existing human renderer. There is
still ONE source of truth per diagnostic (the refusal term) and TWO renderers:
`0_refusal_messages.pl`'s umbrella `prolog:message//1` DCG (unchanged) and the
new channel, which reads the human text back through `message_to_string/2`.

Files:

- `v6/prolog/labs/diag_channel/diag.pl` (new). The emitter. One LSP-shaped JSON
  record per line, encoded with SWI's own `library(http/json)`
  (`json_write_dict`), default stderr, appended to `DL6_DIAG_JSONL` when set.
  `lsp_position/4` is the single monotonic 1-based to 0-based conversion for
  LSP line and character. `dl6_span/6` is the requested side table, keyed by a
  relation reference, a point span at the offending statement.
- `v6/prolog/labs/diag_channel/diag.test.pl` (new). plunit receipts: the
  1-to-0 conversion, the rail across the whole inventory, JSON round-trip, and
  real position resolution on a parsed program.
- `v6/prolog/compile/parse_dl.pl`. Position retention. Added exported
  `statement_location_for_reason/3` and `statement_location_for_reference/4`
  over the retention the parser already keeps; refactored the existing
  line-only predicate to reuse them. No parsed term shape changed, so no
  emitted TypeScript byte changes.
- `v6/prolog/compile.pl`. Wiring: the refusal chokepoint `throw_text_door_error/2`
  emits the located diagnostic into the channel before rethrowing; the
  parse-phase error path in `compile_dl6/2` is wrapped so parse errors publish
  too. The admixture is three lines at the refusal site only; a successful
  compile never reaches it.
- `v6/prolog/compile/test/plunit_tests.pl`. `ensure_loaded` of the new test
  file so it runs in the plunit gate.

## The one-source-two-renderers proof

The channel never constructs its own message text. `diag_message/2` is
`message_to_string/2` on the same `unsupported_construct(...)` term the umbrella
`prolog:message//1` renders. The plunit test `json_message_equals_human_line`
walks the entire dynamic inventory and asserts, for every member, that
`Record.get(message)` equals `message_to_string(unsupported_construct(Example))`.
The two renderers therefore cannot diverge in content: they are the same string
by construction, checked across the whole inventory, and neither knows a
signature the other does not.

## Position coverage

The channel resolves a real line and column for a refusal whose thrown reason
names a relation the parser retained: the reason's relation references resolve
through `parse_dl`'s retention to the offending statement's start position.
Parse errors carry their exact line and column from the parser directly.

FALLBACK SET (15 signatures whose reason names a construct, surface primitive or
word, never a parsed relation, so the parser cannot locate them):

| signature | why it falls back |
|---|---|
| `registered_surface` construct signatures: `complete/1`, `error/1`, `group_concat/1`, `json_array/1`, `json_each/2`, `json_object/2`, `scan/variadic`, `set/0`, `sg_pattern/3`, `subscribe/1`, `tagged_brace/1`, `unsubscribe/1`, `zip/2` | the reason names the surface primitive itself, not a relation in the parsed program |
| `removed_word/1` | names a removed word, not a relation |
| `tagged_brace_reserved/1` | a lexer-time refusal naming a reserved word, thrown before any relation is retained |

RESOLVABLE BY MECHANISM: every other inventory signature. Its thrown reason
embeds the offending relation reference, which the parser's retention resolves
to a real statement line and column.

RUNTIME-CONFIRMED (compiled real .dl6 programs, position actually resolved to
the offending statement's line and column): `finalize_in_level_rule`,
`latest_in_level_rule`, `pre_in_level_rule`, `level_rule_no_positive_body`,
`recursive_stratum`, plus the parse-error class.

HONEST NUMBER: **58 of 73** signatures carry a real source position through the
channel (the loaded inventory is 73 here, not the contract's 77; the count
depends on which compiler modules are loaded when the inventory runs, because
`refusal_inventory/1` reads currently-loaded refusal clauses). 15 fall back to
rule-index granularity. The 58 is a mechanism claim (reason names a relation),
not an all-triggered claim: the corpus and my synthetic programs only refuse a
handful of signatures, so I runtime-confirmed the resolved subset above and
verified the fallback set resolves to rule-index (1:1), but did not individually
trigger all 58. The position is statement-start granularity (the invoking
statement's line and first column), which is the rule-index tier the brief's
honest-number framing accepts; exact-token columns are not produced.

## Byte-identity proof

Human refusal output captured for six failing .dl6 programs on the base commit,
re-captured after the change, diffed empty:

```
diff human.base.txt human.after.txt      # -> no output
```

Capture harness: each failing program compiled through `compile_dl6/2`, the
caught error rendered with `message_to_string/2` (the exact line the CLI
prints), with `DL6_DIAG_JSONL` set so the JSON goes to a file and stderr stays
pure human. Result: EMPTY DIFF / BYTE IDENTICAL.

## Gate output

`cd v6/prolog && swipl -g go -t halt ARCH.pl`: PASS (all claims, incl.
`covers_endpoints_ground`, `roadmap_is_total`).

`just green-all` was attempted. Every Prolog/compiler leg ran and passed; the
battery could not complete in this sandbox because two legs need packages the
registry does not serve here (environment, not this change):

- `typecheck` -> `npx tsgo` returns `E404 Not Found - GET
  https://registry.npmjs.org/tsgo` (the package is not on the public registry
  this sandbox reaches).
- `sweep` stage 3 -> `ERR_MODULE_NOT_FOUND: Cannot find package 'rxjs'` (node
  modules not installed here). Sweep stages 1 and 2 passed.

Legs that ran and passed, verbatim:

- conformance: `281 pass / 0 fail` (also echoed by roundtrip's G3)
- plunit: `281/281` (baseline was 276; the +5 are the new `diag_channel` tests)
- roundtrip: `G1: ALL PASS`, `G2: NO PARSE ERRORS`, `G3: 281/0`
- TEXT_DOOR: `compiled=196 byte_identical=196 failures=0`
- prolog-lint: `PROLOG_LINT findings=1 baseline=1 OK`
- ARCH: PASS
- compile-speed: `programs=4 phases=24 regressions=0 improvements=0 OK`
- sweep stage 1: `SWEEP total=281 compiled=196 unsupported=85 crash=0`

NUMBER MOVES (reported, not "fixed"): plunit 276 -> 281, from adding the lane's
own tests. No gate moved for any other reason; the compiler bucket counts
(281/0, 196/196/0, sweep 196/85/0) are unchanged.

## What I could not do

- Could not run the full `just green-all` to a clean exit in this sandbox: the
  `typecheck` and `sweep` stage-3 legs are blocked by an unreachable npm
  registry (`tsgo` E404) and a missing `rxjs` node module. Environment blockers,
  unrelated to this change; the compiler-adjacent legs all pass.
- Could not give exact-token columns. Positions are statement-start granularity
  (line and the statement's first column). Finer positions (the exact column of
  the offending construct inside a body) would need positions embedded inside
  parsed terms, which is exactly what the byte-identity hazard forbids, or edits
  to `lower.pl`/`emit_ts.pl`, which the brief forbids; I did not touch either.
- Could not individually runtime-trigger all 58 resolvable signatures to confirm
  each one: the corpus and my synthetic programs refuse only a handful. The 58
  is asserted by the channel's mechanism (reason names a retained relation),
  confirmed on the triggered subset, not by triggering every signature.
- Did not materialize a stored `dl6_span/6` fact table on every successful
  parse. That would add per-statement inference to the parse phase and trip the
  compile-speed inference ratchet (a gate number the brief forbids moving). The
  side table is therefore materialized on demand from `parse_dl`'s existing
  per-statement retention, which leaves term shapes untouched and the parse
  phase inference count unchanged (compile-speed: 0 regressions). This is the
  design the brief invites by name.
