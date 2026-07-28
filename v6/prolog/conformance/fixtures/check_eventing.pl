% fixtures/check_eventing.pl : check_eventing lab checks promoted into the
% shared fixture format (AGGREGATE.md 5a row: "the 7-tick diag scenario
% (T1-T7 delta lists verbatim); clock_rel_join_storms (5-vs-13); hook window
% join T6-T7").
%
% Adaptation forced by the shared grammar (FIXTURES.md "Out of scope":
% pattern/grammar matching and stdlib string fns beyond concat are not
% available here): the lab's `file_line(path, line, text)` +
% `contains(text, "eprintln!")` pair is not expressible: this format has no
% substring predicate. The extraction step (turning source text into a lint
% code) is itself out of scope, so `file_line` here carries the ALREADY
% classified code directly: `file_line(path, line_no, code)` with `code` one
% of `eprintln_ban`, `unwrap_ban`, or `none` for a clean line. `diagnostic`
% derives with `Code \== none` standing in for the lab's `contains` guard.
% Every tick-by-tick event in the scenario (which lines flip, when the
% ratchet tightens, when the hook window opens) is preserved exactly; only
% the text-matching step is elided, per FIXTURES.md's own exclusion.
%
% Second adaptation, discovered empirically against engine.pl (frozen, not
% owned by this file): `tick/7` always runs outside arrivals through
% `reverse_preserving/3`, which requires the arrived store to be no SMALLER
% than it was before the tick (`length(Suffix, Kept)` with `Kept =
% length(StoreOld)` cannot split a store that net-shrank). A tick built
% entirely of `-Row` retractions on a Set rel therefore fails outright (no
% throw, a plain unification failure). The lab's wholesale `replace` always
% nets to a non-negative row count (old rows come out, new ones go in), so
% every "line fixed" tick here sends an explicit `-oldcode/+none` PAIR
% instead of a bare retraction: the diagnostic still disappears (`none`
% fails `Code \== none`) and the tick's net row count never goes negative.
% Line 5, never touched, still never appears in either delta list at T3;
% that absence is what wholesale_replace_no_flicker is actually about, and
% it is unaffected by this pairing.
%
% Re-grading check (the task's CRITICAL note, ruling q4): edge-written rows
% only become trigger occurrences for OTHER EDGE RULES at T+1, never same
% tick. This lab's edge rules (diag_history, diag_seen) are both LEAF rels:
% nothing reads them from another EDGE body, so there is no edge-into-edge
% chain in this scenario and NOTHING needed re-grading on that account. The
% one real semantics gap is different and is documented at
% clock_rel_join_storms below: the lab's settle loop de-duplicates a tick's
% fired edges with `sort/2` (a SET), but this engine's Log rels are true
% multisets (q1: every occurrence is stamped and kept), so two independent
% triggers landing on the same derived row in one tick both write.
%
% Same-tick visibility TO LEVEL RULES (turn_diag reading diag_history) DOES
% hold under this engine (verified against engine.pl's tick/7: the final
% level_closure runs over FinalBase, i.e. AFTER edge writes land) exactly as
% the lab's hook needed, so hook_window_join/hook_window_excludes_old and
% hook_window_excludes_fixed promote unchanged.
%
% not promoted:
% - ambiguity 4 / "no edge on departure": `diag_closed_at(path,line,code,
%   tick)` cannot be written because `<+` fires on arrivals only (R4). The
%   lab does not grade a check for this (it is listed as a spec gap in the
%   .md, not a `check/2` clause), so there is nothing to promote; noted here
%   per the task brief so the hole stays visible in the fixture corpus too.
% - ambiguity 2 / row identity via line numbers: the lab states plainly it
%   "uses fixed line numbers and therefore does NOT test this", so no check
%   exists to promote.
% - `count_row_replaces_by_key`: not a separate fixture; the SAME property
%   (a level aggregate row disappearing and a new one appearing is an
%   ordinary boundary set-diff, no `keyed` declaration needed) is exercised
%   by lint_count/2's T2 delta inside diag_scenario_seven_ticks_end_to_end
%   below.
% - `edge_never_retracts`: not a separate fixture. Log rels structurally
%   cannot appear in a `-Row` delta under this engine (boundary_deltas only
%   ever emits `+Row` for Log rels; `-Row` into a Log rel throws at the
%   arrival boundary, see engine_core.pl log_retraction_rejected), so every
%   `deltas(diag_history/4, ...)` and `deltas(diag_seen/4, ...)` line below
%   already demonstrates it by construction (no `-diag_history(...)` or
%   `-diag_seen(...)` ever appears, and could not).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ the LSP loop end to end: diagnostics retract on fix, history survives ═══
% One file, seven ticks (T1-T7), transcribed from check_eventing.pl's
% `scenario/1`.

fixture(diag_scenario_seven_ticks_end_to_end,
  prog([
         keyed(ratchet/2, [1]),
         keyed(hook_window/2, [1]),
         kind(diag_history/4, log), keep(diag_history/4, all) ],
       [ (diagnostic(Path, Line, Code, warning) <-
            ( file_line(Path, Line, Code), Code \== none )),
         (lint_count(Code, count(hit(Path, Line))) <-
            diagnostic(Path, Line, Code, _)),
         (violation(Code, Count, Allowed) <-
            ( lint_count(Code, Count), ratchet(Code, Allowed), Count > Allowed )),
         (unratcheted_lint(Code, Count) <-
            ( lint_count(Code, Count), not(ratchet(Code, _)) )),
         (diag_history(Path, Line, Code, OpenedAt) <+
            ( diagnostic(Path, Line, Code, _), now(OpenedAt) )),
         (turn_diag(Turn, Path, Line, Code, OpenedAt) <-
            ( hook_window(Turn, Since), diagnostic(Path, Line, Code, _),
              diag_history(Path, Line, Code, OpenedAt), OpenedAt > Since )) ]),
  [],
  % T1: two banned calls, ratchet grandfathered at 2.
  [ [ +file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 5, eprintln_ban),
      +ratchet(eprintln_ban, 2) ],
    % T2: a third banned call arrives (line 7). 3 > 2, violation appears.
    [ +file_line(a_rs, 7, eprintln_ban) ],
    % T3: lines 3 and 7 fixed (-oldcode/+none pair, see the header note on
    % reverse_preserving). Line 5's diagnostic holds untouched.
    [ -file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 3, none),
      -file_line(a_rs, 7, eprintln_ban), +file_line(a_rs, 7, none) ],
    % T4: the ratchet tightens to 1 (no edge rule involved: a keyed rel fed
    % by outside arrivals is NOT auto-replaced by key, see rulings.pl /
    % ambiguity 10, so the schedule states -old/+new explicitly).
    [ -ratchet(eprintln_ban, 2), +ratchet(eprintln_ban, 1) ],
    % T5: line 3 regresses and an unwrap lands. 2 > 1 fires against the
    % TIGHTENED ratchet; unwrap_ban has no ratchet row at all.
    [ -file_line(a_rs, 3, none), +file_line(a_rs, 3, eprintln_ban),
      +file_line(a_rs, 9, unwrap_ban) ],
    % T6: the agent-turn hook asks what appeared after tick 4 and still holds.
    [ +hook_window(turn_7, 4) ],
    % T7: everything fixed. turn_diag empties; diag_history does not.
    [ -file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 3, none),
      -file_line(a_rs, 5, eprintln_ban), +file_line(a_rs, 5, none),
      -file_line(a_rs, 9, unwrap_ban), +file_line(a_rs, 9, none) ]
  ],
  [ deltas(diagnostic/4,
      [ [ +diagnostic(a_rs, 3, eprintln_ban, warning), +diagnostic(a_rs, 5, eprintln_ban, warning) ],
        [ +diagnostic(a_rs, 7, eprintln_ban, warning) ],
        [ -diagnostic(a_rs, 3, eprintln_ban, warning), -diagnostic(a_rs, 7, eprintln_ban, warning) ],
        [],
        [ +diagnostic(a_rs, 3, eprintln_ban, warning), +diagnostic(a_rs, 9, unwrap_ban, warning) ],
        [],
        [ -diagnostic(a_rs, 3, eprintln_ban, warning), -diagnostic(a_rs, 5, eprintln_ban, warning),
          -diagnostic(a_rs, 9, unwrap_ban, warning) ] ]),
    % wholesale_replace_no_flicker (R7): line 5's diagnostic AND its
    % file_line row are absent from BOTH lists at T3 even though the file
    % was rewritten that tick: the boundary diff only ever sees pre-vs-post.
    deltas(file_line/3,
      [ [ +file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 5, eprintln_ban) ],
        [ +file_line(a_rs, 7, eprintln_ban) ],
        [ -file_line(a_rs, 3, eprintln_ban), -file_line(a_rs, 7, eprintln_ban),
          +file_line(a_rs, 3, none), +file_line(a_rs, 7, none) ],
        [],
        [ -file_line(a_rs, 3, none),
          +file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 9, unwrap_ban) ],
        [],
        [ -file_line(a_rs, 3, eprintln_ban), -file_line(a_rs, 5, eprintln_ban),
          -file_line(a_rs, 9, unwrap_ban),
          +file_line(a_rs, 3, none), +file_line(a_rs, 5, none), +file_line(a_rs, 9, none) ] ]),
    % lint_count's T2 row is `count_row_replaces_by_key`: an ordinary level
    % boundary diff, -old/+new, no `keyed` declaration required.
    deltas(lint_count/2,
      [ [ +lint_count(eprintln_ban, 2) ],
        [ -lint_count(eprintln_ban, 2), +lint_count(eprintln_ban, 3) ],
        [ -lint_count(eprintln_ban, 3), +lint_count(eprintln_ban, 1) ],
        [],
        [ -lint_count(eprintln_ban, 1), +lint_count(eprintln_ban, 2), +lint_count(unwrap_ban, 1) ],
        [],
        [ -lint_count(eprintln_ban, 2), -lint_count(unwrap_ban, 1) ] ]),
    deltas(violation/3,
      [ [], [ +violation(eprintln_ban, 3, 2) ], [ -violation(eprintln_ban, 3, 2) ], [],
        [ +violation(eprintln_ban, 2, 1) ], [], [ -violation(eprintln_ban, 2, 1) ] ]),
    % v5's second diag rule: a lint code with no ratchet row at all.
    deltas(unratcheted_lint/2,
      [ [], [], [], [], [ +unratcheted_lint(unwrap_ban, 1) ], [],
        [ -unratcheted_lint(unwrap_ban, 1) ] ]),
    deltas(ratchet/2,
      [ [ +ratchet(eprintln_ban, 2) ], [], [],
        [ -ratchet(eprintln_ban, 2), +ratchet(eprintln_ban, 1) ], [], [], [] ]),
    % edge_history_survives_fix / edge_history_reopens: appended on every
    % appearance, silent at T3's fix, a second episode opens at T5.
    deltas(diag_history/4,
      [ [ +diag_history(a_rs, 3, eprintln_ban, 1), +diag_history(a_rs, 5, eprintln_ban, 1) ],
        [ +diag_history(a_rs, 7, eprintln_ban, 2) ],
        [],
        [],
        [ +diag_history(a_rs, 3, eprintln_ban, 5), +diag_history(a_rs, 9, unwrap_ban, 5) ],
        [],
        [] ]),
    % hook_window_join / hook_window_excludes_old: the window opens at T6 and
    % answers in the SAME tick (diag_history writes are visible to level
    % rules same-tick); line 5 has been open since T1 and stays excluded
    % because its diag_history row is opened_at=1, not > since=4.
    deltas(turn_diag/5,
      [ [], [], [], [], [],
        [ +turn_diag(turn_7, a_rs, 3, eprintln_ban, 5), +turn_diag(turn_7, a_rs, 9, unwrap_ban, 5) ],
        % hook_window_excludes_fixed: fixing during the turn empties the
        % hook's answer even though diag_history keeps both episodes.
        [ -turn_diag(turn_7, a_rs, 3, eprintln_ban, 5), -turn_diag(turn_7, a_rs, 9, unwrap_ban, 5) ] ]),
    ticks(7),
    final(diagnostic/4, []),
    final(turn_diag/5, []),
    final(ratchet/2, [ ratchet(eprintln_ban, 1) ]),
    final(diag_history/4,
      [ diag_history(a_rs, 3, eprintln_ban, 1), diag_history(a_rs, 3, eprintln_ban, 5),
        diag_history(a_rs, 5, eprintln_ban, 1), diag_history(a_rs, 7, eprintln_ban, 2),
        diag_history(a_rs, 9, unwrap_ban, 5) ]) ]).

% ═══ the now() kernel receipt ════════════════════════════════════════════════
% Same program, same schedule as above, plus a second edge rule that reads
% the tick from an ordinary rel instead of the kernel `now(Tick)`. Every
% `tick_rel(N)` arrival is itself trigger-eligible (q6: unmarked body, any
% atom fires it), so diag_seen re-fires against every LIVE diagnostic on
% every tick, not just on genuine appearances. diag_history (the GOOD
% branch, kernel now()) stays at exactly the wanted 5 rows, one per
% appearance, in the same run.
%
% `tick_rel` mirrors the lab's own clock: `replace(tick_rel(_), [tick_rel(N)])`
% keeps exactly ONE current-value row alive, never a growing history, so it
% is declared a Set rel here and the schedule sends an explicit -old/+new
% pair each tick (the same manual key discipline used for `ratchet` above:
% direct outside arrivals are not auto-replaced by key, see ambiguity 10).
%
% The lab's own count for the bad branch was 13 (check_eventing.md:129-137),
% computed by a settle loop that de-duplicates a tick's fired edges with
% `sort/2` (a SET). This engine's Log rels are true multisets (q1: every
% occurrence is engine-stamped and kept, never collapsed): on a tick where
% the `tick_rel` arrival and a newly-true `diagnostic` land together (T1,
% T2, T5 here), BOTH independently trigger diag_seen for the same
% (path, line, code, tick) row, and both writes land as separate stamped
% occurrences. That is a strictly worse storm than the lab's dedup allowed:
% MEASURED 18 diag_seen rows against this engine for the identical schedule
% (not the lab's 13, see final(diag_seen/4, ...) below), the corrected
% receipt. The kernel-vs-rel gap is not smaller under q1, it is larger.
% `now()` never arrives, so diag_history never sees this hazard at all and
% stays at exactly 5.
fixture(clock_rel_join_storms,
  prog([
         keyed(ratchet/2, [1]),
         kind(diag_history/4, log), keep(diag_history/4, all),
         kind(diag_seen/4, log), keep(diag_seen/4, all) ],
       [ (diagnostic(Path, Line, Code, warning) <-
            ( file_line(Path, Line, Code), Code \== none )),
         (diag_history(Path, Line, Code, OpenedAt) <+
            ( diagnostic(Path, Line, Code, _), now(OpenedAt) )),
         (diag_seen(Path, Line, Code, At) <+
            ( diagnostic(Path, Line, Code, _), tick_rel(At) )) ]),
  [],
  [ [ +file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 5, eprintln_ban),
      +ratchet(eprintln_ban, 2), +tick_rel(1) ],
    [ +file_line(a_rs, 7, eprintln_ban), -tick_rel(1), +tick_rel(2) ],
    [ -file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 3, none),
      -file_line(a_rs, 7, eprintln_ban), +file_line(a_rs, 7, none),
      -tick_rel(2), +tick_rel(3) ],
    [ -ratchet(eprintln_ban, 2), +ratchet(eprintln_ban, 1), -tick_rel(3), +tick_rel(4) ],
    [ -file_line(a_rs, 3, none), +file_line(a_rs, 3, eprintln_ban),
      +file_line(a_rs, 9, unwrap_ban), -tick_rel(4), +tick_rel(5) ],
    [ -tick_rel(5), +tick_rel(6) ],
    [ -file_line(a_rs, 3, eprintln_ban), +file_line(a_rs, 3, none),
      -file_line(a_rs, 5, eprintln_ban), +file_line(a_rs, 5, none),
      -file_line(a_rs, 9, unwrap_ban), +file_line(a_rs, 9, none),
      -tick_rel(6), +tick_rel(7) ]
  ],
  [ final(diag_history/4,
      [ diag_history(a_rs, 3, eprintln_ban, 1), diag_history(a_rs, 3, eprintln_ban, 5),
        diag_history(a_rs, 5, eprintln_ban, 1), diag_history(a_rs, 7, eprintln_ban, 2),
        diag_history(a_rs, 9, unwrap_ban, 5) ]),
    % 18, not 5: every live diagnostic re-answers on every tick_rel arrival
    % (q6 any-atom) AND every newly-true diagnostic re-answers against the
    % clock's current value (q1 multiset): two independent trigger paths to
    % the SAME row on the ticks they coincide (T1, T2, T5), each a separate
    % stamped write. This is the measured shape of "now() is kernel."
    final(diag_seen/4,
      [ diag_seen(a_rs, 3, eprintln_ban, 1), diag_seen(a_rs, 3, eprintln_ban, 1),
        diag_seen(a_rs, 3, eprintln_ban, 2),
        diag_seen(a_rs, 3, eprintln_ban, 5), diag_seen(a_rs, 3, eprintln_ban, 5),
        diag_seen(a_rs, 3, eprintln_ban, 6),
        diag_seen(a_rs, 5, eprintln_ban, 1), diag_seen(a_rs, 5, eprintln_ban, 1),
        diag_seen(a_rs, 5, eprintln_ban, 2), diag_seen(a_rs, 5, eprintln_ban, 3),
        diag_seen(a_rs, 5, eprintln_ban, 4), diag_seen(a_rs, 5, eprintln_ban, 5),
        diag_seen(a_rs, 5, eprintln_ban, 6),
        diag_seen(a_rs, 7, eprintln_ban, 2), diag_seen(a_rs, 7, eprintln_ban, 2),
        diag_seen(a_rs, 9, unwrap_ban, 5), diag_seen(a_rs, 9, unwrap_ban, 5),
        diag_seen(a_rs, 9, unwrap_ban, 6) ]) ]).
