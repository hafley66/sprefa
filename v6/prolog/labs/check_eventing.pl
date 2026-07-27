% check_eventing.pl : the LSP-diagnostics / agent-turn-hook loop in the
% candidate surface, with a reference tick interpreter that implements the
% level (`<-`) vs edge (`<+`) distinction and grades the retraction deltas.
%
% Run:  swipl -q -l v6/prolog/labs/check_eventing.pl -g go -g halt
%
% The claim under test (LANG.md "Rules, two arrows"):
%   a diagnostic is a LEVEL view  - it must DISAPPEAR when the code is fixed,
%   an audit log is an EDGE rel   - "it happened at tick T" never un-happens.
% v5 conflated them: `diag(...)` rows are a level view, `diag_stage(code,
% "agent-turn")` is a routing table, and nothing in the language recorded WHEN
% a diagnostic first appeared, so the agent-turn hook could not ask "what did
% I break during this turn" - only "what is broken now".
%
% Nothing is imported: other labs in this directory run concurrently.

:- use_module(library(lists)).
:- use_module(library(ordsets)).
:- use_module(library(pairs)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- discontiguous level/1.
:- discontiguous edge/1.

% ═══════════════════════════════════════════════════════════════════════════
% THE PROGRAM, in the candidate surface. Prolog terms; surface text in
% comments above each clause.
% ═══════════════════════════════════════════════════════════════════════════
%
%   enum Severity { Error, Warning }
%   rel file_line(path: Path, line_no: Int, text: Text);
%   rel ratchet(code: Key(Code), allowed: Int);
%   rel hook_window(turn: Key(TurnId), since: Tick);
%
% file_line is NOT keyed on (path, line_no): the extractor replaces a file's
% rows WHOLESALE (see the `replace/2` input op below and the .md, ambiguity 1).

keyed(ratchet, 1).
keyed(hook_window, 1).

% diagnostic(path, line, code, severity) <-
%     file_line(path, line, text), contains(text, "eprintln!");
level( diagnostic(Path, Line, eprintln_ban, warning) <-
       ( file_line(Path, Line, Text), contains(Text, "eprintln!") ) ).

level( diagnostic(Path, Line, unwrap_ban, warning) <-
       ( file_line(Path, Line, Text), contains(Text, ".unwrap()") ) ).

% lint_count(code, count(hit(path, line))) <- diagnostic(path, line, code, _);
level( lint_count(Code, count(hit(Path, Line))) <-
       diagnostic(Path, Line, Code, _) ).

% violation(code, count, allowed) <-
%     lint_count(code, count), ratchet(code, allowed), count > allowed;
level( violation(Code, Count, Allowed) <-
       ( lint_count(Code, Count), ratchet(Code, Allowed), Count > Allowed ) ).

% unratcheted_lint(code, count) <- lint_count(code, count), !ratchet(code, _);
level( unratcheted_lint(Code, Count) <-
       ( lint_count(Code, Count), \+ ratchet(Code, _) ) ).

% THE EDGE RULE. `now(tick)` reads the phantom tick column; it is not a rel
% atom, so it is never an arrival and never re-fires the rule (see the
% diag_seen twin below for what happens when the clock IS a rel).
% diag_history(path, line, code, opened_at) <+
%     diagnostic(path, line, code, _), now(opened_at);
edge( diag_history(Path, Line, Code, OpenedAt) <+
      ( diagnostic(Path, Line, Code, _), now(OpenedAt) ) ).

% The same rule written against a clock REL, kept only to grade the hazard.
% diag_seen(path, line, code, at) <+ diagnostic(path, line, code, _), tick_rel(at);
edge( diag_seen(Path, Line, Code, At) <+
      ( diagnostic(Path, Line, Code, _), tick_rel(At) ) ).

% THE HOOK ASK, as a rule: level view joined against edge history.
% turn_diag(turn, path, line, code, opened_at) <-
%     hook_window(turn, since), diagnostic(path, line, code, _),
%     diag_history(path, line, code, opened_at), opened_at > since;
level( turn_diag(Turn, Path, Line, Code, OpenedAt) <-
       ( hook_window(Turn, Since),
         diagnostic(Path, Line, Code, _),
         diag_history(Path, Line, Code, OpenedAt),
         OpenedAt > Since ) ).

% ═══════════════════════════════════════════════════════════════════════════
% THE REFERENCE INTERPRETER. state(Base, Level, Edge); one tick = apply input
% deltas, settle levels+edges, diff against the tick boundary.
% ═══════════════════════════════════════════════════════════════════════════

empty_state(state([], [], [])).

% run(+TickInputs, -Trace, -FinalState). Trace = list of delta(Tick, Plus, Minus).
run(TickInputs, Trace, FinalState) :-
    empty_state(Start),
    run_ticks(TickInputs, 1, Start, Trace, FinalState).

run_ticks([], _, State, [], State).
run_ticks([Inputs | Rest], Tick, StateIn, [Delta | Trace], StateOut) :-
    tick_step(Tick, Inputs, StateIn, StateMid, Delta),
    NextTick is Tick + 1,
    run_ticks(Rest, NextTick, StateMid, Trace, StateOut).

tick_step(Tick, Inputs, state(Base0, Level0, Edge0), state(Base, Level, Edge),
          delta(Tick, Plus, Minus)) :-
    apply_inputs(Inputs, Base0, Base),
    settle(Tick, Base, Base0, Level0, Edge0, Edge0, 0, Level, Edge),
    arrivals(Base, Base0, Level, Level0, Edge, Edge0, Plus),
    ord_subtract(Base0, Base, GoneBase),
    ord_subtract(Level0, Level, GoneLevel),
    ord_union(GoneBase, GoneLevel, Minus).

arrivals(Base, Base0, Level, Level0, Edge, Edge0, Plus) :-
    ord_subtract(Base, Base0, NewBase),
    ord_subtract(Level, Level0, NewLevel),
    ord_subtract(Edge, Edge0, NewEdge),
    ord_union(NewBase, NewLevel, Half),
    ord_union(Half, NewEdge, Plus).

% Settle both rule kinds inside the tick: levels are recomputed from scratch,
% edges append, and an appended edge row is visible to the level rules in the
% SAME tick (turn_diag reads diag_history). Edge rows only grow inside the
% loop, so it terminates; the round bound catches a diverging program.
settle(Tick, Base, Base0, Level0, Edge0, EdgeCur, Round, Level, Edge) :-
    Round < 20,
    ord_union(Base, EdgeCur, Ground),
    level_fixpoint(Ground, LevelNow),
    arrivals(Base, Base0, LevelNow, Level0, EdgeCur, Edge0, Arrived),
    ord_union(Ground, LevelNow, Facts),
    fired_edges(Arrived, Facts, Tick, Fired),
    ord_union(EdgeCur, Fired, EdgeNext),
    (   EdgeNext == EdgeCur
    ->  Level = LevelNow, Edge = EdgeCur
    ;   NextRound is Round + 1,
        settle(Tick, Base, Base0, Level0, Edge0, EdgeNext, NextRound, Level, Edge)
    ).

% Level rules: least fixpoint, recomputed from the ground set every round so
% that aggregates and negation see a settled input rather than an accumulation.
level_fixpoint(Ground, Level) :- level_rounds(Ground, [], 0, Level).

level_rounds(Ground, Current, Round, Level) :-
    Round < 20,
    ord_union(Ground, Current, Facts),
    findall(Row,
            ( level(Head <- Body), derive(Head, Body, Facts, Rows), member(Row, Rows) ),
            Derived0),
    sort(Derived0, Derived),
    (   Derived == Current
    ->  Level = Current
    ;   NextRound is Round + 1,
        level_rounds(Ground, Derived, NextRound, Level)
    ).

% derive(+Head, +Body, +Facts, -Rows). Two shapes: plain projection, and
% group-by-count when a head argument is count(Term).
derive(Head, Body, Facts, Rows) :-
    Head =.. [Name | Args],
    (   count_slot(Args, Index, Counted, GroupArgs)
    ->  findall(GroupArgs-Counted, solve(Body, Facts, no_tick), Pairs0),
        sort(Pairs0, Pairs),
        group_pairs_by_key(Pairs, Grouped),
        findall(Row,
                ( member(Group-Values, Grouped),
                  length(Values, Total),
                  nth0(Index, FullArgs, Total, Group),
                  Row =.. [Name | FullArgs] ),
                Rows)
    ;   findall(Head, ( solve(Body, Facts, no_tick), ground(Head) ), Rows)
    ).

% nonvar guard: an unbound head argument would otherwise unify with count(_)
% and turn every rule into an aggregate.
count_slot(Args, Index, Counted, GroupArgs) :-
    nth0(Index, Args, Arg, GroupArgs), nonvar(Arg), Arg = count(Counted), !.

% Edge firing: a rule fires when at least ONE body rel atom unifies with a row
% that ARRIVED this tick; the remaining atoms join against the current sets
% (LANG.md's semi-naive shape).
fired_edges(Arrived, Facts, Tick, Rows) :-
    findall(Head,
            ( edge(Head <+ Body),
              body_atoms(Body, Atoms),
              member(Atom, Atoms),
              member(Arrival, Arrived),
              Atom = Arrival,
              solve(Body, Facts, Tick),
              ground(Head) ),
            Rows0),
    sort(Rows0, Rows).

solve(true, _, _) :- !.
solve((Left, Right), Facts, Tick) :- !,
    solve(Left, Facts, Tick), solve(Right, Facts, Tick).
solve(\+ Atom, Facts, Tick) :- !, \+ solve(Atom, Facts, Tick).
solve(now(Tick), _, Tick) :- !, Tick \== no_tick.
solve(Guard, _, _) :- is_guard(Guard), !, eval_guard(Guard).
solve(Atom, Facts, _) :- member(Atom, Facts).

is_guard(Guard) :- callable(Guard), functor(Guard, Name, 2), guard_name(Name).
guard_name(>). guard_name(>=). guard_name(<). guard_name(=<).
guard_name(==). guard_name(\==). guard_name(contains).

eval_guard(contains(Text, Needle)) :- !, sub_string(Text, _, _, _, Needle).
eval_guard(Guard) :- call(Guard).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(\+ _, []) :- !.
body_atoms(now(_), []) :- !.
body_atoms(Guard, []) :- is_guard(Guard), !.
body_atoms(Atom, [Atom]).

% Input ops: add(Row) | del(Row) | replace(Pattern, Rows).
% `replace` is the extraction op: every base row matching Pattern goes away and
% the listed rows land, in ONE tick, so a row present before and after never
% shows up in either delta list.
apply_inputs([], Base, Base).
apply_inputs([Op | Ops], Base0, Base) :-
    apply_input(Op, Base0, Base1),
    apply_inputs(Ops, Base1, Base).

apply_input(add(Row), Base0, Base) :- insert_row(Row, Base0, Base).
apply_input(del(Row), Base0, Base) :- ord_del_element(Base0, Row, Base).
apply_input(replace(Pattern, Rows), Base0, Base) :-
    findall(Old, ( member(Old, Base0), subsumes_term(Pattern, Old) ), Stale0),
    sort(Stale0, Stale),
    ord_subtract(Base0, Stale, Kept),
    foldl(insert_row, Rows, Kept, Base).

% A keyed rel holds at most one row per key: the insert REPLACES, which is
% where the ratchet's -old/+new delta comes from.
insert_row(Row, Base0, Base) :-
    functor(Row, Name, Arity),
    (   keyed(Name, KeyArity)
    ->  Row =.. [Name | Args],
        length(KeyPrefix, KeyArity),
        append(KeyPrefix, _, Args),
        findall(Old,
                ( member(Old, Base0), functor(Old, Name, Arity),
                  Old =.. [Name | OldArgs], append(KeyPrefix, _, OldArgs) ),
                Displaced0),
        sort(Displaced0, Displaced),
        ord_subtract(Base0, Displaced, Kept)
    ;   Kept = Base0
    ),
    ord_add_element(Kept, Row, Base).

% ═══════════════════════════════════════════════════════════════════════════
% THE SCENARIO: one file, edited across seven ticks.
% ═══════════════════════════════════════════════════════════════════════════

src("src/a.rs").

line(head,    1, "fn main() {").
line(bad3,    3, "    eprintln!(\"boom\");").
line(good3,   3, "    tracing::info!(\"boom\");").
line(bad5,    5, "    eprintln!(\"again\");").
line(good5,   5, "    tracing::debug!(\"again\");").
line(bad7,    7, "    eprintln!(\"third\");").
line(good7,   7, "    tracing::warn!(\"third\");").
line(unwrap9, 9, "    let value = maybe().unwrap();").
line(good9,   9, "    let value = maybe()?;").

% file(+Names, -ReplaceOp): the extractor's wholesale per-file row replacement.
file(Names, replace(file_line(Path, _, _), Rows)) :-
    src(Path),
    findall(file_line(Path, No, Text),
            ( member(Name, Names), line(Name, No, Text) ),
            Rows).

scenario(Ticks) :-
    file([head, bad3, bad5],                   Tick1File),
    file([head, bad3, bad5, bad7],             Tick2File),
    file([head, good3, bad5, good7],           Tick3File),
    file([head, bad3, bad5, good7, unwrap9],   Tick5File),
    file([head, good3, good5, good7, good9],   Tick7File),
    Ticks = [
      % T1: two banned calls, ratchet grandfathered at 2. No violation.
      [ Tick1File, add(ratchet(eprintln_ban, 2)), replace(tick_rel(_), [tick_rel(1)]) ],
      % T2: a third banned call arrives. 3 > 2, violation appears.
      [ Tick2File, replace(tick_rel(_), [tick_rel(2)]) ],
      % T3: lines 3 and 7 fixed. Violation and both diagnostics retract;
      %     line 5's diagnostic holds across the wholesale replace.
      [ Tick3File, replace(tick_rel(_), [tick_rel(3)]) ],
      % T4: the ratchet tightens to 1 (keyed replace). Count is 1: still green.
      [ add(ratchet(eprintln_ban, 1)), replace(tick_rel(_), [tick_rel(4)]) ],
      % T5: line 3 regresses and an unwrap lands. 2 > 1 fires against the
      %     TIGHTENED ratchet; unwrap_ban has no ratchet row at all.
      [ Tick5File, replace(tick_rel(_), [tick_rel(5)]) ],
      % T6: the agent-turn hook asks: what appeared after tick 4 and still holds?
      [ add(hook_window(turn_7, 4)), replace(tick_rel(_), [tick_rel(6)]) ],
      % T7: everything fixed. turn_diag empties; diag_history does not.
      [ Tick7File, replace(tick_rel(_), [tick_rel(7)]) ]
    ].

trace(Trace, Final) :- scenario(Ticks), run(Ticks, Trace, Final).

at_tick(Trace, Tick, Plus, Minus) :- memberchk(delta(Tick, Plus, Minus), Trace).

rows_of(state(Base, Level, Edge), Name, Rows) :-
    ord_union(Base, Level, Half),
    ord_union(Half, Edge, All),
    findall(Row, ( member(Row, All), functor(Row, Name, _) ), Rows).

state_after(Tick, State) :-
    scenario(Ticks),
    length(Prefix, Tick), append(Prefix, _, Ticks),
    empty_state(Start),
    run_ticks(Prefix, 1, Start, _, State).

% ═══════════════════════════════════════════════════════════════════════════
% THE GRADER
% ═══════════════════════════════════════════════════════════════════════════

% (b) the level rule derives a diagnostic per offending line.
check(level_diag_appears,
      ( trace(Trace, _), at_tick(Trace, 1, Plus, _),
        memberchk(diagnostic("src/a.rs", 3, eprintln_ban, warning), Plus),
        memberchk(diagnostic("src/a.rs", 5, eprintln_ban, warning), Plus),
        memberchk(lint_count(eprintln_ban, 2), Plus) )).

% (c) THE HEADLINE: fixing the code retracts the diagnostic. -diagnostic in
% the tick-3 delta, and no + for the same row.
check(level_diag_retracts_on_fix,
      ( trace(Trace, _), at_tick(Trace, 3, Plus, Minus),
        memberchk(diagnostic("src/a.rs", 3, eprintln_ban, warning), Minus),
        memberchk(diagnostic("src/a.rs", 7, eprintln_ban, warning), Minus),
        \+ memberchk(diagnostic("src/a.rs", 3, eprintln_ban, warning), Plus) )).

% (a) wholesale per-file replacement does NOT flicker a row that holds before
% and after: line 5 is in neither delta list at tick 3.
check(wholesale_replace_no_flicker,
      ( trace(Trace, _), at_tick(Trace, 3, Plus, Minus),
        \+ memberchk(diagnostic("src/a.rs", 5, eprintln_ban, warning), Plus),
        \+ memberchk(diagnostic("src/a.rs", 5, eprintln_ban, warning), Minus),
        \+ memberchk(file_line("src/a.rs", 5, _), Plus),
        \+ memberchk(file_line("src/a.rs", 5, _), Minus) )).

% ... but a line that MOVED does churn: the count row is (path, line_no), so
% the delta is keyed on a coordinate the edit can shift. Receipt for the .md.
check(count_row_replaces_by_key,
      ( trace(Trace, _), at_tick(Trace, 2, Plus, Minus),
        memberchk(lint_count(eprintln_ban, 3), Plus),
        memberchk(lint_count(eprintln_ban, 2), Minus) )).

% (d) the edge rel appended on appearance and survived the fix.
check(edge_history_survives_fix,
      ( state_after(3, State), rows_of(State, diag_history, Rows),
        memberchk(diag_history("src/a.rs", 3, eprintln_ban, 1), Rows),
        memberchk(diag_history("src/a.rs", 7, eprintln_ban, 2), Rows),
        state_after(3, S2), rows_of(S2, diagnostic, Live),
        \+ memberchk(diagnostic("src/a.rs", 3, eprintln_ban, _), Live) )).

% a re-appearance opens a SECOND episode; the first row stays.
check(edge_history_reopens,
      ( trace(Trace, _), at_tick(Trace, 5, Plus, _),
        memberchk(diag_history("src/a.rs", 3, eprintln_ban, 5), Plus),
        state_after(5, State), rows_of(State, diag_history, Rows),
        memberchk(diag_history("src/a.rs", 3, eprintln_ban, 1), Rows) )).

% edge rows never retract, at any tick, for any reason.
check(edge_never_retracts,
      ( trace(Trace, _),
        forall(( member(delta(_, _, Minus), Trace), member(Row, Minus) ),
               ( \+ functor(Row, diag_history, _), \+ functor(Row, diag_seen, _) )) )).

% THE CLOCK HAZARD: the same rule written against a clock REL re-fires on
% every tick, because the clock row's arrival is an arrival like any other.
% 5 history rows (one per appearance) vs 13 seen rows (one per live
% diagnostic per tick).
check(clock_rel_join_storms,
      ( state_after(7, State),
        rows_of(State, diag_history, History), length(History, 5),
        rows_of(State, diag_seen, Seen), length(Seen, 13) )).

% (e) THE RATCHET, full sequence.
check(ratchet_holds_at_baseline,
      ( trace(Trace, _), at_tick(Trace, 1, Plus, _),
        \+ memberchk(violation(eprintln_ban, _, _), Plus),
        state_after(1, State), rows_of(State, violation, []) )).

check(ratchet_fires_on_growth,
      ( trace(Trace, _), at_tick(Trace, 2, Plus, _),
        memberchk(violation(eprintln_ban, 3, 2), Plus) )).

check(ratchet_violation_retracts,
      ( trace(Trace, _), at_tick(Trace, 3, _, Minus),
        memberchk(violation(eprintln_ban, 3, 2), Minus),
        state_after(3, State), rows_of(State, violation, []) )).

% the keyed rel replaces: -old/+new in one tick, no violation on the way past.
check(ratchet_tightens_by_key,
      ( trace(Trace, _), at_tick(Trace, 4, Plus, Minus),
        memberchk(ratchet(eprintln_ban, 1), Plus),
        memberchk(ratchet(eprintln_ban, 2), Minus),
        \+ memberchk(violation(_, _, _), Plus),
        state_after(4, State), rows_of(State, ratchet, [ratchet(eprintln_ban, 1)]) )).

check(tightened_ratchet_catches_regrowth,
      ( trace(Trace, _), at_tick(Trace, 5, Plus, _),
        memberchk(violation(eprintln_ban, 2, 1), Plus) )).

% v5's second diag rule: a lint code with no ratchet row at all.
check(unratcheted_lint_fires,
      ( trace(Trace, _), at_tick(Trace, 5, Plus, _),
        memberchk(unratcheted_lint(unwrap_ban, 1), Plus),
        trace(Trace, _), at_tick(Trace, 7, _, Minus7),
        memberchk(unratcheted_lint(unwrap_ban, 1), Minus7) )).

% (3) THE HOOK: level view joined with edge history, windowed by a demand row.
check(hook_window_join,
      ( trace(Trace, _), at_tick(Trace, 6, Plus, _),
        memberchk(turn_diag(turn_7, "src/a.rs", 3, eprintln_ban, 5), Plus),
        memberchk(turn_diag(turn_7, "src/a.rs", 9, unwrap_ban, 5), Plus),
        state_after(6, State), rows_of(State, turn_diag, Rows), length(Rows, 2) )).

% the diagnostic open since tick 1 is NOT in the turn's window.
check(hook_window_excludes_old,
      ( state_after(6, State), rows_of(State, turn_diag, Rows),
        \+ memberchk(turn_diag(_, "src/a.rs", 5, _, _), Rows) )).

% fixing during the turn empties the hook's answer; history keeps the record.
check(hook_window_excludes_fixed,
      ( state_after(7, State), rows_of(State, turn_diag, []),
        rows_of(State, diagnostic, []),
        rows_of(State, diag_history, History), length(History, 5),
        memberchk(diag_history("src/a.rs", 9, unwrap_ban, 5), History) )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).
