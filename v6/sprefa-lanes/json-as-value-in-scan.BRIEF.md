# BRIEF: json as a value in scan

## Base
- Branch: `feature/json-as-value-in-scan`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `26f3f25f` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## USER DECISION 2026-08-11, verbatim
"get json as value in scan done please"

Two pieces, in this order, measured separately.

## PIECE 1: the brace-literal `:=` document

`v6/prolog/lower.pl:558-559`:

```prolog
    ; json_value_expr(Expr)
    -> throw(unsupported_construct(json_value_expression(Expr)))
```

That arm sits in `compile_expr/7`'s guard chain, so a JSON document written in
value position stops. `json_value_expr/1` is nearby; read it and state exactly
what it matches before changing anything.

`plans/2026-08-09-scan-into-json-research.md` already traced this and
classified it: **one-door, unfinished work**, not a real impossibility. Read
that document in full first; sections 1, 3, 5 and 6 are the ones that decide
your approach. Section 5 is the SQLite capability check on the pinned runtime.

Lift it so a JSON document literal is admissible in value position. Do NOT
widen anything else in that guard chain by accident: the arm below it turns an
unrecognized compound into `json_object('fn', ..., 'args', json_array(...))`,
and that behaviour stays exactly as it is for everything that reaches it today.

## PIECE 2: candidate B, the `json_patch` fold

From the research doc's verdict table, candidate B is the streaming one with
the lowest lowering cost and the highest reuse:

> near pass-through rendering on two already-`direct`-encoded json columns, one
> registry row; identical `pre/1` + UPSERT skeleton as candidate A with less
> new SQL-generation code

Its semantics are RFC 7396 merge-patch, a specified standard rather than an
invented one. It needs an oracle predicate, which does not exist yet.

Candidate C already landed (`json_group_object` aggregate head, merged earlier
today). Candidate A is NOT in scope; if piece 1 makes A cheap, say so in the
report and stop there.

## THE GATE THAT DECIDES THIS ARC
Oracle parity. Every fixture you add runs through the existing conformance
machinery and the emitted SQL must agree with the swipl oracle
(`v6/prolog/conformance/level_eval.pl`, `engine.pl`) on the tick log, byte for
byte. Not "same final rows". Not "looks right".

If you add an oracle predicate for merge-patch, it implements RFC 7396, and you
cite the clause of the RFC for each behavior: null deletes a key, objects merge
recursively, arrays and scalars replace wholesale.

## Do not cheat
| forbidden | why |
|---|---|
| deleting or weakening a test that goes red | the test is the finding |
| widening a fixture's expected output to match new behavior | grading yourself |
| making the oracle match the emitter | the oracle is the reference; it moves only for a cited RFC clause |
| catching the throw and emitting partial SQL | silently wrong beats loudly stopped, and this repo prefers the stop |
| a duplicate-key path that produces invalid JSON silently | see the house pattern below |
| `--no-verify` on any commit | a blocked command ends the approach |

The house pattern for an impossible case is at `lower.pl:5015-5022`: the
`json_object` arm raises by emitting `json('json_object_dup_key')`, which is
not valid JSON text, so SQLite fails the statement. That is deliberate and was
verified against sqlite3 directly. Match that shape if you need it; do not
invent a sentinel value.

**Fail-first receipts required.** Each behavior: the test red before your
change, green after, both verbatim in the report.

**Sabotage receipt required.** Once green, break the merge-patch semantics in
one line, show the gate catching it, revert. Verbatim both ways.

## Every .dl snippet in your report carries its rx lowering
Repo law: a construct whose rxjs lowering cannot be written is a design defect.
The research doc already gives rx lowerings for all three candidates; yours
must match or explain the difference.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/lower.pl` | full |
| `v6/prolog/compile/registry.pl` | add rows |
| `v6/prolog/analyze.pl` | only if the construct needs classifying |
| `v6/prolog/conformance/level_eval.pl`, `engine.pl` | the oracle predicate for merge-patch ONLY |
| `v6/prolog/conformance/fixtures/**`, `v6/prolog/compile/test/**` | add fixtures and tests |
| `plans/2026-08-11-json-as-value-in-scan.md` | create |

Forbidden: `v6/prolog/strat.pl`, `emit_ts.pl`, `compile/6_emit_dd_plan.pl`,
`compile/parse_dl_dcg.pl`, `0_type_plane.pl`, `0_option_expand.pl`,
`0_generic_expand.pl` (four lanes own those right now), plus `v6/dd-runner/**`,
`v6/boop/**`, `v6/labs/**`, `.github/**`, `chat_log/**`.

Five other lanes are live. If a file you need is on that forbidden list, STOP
and report which and why; do not edit it.

## Gates, every commit
```bash
cd v6 && just conformance     # 372 PASS 0 FAIL on base
cd v6 && just plunit          # see KNOWN RED
cd v6 && just text-door       # compiled=272 byte_identical=272 failures=0
cd v6 && just green-all       # final; report the delta against your own stashed diff
cd v6/tsv2 && bash scripts/sweep.sh   # regenerate the manifest; report the compiled count
```

The manifest at `v6/prolog/compile/out/manifest.json` is the verdict on what
the language accepts. Report its `compiled` count before and after.

**KNOWN RED ON BASE, do not chase, do not fix, do not allowlist:** `plunit`
(`catalog_plane_rail:level_plane_family_corpus_counts`, `plunit_tests.pl:1312`,
1 of 598), `rtkq-golden` / `flagship` / `extraction-live` / `lsp-diags` (all
four: missing release extractor binary), `compile-speed` (baseline 2026-08-07),
`tsv2-test` (`hostDecode.test.ts:144`), `golden-flex` (stale `json_object/2`
excuse). Full list: `.github/CI-KNOWN-RED.md`. Measure green-all on your base
FIRST. ZERO legs may turn red.

## Worktree setup you need before any gate
`node_modules` is absent in a fresh worktree: `pnpm install` in `v6/tsv2` and
`v6/sprefa-store/js`. The text-door corpus is GENERATED: `cd v6/tsv2 && bash
scripts/sweep.sh`. The pre-commit rail needs
`v6/sprefa-extract/target/release/extract`; copy the prebuilt one from
/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract to
the same relative path. Five lanes have been stranded by this today.

## Commit discipline
Commit per measured step. A lane lost its whole run to a machine sleep today
because it batched.

## Deliverable
`plans/2026-08-11-json-as-value-in-scan.md` with:
1. What `json_value_expr/1` matched before and after, with the guard chain
   shown, and proof nothing else in that chain widened.
2. Piece 1 and piece 2 measured separately: manifest `compiled` count after
   each.
3. Fail-first receipts, verbatim both directions.
4. The sabotage receipt, verbatim.
5. The RFC 7396 clause cited per merge-patch behavior.
6. Every new .dl spelling with its rx lowering.
7. Gate output verbatim and the green-all delta.
8. "ARCH ROW TO ADD" with the row text; `ARCH.pl` is not yours to edit.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. Max 2 consecutive
  comment lines. Fail-first receipts in TEST headers are the named exception.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- dl variable names are descriptive, never single-letter, in every snippet.
- Tables and lists over prose. Numbers come from tool output only.
- Never announce location in text ("here is", "below is", "the following").
