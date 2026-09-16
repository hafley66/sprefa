# report: dl6 text-door fact seam (finding F1)

## what changed

Bodiless ground clauses in `.dl6` now partition out of the rules list into the
`Initial` seed argument of the fixture term, so they compile through the text
door as seed rows. Non-ground bodiless clauses keep refusing.

| file | change |
|---|---|
| `v6/prolog/compile.pl` | exported `dl6_seeded_form/3`; `compile_dl6/2` partitions `prog(Decls, Rules)` into facts vs real rules, passes `fixture(Name, prog(Decls, RealRules), Facts, [], [])`; `partition_dl6_facts/3` preserves order; `dl6_fact/2` recognizes `Head <- true` ground and bare ground terms |
| `v6/prolog/compile/scripts/bop_check.pl` | imports and calls `dl6_seeded_form/3` before building its own fixture term (step 3; it does parse .dl6 text) |
| `v6/prolog/6_profile.pl` | `compile_dl6_profiled/3` threads the same partition into fixture construction and `boot_statements/5` (step 4 applies) |
| `v6/prolog/compile/test/plunit_tests.pl` | added `fact_seeding` block: `dl6_fact_seeds_initial`, `dl6_fact_nonground_refuses`, `dl6_fact_derives` |

## RED receipt (fail-first, run before the fix)

Probe `PROBE/probe_fact_seed.dl6` through
`bash v6/prolog/compile/scripts/compile_dl6.sh PROBE/probe_fact_seed.dl6 /tmp/probe.ts`:

```
{"code":"level_rule_no_positive_body/1","message":"PROBE/probe_fact_seed.dl6:4: unsupported_construct: compiler refused rule 'level_rule_no_positive_body' for rel 'max_run/1' (level_rule_no_positive_body)","range": {"end": {"character":0,"line":3},"start": {"character":0,"line":3}},"severity":1,"source":"dl6","uri":"file://PROBE/probe_fact_seed.dl6"}
ERROR: [Thread main] -g compile_dl6('PROBE/probe_fact_seed.dl6', '/tmp/probe.ts'): PROBE/probe_fact_seed.dl6:4: unsupported_construct: compiler refused rule 'level_rule_no_positive_body' for rel 'max_run/1' (level_rule_no_positive_body)
```

## deviations

Reality deviated from the brief's receipt for the parsed shape of a bodiless
clause. The brief stated `normalize_host_rule/3`'s catch-all (parse_dl.pl:297)
passes a bare head term through unchanged, so facts would arrive as bare ground
terms. In fact the grammar`s `rule_stmt` (parse_dl.pl:1051) fills an absent body
with `true`, so a fact arrives in `Rules` as `Head <- true` (e.g.
`max_run(2) <- true`), which is a `(_ <- _)` form and would have failed the
brief's `"not of the form (_ <- _)"` fact test. `dl6_fact/2` therefore detects
`Head <- true` with `ground(Head)` as a fact and extracts the head; a bare
ground term clause remains as a fallback. Everything else follows the brief
unchanged.

Non-ground bodiless clause observed refusal code: `level_rule_no_positive_body`
(any `unsupported_construct` suffices per brief; this is the exact one).

## validation

`dl6_fact_seeds_initial` now passes (asserts the emitted boot row
`INSERT OR IGNORE INTO "max_run" ("limit_lines") VALUES (?)` with params `[2]`).

Verbatim outputs below captured at validation time (all from `cd v6`).

```
just conformance   -> 294 PASS / 0 FAIL (exit 0)
just text-door     -> TEXT_DOOR compiled=206 byte_identical=206 failures=0
just plunit        -> 327 passed / 0 failed (324 existing + 3 new) (exit 0)
bash prolog/compile/scripts/compile_dl6.sh ../PROBE/probe_fact_seed.dl6 /tmp/probe_out.ts
                   -> PROBE_COMPILES (wrote /tmp/probe_out.ts)
bop_check smoke    -> BOP_CHECK_FILE=.../probe_fact_seed.dl6 bop_check.pl -> exit 0 (clean)
```

