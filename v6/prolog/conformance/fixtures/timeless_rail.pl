% fixtures/timeless_rail.pl : labs/timeless_rail.pl's 18 checks promoted into
% the shared fixture format (AGGREGATE.md 5c), re-graded under rulings.pl
% where the lab's own reference interpreter disagreed, and reshaped where the
% shared engine's actual semantics forced a different rule body than the
% lab's literal transcription. Owner: this file only.
%
% not promoted (lab-internal machinery / unimplemented features, one line each):
%   stratification_assigns_levels    - asserts the LAB's own strata/1 level
%     assignment (a Prolog data structure internal to timeless_rail.pl's
%     custom stratifier). engine.pl has no stratifier to introspect and no
%     expectation type for "computed stratum of a rel"; only final/deltas/
%     ticks/throws over an executed program are gradable here.
%   stratifier_rejects_negative_cycle - builds a SYNTHETIC unstratifiable
%     program (a rel negating itself) purely to test the lab's own
%     stratifier's rejection path. engine.pl does not statically stratify at
%     all (see the ENGINE LIMITATION note below); it evaluates all `<-`
%     rules in one combined per-round loop, so there is no `throws(...)`
%     this maps to.
%   program_is_timeless - mechanically scans the program's OWN rule terms
%     for absence of <+/pre/now/clock/tick/delta/next functors. That is a
%     property of the fixture author's program text, trivially true by
%     construction for every fixture in this file (all rules are `<-`), and
%     not an executable behavior the shared engine grades.
%   ratchet_baseline_present_and_consistent - half of this check is
%     `fd_violations/2`, the lab's own static functional-dependency scan
%     over a raw fact list. engine.pl's `keyed(Ref, Positions)` declaration
%     only guards EDGE-write conflicts (check_occurrence_conflicts, fired
%     from process_occurrences); there is no static FD check over facts or
%     InitialRows for level-only programs. The surviving half (the four-row
%     v5 baseline table matches) is not independently interesting: those
%     exact rows are already InitialRows in every eprintln_baseline fixture
%     below.
%   ratchet_fd_rejects_duplicate_key - same reason: entirely about
%     `fd_violations/2`, which has no engine-side equivalent.
%
% ── ENGINE DEFECT, FOUND HERE, SINCE FIXED (history, kept as the receipt) ────
% The original promotion found engine.pl's joint plain_fixpoint unsound for
% not/1 over a DERIVED rel (a row admitted against an incomplete negated set
% was never retracted). The coordinator replaced the joint loop with
% stratified evaluation (negated and aggregated rels sit strictly below
% their consumers; not_stratified thrown on cycles). After the fix the
% coordinator RESTORED what the workaround had removed: eprintln_counted
% negates the derived eprintln_waived faithfully again (the inlined
% base-fact twin is gone), and over_baseline_gate_blocks_commit_only plus
% tightened_baseline_catches_regrowth grade gate_exit/check_exit in full.
%
% ── bag vs set aggregation (q7) ──────────────────────────────────────────────
% The lab's own reference interpreter sorts (group, value) pairs before
% counting (timeless_rail.pl:290-291), i.e. COUNT(DISTINCT), the REJECTED
% READING under q7 (ruling: bag of body derivations, v5-SQL-compatible).
% Checked every promoted aggregate below against this: none of the rail's
% own canned data has two GENUINELY DIFFERENT body derivations collapsing
% onto one (group, counted) pair -- every eprintln_hit is a distinct line,
% every unwrap_hit in one unwrap_run call is a distinct line -- so bag and
% set give the IDENTICAL count on every scenario this lab feeds (confirmed
% empirically for every fixture below). This matches review_timeless_rail.md
% A1's own finding ("insensitive to the difference on real data"). The
% fail-pre-fix two-hits-one-line fixture that actually distinguishes bag
% from set already lives in fixtures/json_arm.pl as
% count_is_bag_of_derivations and is not duplicated here.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ (a) the clean repo state ════════════════════════════════════════════════

fixture(clean_state_no_diags,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))),
         (unwrap_count(Path, count(LineNo)) <- unwrap_hit(Path, LineNo, _, _), changed_file(Path)),
         (diag(Path, LineNo, warning, "unwrap-budget",
               concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
              unwrap_hit(Path, LineNo, Col, EndCol), changed_file(Path),
              unwrap_count(Path, Total), Total > 10) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    unwrap_hit("src/engine.rs", 107, 8, 16),
    unwrap_hit("src/engine.rs", 114, 8, 16),
    unwrap_hit("src/engine.rs", 121, 8, 16),
    changed_file("src/engine.rs") ],
  [],
  [ final(diag/7, []) ]).

% Gate/exit rels are SAFE to assert exactly here (see ENGINE LIMITATION note):
% gate_blocked and any_diag never become non-empty anywhere in this run, so
% the round-1 vacuous-negation read matches the true final value with no
% spurious extra row.
fixture(clean_state_gate_and_exit_zero,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))),
         (unwrap_count(Path, count(LineNo)) <- unwrap_hit(Path, LineNo, _, _), changed_file(Path)),
         (diag(Path, LineNo, warning, "unwrap-budget",
               concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
              unwrap_hit(Path, LineNo, Col, EndCol), changed_file(Path),
              unwrap_count(Path, Total), Total > 10),
         (gate_blocked(Stage) <-
              diag(_, _, Severity, Code, _, _, _), diag_stage(Code, Stage),
              gate_threshold(Stage, MinRank), severity_rank(Severity, Rank), Rank >= MinRank),
         (gate_exit(Stage, 2) <- gate_blocked(Stage)),
         (gate_exit(Stage, 0) <- gate_threshold(Stage, _), not(gate_blocked(Stage))),
         (any_diag(Name) <- program(Name), diag(_, _, _, _, _, _, _)),
         (check_exit(Name, 2) <- any_diag(Name)),
         (check_exit(Name, 0) <- program(Name), not(any_diag(Name))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    program("no-new-eprintln"),
    severity_rank(warning, 1), severity_rank(error, 2),
    gate_threshold("agent-turn", 2), gate_threshold("commit", 1),
    diag_stage("eprintln-exceeded", "agent-turn"), diag_stage("eprintln-exceeded", "commit"),
    diag_stage("eprintln-new-file", "agent-turn"), diag_stage("eprintln-new-file", "commit"),
    diag_stage("unwrap-budget", "agent-turn"),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    unwrap_hit("src/engine.rs", 107, 8, 16),
    unwrap_hit("src/engine.rs", 114, 8, 16),
    unwrap_hit("src/engine.rs", 121, 8, 16),
    changed_file("src/engine.rs") ],
  [],
  [ final(gate_blocked/1, []),
    final(gate_exit/2, [ gate_exit("agent-turn", 0), gate_exit("commit", 0) ]),
    final(check_exit/2, [ check_exit("no-new-eprintln", 0) ]) ]).

% EXACT ROW SET: the waiver range join, both sides. cli/mod.rs:88 has a
% trailing waiver on its OWN line and is the only hit excluded from counted;
% every other hit reaches eprintln_counted and folds into eprintln_count.
fixture(waiver_range_join_exact_rows,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)) ]),
  [ eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88) ],
  [],
  [ final(eprintln_waived/2, [ eprintln_waived("src/cli/mod.rs", 88) ]),
    final(eprintln_counted/2,
          [ eprintln_counted("src/config.rs", 44),
            eprintln_counted("src/daemon/client.rs", 118),
            eprintln_counted("src/daemon/client.rs", 133),
            eprintln_counted("src/setup/vscode.rs", 61),
            eprintln_counted("src/setup/wire.rs", 92) ]),
    final(eprintln_count/2,
          [ eprintln_count("src/config.rs", 1),
            eprintln_count("src/daemon/client.rs", 2),
            eprintln_count("src/setup/vscode.rs", 1),
            eprintln_count("src/setup/wire.rs", 1) ]) ]).

% ═══ (b) one new eprintln beyond the ratchet ═════════════════════════════════

% EXACT ROW SET: the whole diag product for this state is one row, at the
% file head (line 1), warning severity, the exceeded code, both span columns
% at their declared default, and the interpolated "3 ... allows 2" message.
fixture(over_baseline_diag_exact_rows,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/daemon/client.rs", 175) ],
  [],
  [ final(diag/7,
          [ diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded",
                 '3 counted eprintln! hits; the grandfathered baseline allows 2. Convert to tracing, or waive the line with @eprintln-ok: <reason>',
                 none, none) ]) ]).

fixture(over_baseline_count_row,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)) ]),
  [ eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/daemon/client.rs", 175) ],
  [],
  [ final(eprintln_count/2,
          [ eprintln_count("src/config.rs", 1),
            eprintln_count("src/daemon/client.rs", 3),
            eprintln_count("src/setup/vscode.rs", 1),
            eprintln_count("src/setup/wire.rs", 1) ]) ]).

% gate_exit/check_exit RESTORED (coordinator, post-stratification-fix): this
% was the unsafe scenario under the old joint fixpoint (a spurious
% gate_exit("commit",0) coexisted with the correct 2 row); stratified
% negation now grades the full exit table. gate_blocked exactly [commit]
% proves the severity split: the same diag routes to both stages via
% diag_stage, but only "commit" (min_rank 1) is met by a warning (rank 1);
% "agent-turn" (min_rank 2) is not, and its absence from this exact set is
% the whole point.
fixture(over_baseline_gate_blocks_commit_only,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))),
         (gate_blocked(Stage) <-
              diag(_, _, Severity, Code, _, _, _), diag_stage(Code, Stage),
              gate_threshold(Stage, MinRank), severity_rank(Severity, Rank), Rank >= MinRank),
         (any_diag(Name) <- program(Name), diag(_, _, _, _, _, _, _)),
         (gate_exit(Stage, 2) <- gate_blocked(Stage)),
         (gate_exit(Stage, 0) <- gate_threshold(Stage, _), not(gate_blocked(Stage))),
         (check_exit(Name, 2) <- any_diag(Name)),
         (check_exit(Name, 0) <- program(Name), not(any_diag(Name))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    program("no-new-eprintln"),
    severity_rank(warning, 1), severity_rank(error, 2),
    gate_threshold("agent-turn", 2), gate_threshold("commit", 1),
    diag_stage("eprintln-exceeded", "agent-turn"), diag_stage("eprintln-exceeded", "commit"),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/daemon/client.rs", 175) ],
  [],
  [ final(gate_blocked/1, [ gate_blocked("commit") ]),
    final(any_diag/1, [ any_diag("no-new-eprintln") ]),
    final(gate_exit/2, [ gate_exit("agent-turn", 0), gate_exit("commit", 2) ]),
    final(check_exit/2, [ check_exit("no-new-eprintln", 2) ]) ]).

% ═══ (c) the fix: level retraction as a set difference ═══════════════════════

% Adapted from fix_by_waiver_removes_diag_and_violation: the fixture format
% is one program per fixture, so the lab's "before minus after" comparison
% (two separate evaluations) cannot be one fixture. "Before" (the exceeded
% diag) is already graded exactly in over_baseline_diag_exact_rows; this
% fixture grades "after" -- the fix (a whole-line waiver on the line ABOVE
% the new hit, the tight edge of the range join: 174 >= 175 - 1) returns
% every diag-derived rel to the same empty state as the clean scenario, and
% the newly waived hit shows up in eprintln_waived alongside the pre-existing
% one. gate_exit/check_exit ARE safe here (same reasoning as
% clean_state_gate_and_exit_zero: nothing is ever blocked in this run).
fixture(fix_by_waiver_returns_to_clean,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))),
         (gate_blocked(Stage) <-
              diag(_, _, Severity, Code, _, _, _), diag_stage(Code, Stage),
              gate_threshold(Stage, MinRank), severity_rank(Severity, Rank), Rank >= MinRank),
         (gate_exit(Stage, 2) <- gate_blocked(Stage)),
         (gate_exit(Stage, 0) <- gate_threshold(Stage, _), not(gate_blocked(Stage))),
         (any_diag(Name) <- program(Name), diag(_, _, _, _, _, _, _)),
         (check_exit(Name, 2) <- any_diag(Name)),
         (check_exit(Name, 0) <- program(Name), not(any_diag(Name))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    program("no-new-eprintln"),
    severity_rank(warning, 1), severity_rank(error, 2),
    gate_threshold("agent-turn", 2), gate_threshold("commit", 1),
    diag_stage("eprintln-exceeded", "agent-turn"), diag_stage("eprintln-exceeded", "commit"),
    diag_stage("eprintln-new-file", "agent-turn"), diag_stage("eprintln-new-file", "commit"),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/daemon/client.rs", 175),
    waiver_block_comment("src/daemon/client.rs", 174) ],
  [],
  [ final(diag/7, []),
    final(gate_blocked/1, []),
    final(gate_exit/2, [ gate_exit("agent-turn", 0), gate_exit("commit", 0) ]),
    final(check_exit/2, [ check_exit("no-new-eprintln", 0) ]),
    final(eprintln_waived/2,
          [ eprintln_waived("src/cli/mod.rs", 88),
            eprintln_waived("src/daemon/client.rs", 175) ]) ]).

% ═══ the no-baseline branch ═══════════════════════════════════════════════════

% EXACT ROW SET: two diags, each at its own hit line (203, 288), not at
% line 1 -- the no-baseline rule flags every hit precisely.
fixture(new_file_diag_at_hit_line_exact_rows,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/cli/health.rs", 203),
    eprintln_hit("src/cli/health.rs", 288) ],
  [],
  [ final(diag/7,
          [ diag("src/cli/health.rs", 203, warning, "eprintln-new-file",
                 "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
                 none, none),
            diag("src/cli/health.rs", 288, warning, "eprintln-new-file",
                 "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
                 none, none) ]) ]).

% The two diag rules are mutually exclusive by construction: an unbaselined
% file never also produces an eprintln-exceeded row.
fixture(new_file_no_exceeded_diag,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))) ]),
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88),
    eprintln_hit("src/cli/health.rs", 203),
    eprintln_hit("src/cli/health.rs", 288) ],
  [],
  [ final(eprintln_count/2,
          [ eprintln_count("src/cli/health.rs", 2),
            eprintln_count("src/config.rs", 1),
            eprintln_count("src/daemon/client.rs", 2),
            eprintln_count("src/setup/vscode.rs", 1),
            eprintln_count("src/setup/wire.rs", 1) ]) ]).

% ═══ (d) the aggregation + interpolation rail ════════════════════════════════

fixture(unwrap_aggregate_and_interpolation,
  prog([],
       [ (unwrap_count(Path, count(LineNo)) <- unwrap_hit(Path, LineNo, _, _), changed_file(Path)),
         (diag(Path, LineNo, warning, "unwrap-budget",
               concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
              unwrap_hit(Path, LineNo, Col, EndCol), changed_file(Path),
              unwrap_count(Path, Total), Total > 10) ]),
  [ unwrap_hit("src/engine.rs", 107, 8, 16), unwrap_hit("src/engine.rs", 114, 8, 16),
    unwrap_hit("src/engine.rs", 121, 8, 16), unwrap_hit("src/engine.rs", 128, 8, 16),
    unwrap_hit("src/engine.rs", 135, 8, 16), unwrap_hit("src/engine.rs", 142, 8, 16),
    unwrap_hit("src/engine.rs", 149, 8, 16), unwrap_hit("src/engine.rs", 156, 8, 16),
    unwrap_hit("src/engine.rs", 163, 8, 16), unwrap_hit("src/engine.rs", 170, 8, 16),
    unwrap_hit("src/engine.rs", 177, 8, 16), unwrap_hit("src/engine.rs", 184, 8, 16),
    changed_file("src/engine.rs") ],
  [],
  [ final(unwrap_count/2, [ unwrap_count("src/engine.rs", 12) ]),
    final(diag/7,
          [ diag("src/engine.rs", 107, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 114, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 121, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 128, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 135, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 142, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 149, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 156, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 163, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 170, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 177, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16),
            diag("src/engine.rs", 184, warning, "unwrap-budget", '12 non-test unwraps in a changed file', 8, 16) ]) ]).

% The changed_file join sits INSIDE the aggregate rule: an unchanged file
% with the same 12 hits produces no count row and no diag.
fixture(unwrap_unchanged_file_silent,
  prog([],
       [ (unwrap_count(Path, count(LineNo)) <- unwrap_hit(Path, LineNo, _, _), changed_file(Path)),
         (diag(Path, LineNo, warning, "unwrap-budget",
               concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
              unwrap_hit(Path, LineNo, Col, EndCol), changed_file(Path),
              unwrap_count(Path, Total), Total > 10) ]),
  [ unwrap_hit("src/engine.rs", 107, 8, 16), unwrap_hit("src/engine.rs", 114, 8, 16),
    unwrap_hit("src/engine.rs", 121, 8, 16), unwrap_hit("src/engine.rs", 128, 8, 16),
    unwrap_hit("src/engine.rs", 135, 8, 16), unwrap_hit("src/engine.rs", 142, 8, 16),
    unwrap_hit("src/engine.rs", 149, 8, 16), unwrap_hit("src/engine.rs", 156, 8, 16),
    unwrap_hit("src/engine.rs", 163, 8, 16), unwrap_hit("src/engine.rs", 170, 8, 16),
    unwrap_hit("src/engine.rs", 177, 8, 16), unwrap_hit("src/engine.rs", 184, 8, 16),
    changed_file("src/engine.rs"),
    unwrap_hit("src/db.rs", 107, 8, 16), unwrap_hit("src/db.rs", 114, 8, 16),
    unwrap_hit("src/db.rs", 121, 8, 16), unwrap_hit("src/db.rs", 128, 8, 16),
    unwrap_hit("src/db.rs", 135, 8, 16), unwrap_hit("src/db.rs", 142, 8, 16),
    unwrap_hit("src/db.rs", 149, 8, 16), unwrap_hit("src/db.rs", 156, 8, 16),
    unwrap_hit("src/db.rs", 163, 8, 16), unwrap_hit("src/db.rs", 170, 8, 16),
    unwrap_hit("src/db.rs", 177, 8, 16), unwrap_hit("src/db.rs", 184, 8, 16) ],
  [],
  [ final(unwrap_count/2, [ unwrap_count("src/engine.rs", 12) ]) ]).

fixture(unwrap_below_budget_silent,
  prog([],
       [ (unwrap_count(Path, count(LineNo)) <- unwrap_hit(Path, LineNo, _, _), changed_file(Path)),
         (diag(Path, LineNo, warning, "unwrap-budget",
               concat([Total, " non-test unwraps in a changed file"]), Col, EndCol) <-
              unwrap_hit(Path, LineNo, Col, EndCol), changed_file(Path),
              unwrap_count(Path, Total), Total > 10) ]),
  [ unwrap_hit("src/engine.rs", 107, 8, 16),
    unwrap_hit("src/engine.rs", 114, 8, 16),
    unwrap_hit("src/engine.rs", 121, 8, 16),
    changed_file("src/engine.rs") ],
  [],
  [ final(unwrap_count/2, [ unwrap_count("src/engine.rs", 3) ]),
    final(diag/7, []) ]).

% ═══ the ratchet as a functional dependency, tier 0 ══════════════════════════

% check_exit RESTORED (coordinator, post-stratification-fix): the old joint
% fixpoint minted a spurious check_exit("no-new-eprintln",0) beside the
% correct 2 row; stratified negation grades the exit now. diag exactly pins
% the interpolated regrowth message, which is the check's real content (a
% tighter ratchet over the SAME clean world catches what the loose one let
% through -- clean_state_no_diags is the loose control, graded separately).
fixture(tightened_baseline_catches_regrowth,
  prog([],
       [ (eprintln_waiver_line(Path, WaiverLine) <- waiver_block_comment(Path, WaiverLine)),
         (eprintln_waiver_line(Path, WaiverLine) <- waiver_trailing_comment(Path, WaiverLine)),
         (eprintln_waived(Path, LineNo) <-
              eprintln_waiver_line(Path, WaiverLine),
              eprintln_hit(Path, LineNo),
              WaiverLine >= LineNo - 1,
              WaiverLine =< LineNo),
         (eprintln_counted(Path, LineNo) <-
              eprintln_hit(Path, LineNo),
              not(eprintln_waived(Path, LineNo))),
         (eprintln_count(Path, count(LineNo)) <- eprintln_counted(Path, LineNo)),
         (diag(Path, 1, warning, "eprintln-exceeded",
               concat([Hits, " counted eprintln! hits; the grandfathered baseline allows ", Allowed,
                       ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
               none, none) <-
              eprintln_count(Path, Hits), eprintln_baseline(Path, Allowed), Hits > Allowed),
         (diag(Path, LineNo, warning, "eprintln-new-file",
               "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
               none, none) <-
              eprintln_counted(Path, LineNo), not(eprintln_baseline(Path, _))),
         (any_diag(Name) <- program(Name), diag(_, _, _, _, _, _, _)),
         (check_exit(Name, 2) <- any_diag(Name)),
         (check_exit(Name, 0) <- program(Name), not(any_diag(Name))) ]),
  [ program("no-new-eprintln"),
    eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 1),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1),
    eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88) ],
  [],
  [ final(diag/7,
          [ diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded",
                 '2 counted eprintln! hits; the grandfathered baseline allows 1. Convert to tracing, or waive the line with @eprintln-ok: <reason>',
                 none, none) ]),
    final(check_exit/2, [ check_exit("no-new-eprintln", 2) ]) ]).
