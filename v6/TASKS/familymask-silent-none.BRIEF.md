# familymask-silent-none (F2): unknown family names become a named stop, never silence

Repo: sprefa. Base sha: 988e2b514204735869ce2964008bdbea8ad91bc8 (origin/main).
FIRST ACTION: `git merge --ff-only 988e2b514204735869ce2964008bdbea8ad91bc8`. Failure = STOP AND REPORT.

Issue: `issues/familymask-silent-none/item.md`. Golden that pins the defect:
`v6/tsv2/goldens/scip_combo/7_door_skew_family.dl6` (its header comment is an accurate mechanism writeup).

Files you own:
- `v6/sprefa-engine-rs/src/hosts.rs` (the mask parser arm only, ~:855-870)
- `v6/sprefa-extract/src/bin/extract.rs` (`parse_mask` ~:482-494 and the doc comment above `parse_arms` ~:458-461)
- `v6/tsv2/goldens/scip_combo/7_door_skew_family.dl6` and `8_gate.sh` (grading of program 7 only)
- tests colocated with the two parsers
Forbidden: every other file. Do not touch `family_mode`, `parse_arms` behavior, the other six scip_combo programs, or any other golden.

## The defect

`hosts.rs:864` `_ => {}` swallows any `--family` value outside cst/type/types/call/df, so
`--family diet_scip` leaves `FamilyMask::NONE`, `dispatch` extracts nothing, and the host
returns rc=0 with zero fact lines. Ten lines up (`hosts.rs:846`) an unknown FLAG is a named
stop. Same parser, opposite contracts. The CLI twin `parse_mask` (bin/extract.rs:490) has the
same silent catch-all.

## The fix

1. `hosts.rs` mask arm: `_ => return Err(named(format!("family \`{name}\` is not a known family; in-process families are cst, type, call, df")))` — mirror the flag arm's wording style. NOTE: `scip` and `diet_scip` are real MODE names in the CLI (`family_mode`, bin/extract.rs:182) but are NOT linked in-process; they take this same named stop (say so in the message when the name is scip/diet_scip: "mode `X` is not linked in-process").
2. `bin/extract.rs` `parse_mask`: return `Result`, refuse unknown names the same way. `family_mode` consumes scip/diet_scip before `parse_mask` runs, so mode names never reach it legitimately; anything unknown that arrives is a typo. Update the doc comment above `parse_arms` (bin/extract.rs:458-461): its "phase-1 mask can afford to ignore noise" argument is what this fix deletes; rewrite it to state the new contract.
3. Golden flip: `7_door_skew_family` STAYS a pinned disagreement, but the pin changes. Old pin: Rust door rc=0 with zero rows (silence). New pin: Rust door stops with the named family error; TS door still answers its 2 `resolved_edge` rows (diet_scip is a real mode there). Update the dl6 header comment and the `8_gate.sh` grading of program 7 so the gate goes RED if the silent-zero-rows behavior ever returns. Touch only program 7's grading in 8_gate.sh.

## Receipts (paste outputs, check every rc explicitly, never pipe a gate through tail alone)

```bash
cargo test -p sprefa-engine-rs           # from v6/sprefa-engine-rs; add a test: unknown family name -> Err naming it
cargo test -p sprefa-extract             # add a test: parse_mask refuses an unknown name
bash v6/sprefa-engine-rs/grade.sh        # RUST-GRADE per fixture; run after cargo build, never on a stale binary
bash v6/tsv2/goldens/scip_combo/8_gate.sh  # run THREE times back-to-back; all three verdicts identical
```

ALWAYS `cargo build` before any gate that runs a binary (stale-binary false-green is failure-modes #49).

Style: no eprintln! in src/**; comments state only constraints the code cannot show; no change-log narrative in comments. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime. The word "refusal" stays out of prose; "named stop" is the phrase.

Commit on your branch. Never push. Never commit to main.
