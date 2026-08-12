# 2026-08-12 green-all triage

Measured 2026-08-12 on base `154ae23c` in a fresh worktree, machine quiet
(all boop lanes 0.0-0.2% cpu during the timing-sensitive legs). Each leg run 3
times, one at a time, never the whole gate. See
`.github/CI-KNOWN-RED.md` for the allowlist this measurement produced.

## TOC

1. Verdict table
2. Real failures
3. Legs deleted from the ledger (green 3/3)
4. Flaky legs
5. Fix-these-first ranking

## 1. Verdict table

| leg | 3-run result | verdict | root cause (one line) | throw site |
|---|---|---|---|---|
| roundtrip | FAIL 3/3 | real | print_dl/parse_dl round-trip not variant-stable for the mutual-recursion fixture | `v6/prolog/compile/scripts/roundtrip.sh:132` |
| extraction-live | PASS 3/3 | stale (ledger) | `EXTRACTION LIVE HOLDS`; passes once the release extractor is present | none |
| lsp-diags | FAIL 3/3 | real | LSP client never receives both diagnostics for b.ts (driver.log stalls at READY); needs the v5 `dl` binary, then fails B1 deterministically | `v6/tsv2/scripts/lsp-diags.sh:266` |
| flagship | FAIL 3/3 | real | pinned corpus moved since the v5 golden: digest `b8d03946` now `8e3874d5`; golden must be regenerated | `v6/tsv2/scripts/flagship-callgraph.sh:287` |
| getting-started | FAIL 3/3 | real | engine error text changed to `rule-index unavailable:`, doc block 24 still prints the old `broken.dl6:4:` message | `v6/tsv2/scripts/getting-started.sh:224` |
| golden-flex | FAIL 3/3 | real | `json_object/2` stale `refused` excuse (now live) + `json_patch/2` unexercised; 69 registry constructs, 2 unaccounted | `v6/prolog/compile/scripts/golden_coverage.pl:174,178` |
| tsv2-test | FAIL 3/3 | real | sh host grid decode: a 0-row demand answers with rows; decoded per-demand counts `[0,1,2,3]` expected, `[1,2,2,3]` actual. (Needs `gen_emitted/` first, produced by `just sweep`.) | `v6/tsv2/tests/hostDecode.test.ts:144` |
| plunit | FAIL 3/3 | real | 6 unit-test failures now (ledger records 1); catalog_plane_rail + expression_inventory + rel_zero_arity + 3 json_merge_patch, incl. two `no_exception` | `v6/prolog/compile/test/plunit_tests.pl:1314,4561,5809,7684,7739,7743` |
| rtkq-golden | FAIL 3/3 | real | api_endpoint row order mismatch: engine emits updateUser-before-listUsers, order-sensitive golden expects listUsers-first (spans identical, not a corpus move) | `v6/tsv2/labs/1_rtkq-extraction-golden.ts:200` |
| compile-speed | FAIL 3/3 | real | `COMPILE_SPEED regressions=16 improvements=0` vs 2026-08-07 baseline; golden-flex lower +178%, emit +120% | `v6/prolog/compile/scripts/1_compile_speed.sh:248` |
| scale-floor | FAIL 3/3 | real | stmts/tick `[39,43]` flat at BOTH 10k and 1k (delta-proportionality holds) but expected pin `[37,41]` is stale by a constant +2 | `v6/tsv2/scripts/7_scale-floor.sh:240` |
| memory-soak | FAIL 3/3 | real | sqlite page count grows ~2x across the soak: second-quarter 24.8 -> final-quarter 49.5 vs +10% ceiling 27.2; storage not flat | `v6/tsv2/scripts/memory-soak.ts:327` |
| leak-soak | PASS 3/3 (fresh TMPDIR per run) | green (build/artifact, not defect) | passes on a clean TMPDIR; the leg leaves a literal `dl-perf.XXXXXX.jsonl` (mktemp template has text after `XXXXXX`, so no substitution) that collides on TMPDIR reuse | n/a |
| serve-leak-soak | PASS 2/3, FAIL 1/3 | flaky | 1/3 left a transient `Immediate 0->1` handle pending at the sampling instant after 20 swap cycles | `v6/tsv2/tests/serveLeak.test.ts:148` |

Three ledger rows contradicted the coordinator tonight: `lsp-diags` is a real
3/3 failure (not a stale-entry PASS), `getting-started` fails at block 24 (not
the `lex_token/2` warning the ledger carried), and `plunit` has six failures
(not one). `extraction-live` is confirmed green and is deleted.

## 2. Real failures

### roundtrip (new, was not in the ledger)

Verbatim:
```
G1 round-trip: 391 / 392 fixtures pass
  FAIL mutual_recursion_matches_oracle (.../fixtures/engine_core.pl): fail(not_variant)
G1: FAILURES PRESENT
```

Throw site: `v6/prolog/compile/scripts/roundtrip.sh:132`
```
130  do_g1_one(Prog, Bindings, Status) :-
131      print_dl_program(Prog, Bindings, Text),
132      parse_dl(Codes, Prog2, _Bindings2, _Findings),
133      ( Prog =@= Prog2 -> Status = pass ; Status = fail(not_variant) ).
```

Fixture `engine_core.pl:452` is a three-clause mutual recursion
(`even`/`odd` cross-rules). Round-trip is a variant mismatch: parse(print(T))
is not `=@=` to T for that shape. Fix (not applied, and read-only by order):
repair `print_dl.pl`/`parse_dl.pl` so the mutual-recursion rule set round-trips
to a variant, then regenerate `dl_view/`; the golden count rises to 392/392.

### flagship

Verbatim:
```
FAIL  the corpus MOVED since the v5 golden was captured (golden b8d03946961b7e67678a119c3f092a49a28416e9554f1fcffe39425dc8f52162, now 8e3874d544830722627f3817941ef32ee4b4572f008ff6bf75d95818bdaf2f34).
      Grading v6 against a golden from a different corpus is a false green.
      Regenerate from v6/tsv2 with:
      FLAGSHIP_V5_WRITE=1 bash scripts/flagship-callgraph.sh
```

Throw site: `v6/tsv2/scripts/flagship-callgraph.sh:287`
```
285  want="$(manifest_field corpus_sha256)"
286  got="$(corpus_digest)"
287  [ "$want" = "$got" ] \
288      || fail "the corpus MOVED since the v5 golden was captured (golden $want, now $got).
```

Fix (not applied): the pinned corpus (files under `v6/sprefa-extract/`)
changed since the golden was captured. Regenerate with
`cd v6/tsv2 && FLAGSHIP_V5_WRITE=1 bash scripts/flagship-callgraph.sh`, then
commit the updated `v6/tsv2/goldens/flagship_callgraph/v5_golden/MANIFEST.tsv`
and per-rel `.tsv`s. Verifying the corpus move is deliberate before writing.

### getting-started

Verbatim:
```
FAIL  block 24: output does not match the doc
      --- doc
      +++ actual
      -{"code":"log_on_level_headed_rel/1","message":"broken.dl6:4: unsupported_construct: ...","range": {"end": {"character":0,"line":3},...}}
      +{"code":"log_on_level_headed_rel/1","message":"rule-index unavailable: unsupported_construct: ...","range": {"end": {"character":0,"line":0},...}}
GETTING STARTED STALE: 1 of 24 block(s) disagree with v6/GETTING-STARTED.md
```

Throw site: `v6/tsv2/scripts/getting-started.sh:224`
```
221  want = normalize(expected, "norm=bytes" in flags)
222  got = normalize(actual, "norm=bytes" in flags)
223  if want == got: ... continue
224  print(f"FAIL  block {number}: output does not match the doc")
```

`v6/GETTING-STARTED.md:380-381` still print the old engine message
(`broken.dl6:4: unsupported_construct...` with `line:3`); the engine now
emits `rule-index unavailable: unsupported_construct...` at `line:0`. Fix
(not applied): update the two block-24 lines (and the two at `:353-354` if the
same drift applies) to the new message text.

### golden-flex

Verbatim:
```
FAIL  coverage gate: GOLDEN_COVERAGE FAIL: json_object/2 is excused as 'registry status `refused`' but its registry status is now live -- the excuse is stale
GOLDEN_COVERAGE FAIL: json_patch/2 (expression) is a registry construct the golden does not exercise -- add it to golden-flex.dl6, or record a named unsupported construct for it in expected_absent/2 AND in the golden's header
GOLDEN_COVERAGE 69 registry constructs, 2 unaccounted for
```

Throw site: `v6/prolog/compile/scripts/golden_coverage.pl:174` and `:178`
```
173  format(atom(Problem),
174         "~w is excused as ~q but its registry status is now ~w -- the excuse is stale", ...)
177  format(atom(Problem),
178         "~w (~w) is a registry construct the golden does not exercise -- add it to golden-flex.dl6, ...", ...)
```

Fix (not applied): `json_object/2` went live but a stale `expected_absent/2`
excuse (`refused`) still covers it; remove the excuse and exercise it in
`golden-flex.dl6`. `json_patch/2` is live and unexercised; add it to
`golden-flex.dl6` (or record it as a named absent construct).

### tsv2-test

Verbatim:
```
AssertionError [ERR_ASSERTION]: the decoded row count per demand, straight off the trace: [2,1,2,3]
+ actual - expected
  [ -0, 1, 2, +2, 3 ]
actual: [ 1, 2, 2, 3 ], expected: [ 0, 1, 2, 3 ], operator: deepStrictEqual
```

Throw site: `v6/tsv2/tests/hostDecode.test.ts:144`
```
142  assert.deepEqual(
143      answers().slice().sort((l, r) => l - r),
144      [0, 1, 2, 3],
145      `the decoded row count per demand, straight off the trace: ${JSON.stringify(answers())}`)
```

A cardinality-0 `want=["0"]` sh-host demand answers with a row instead of
zero: the trace rows are `[2,1,2,3]`, i.e. the zero-row demand decodes to 1
row. Fix (not applied): the sh host decode path (`serve/1_hosts.ts`,
`decodeObjectItems`, or the grid host template) mis-decodes an empty answer as
one row. Requires `gen_emitted/` first (`just sweep`); regeneration is setup,
not the fix.

### plunit

Verbatim (6 of 621 failed, summary `ERROR: [Thread main] 6 tests failed`):
```
catalog_plane_rail:level_plane_family_corpus_counts .. **FAILED
expression_inventory:inventory_is_exactly_the_expected_rows .. **FAILED
rel_zero_arity:a_root_rel_zero_still_has_no_storage .. **FAILED
json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard .. **FAILED
json_merge_patch:merge_patch_stops_on_the_json_null_stand_in .. no_exception
json_merge_patch:merge_patch_stops_on_a_nested_json_null_stand_in .. no_exception
```

Throw sites: `v6/prolog/compile/test/plunit_tests.pl:1314,4561,5809,7684,7739,7743`
(the assertions are inside each named test). The three `json_merge_patch`
failures are `no_exception`: a test that expects the lowering to throw now does
not, so a guard on the json-null stand-in stopped firing (a recent json feature
landed after the ledger's single entry). Fix (not applied): trace the
`json_merge_patch` guard + `expression_inventory` + `rel_zero_arity` +
`catalog_plane_rail` bodies; the ledger's single `catalog_plane_rail` row
under-records the defect.

### rtkq-golden

Verbatim:
```
ERR_ASSERTION, deepStrictEqual
actual: { rows: [ ['api.ts',272,468,'updateUser','mutation',327,337], ['api.ts',272,468,'listUsers','query',407,416] ] }
expected:{ rows: [ ['api.ts',272,468,'listUsers','query',407,416], ['api.ts',272,468,'updateUser','mutation',327,337] ] }
```

Throw site: `v6/tsv2/labs/1_rtkq-extraction-golden.ts:200`
```
198  assert.deepEqual(await json(server.port, "/idb/api_endpoint"), {
199      rows: [
200        ["api.ts", 272, 468, "listUsers", "query", 407, 416],
201        ["api.ts", 272, 468, "updateUser", "mutation", 327, 337],
202      ],
203  });
```

The two rows are byte-identical in spans; only order differs. The assertion is
order-sensitive `deepStrictEqual` and the emission order flipped. Fix (not
applied): order the two rows by a stable key in the golden (or sort before
comparing), after confirming the emission order is intentionally sorted and
not an MAP/object-iteration nondeterminism that should be fixed at the source.

### compile-speed

Verbatim:
```
COMPILE_SPEED regressions=16 improvements=0 FAIL
  golden-flex  lower  147557 -> 410329  +178.1%  REGRESSION
  golden-flex  emit   467429 -> 1031141 +120.6%  REGRESSION
  flagship-flow lower   173620 -> 355848  +105.0%  REGRESSION
  ... 13 more
```

Throw site: `v6/prolog/compile/scripts/1_compile_speed.sh:248`
```
246  echo "COMPILE_SPEED programs=$program_count phases=$phase_rows regressions=0 improvements=0 OK"
247  else
248  echo "COMPILE_SPEED regressions=$regressions improvements=$improvements FAIL"
```

Inference counts rose across 16 phases in flagship-flow, golden-flex,
flagship-callgraph and door-handwritten vs the 2026-08-07 baseline. Fix (not
applied): re-baseline with `bash scripts/1_compile_speed.sh --write-baseline`
only after confirming the shift is a real compile-cost change (likely the same
compiler churn hitting plunit) and not an acceptable feature growth.

### scale-floor

Verbatim:
```
stmts/tick set @10000   [37,41]   [39,43]   FAIL
same set @1000          [39,43]   [39,43]   OK
SCALE_FLOOR s2 rows=10000 FAIL
```

Throw site: `v6/tsv2/scripts/7_scale-floor.sh:240`
```
239  printf '%-26s %16s %16s   %s\n' "gated counter" expected measured verdict
240  check "stmts/tick set @$rows" "$expected_statements_set" "$statements_per_tick_set"
```

The set is a flat `[39,43]` at both 10k and 1k, so the equality-across-sizes
delta-proportionality proof HOLDS; the pinned expected `[37,41]` is a stale
constant +2. Fix (not applied): accept the new steady-state `[39,43]` as the
expected set (the gate compares against the prior secured run in
`goldens/scale-floor-history.jsonl`); the flatness property is intact.

### memory-soak

Verbatim:
```
FAIL  sqlite_page_count_flat: second-quarter mean 24.8, final-quarter mean 49.5, ceiling 27.2 (tolerance +10%)
TSV2_SOAK_FAIL: sqlite_page_count_flat
```

Throw site: `v6/tsv2/scripts/memory-soak.ts:327`
```
325  findings.push(check_flat_mean("heap_used_flat", ..., tolerance));
326  findings.push(check_flat_mean("sqlite_page_count_flat",
327      mean(second_quarter.map((e) => e.page_count)), mean(final_quarter.map((e) => e.page_count)), tolerance));
```

rss and heap stay flat; only the sqlite page count doubles (24.8 -> 49.5), so
the storage layer grows across sustain. A genuine defect in the storage/soak
path. Fix (not applied): trace why page_count doubles under assert/retract
churn in the sqlite arm.

### lsp-diags (contradicts the coordinator PASS)

Verbatim:
```
PASS  phase B0  dl --lsp --diag-db initialized over real stdio ...
FAIL  phase B1: the real LSP client never received both diagnostics for b.ts: READY
```

Throw site: `v6/tsv2/scripts/lsp-diags.sh:266`
```
265  await_log_line "$WORK/driver.log" "PASS appeared" 30 \
266    || fail "phase B1: the real LSP client never received both diagnostics for b.ts: $(cat "$WORK/driver.log")"
267  say "PASS  phase B1  $(grep 'PASS appeared' "$WORK/driver.log")"
```

`lsp_diag_driver.py` logs `READY` then never logs `PASS appeared`. Phases
A1-A4 (a.ts diag_v5 rows) pass and B0 initializes stdio, but the b.ts push
never delivers both diagnostics to the LSP client. Needs the v5 `dl` binary
(a build-step gap in a fresh worktree: copy `target/release/dl` from the base
repo or run `cargo build --release --bin dl`), then fails B1 every run. Fix
(not applied): the b.ts diag delivery over the real LSP stdio channel.

## 3. Legs deleted from the ledger (green 3/3)

| leg | 3/3 receipt |
|---|---|
| extraction-live | `EXTRACTION LIVE HOLDS`, exit 0, 3/3 (phases 1-9 all PASS). Green only because the release extractor binary was copied per the worktree setup; the ledger's `no release extractor` text was the parallel gate racing the `cargo build --release` on first run, not a code defect. |

`leak-soak` also passes 3/3 with a fresh clean `$TMPDIR` per run and is not
added to the ledger; it was already absent (see verdict table).

## 4. Flaky legs

| leg | result | sensitive to |
|---|---|---|
| serve-leak-soak | 2 pass / 1 fail (clean TMPDIR per run) | transient `setImmediate` handle pending at the resource-count sampling instant; the 20-cycle `names_with_growth(handles)` check (`serveLeak.test.ts:148`) races the event loop. Not the stale-TMPDIR class. Add to the allowlist as flaky. |

## 5. Fix-these-first ranking

Cheapest, lowest-risk real fix first.

| rank | leg | fix | why cheap |
|---|---|---|---|
| 1 | flagship | regenerate the golden (`FLAGSHIP_V5_WRITE=1`) | data regen, no code; the gate itself prints the command |
| 2 | getting-started | update doc block 24 to the new `rule-index unavailable:` text | one doc text diff, engine already correct |
| 3 | scale-floor | accept steady `[39,43]` as the expected stmts set | flatness proof already holds; only the pin is stale |
| 4 | golden-flex | remove the stale `json_object/2` excuse + exercise `json_patch/2` | two golden-content edits driven by `expected_absent/2` |
| 5 | rtkq-golden | stable-order the two `api_endpoint` rows in the assertion | sort or fix emission order; spans are already correct |
| 6 | roundtrip | repair print/parse round-trip for the mutual-recursion shape | one fixture's variant mismatch in the print/parse door |
| 7 | tsv2-test | fix the sh-host zero-row decode | one decode path; needs `gen_emitted/` present |
| 8 | plunit | trace the 6 plunit failures | compiler-churn cluster, one change likely drives several |
| 9 | compile-speed | verify shift is real, then re-baseline | depends on plunit root cause |
| 10 | lsp-diags | fix b.ts diag delivery over stdio | hardest runtime trace |
| 11 | memory-soak | trace sqlite page-count growth | storage-layer, deeper |

Flaky, separate lane: serve-leak-soak (allowlist as flaky, do not block on it).
