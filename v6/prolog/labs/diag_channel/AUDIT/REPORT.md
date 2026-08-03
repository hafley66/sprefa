# prolog audit: REPORT

## Base proof

`git merge --ff-only 8907a040` from the worktree root: `Already up to date.`.
No blockage; full audit performed.

## Restore proof

Every mutation recorded a shasum-256 before and after, was backed to
`/tmp/*.bak`, and verified byte-identical on restore.

| target | before | after restore | result |
|---|---|---|---|
| `labs/diag_channel/` (finding-1 move) | sha256 per file in `/tmp/labsum_before.txt` | diff clean | exact |
| `labs/diag_channel/` (finding-3 move) | `/tmp/labsum_f3_before.txt` | diff clean | exact |
| `diag.pl` (sab A, lsp off-by-one) | `516cc004...` | `516cc004...` | exact |
| `diag.pl` (sab B, message divergence) | `516cc004...` | `516cc004...` | exact |
| `swi-scc/.../scc.pl` (sab, remove outer sort) | `a36909eb...` | `a36909eb...` | exact |

Final `git status --short` in audit-pl shows only
`?? v6/prolog/labs/diag_channel/AUDIT/`; `git -C swi-scc status --short` is
empty. No sabotage left live.

Seeded finding 2 (uri) is re-confirmed by the same functional run that
confirmed finding 1; both verified below.

## 1. Sabotage table

Baselines green first: `diag.test.pl` 5/5 exit 0; `scc.test.pl` all pass exit 0.

| suite | defect introduced | tests red | exit | covered? |
|---|---|---|---|---|
| diag.test.pl | `lsp_position/4` off by one the other direction (`Line - 1` -> `Line`) | 2 (`one_based_to_zero_based`, `parse_error_position_is_exact_in_record`) | 1 | yes |
| diag.test.pl | JSON `message` diverged for all terms (`SAB-` prefix in `diag_message`) | 1 (`json_message_equals_human_line`, whole-inventory walk) | 1 | yes |
| scc.test.pl | removed outer `sort/2` in `group_by_lowlink` | 1 (`tarjan_matches_kosaraju`, diamond forall iteration) | 1 | yes |

Seed-receipt discrepancy: the contract said removing the outer `sort` turns
"3 tests red". Re-verification showed exit 1 but only 1 test red (the
`tarjan_matches_kosaraju` forall, failing on the `diamond` iteration); the
other two tests stayed green. Nonzero exit and red are re-confirmed; the "3"
count does not reproduce.

## 2. The two-renderer property under attack

`diag_record/3` builds `message` from `diag_message(Term, Message)` which is
`message_to_string(Term, Message)`. The same predicate, same term, drives the
human line. The one umbrella `prolog:message(unsupported_construct(_))` in
`0_refusal_messages.pl:23` (plus the `dl_parse_error` clause at
`compile/parse_dl.pl:198`) is the single renderer; the channel adds no second
table and no text construction of its own. The sabotage B (all-`SAB-` prefix)
went red across the whole inventory walk, so the rail is effective.

Attack results:
- A signature not present in `refusal_inventory/1` still renders through the
  same `prolog:message` umbrella via the generic fallback for BOTH channels,
  so JSON and human cannot diverge even out-of-inventory. It degrades to the
  generic "compiler refused rule 'X' (X)" in both, which is a quality issue,
  not a divergence.
- No path in the channel constructs message text itself. Property holds.

## 3. Cost on the success path

`just compile-speed` from `v6/prolog`:
`COMPILE_SPEED programs=4 phases=24 regressions=0 improvements=0 OK`.

`8907a040` changed only `compile.pl`, `compile/parse_dl.pl`, the two lab files
and `plunit_tests.pl`; it did not touch the compile-speed baseline tsv. The
baseline therefore predates the wiring, and regressions=0 means the diag
channel added zero inference cost on the success path. Emitter call sites
(`compile.pl:208,234,241`) all sit in catch/failure handlers; `compile_dl6`
and `compile_program_phases` never call it on success. `statement_location_*`
and `dl6_span` are lazy, invoked only when a diagnostic asks. Claim confirmed.

## 4. Position correctness

Mechanism confirmed wrong. `statement_location_for_reason/4`
(`compile/parse_dl.pl:210`) sorts the relation references in the reason, then
`statement_location_for_reference/4` returns the FIRST `source_statement_fact`
(in parse/assertion order) whose statement contains the relation as a
`sub_term`. When the offending relation is mentioned by an earlier valid
statement, the diagnostic points at that earlier statement, not the offender.

All 12 `.dl6` probes compiled through `compile_dl6`; emitted `range.start`
checked against the file by hand (`nl -ba`).

| probe | refusal | offender line | emitted line | verdict |
|---|---|---|---|---|
| p1 / s2 | finalize_in_level_rule | 3 | 3 | CORRECT |
| s1 | latest_in_level_rule | 3 | 3 | CORRECT |
| s3 | pre_in_level_rule | 3 | 3 | CORRECT |
| s5 | log_on_level_headed_rel | 3 | 3 | CORRECT |
| s4 | keep_on_non_log_rel | 1 | 1 | CORRECT |
| p2 | finalize_in_level_rule | 5 | 4 | WRONG |
| p6 | latest_in_level_rule | 5 | 4 | WRONG |
| d3 | pre_in_level_rule | 5 | 4 | WRONG |
| d5 | log_on_level_headed_rel | 5 | 4 | WRONG |
| p5 | now_in_level_rule | 5 | 1 (fallback 0,0) | fallback, visible |
| p3, p4 | dl_parse_error | exact | exact | CORRECT |

Every decoy case resolved to the first rule mentioning the relation instead
of the rule that actually offends. 5 signatures triggered with a real
position (finalize, latest, pre, log_on_level_headed, keep_on_non_log): all
single-statement controls correct, all 4 decoy variants WRONG. Now_in_level_rule
falls back to (0,0), visible per the contract's rule. Triggered 4 wrong out
of 4 decoy-constructible resolvable signatures. The report's "resolve to a
real position" is true; "resolve to the OFFENDING statement" is false when
the relation is shared.

## 5. Couplings, state, and failure modes

- Coupling (seeded finding 1, re-confirmed live): moving `labs/diag_channel/`
  aside makes the refusal path throw `compile:emit_diag_file/2: Unknown
  procedure`; exit stays 2 but the structured diagnostic is replaced by an
  internal existence error. `compile.pl:35` depends on a dir the lab protocol
  deletes on landing.
- uri (seeded finding 2, re-confirmed): `"uri":"/tmp/audit_scratch/broken.dl6"`
  is a bare path. Beyond that, the record's `range` is a zero-width point
  (start == end, statement-start granularity, never a token span), which is
  valid LSP but under-informative. `severity: 1` (Error), string `code`,
  string `source` are LSP-conformant.
- State: `diag_stream_open/0` opens DL6_DIAG_JSONL once per process and never
  closes the handle (module comment admits a per-process active stream). It is
  guarded by `nb_current`, so a multithreaded server gets one open append
  handle per emitting thread. `set_diag_file/1` mutates a thread-local blame
  each emission; consistent within an `emit_diag_file/2` call.
- Unwritable DL6_DIAG_JSONL failure mode: setting it to a path in a 0555
  directory turns a refusal into a raw `open/3: Permission denied`; exit is
  still 2, indistinguishable from a normal refusal, and the actual diagnostic
  is swallowed (nothing written anywhere, no fallback to stderr).

## 6. scc_extract independent verification

- Verbatim: `diff` of the extracted `scc/2`..`v_in_stack` bodies against
  `/opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/library/clp/clpfd.pl`
  lines 5892-5962 is clean. The module adds only attribution,
  module/export scaffolding, and the wrapper.
- 11-shape agreement: `scc.test.pl` passed green (all three differential tests
  over all 11 shapes).
- Fuzz re-run with a different method/seed: 4 sizes x 3 densities x 30 seeds =
  360 graphs, `DISAGREEMENTS=0` against the hand-written Kosaraju.
- O(V^2) cause confirmed by two-size measurement on a chain:

| N | tarjan wall ms / inferences | kosaraju wall ms / inferences |
|---|---|---|
| 500 | 43 / 780,472 | 4 / 67,912 |
| 1000 | 167 / 3,059,972 | 7 / 143,766 |
| 2000 | 659 / 12,118,972 | 16 / 303,476 |

Tarjan inferences scale 3.92x and 3.96x per size doubling (quadratic);
Kosaraju 2.12x and 2.11x (linear). The `==` identity scan over the paired
atom list, run at every successor lookup and vertex map, is the measured
cause. The lane's "not worth using" verdict and its stated cause both hold.

## Ranked findings

1. WRONG POSITION (highest severity): a refusal names a relation, and when
   that relation is mentioned by an earlier valid statement the diagnostic
   resolves to that earlier statement, not the offender. Confirmed live in
   four signatures (finalize, latest, pre, log_on_level_headed_rel). This is
   the lie the editor draws: a fallback is visibly a fallback, a wrong
   position is not. Without a decoy the position is correct, which is why the
   lane's own single-statement test (`refusal_resolves_real_statement_position`)
   never catches it.
2. Seeded finding 1 confirmed: `compile.pl:35` hard-depends on a lab directory
   the repo deletes on landing; refusal path loses the diagnostic when it dies.
3. Seeded finding 2 confirmed: `uri` is a bare path, not `file://`.
4. Failure mode: unwritable DL6_DIAG_JSONL swallows the diagnostic and exits 2
   with a raw `open/3` permission error.
5. Minor: per-process open stream never closed; multiple handles under
   threads.

## What I could not check

- I could not trigger all 58 "resolvable by mechanism" signatures through real
  programs; the mechanism is shared (same resolver), so I stopped at 5
  signatures plus the fallback and parse-error classes, all running the same
  `statement_location_for_reason` path. A signature that carries a
  relation-reference different in shape from these could behave differently
  and was not probed.
- I could not prove fuzz agreement for all graphs; the re-run covers a finite
  360-graph sample from my own generator, not the lane's exact 360.
- I did not byte-diff emitted TypeScript with the channel present versus
  absent on a successful compile; I inferred zero success-path cost from
  baseline regressions=0 and from `lower.pl`/`emit_ts.pl` being untouched by
  the commit, rather than measuring emitted bytes directly.
- I could not confirm exact LSP client acceptance of the envelope-shaped
  record (uri smuggled into the Diagnostic object); this needs a running
  client and is stated as a design departure, not tested against one.
