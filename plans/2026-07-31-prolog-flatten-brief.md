# Prolog folder-cycle repair: the 9-move flatten (luna brief, 2026-07-31)

User-approved execution of the auto-factorization verdict's 9-move repair
(plans/2026-07-31-auto-factorization-verdict.md section 4c). NO-COMMIT flow:
leave the tree dirty, coordinator reviews and commits. Work alone, no
parallelism.

## The change
Move exactly these 9 files from v6/prolog/compile/ to v6/prolog/:

  3_clock_check.pl  6_profile.pl  analyze.pl  compile.pl  emit_ts.pl
  lower.pl  print_dl.pl  strat.pl  sweep.pl

PURE MOVES: file contents change only where a path or module reference
requires it. No renames (the numbering table is a separate ruled arc), no
refactors, no comment changes, no new comments.

compile/ remains, holding registry.pl, parse_dl.pl, oracle_dump.pl,
1_emit_registry_docs.pl, 2_emit_cli_inventory.pl, scripts/, test/, out/, and
the docs. After the move every surviving cross-folder dependency must point
v6/prolog -> v6/prolog/compile (that directional invariant IS the repair;
verify it by grepping use_module paths both directions and state the counts).

## Reference hunt (the real work)
Update every LIVE reference to the moved files' paths:
- use_module/consult/ensure_loaded across v6/prolog/**（including
  compile/test/*.pl relative loads and conformance harnesses like
  dl6_oracle.pl, bop_check.pl, sweep drivers).
- Shell scripts: v6/prolog/compile/scripts/*.sh (compile_dl6.sh,
  1_compile_speed.sh, roundtrip.sh, text_door_receipt.sh, others found by
  grep) load compile.pl / sweep.pl / 6_profile.pl by path.
- v6/justfile recipes that cd into prolog/compile and name these files.
- TS-side scripts (v6/tsv2/scripts/*.ts, *.sh) that spawn swipl with paths.
- prolog_lint.pl cluster entries if they name moved files.
Find them by grep for each of the 9 basenames, not from memory. RECORD docs
(ARCH.pl comments, plans/, chat_log/, *.md) are untouched; the atlas
regenerates in a later arc.

## Sandbox facts (from today's lanes)
- Prefix every receipt with LC_ALL=en_US.UTF-8 (C-locale crashes the NFC/NFD
  fixture; environmental).
- Socket-binding tests cannot run (listen EPERM); use fingerprint-unchanged.
- node_modules is pre-seeded by the coordinator; if something is missing,
  STOP AND REPORT.
- No git write commands at all.

## Receipts (all via v6/tools/run-capped.sh, stated budgets, paste outputs)
Run a BASELINE of each before any move, then after:
- conformance (expect 281), plunit (expect 271), TEXT_DOOR (196/196/0),
  roundtrip, sweep both modes (196/195/0, crash=0), compile-speed gate
  (regressions=0), prolog-lint (findings=1 baseline=1 OK).
- The directional grep receipt: zero use_module references from compile/
  files to v6/prolog files after the move.
- git diff --stat plus confirmation that content diffs are path lines only
  (show the per-file diff for 3 moved files).

## Report shape
Base sha verified; the move list; reference-site count per category; both
receipt runs; the directional invariant counts; deviations loudly; STOP on
any non-sandbox surprise.
