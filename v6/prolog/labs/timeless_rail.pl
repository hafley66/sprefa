% timeless_rail.pl : the tier-0 exit criterion as a lab.
%
% Run:  swipl -q -l v6/prolog/labs/timeless_rail.pl -g go -g halt
%
% Two v5 lint rails are transcribed into the CANDIDATE surface and then
% reference-interpreted here:
%
%   .dl/no-new-eprintln.dl  - ratchet + waiver range join + negation + count
%   .dl/rails.dl:107-134    - the unwrap budget: count aggregate + `${n}`
%                             string interpolation + a changed-file join
%
% This is the TIMELESS fragment only. Level rules (`<-`), stratified negation,
% count aggregation, facts, keyed rels read as functional dependencies. No
% `<+`, no `pre`, no `now`, no clock, no tick. A "repo state" is a canned set
% of world rows; a scenario is one evaluation of the whole program against one
% such set. Retraction is graded as the set difference between two
% evaluations, which is exactly what tier 4 will later turn into a -row.
%
% Nothing is imported from the sibling labs: shapes are copied, not shared.
%
% The surface text of the transcribed program lives in timeless_rail.md and is
% repeated in comments above each clause. Where LANG.md has no construct the
% transcription needs, the invented spelling is marked INVENTION n and
% justified in the .md.

:- use_module(library(lists)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).

:- op(990, xfx, <-).
:- discontiguous rule/1.

% ═══════════════════════════════════════════════════════════════════════════
% THE PROGRAM, in the candidate surface.
%
% Declarations (surface text; nothing to execute, the interpreter reads only
% the rules, the keyed/2 table and the canned rows):
%
%   enum Severity { Error, Warning }
%
%   # INVENTION 1: `from world` = a rel no program rule may head. Tier 1 gives
%   # it a `bind` (scan + match_line / comment / ast_yaml); tier 0 feeds it
%   # canned rows, which is what this lab does.
%   rel eprintln_hit(path: Path, line_no: Int) from world;
%   rel waiver_block_comment(path: Path, line_no: Int) from world;
%   rel waiver_trailing_comment(path: Path, line_no: Int) from world;
%   rel unwrap_hit(path: Path, line_no: Int, col: Int, end_col: Int) from world;
%   rel changed_file(path: Path) from world;
%
%   rel eprintln_waiver_line(path: Path, waiver_line: Int);
%   rel eprintln_waived(path: Path, line_no: Int);
%   rel eprintln_counted(path: Path, line_no: Int);
%   rel eprintln_count(path: Path, hits: Int);
%   rel unwrap_count(path: Path, hits: Int);
%
%   # INVENTION 7: Key(T) in tier 0 is a functional dependency, checked
%   # statically over the fact set. No replace, no -old/+new, no time.
%   rel eprintln_baseline(path: Key(Path), allowed: Int);
%
%   # INVENTION 5 (named columns) + INVENTION 6 (column defaults): both rails
%   # head the same product rel with different column sets.
%   rel diag(path: Path, line_no: Int, severity: Severity, code: Code,
%            msg: Str, col: Int = none, end_col: Int = none);
%   rel diag_stage(code: Code, stage: Stage);
%
%   rel severity_rank(severity: Key(Severity), rank: Int);
%   rel gate_threshold(stage: Key(Stage), min_rank: Int);
%   rel gate_blocked(stage: Stage);
%   rel gate_exit(stage: Key(Stage), exit_code: Int);
%
%   # INVENTION 8: a singleton rel to anchor a whole-program negation.
%   rel program(name: Key(Str));
%   rel any_diag(program: Str);
%   rel check_exit(program: Key(Str), exit_code: Int);
%
%   # INVENTION 10: the ask, and the gate it drives.
%   ? diag(path, line_no, severity, code, msg);
% ═══════════════════════════════════════════════════════════════════════════

% Rels whose declaration carries Key(...) in the leading column position.
% Tier 0 reads this as: at most one row per key prefix, checked, never
% enforced by replacement.
keyed(eprintln_baseline, 1).
keyed(severity_rank, 1).
keyed(gate_threshold, 1).
keyed(gate_exit, 1).
keyed(program, 1).
keyed(check_exit, 1).

% ---- waiver union (INVENTION 12: several rules may head one rel) ------------
% v5 unions two source rules into eprintln_waiver_line because the `comment`
% op only sees whole-line comments and `match_line` is needed for the trailing
% form. Both are world rels here; the union is a plain two-rule rel.
%
%   eprintln_waiver_line(path, waiver_line) <-
%       waiver_block_comment(path, waiver_line);
rule( eprintln_waiver_line(Path, WaiverLine) <-
      ( waiver_block_comment(Path, WaiverLine) ) ).

%   eprintln_waiver_line(path, waiver_line) <-
%       waiver_trailing_comment(path, waiver_line);
rule( eprintln_waiver_line(Path, WaiverLine) <-
      ( waiver_trailing_comment(Path, WaiverLine) ) ).

% ---- the range join (INVENTION 2: comparison and arithmetic in a body) ------
% A waiver on the hit line or the line above exempts the hit.
%
%   eprintln_waived(path, line_no) <-
%       eprintln_waiver_line(path, waiver_line),
%       eprintln_hit(path, line_no),
%       waiver_line >= line_no - 1,
%       waiver_line <= line_no;
rule( eprintln_waived(Path, LineNo) <-
      ( eprintln_waiver_line(Path, WaiverLine),
        eprintln_hit(Path, LineNo),
        WaiverLine >= LineNo - 1,
        WaiverLine =< LineNo ) ).

% ---- negation (INVENTION 3) ------------------------------------------------
%   eprintln_counted(path, line_no) <-
%       eprintln_hit(path, line_no), !eprintln_waived(path, line_no);
rule( eprintln_counted(Path, LineNo) <-
      ( eprintln_hit(Path, LineNo),
        \+ eprintln_waived(Path, LineNo) ) ).

% ---- aggregation (INVENTION 4: head-position count, implicit grouping) ------
%   eprintln_count(path, count(line_no)) <- eprintln_counted(path, line_no);
rule( eprintln_count(Path, count(LineNo)) <-
      ( eprintln_counted(Path, LineNo) ) ).

% ---- diag rule 1: a grandfathered file grew past its baseline ---------------
% INVENTION 9 (`${var}` interpolation) is used here as a deliberate
% improvement over the v5 text, which reports no numbers. See the .md.
%
%   diag(
%       path: path,
%       line_no: 1,
%       severity: Warning,
%       code: "eprintln-exceeded",
%       msg: "${hits} counted eprintln! hits; the grandfathered baseline allows
%             ${allowed}. Convert to tracing, or waive the line with
%             @eprintln-ok: <reason>"
%   ) <-
%       eprintln_count(path, hits),
%       eprintln_baseline(path, allowed),
%       hits > allowed;
rule( diag(Path, 1, warning, "eprintln-exceeded",
           interp([Hits, " counted eprintln! hits; the grandfathered baseline allows ",
                   Allowed, ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"]),
           none, none) <-
      ( eprintln_count(Path, Hits),
        eprintln_baseline(Path, Allowed),
        Hits > Allowed ) ).

% ---- diag rule 2: a file with no baseline row at all ------------------------
% Flags every hit line precisely, so line_no is the hit line, not 1.
%
%   diag(
%       path: path,
%       line_no: line_no,
%       severity: Warning,
%       code: "eprintln-new-file",
%       msg: "new eprintln! outside the grandfathered baseline; convert to
%             tracing, or waive with @eprintln-ok: <reason>"
%   ) <-
%       eprintln_counted(path, line_no),
%       !eprintln_baseline(path, _);
rule( diag(Path, LineNo, warning, "eprintln-new-file",
           "new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>",
           none, none) <-
      ( eprintln_counted(Path, LineNo),
        \+ eprintln_baseline(Path, _) ) ).

% ---- the second rail: unwrap budget (.dl/rails.dl:129-134) ------------------
%   unwrap_count(path, count(line_no)) <-
%       unwrap_hit(path, line_no, _, _), changed_file(path);
rule( unwrap_count(Path, count(LineNo)) <-
      ( unwrap_hit(Path, LineNo, _, _),
        changed_file(Path) ) ).

% The span columns ride the same diag rel via the defaults of INVENTION 6:
% the eprintln rules above omit col/end_col, this one supplies them.
%
%   diag(
%       path: path, line_no: line_no, col: col, end_col: end_col,
%       severity: Warning, code: "unwrap-budget",
%       msg: "${total} non-test unwraps in a changed file"
%   ) <-
%       unwrap_hit(path, line_no, col, end_col),
%       changed_file(path),
%       unwrap_count(path, total),
%       total > 10;
rule( diag(Path, LineNo, warning, "unwrap-budget",
           interp([Total, " non-test unwraps in a changed file"]),
           Col, EndCol) <-
      ( unwrap_hit(Path, LineNo, Col, EndCol),
        changed_file(Path),
        unwrap_count(Path, Total),
        Total > 10 ) ).

% ---- the gate, expressed as ordinary rules (no new syntax) ------------------
% rails.dl's header states the policy: error severity blocks the agent
% (exit 2), warn reports without failing. The lab assumes the commit stage is
% stricter and blocks on any routed diag. Both thresholds are FACTS, so the
% policy is data, not language.
%
%   gate_blocked(stage) <-
%       diag(severity: severity, code: code),
%       diag_stage(code, stage),
%       gate_threshold(stage, min_rank),
%       severity_rank(severity, rank),
%       rank >= min_rank;
rule( gate_blocked(Stage) <-
      ( diag(_, _, Severity, Code, _, _, _),
        diag_stage(Code, Stage),
        gate_threshold(Stage, MinRank),
        severity_rank(Severity, Rank),
        Rank >= MinRank ) ).

%   gate_exit(stage, 2) <- gate_blocked(stage);
%   gate_exit(stage, 0) <- gate_threshold(stage, _), !gate_blocked(stage);
rule( gate_exit(Stage, 2) <-
      ( gate_blocked(Stage) ) ).
rule( gate_exit(Stage, 0) <-
      ( gate_threshold(Stage, _),
        \+ gate_blocked(Stage) ) ).

% ---- `dl <program> --check` exit code, as rows -----------------------------
% The CLI gate is "the ask returned at least one row". Expressing it needs a
% singleton to hang the negation on (INVENTION 8), because datalog has no
% zero-arity rel and "no rows anywhere" is not a row.
%
%   any_diag(name) <- program(name), diag(path: _, line_no: _);
%   check_exit(name, 2) <- any_diag(name);
%   check_exit(name, 0) <- program(name), !any_diag(name);
rule( any_diag(Name) <-
      ( program(Name),
        diag(_, _, _, _, _, _, _) ) ).
rule( check_exit(Name, 2) <-
      ( any_diag(Name) ) ).
rule( check_exit(Name, 0) <-
      ( program(Name),
        \+ any_diag(Name) ) ).

% ═══════════════════════════════════════════════════════════════════════════
% THE REFERENCE INTERPRETER
%
% One evaluation = stratify, then run a least fixpoint per stratum over the
% accumulated fact set. Negation and aggregation always read a settled input
% because the stratifier puts their sources strictly lower.
% ═══════════════════════════════════════════════════════════════════════════

evaluate(BaseRows, AllRows) :-
    strata(Assignment),
    findall(Stratum, member(_-Stratum, Assignment), Numbers),
    max_list([0 | Numbers], MaxStratum),
    sort(BaseRows, Base),
    eval_strata(0, MaxStratum, Assignment, Base, AllRows).

eval_strata(Current, Max, _, Rows, Rows) :- Current > Max, !.
eval_strata(Current, Max, Assignment, RowsIn, RowsOut) :-
    findall((Head <- Body),
            ( rule(Head <- Body),
              functor(Head, Name, _),
              memberchk(Name-Current, Assignment) ),
            Rules),
    stratum_fixpoint(Rules, RowsIn, RowsMid, 0),
    NextStratum is Current + 1,
    eval_strata(NextStratum, Max, Assignment, RowsMid, RowsOut).

stratum_fixpoint(Rules, RowsIn, RowsOut, Round) :-
    Round < 50,
    findall(Row,
            ( member(Rule, Rules), derive(Rule, RowsIn, Derived), member(Row, Derived) ),
            New0),
    sort(New0, New),
    ord_union(RowsIn, New, RowsNext),
    (   RowsNext == RowsIn
    ->  RowsOut = RowsIn
    ;   NextRound is Round + 1,
        stratum_fixpoint(Rules, RowsNext, RowsOut, NextRound) ).

% derive(+Rule, +Facts, -Rows). Two head shapes: plain projection (rendering
% any `${...}` interpolation once the body has bound its variables), and
% group-by-count when one head column is count(Term).
derive((Head <- Body), Facts, Rows) :-
    Head =.. [Name | Args],
    (   count_slot(Args, Index, Counted, GroupArgs)
    ->  findall(GroupArgs-Counted, solve(Body, Facts), Pairs0),
        sort(Pairs0, Pairs),
        group_pairs_by_key(Pairs, Grouped),
        findall(Row,
                ( member(Group-Values, Grouped),
                  length(Values, Total),
                  nth0(Index, FullArgs, Total, Group),
                  Row =.. [Name | FullArgs] ),
                Rows)
    ;   findall(Rendered,
                ( solve(Body, Facts), render_head(Head, Rendered), ground(Rendered) ),
                Rows) ).

% nonvar guard: an unbound head column would otherwise unify with count(_) and
% turn every rule into an aggregate.
count_slot(Args, Index, Counted, GroupArgs) :-
    nth0(Index, Args, Arg, GroupArgs), nonvar(Arg), Arg = count(Counted), !.

% INVENTION 9: `${var}` interpolation renders after the join binds the vars.
render_head(Head, Rendered) :-
    Head =.. [Name | Args],
    maplist(render_column, Args, RenderedArgs),
    Rendered =.. [Name | RenderedArgs].

render_column(Column, Text) :-
    nonvar(Column), Column = interp(Parts), !,
    atomics_to_string(Parts, Text).
render_column(Column, Column).

solve(true, _) :- !.
solve((Left, Right), Facts) :- !,
    solve(Left, Facts), solve(Right, Facts).
solve(\+ Atom, Facts) :- !, \+ solve(Atom, Facts).
solve(Guard, _) :- is_guard(Guard), !, call(Guard).
solve(Atom, Facts) :- member(Atom, Facts).

is_guard(Guard) :- callable(Guard), functor(Guard, Name, 2), guard_name(Name).
guard_name(>). guard_name(>=). guard_name(<). guard_name(=<).
guard_name(=:=). guard_name(=\=).

% ---- stratification --------------------------------------------------------
% Constraints: a positive body rel is at most the head's stratum; a negated
% body rel and every source of an aggregate head are strictly below it. The
% least assignment satisfying all of them is reached by relaxation from zero.

body_items((Left, Right), Items) :- !,
    body_items(Left, LeftItems), body_items(Right, RightItems),
    append(LeftItems, RightItems, Items).
body_items(Item, [Item]).

body_rel_names(Body, Positives, Negatives) :-
    body_items(Body, Items),
    findall(Name,
            ( member(Item, Items), Item \= (\+ _), \+ is_guard(Item),
              functor(Item, Name, _) ),
            Positives0),
    findall(Name,
            ( member(Item, Items), Item = (\+ Atom), functor(Atom, Name, _) ),
            Negatives0),
    sort(Positives0, Positives),
    sort(Negatives0, Negatives).

all_rel_names(Rules, Names) :-
    findall(Name,
            ( member(Head <- Body, Rules),
              (   functor(Head, Name, _)
              ;   body_rel_names(Body, Positives, Negatives),
                  ( member(Name, Positives) ; member(Name, Negatives) ) ) ),
            Names0),
    sort(Names0, Names).

% strata/1 stratifies the program; strata_of/2 takes any rule list so the
% rejection path is testable without mutating the program.
strata(Assignment) :-
    findall((Head <- Body), rule(Head <- Body), Rules),
    strata_of(Rules, Assignment).

strata_of(Rules, Assignment) :-
    all_rel_names(Rules, Names),
    findall(Name-0, member(Name, Names), Initial),
    relax(Rules, Initial, 0, Assignment).

% A negative or aggregate cycle makes the constraint set unsatisfiable: the
% relaxation never settles, and the round bound turns that into a rejection.
relax(Rules, Current, Round, Final) :-
    Round < 100,
    relax_step(Rules, Current, Next),
    (   Next == Current
    ->  Final = Current
    ;   NextRound is Round + 1,
        relax(Rules, Next, NextRound, Final) ).

relax_step(Rules, Current, Next) :-
    findall(Name-Value,
            ( member(Name-Old, Current),
              findall(Need, stratum_need(Rules, Name, Current, Need), Needs),
              max_list([Old | Needs], Value) ),
            Next).

stratum_need(Rules, Name, Assignment, Need) :-
    member(Head <- Body, Rules),
    functor(Head, Name, _),
    body_rel_names(Body, Positives, Negatives),
    Head =.. [_ | Args],
    (   count_slot(Args, _, _, _) -> Bump = 1 ; Bump = 0 ),
    (   member(RelName, Positives),
        memberchk(RelName-Below, Assignment),
        Need is Below + Bump
    ;   member(RelName, Negatives),
        memberchk(RelName-Below, Assignment),
        Need is Below + 1 ).

% ---- the tier-0 reading of Key(T): a functional dependency, checked ---------
fd_violations(Rows, Violations) :-
    findall(fd_violation(Name, KeyPrefix),
            ( keyed(Name, KeyArity),
              member(RowA, Rows), functor(RowA, Name, _), RowA =.. [Name | ArgsA],
              length(KeyPrefix, KeyArity), append(KeyPrefix, RestA, ArgsA),
              member(RowB, Rows), functor(RowB, Name, _), RowB =.. [Name | ArgsB],
              append(KeyPrefix, RestB, ArgsB),
              RestA \== RestB ),
            Violations0),
    sort(Violations0, Violations).

% ═══════════════════════════════════════════════════════════════════════════
% REPO STATES: canned world rows, plus the program's own fact sets.
% ═══════════════════════════════════════════════════════════════════════════

% The grandfathered baseline, copied from .dl/no-new-eprintln.dl:67-70, plus
% the policy tables the gate rules read.
fact_set(standard, Facts) :-
    baseline_rows(standard, Baselines),
    policy_rows(Policy),
    append(Baselines, Policy, Facts).

% A tightened ratchet: src/daemon/client.rs lowered from 2 to 1 by a commit
% that was supposed to shrink the file. Tier 0 sees a DIFFERENT fact set, not
% a replacement of a row.
fact_set(tightened, Facts) :-
    baseline_rows(tightened, Baselines),
    policy_rows(Policy),
    append(Baselines, Policy, Facts).

% A hand-edited baseline table with two rows for one path: the FD is what
% catches it, and it is a static rejection, not a runtime replace.
fact_set(duplicate_baseline, Facts) :-
    baseline_rows(standard, Baselines),
    policy_rows(Policy),
    append([Baselines, [eprintln_baseline("src/daemon/client.rs", 5)], Policy], Facts).

baseline_rows(standard,
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 2),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1) ]).

baseline_rows(tightened,
  [ eprintln_baseline("src/config.rs", 1),
    eprintln_baseline("src/daemon/client.rs", 1),
    eprintln_baseline("src/setup/vscode.rs", 1),
    eprintln_baseline("src/setup/wire.rs", 1) ]).

policy_rows(
  [ program("no-new-eprintln"),
    severity_rank(warning, 1),
    severity_rank(error, 2),
    gate_threshold("agent-turn", 2),
    gate_threshold("commit", 1),
    diag_stage("eprintln-exceeded", "agent-turn"),
    diag_stage("eprintln-exceeded", "commit"),
    diag_stage("eprintln-new-file", "agent-turn"),
    diag_stage("eprintln-new-file", "commit"),
    diag_stage("unwrap-budget", "agent-turn") ]).

% ---- world rows ------------------------------------------------------------
% Every file is at or under its baseline. src/cli/mod.rs has a hit AND a
% trailing waiver on the same line, so it never reaches eprintln_counted and
% never trips the no-baseline rule.
world(clean, Rows) :-
    clean_eprintln(Eprintln),
    unwrap_run("src/engine.rs", 3, Unwraps),
    append([Eprintln, Unwraps, [changed_file("src/engine.rs")]], Rows).

% One new eprintln! lands in a grandfathered file: 3 counted against 2 allowed.
world(over_baseline, Rows) :-
    world(clean, Clean),
    append(Clean, [eprintln_hit("src/daemon/client.rs", 175)], Rows).

% The fix: the new hit stays but gets a whole-line waiver on the line ABOVE,
% which is the tighter half of the range join (174 >= 175 - 1).
world(fixed, Rows) :-
    world(over_baseline, Over),
    append(Over, [waiver_block_comment("src/daemon/client.rs", 174)], Rows).

% A brand-new file starts printing: no baseline row exists for it at all.
world(new_file, Rows) :-
    world(clean, Clean),
    append(Clean, [ eprintln_hit("src/cli/health.rs", 203),
                    eprintln_hit("src/cli/health.rs", 288) ], Rows).

% The unwrap budget: 12 in a changed file (over), 12 in an unchanged one.
world(unwrap_over, Rows) :-
    clean_eprintln(Eprintln),
    unwrap_run("src/engine.rs", 12, Changed),
    unwrap_run("src/db.rs", 12, Unchanged),
    append([Eprintln, Changed, Unchanged, [changed_file("src/engine.rs")]], Rows).

clean_eprintln(
  [ eprintln_hit("src/config.rs", 44),
    eprintln_hit("src/daemon/client.rs", 118),
    eprintln_hit("src/daemon/client.rs", 133),
    eprintln_hit("src/setup/vscode.rs", 61),
    eprintln_hit("src/setup/wire.rs", 92),
    eprintln_hit("src/cli/mod.rs", 88),
    waiver_trailing_comment("src/cli/mod.rs", 88) ]).

unwrap_run(Path, HitCount, Rows) :-
    numlist(1, HitCount, Indexes),
    findall(unwrap_hit(Path, LineNo, 8, 16),
            ( member(Index, Indexes), LineNo is 100 + Index * 7 ),
            Rows).

% ---- scenarios -------------------------------------------------------------
scenario(clean,         standard,           clean).
scenario(over_baseline, standard,           over_baseline).
scenario(fixed,         standard,           fixed).
scenario(new_file,      standard,           new_file).
scenario(unwrap_over,   standard,           unwrap_over).
scenario(tightened,     tightened,          clean).
scenario(fd_conflict,   duplicate_baseline, clean).

base_rows(Name, Base) :-
    scenario(Name, FactSetName, WorldName),
    fact_set(FactSetName, Facts),
    world(WorldName, WorldRows),
    append(Facts, WorldRows, Base).

run(Name, Rows) :-
    base_rows(Name, Base),
    evaluate(Base, Rows).

rows_of(Rows, Name, Selected) :-
    findall(Row, ( member(Row, Rows), functor(Row, Name, _) ), Selected).

% exact/3: the whole row set for one rel, compared as a set.
exact(Rows, Name, Expected) :-
    rows_of(Rows, Name, Actual),
    sort(Expected, ExpectedSorted),
    Actual == ExpectedSorted.

% Message text used in more than one expectation.
new_file_msg("new eprintln! outside the grandfathered baseline; convert to tracing, or waive with @eprintln-ok: <reason>").

exceeded_msg(Hits, Allowed, Text) :-
    atomics_to_string([Hits, " counted eprintln! hits; the grandfathered baseline allows ",
                       Allowed, ". Convert to tracing, or waive the line with @eprintln-ok: <reason>"],
                      Text).

% ═══════════════════════════════════════════════════════════════════════════
% THE GRADER
% ═══════════════════════════════════════════════════════════════════════════

% ---- the tier-0 checker claims --------------------------------------------

% Stratification puts negation and aggregation above their sources. This is
% the whole reason the timeless fragment needs a checker at all.
check(stratification_assigns_levels,
      ( strata(Assignment),
        memberchk(eprintln_hit-0, Assignment),
        memberchk(eprintln_waiver_line-0, Assignment),
        memberchk(eprintln_waived-0, Assignment),
        memberchk(eprintln_counted-1, Assignment),
        memberchk(unwrap_count-1, Assignment),
        memberchk(eprintln_count-2, Assignment),
        memberchk(diag-2, Assignment),
        memberchk(gate_blocked-2, Assignment),
        memberchk(gate_exit-3, Assignment),
        memberchk(check_exit-3, Assignment) )).

% The same stratifier REJECTS a program whose negation cycles. This is the one
% tier-0 checker obligation that INVENTION 3 creates.
check(stratifier_rejects_negative_cycle,
      ( findall((Head <- Body), rule(Head <- Body), Rules),
        strata_of(Rules, _),
        Cycle = ( unstratifiable(Path) <-
                  ( eprintln_hit(Path, _), \+ unstratifiable(Path) ) ),
        \+ strata_of([Cycle | Rules], _) )).

% Nothing temporal is reachable from the program text: every rule is `<-`,
% and no `<+`, `pre`, `now`, `clock` or `delta` term appears anywhere.
check(program_is_timeless,
      ( forall(rule(Rule), Rule = (_ <- _)),
        forall(( rule(Head <- Body), member(Term, [Head, Body]) ),
               \+ mentions_temporal(Term)) )).

% ---- (a) the clean repo state ---------------------------------------------

check(clean_state_no_diags,
      ( run(clean, Rows), exact(Rows, diag, []) )).

check(clean_state_gate_and_exit_zero,
      ( run(clean, Rows),
        exact(Rows, gate_blocked, []),
        exact(Rows, gate_exit, [gate_exit("agent-turn", 0), gate_exit("commit", 0)]),
        exact(Rows, check_exit, [check_exit("no-new-eprintln", 0)]) )).

% EXACT ROW SET 1: the waiver range join, both sides. One hit is waived, and
% it is exactly the one with a trailing waiver on its own line; every other
% hit reaches eprintln_counted.
check(waiver_range_join_exact_rows,
      ( run(clean, Rows),
        exact(Rows, eprintln_waived, [eprintln_waived("src/cli/mod.rs", 88)]),
        exact(Rows, eprintln_counted,
              [ eprintln_counted("src/config.rs", 44),
                eprintln_counted("src/daemon/client.rs", 118),
                eprintln_counted("src/daemon/client.rs", 133),
                eprintln_counted("src/setup/vscode.rs", 61),
                eprintln_counted("src/setup/wire.rs", 92) ]),
        exact(Rows, eprintln_count,
              [ eprintln_count("src/config.rs", 1),
                eprintln_count("src/daemon/client.rs", 2),
                eprintln_count("src/setup/vscode.rs", 1),
                eprintln_count("src/setup/wire.rs", 1) ]) )).

% ---- (b) one new eprintln beyond the ratchet ------------------------------

% EXACT ROW SET 2: the whole diag product for the state, with the file at
% line 1 (the rail reports at the file head), warning severity, the exceeded
% code, and the interpolated counts.
check(over_baseline_diag_exact_rows,
      ( run(over_baseline, Rows),
        exceeded_msg(3, 2, Msg),
        exact(Rows, diag,
              [ diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded", Msg, none, none) ]) )).

check(over_baseline_count_row,
      ( run(over_baseline, Rows),
        rows_of(Rows, eprintln_count, Counts),
        memberchk(eprintln_count("src/daemon/client.rs", 3), Counts),
        \+ memberchk(eprintln_count("src/daemon/client.rs", 2), Counts) )).

% The severity split is real: a warning routes to the agent turn without
% blocking it, and blocks the commit stage. gate_blocked is exactly ["commit"].
% `dl .dl/no-new-eprintln.dl --check` is "the ask returned a row" -> exit 2,
% independent of severity; the staged gates read severity.
check(over_baseline_gate_blocks_commit_only,
      ( run(over_baseline, Rows),
        exact(Rows, gate_blocked, [gate_blocked("commit")]),
        exact(Rows, gate_exit, [gate_exit("agent-turn", 0), gate_exit("commit", 2)]),
        exact(Rows, any_diag, [any_diag("no-new-eprintln")]),
        exact(Rows, check_exit, [check_exit("no-new-eprintln", 2)]) )).

% ---- (c) the fix: level retraction as a set difference --------------------

check(fix_by_waiver_removes_diag_and_violation,
      ( run(over_baseline, Before), run(fixed, After),
        rows_of(Before, diag, BeforeDiags), rows_of(After, diag, AfterDiags),
        exceeded_msg(3, 2, Msg),
        ord_subtract(BeforeDiags, AfterDiags,
                     [diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded", Msg, none, none)]),
        AfterDiags == [],
        exact(After, gate_blocked, []),
        exact(After, check_exit, [check_exit("no-new-eprintln", 0)]),
        rows_of(After, eprintln_waived, Waived),
        memberchk(eprintln_waived("src/daemon/client.rs", 175), Waived) )).

% ---- the no-baseline branch -----------------------------------------------

% EXACT ROW SET 3: two diags, each at its own hit line, not at line 1.
check(new_file_diag_at_hit_line_exact_rows,
      ( run(new_file, Rows), new_file_msg(Msg),
        exact(Rows, diag,
              [ diag("src/cli/health.rs", 203, warning, "eprintln-new-file", Msg, none, none),
                diag("src/cli/health.rs", 288, warning, "eprintln-new-file", Msg, none, none) ]) )).

% The two diag rules are mutually exclusive by construction: the file with no
% baseline row never also produces an eprintln-exceeded row.
check(new_file_no_exceeded_diag,
      ( run(new_file, Rows),
        rows_of(Rows, diag, Diags),
        \+ ( member(diag("src/cli/health.rs", _, _, "eprintln-exceeded", _, _, _), Diags) ),
        rows_of(Rows, eprintln_count, Counts),
        memberchk(eprintln_count("src/cli/health.rs", 2), Counts) )).

% ---- (d) the aggregation + interpolation rail -----------------------------

check(unwrap_aggregate_and_interpolation,
      ( run(unwrap_over, Rows),
        memberchk(unwrap_count("src/engine.rs", 12), Rows),
        rows_of(Rows, diag, Diags), length(Diags, 12),
        memberchk(diag("src/engine.rs", 107, warning, "unwrap-budget",
                       "12 non-test unwraps in a changed file", 8, 16), Diags),
        memberchk(diag("src/engine.rs", 184, warning, "unwrap-budget",
                       "12 non-test unwraps in a changed file", 8, 16), Diags) )).

% The changed_file join is inside the aggregate rule, so an unchanged file
% with the same 12 hits produces no count row and no diag.
check(unwrap_unchanged_file_silent,
      ( run(unwrap_over, Rows),
        exact(Rows, unwrap_count, [unwrap_count("src/engine.rs", 12)]),
        rows_of(Rows, diag, Diags),
        \+ member(diag("src/db.rs", _, _, _, _, _, _), Diags) )).

check(unwrap_below_budget_silent,
      ( run(clean, Rows),
        exact(Rows, unwrap_count, [unwrap_count("src/engine.rs", 3)]),
        exact(Rows, diag, []) )).

% ---- the ratchet as a functional dependency, tier 0 -----------------------

check(ratchet_baseline_present_and_consistent,
      ( run(clean, Rows),
        exact(Rows, eprintln_baseline,
              [ eprintln_baseline("src/config.rs", 1),
                eprintln_baseline("src/daemon/client.rs", 2),
                eprintln_baseline("src/setup/vscode.rs", 1),
                eprintln_baseline("src/setup/wire.rs", 1) ]),
        fd_violations(Rows, []) )).

% A tightened baseline over the SAME clean world catches regrowth that the
% loose baseline let through: 2 counted against 1 allowed.
check(tightened_baseline_catches_regrowth,
      ( run(clean, Loose), exact(Loose, diag, []),
        run(tightened, Tight), exceeded_msg(2, 1, Msg),
        exact(Tight, diag,
              [ diag("src/daemon/client.rs", 1, warning, "eprintln-exceeded", Msg, none, none) ]),
        exact(Tight, check_exit, [check_exit("no-new-eprintln", 2)]) )).

% Two baseline rows for one path is an FD violation, caught over the fact set
% with no runtime notion of replacement.
check(ratchet_fd_rejects_duplicate_key,
      ( base_rows(fd_conflict, Base),
        fd_violations(Base, Violations),
        Violations == [fd_violation(eprintln_baseline, ["src/daemon/client.rs"])],
        base_rows(clean, CleanBase),
        fd_violations(CleanBase, []) )).

mentions_temporal(Term) :-
    contains_functor(Term, Name),
    memberchk(Name, [<+, pre, now, clock, tick, delta, next]).

contains_functor(Term, Name) :-
    nonvar(Term),
    compound(Term),
    (   functor(Term, Name, _)
    ;   arg(_, Term, Argument), contains_functor(Argument, Name) ).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
