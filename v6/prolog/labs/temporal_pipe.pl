% temporal_pipe.pl : is `|>` a real operator, and what does it cost in prolog?
%
% Run:     swipl -q -l v6/prolog/labs/temporal_pipe.pl -g go -g halt
% Trace:   swipl -q -l v6/prolog/labs/temporal_pipe.pl -g report -g halt
%
% Claim under test (user proposal, 2026-07-27): a temporal pipe `|>` that may
% ONLY cross a time cut (yield point, edge append, key replace), with comma
% staying the within-cut join. A chain desugars to N rules with generated
% intermediate rel names. Claimed bonuses: the piped-in atom is unambiguously
% the trigger (consolidation ruling R5), and mode threads visibly per stage.
%
% SPELLING DEVIATION, forced and graded (check pipe_glyph_needs_quotes):
% `|>` cannot be lexed by any ISO/SWI prolog reader. `|` is a SOLO character,
% not a symbol character, so `|` and `>` never fuse into one atom. The lab
% therefore writes the pipe as `~>` (all symbol chars) and grades the `|>`
% failure separately. Priority and associativity, the things actually under
% test, are glyph-independent.
%
% OTHER DEVIATIONS from LANG.md, all deliberate:
%   only/1        the per-atom trigger marker LANG.md does not have. This lab
%                 needs it to state R5 at all; the pipe GENERATES it, which is
%                 the whole R5 argument. Hand-written comparison programs write
%                 it out by hand.
%   effect demand the effect's demand row is not modeled. Stage 1 mentions the
%                 effect rel and the response row is injected at a later tick,
%                 which is the yield the boundary law cares about.
%   not/1         stands in for the surface's `!rel`, which is unlexable in
%                 prolog (check bang_negation_is_unlexable).

:- use_module('../src/grader.pl').
:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(ordsets)).

:- op(1150, xfx, <-).       % LEVEL rule
:- op(1150, xfx, <+).       % EDGE rule
:- op(1100, xfy, ~>).       % THE PIPE, at the lab's working priority
:- op(1100, xfy, '|>').     % the user's glyph, declared so the read can be
                            % attempted; only the QUOTED form ever reads

:- dynamic current_program/1.
:- discontiguous in_program/1, rule_of/2, chain_of/2, decl_of/2,
                 scenario/4, check/2.

% ═══ the reader hook ═══════════════════════════════════════════════════════
% `in_program(Name).` opens a program; every following bare `<-` / `<+` clause
% joins it. A body whose top functor is `~>` is stored RAW as a chain, never
% desugared at load time, because half this lab's grading is about desugar
% REJECTING things and a throwing term_expansion would kill the file.

term_expansion(in_program(Name), [in_program(Name)]) :-
    retractall(current_program(_)),
    assertz(current_program(Name)).

term_expansion((Head <+ Body), Stored) :-
    current_program(Name), store_clause(Name, (<+), Head, Body, Stored).
term_expansion((Head <- Body), Stored) :-
    current_program(Name), store_clause(Name, (<-), Head, Body, Stored).

store_clause(Name, Arrow, Head, Body, chain_of(Name, chain(Arrow, Head, Body))) :-
    nonvar(Body), Body = (_ ~> _), !.
store_clause(Name, (<+), Head, Body, rule_of(Name, (Head <+ Body))).
store_clause(Name, (<-), Head, Body, rule_of(Name, (Head <- Body))).

% ═══ THE DESUGARER ═════════════════════════════════════════════════════════
% desugar(Program, chain(Arrow, Head, Chain), Rules, Cuts)
%
%   N stages  ->  N rules.
%   rule 1      Inter1(Carried1)  <+  Stage1
%   rule k      InterK(CarriedK)  <+  only(Inter(k-1)(Carried(k-1))), StageK
%   rule N      Head              <Arrow>  only(Inter(N-1)(...)), StageN
%
% Intermediate writes are ALWAYS `<+`: a cut was crossed, so the intermediate
% is an occurrence, not a membership view. The declared arrow governs the
% final head only. That is the first clause-shape departure: one arrow token
% in the text, N writes in the lowering.

desugar(Program, chain(Arrow, Head, Chain), Rules, Cuts) :-
    chain_stages(Chain, Stages),
    length(Stages, StageCount),
    StageCount >= 2,
    chain_cuts(Program, Head, Stages, Cuts),
    check_head_safety(Head, Stages),
    Boundaries is StageCount - 1,
    numlist(1, Boundaries, BoundaryIndices),
    findall(Carried, ( member(Index, BoundaryIndices),
                       carried_columns(Head, Stages, Index, Carried) ),
            CarriedLists0),
    reattach_variables(Head, Stages, CarriedLists0, CarriedLists),
    functor(Head, HeadName, _),
    maplist(intermediate_name(HeadName), BoundaryIndices, Names),
    build_rules(Arrow, Head, Stages, CarriedLists, Names, Rules).

% ── stage flattening. A stage may NOT contain a pipe anywhere inside it.
%    The nesting parses fine (prolog is happy); the desugarer is what says no.
chain_stages(Chain, [Stage | Rest]) :-
    nonvar(Chain), Chain = (Stage ~> Tail), !,
    reject_inner_pipe(Stage),
    chain_stages(Tail, Rest).
chain_stages(Stage, [Stage]) :-
    reject_inner_pipe(Stage).

reject_inner_pipe(Stage) :-
    (   contains_pipe(Stage)
    ->  throw(nested_pipe_in_stage(Stage))
    ;   true ).

contains_pipe(Term) :-
    nonvar(Term),
    (   Term = (_ ~> _)
    ->  true
    ;   compound(Term), arg(_, Term, Argument), contains_pipe(Argument) ).

% ── THE BOUNDARY LAW. A `|>` is legal only if the boundary crosses time.
%    Evidence is read off the SOURCE stage, except the last boundary, which
%    the head's own storage kind may justify. Every clause consults
%    decl_of/2: the law is NOT decidable from the chain text alone.
chain_cuts(Program, Head, Stages, Cuts) :-
    length(Stages, StageCount),
    Boundaries is StageCount - 1,
    numlist(1, Boundaries, Indices),
    maplist(one_cut(Program, Head, Stages), Indices, Cuts).

one_cut(Program, Head, Stages, Index, cut(Index, Kind, Evidence)) :-
    (   cut_evidence(Program, Head, Stages, Index, Kind, Evidence)
    ->  true
    ;   nth1(Index, Stages, Stage),
        throw(no_time_cut(Index, Stage)) ).

cut_evidence(Program, _Head, Stages, Index, yield, source_stage(Ref)) :-
    nth1(Index, Stages, Stage),
    stage_mentions(Program, Stage, effect(Ref)), !.
cut_evidence(Program, _Head, Stages, Index, edge_append, source_stage(Ref)) :-
    nth1(Index, Stages, Stage),
    stage_mentions(Program, Stage, append(Ref)), !.
cut_evidence(Program, Head, Stages, Index, key_replace, head(Ref)) :-
    last_boundary(Stages, Index),
    rel_of(Head, Ref),
    decl_of(Program, keyed(Ref, _)), !.
cut_evidence(Program, Head, Stages, Index, edge_append, head(Ref)) :-
    last_boundary(Stages, Index),
    rel_of(Head, Ref),
    decl_of(Program, append(Ref)), !.

last_boundary(Stages, Index) :- length(Stages, Count), Index =:= Count - 1.

stage_mentions(Program, Stage, Declaration) :-
    body_atoms(Stage, Atoms),
    member(Atom, Atoms),
    rel_of(Atom, Ref),
    declaration_ref(Declaration, Ref),
    decl_of(Program, Declaration), !.

declaration_ref(effect(Ref), Ref).
declaration_ref(append(Ref), Ref).

% ── head safety: every head variable must be bound by some stage.
check_head_safety(Head, Stages) :-
    term_variables(Head, HeadVariables),
    term_variables(Stages, StageVariables),
    (   member(Loose, HeadVariables), \+ same_variable_in(Loose, StageVariables)
    ->  throw(unsafe_head_variable(Head))
    ;   true ).

% ── BINDING FLOW, the one rule: a variable crosses boundary k when it is
%    bound upstream of k AND referenced downstream of k (later stages or the
%    head). Inferred, not declared. Order = first appearance upstream.
%    Consequence graded below: a variable used in stage 1 and stage 3 but not
%    stage 2 is still carried through boundary 2, so skipping works.
carried_columns(Head, Stages, Index, Carried) :-
    length(Prefix, Index),
    append(Prefix, Suffix, Stages),
    term_variables(Prefix, Upstream),
    term_variables(Suffix + Head, Downstream),
    keep_shared(Upstream, Downstream, Carried).

keep_shared([], _, []).
keep_shared([Variable | Rest], Pool, Kept) :-
    (   same_variable_in(Variable, Pool)
    ->  Kept = [Variable | More]
    ;   Kept = More ),
    keep_shared(Rest, Pool, More).

same_variable_in(Variable, [Head | Tail]) :-
    (   Variable == Head -> true ; same_variable_in(Variable, Tail) ).

% findall/3 copies terms, which would sever the carried variables from the
% stages they came from. Recompute in one pass instead.
reattach_variables(Head, Stages, Copies, Real) :-
    length(Copies, Count),
    numlist(1, Count, Indices),
    maplist(carried_columns(Head, Stages), Indices, Real).

intermediate_name(HeadName, Index, Name) :-
    format(atom(Name), "pipe_~w_~w", [HeadName, Index]).

build_rules(Arrow, Head, [FirstStage | RestStages],
            [FirstCarried | RestCarried], [FirstName | RestNames],
            [(FirstInter <+ FirstStage) | Tail]) :-
    FirstInter =.. [FirstName | FirstCarried],
    build_tail(Arrow, Head, RestStages, RestCarried, RestNames, FirstInter, Tail).

build_tail(Arrow, Head, [LastStage], [], [], PrevInter, [Rule]) :- !,
    make_rule(Arrow, Head, (only(PrevInter), LastStage), Rule).
build_tail(Arrow, Head, [Stage | More], [Carried | MoreCarried],
           [Name | MoreNames], PrevInter, [Rule | Rest]) :-
    Inter =.. [Name | Carried],
    Rule = (Inter <+ (only(PrevInter), Stage)),
    build_tail(Arrow, Head, More, MoreCarried, MoreNames, Inter, Rest).

make_rule((<+), Head, Body, (Head <+ Body)).
make_rule((<-), Head, Body, (Head <- Body)).

% ═══ program assembly ══════════════════════════════════════════════════════

program(Name, prog(Rules, Decls)) :-
    findall(Rule, rule_of(Name, Rule), Plain),
    findall(Generated, ( chain_of(Name, Chain),
                         desugar(Name, Chain, Generated, _) ), Lists),
    append([Plain | Lists], Rules),
    findall(Decl, decl_of(Name, Decl), Decls).

chain_rules(Name, Rules) :-
    chain_of(Name, Chain), desugar(Name, Chain, Rules, _).

chain_cut_kinds(Name, Cuts) :-
    chain_of(Name, Chain), desugar(Name, Chain, _, Cuts).

% ═══ the tick, copied from merge_family.pl with ONE change ═════════════════
% merge_family's tick fires edge rules on rows that arrived from OUTSIDE. A
% pipe chain is edge feeding edge, so rows an edge rule WROTE this tick must
% be arrivals for the next one, or a chain past two stages never moves. The
% carry set is that change; ambiguity 6 in the .md.

rel_of(Atom, Name/Arity) :- functor(Atom, Name, Arity).

rule_kind_of(Rules, Ref, level) :- member((Head <- _), Rules), rel_of(Head, Ref), !.
rule_kind_of(Rules, Ref, edge)  :- member((Head <+ _), Rules), rel_of(Head, Ref), !.
rule_kind_of(_,     _,   source).

level_rel(Rules, Ref) :- rule_kind_of(Rules, Ref, level).

decl_key(Decls, Ref, KeyPositions) :- memberchk(keyed(Ref, KeyPositions), Decls).

key_of(KeyPositions, Row, Key) :-
    Row =.. [_ | Args],
    findall(Column, ( member(Position, KeyPositions), nth1(Position, Args, Column) ), Key).

solve(true,          _,    _)    :- !.
solve((Left, Right), Rows, Prev) :- !, solve(Left, Rows, Prev), solve(Right, Rows, Prev).
solve(not(Goal),     Rows, Prev) :- !, \+ solve(Goal, Rows, Prev).
solve(only(Atom),    Rows, _)    :- !, member(Atom, Rows).
solve(pre(Atom),     _,    Prev) :- !, member(Atom, Prev).
solve(Goal,          _,    _)    :- arithmetic_goal(Goal), !, call(Goal).
solve(Atom,          Rows, _)    :- member(Atom, Rows).

arithmetic_goal(Goal) :-
    functor(Goal, Name, Arity),
    memberchk(Name/Arity, [ (is)/2, (<)/2, (>)/2, (=<)/2, (>=)/2,
                            (=:=)/2, (=\=)/2, (==)/2, (\==)/2, (=)/2 ]).

body_atoms((Left, Right), Atoms) :- !,
    body_atoms(Left, LeftAtoms), body_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
body_atoms(only(Atom), [Atom]) :- !.
body_atoms(pre(_),     []) :- !.
body_atoms(not(_),     []) :- !.
body_atoms(true,       []) :- !.
body_atoms(Goal,       []) :- arithmetic_goal(Goal), !.
body_atoms(Atom,       [Atom]).

% R5, mechanised: a body carrying `only/1` markers fires on THOSE atoms only.
% Everything else keeps LANG.md's any-atom rule.
trigger_atoms(Body, Triggers) :-
    marked_atoms(Body, Marked),
    (   Marked == [] -> body_atoms(Body, Triggers) ; Triggers = Marked ).

marked_atoms((Left, Right), Atoms) :- !,
    marked_atoms(Left, LeftAtoms), marked_atoms(Right, RightAtoms),
    append(LeftAtoms, RightAtoms, Atoms).
marked_atoms(only(Atom), [Atom]) :- !.
marked_atoms(_, []).

level_closure(Rules, Base, Prev, Level) :- level_step(Rules, Base, Prev, [], Level).

level_step(Rules, Base, Prev, Known0, Level) :-
    append(Base, Known0, Visible),
    findall(Head, ( member((Head <- Body), Rules), solve(Body, Visible, Prev) ), Heads),
    append(Known0, Heads, Merged0),
    sort(Merged0, Merged),
    (   Merged == Known0
    ->  Level = Known0
    ;   level_step(Rules, Base, Prev, Merged, Level) ).

edge_derivations(Rules, Now, Prev, Arrived, Derived) :-
    findall(Head,
            ( member((Head <+ Body), Rules),
              trigger_atoms(Body, Triggers),
              member(Trigger, Triggers),
              member(Trigger, Arrived),
              solve(Body, Now, Prev) ),
            Derived0),
    dedupe_keep_order(Derived0, Derived).

check_key_conflicts(Decls, Derived) :-
    forall(( member(Row, Derived), rel_of(Row, Ref),
             decl_key(Decls, Ref, KeyPositions), key_of(KeyPositions, Row, Key) ),
           ( findall(Other, ( member(Other, Derived), rel_of(Other, Ref),
                              key_of(KeyPositions, Other, Key) ), Others0),
             sort(Others0, Others),
             (   Others = [_] -> true ; throw(keyed_conflict(Ref, Key, Others)) ) )).

row_write(Decls, Current, Row, Write) :-
    rel_of(Row, Ref),
    (   decl_key(Decls, Ref, KeyPositions)
    ->  key_of(KeyPositions, Row, Key),
        (   member(Old, Current), rel_of(Old, Ref), key_of(KeyPositions, Old, Key)
        ->  ( Old == Row -> Write = noop(Row) ; Write = replace(Old, Row) )
        ;   Write = add(Row) )
    ;   ( memberchk(Row, Current) -> Write = noop(Row) ; Write = add(Row) ) ).

apply_writes(Rows, [], Rows).
apply_writes(Rows, [add(Row) | Rest], Out) :- !, apply_writes([Row | Rows], Rest, Out).
apply_writes(Rows, [replace(Old, New) | Rest], Out) :- !,
    exclude(==(Old), Rows, Kept), apply_writes([New | Kept], Rest, Out).
apply_writes(Rows, [noop(_) | Rest], Out) :- apply_writes(Rows, Rest, Out).

written_rows([], []).
written_rows([add(Row) | Rest], [Row | Out])         :- !, written_rows(Rest, Out).
written_rows([replace(_, Row) | Rest], [Row | Out])  :- !, written_rows(Rest, Out).
written_rows([noop(_) | Rest], Out)                  :- written_rows(Rest, Out).

apply_arrivals(Rows, [], Rows).
apply_arrivals(Rows, [Delta | Rest], Out) :-
    (   Delta = +Fact -> Next = [Fact | Rows]
    ;   Delta = -Fact, exclude(==(Fact), Rows, Next) ),
    apply_arrivals(Next, Rest, Out).

tick(prog(Rules, Decls), StartAll, CarryIn, Arrivals, NextAll, CarryOut, Deltas) :-
    findall(Row, ( member(Row, StartAll), rel_of(Row, Ref), \+ level_rel(Rules, Ref) ),
            StartPersistent),
    apply_arrivals(StartPersistent, Arrivals, WithArrivals0),
    sort(WithArrivals0, WithArrivals),
    level_closure(Rules, WithArrivals, StartAll, MidLevel),
    union_rows(WithArrivals, MidLevel, MidAll),
    ord_subtract(MidAll, StartAll, FreshArrivals),
    append(CarryIn, FreshArrivals, Arrived0),
    dedupe_keep_order(Arrived0, Arrived),
    edge_derivations(Rules, MidAll, StartAll, Arrived, Derived),
    check_key_conflicts(Decls, Derived),
    maplist(row_write(Decls, WithArrivals), Derived, Writes),
    apply_writes(WithArrivals, Writes, NextPersistent0),
    sort(NextPersistent0, NextPersistent),
    level_closure(Rules, NextPersistent, StartAll, NextLevel),
    union_rows(NextPersistent, NextLevel, NextAll),
    written_rows(Writes, CarryOut),
    row_deltas(StartAll, NextAll, Deltas).

row_deltas(Old, New, Deltas) :-
    ord_subtract(Old, New, Removed),
    ord_subtract(New, Old, Added),
    findall(-Row, member(Row, Removed), Retractions),
    findall(+Row, member(Row, Added),   Assertions),
    append(Retractions, Assertions, Deltas).

union_rows(Left, Right, Union) :- append(Left, Right, Both), sort(Both, Union).

dedupe_keep_order([], []).
dedupe_keep_order([Item | Rest], [Item | Out]) :-
    exclude(==(Item), Rest, Filtered), dedupe_keep_order(Filtered, Out).

run_trace(Name, Initial, ArrivalTicks, FinalRows, DeltaTicks) :-
    program(Name, Prog), sort(Initial, Start),
    run_ticks(Prog, Start, [], ArrivalTicks, FinalRows, DeltaTicks).

run_ticks(_, Rows, _, [], Rows, []).
run_ticks(Prog, Rows, Carry, [Arrivals | Rest], Final, [Deltas | More]) :-
    tick(Prog, Rows, Carry, Arrivals, Next, NextCarry, Deltas),
    run_ticks(Prog, Next, NextCarry, Rest, Final, More).

run_named(ScenarioName, FinalRows, DeltaTicks) :-
    scenario(ScenarioName, Program, Initial, ArrivalTicks),
    run_trace(Program, Initial, ArrivalTicks, FinalRows, DeltaTicks).

rel_rows(Ref, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), rel_of(Row, Ref) ), Selected).

rel_deltas(Ref, DeltaTicks, Selected) :- maplist(rel_delta_tick(Ref), DeltaTicks, Selected).

rel_delta_tick(Ref, Deltas, Selected) :-
    findall(Delta, ( member(Delta, Deltas), delta_row(Delta, Row), rel_of(Row, Ref) ), Selected).

delta_row(+Row, Row).
delta_row(-Row, Row).

% ═══ reading experiments: what prolog actually does with the surface ═══════

read_shape(Text, Shape, PrincipalFunctor) :-
    (   catch(read_term_from_atom(Text, Term, []), Error, Term = threw(Error))
    ->  true
    ;   Term = read_failed ),
    term_shape(Term, Shape),
    (   compound(Term) -> functor(Term, Name, Arity), PrincipalFunctor = Name/Arity
    ;   PrincipalFunctor = none ).

term_shape(Term, hole)  :- var(Term), !.
term_shape(Term, Term)  :- atomic(Term), !.
term_shape(Term, Shape) :-
    Term =.. [Functor | Args], maplist(term_shape, Args, ArgShapes),
    Shape =.. [Functor | ArgShapes].

% read the same text under a temporary priority for `~>`, then put it back.
read_at_priority(Priority, Type, Text, Shape, PrincipalFunctor) :-
    setup_call_cleanup(op(Priority, Type, ~>),
                       read_shape(Text, Shape, PrincipalFunctor),
                       op(1100, xfy, ~>)).

threw_syntax_error(Shape) :- Shape = threw(error(syntax_error(_), _)).

% A literal `'.'(A, B)` written in a clause body is rewritten by SWI's own
% functional-notation expansion into a ./3 dict call, so the checks below
% build the term with =.. instead of writing it. That the LAB cannot even
% quote the shape it is grading is part of the dot-access finding.
dot_term(Left, Key, Term)     :- Term =.. ['.', Left, Key].
dot_goal(Left, Key, Out, Goal) :- Goal =.. ['.', Left, Key, Out].

% ═══ PROGRAM 1: the ghcacher chain, three stages ═══════════════════════════
% demand join  |>  response fold  |>  append. The yield is faked by injecting
% the fetch response as an arrival at a later tick.

in_program(pipe_feed).

decl_of(pipe_feed, effect(fetch/4)).
decl_of(pipe_feed, append(change_log/3)).

change_log(Endpoint, Stars, Client) <+
      watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
      fetch(Endpoint, PrevTag, Bucket, Result)
    ~> Result = fresh(_Tag, Body), stars_of(Body, Stars)
    ~> subscribed_to(Client, Endpoint).

% ═══ PROGRAM 2: the same three rules, written out by hand ═════════════════
% Intermediate rels named by a human, `only/1` typed by a human. The desugar
% claim is that program 1 and program 2 produce the same observable trace.

in_program(hand_feed).

decl_of(hand_feed, append(change_log/3)).

demand_row(Endpoint, Result) <+
    watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
    fetch(Endpoint, PrevTag, Bucket, Result).

folded_row(Endpoint, Stars) <+
    only(demand_row(Endpoint, Result)),
    Result = fresh(_Tag, Body), stars_of(Body, Stars).

change_log(Endpoint, Stars, Client) <+
    only(folded_row(Endpoint, Stars)), subscribed_to(Client, Endpoint).

% ═══ PROGRAM 3: the same three rules with the trigger marker REMOVED ══════
% LANG.md's any-atom edge trigger. This is what R5 lost when delta() died.

in_program(unmarked_feed).

decl_of(unmarked_feed, append(change_log/3)).

demand_row(Endpoint, Result) <+
    watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
    fetch(Endpoint, PrevTag, Bucket, Result).

folded_row(Endpoint, Stars) <+
    demand_row(Endpoint, Result),
    Result = fresh(_Tag, Body), stars_of(Body, Stars).

change_log(Endpoint, Stars, Client) <+
    folded_row(Endpoint, Stars), subscribed_to(Client, Endpoint).

% ── one arrival schedule, shared by programs 1-3 ───────────────────────────
feed_ticks(
  [ [ +watch(cli), +cache_tag(cli, no_tag), +stars_of(body1, 42),
      +subscribed_to(alice, cli), +subscribed_to(bob, other) ]
  , [ +every_300(bucket1) ]
  , [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ]   % the yield lands
  , [ ]
  , [ ]
  , [ +subscribed_to(carol, cli) ]                           % late subscriber
  ]).

scenario(feed_piped,    pipe_feed,      [], Ticks) :- feed_ticks(Ticks).
scenario(feed_hand,     hand_feed,      [], Ticks) :- feed_ticks(Ticks).
scenario(feed_unmarked, unmarked_feed,  [], Ticks) :- feed_ticks(Ticks).

% ═══ PROGRAM 4: two stages, keyed head, cut = the yield ═══════════════════

in_program(pipe_cache).

decl_of(pipe_cache, effect(fetch/4)).
decl_of(pipe_cache, keyed(cache/2, [1])).

cache(Endpoint, Tag) <+
      watch(Endpoint), cache(Endpoint, PrevTag), every_300(Bucket),
      fetch(Endpoint, PrevTag, Bucket, Result)
    ~> Result = fresh(Tag, _Body).

scenario(cache_replace, pipe_cache, [ cache(cli, no_tag) ],
  [ [ +watch(cli) ]
  , [ +every_300(bucket1) ]
  , [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ]
  , [ ]
  ]).

% ═══ PROGRAM 5: one stage carrying negation AND a comparison ══════════════

in_program(pipe_guard).

decl_of(pipe_guard, effect(fetch/4)).
decl_of(pipe_guard, append(alert/2)).

alert(Endpoint, Stars) <+
      watch(Endpoint), every_300(Bucket), fetch(Endpoint, no_tag, Bucket, Result)
    ~> Result = fresh(_Tag, Body), stars_of(Body, Stars),
       Stars > 100, not(muted(Endpoint)).

guard_ticks(Muted, Stars,
  [ [ +watch(cli), +stars_of(body1, Stars) | Muted ]
  , [ +every_300(bucket1) ]
  , [ +fetch(cli, no_tag, bucket1, fresh(tag_w1, body1)) ]
  , [ ]
  ]).

scenario(guard_fires,     pipe_guard, [], Ticks) :- guard_ticks([], 420, Ticks).
scenario(guard_muted,     pipe_guard, [], Ticks) :- guard_ticks([+muted(cli)], 420, Ticks).
scenario(guard_too_small, pipe_guard, [], Ticks) :- guard_ticks([], 42, Ticks).

% ═══ PROGRAM 6/7: one contested chain, two declaration sets ═══════════════
% Byte-identical chain text. Under pipe_no_cut nothing declares a cut and the
% desugarer refuses. Under pipe_declared_cut, cache_tag is an append log, so
% the same text is legal. This IS the answer to "is the law decidable at
% desugar time".

in_program(pipe_no_cut).

hot(Endpoint) <+ watch(Endpoint), cache_tag(Endpoint, Tag) ~> Tag \== no_tag.

in_program(pipe_declared_cut).

decl_of(pipe_declared_cut, append(cache_tag/2)).

hot(Endpoint) <+ watch(Endpoint), cache_tag(Endpoint, Tag) ~> Tag \== no_tag.

% ═══ PROGRAM 8: a pipe nested inside a stage ══════════════════════════════
% Parses fine. Rejected by the desugarer, not the reader.

in_program(pipe_nested).

decl_of(pipe_nested, effect(fetch/4)).

out(Endpoint) <+
      watch(Endpoint)
    ~> (every_300(Bucket) ~> fetch(Endpoint, no_tag, Bucket, _Result))
    ~> true.

% ═══ PROGRAM 9: a head variable no stage binds ════════════════════════════

in_program(pipe_unsafe_head).

decl_of(pipe_unsafe_head, effect(fetch/4)).
decl_of(pipe_unsafe_head, append(out/2)).

out(Endpoint, _Missing) <+
      watch(Endpoint), every_300(Bucket), fetch(Endpoint, no_tag, Bucket, Result)
    ~> Result = fresh(_Tag, _Body).

% ═══ grading ═══════════════════════════════════════════════════════════════
% ── (1) PARSEABILITY ───────────────────────────────────────────────────────

% The glyph itself. `|` is a solo character, so `|>` never lexes as one atom
% no matter what op/3 says; only the quoted atom reads.
check(pipe_glyph_needs_quotes,
  ( read_shape('watch(Endpoint) |> fetch(Endpoint)', BareShape, _),
    threw_syntax_error(BareShape),
    read_shape('watch(Endpoint) ''|>'' fetch(Endpoint)', QuotedShape, QuotedFunctor),
    QuotedFunctor == ('|>')/2,
    QuotedShape == '|>'(watch(hole), fetch(hole)) )).

% Below comma: the pipe binds TIGHTER, so stage 1 collapses to its LAST atom
% and every multi-atom stage would need parentheses.
check(pipe_below_comma_inverts_stages,
  ( read_at_priority(900, xfy, 'watch(Endpoint), every_300(Bucket) ~> fetch(Endpoint)',
                     Shape, Functor),
    Functor == (',')/2,
    Shape == ','(watch(hole), ~>(every_300(hole), fetch(hole))) )).

% Above comma, below the rule arrows: a stage IS a comma-joined body.
check(pipe_above_comma_groups_stages,
  ( read_shape('watch(Endpoint), every_300(Bucket) ~> fetch(Endpoint), keep(Bucket)',
               Shape, Functor),
    Functor == (~>)/2,
    Shape == ~>( ','(watch(hole), every_300(hole)),
                 ','(fetch(hole), keep(hole)) ) )).

% With `<+` at 1150 and the pipe at 1100, the rule arrow stays the principal
% functor and the whole chain is its body. This is the working precedence.
check(pipe_under_arrow_keeps_one_head,
  ( read_shape('change_log(Endpoint) <+ watch(Endpoint), every_300(Bucket) ~> fetch(Endpoint)',
               Shape, Functor),
    Functor == (<+)/2,
    Shape = <+(change_log(hole), Body),
    functor(Body, ~>, 2) )).

% BREAK 1: give the pipe the same priority as the rule arrow and nothing
% reads. `<+` is xfx 1150, so its right argument must stay under 1150.
check(pipe_at_arrow_priority_clashes,
  ( read_at_priority(1150, xfy, 'change_log(Endpoint) <+ watch(Endpoint) ~> fetch(Endpoint)',
                     Shape, _),
    threw_syntax_error(Shape) )).

% BREAK 2: lift the pipe ABOVE the rule arrows and the meaning flips. The
% chain becomes a sequence of whole CLAUSES, and a chain written with one
% head loses that head off the last stage. Both readings graded.
check(pipe_above_arrow_becomes_clause_chain,
  ( read_at_priority(1175, xfy, 'demand(X) <+ watch(X) ~> folded(X) <+ demand(X)',
                     ChainShape, ChainFunctor),
    ChainFunctor == (~>)/2,
    ChainShape == ~>( <+(demand(hole), watch(hole)),
                      <+(folded(hole), demand(hole)) ),
    read_at_priority(1175, xfy, 'change_log(X) <+ watch(X), every_300(B) ~> fetch(X)',
                     LostShape, LostFunctor),
    LostFunctor == (~>)/2,
    LostShape == ~>( <+(change_log(hole), ','(watch(hole), every_300(hole))),
                     fetch(hole) ) )).

% BREAK 3: dot access. `cache(Endpoint).tag` reads only because there is no
% space after the dot. Put a space in and prolog ends the clause: the chain
% silently loses its last stage and `tag` becomes a separate term.
check(dot_access_truncates_on_space,
  ( read_shape('out(X) <+ watch(X) ~> cache(X).tag', GluedShape, _),
    dot_term(cache(hole), tag, DotShape),
    GluedShape == <+(out(hole), ~>(watch(hole), DotShape)),
    read_shape('out(X) <+ watch(X) ~> cache(X). tag', SpacedShape, _),
    SpacedShape == <+(out(hole), ~>(watch(hole), cache(hole))) )).

% BREAK 3b: even glued, the dot is SWI dict syntax, not a field read. In a
% fact argument it rewrites the fact into a rule whose body calls ./3, and
% ./3 on a non-dict throws. There is no honest way to spell `x.field` in a
% prolog-read surface.
check(dot_access_in_fact_arg_becomes_a_rule,
  ( read_term_from_atom('watched(cache(Endpoint).tag)', FactTerm, []),
    expand_term(FactTerm, Expanded),
    Expanded = (watched(_) :- ExpandedGoal),
    dot_goal(cache(_), tag, _, ExpectedGoal),
    ExpandedGoal =@= ExpectedGoal,
    dot_goal(cache(cli), tag, _, RuntimeGoal),
    catch(call(RuntimeGoal), RuntimeError, true),
    nonvar(RuntimeError),
    RuntimeError = error(type_error(dict, cache(cli)), _) )).

% The surface's `!rel` negation does not lex either: `!` is the cut atom and
% cannot prefix a term. The lab writes not/1 for that reason.
check(bang_negation_is_unlexable,
  ( read_shape('watch(Endpoint), !muted(Endpoint)', Shape, _),
    threw_syntax_error(Shape) )).

% The two other LANG.md surface shapes a stage could want. Neither reads:
% `=>` is SWI's SSU operator at 1200 xfx, so it cannot sit under the comma
% inside `{}`, and prolog has no juxtaposition, so `Entry { ... }` is two
% terms with nothing between them.
check(struct_and_match_blocks_do_not_read,
  ( read_shape('{ 200 => fresh, 304 => unchanged }', MatchShape, _),
    threw_syntax_error(MatchShape),
    read_shape('Entry { tag, rest }', StructShape, _),
    threw_syntax_error(StructShape) )).

% ── (2) DESUGAR + SEMANTICS ────────────────────────────────────────────────

check(chain_desugars_to_three_rules,
  ( chain_rules(pipe_feed, Rules),
    length(Rules, 3),
    Rules =@=
      [ ( pipe_change_log_1(Endpoint, Result) <+
            ( watch(Endpoint), cache_tag(Endpoint, PrevTag), every_300(Bucket),
              fetch(Endpoint, PrevTag, Bucket, Result) ) )
      , ( pipe_change_log_2(Endpoint, Stars) <+
            ( only(pipe_change_log_1(Endpoint, Result)),
              ( Result = fresh(_Tag, Body), stars_of(Body, Stars) ) ) )
      , ( change_log(Endpoint, Stars, Client) <+
            ( only(pipe_change_log_2(Endpoint, Stars)),
              subscribed_to(Client, Endpoint) ) )
      ] )).

check(cut_kinds_are_yield_then_edge_append,
  ( chain_cut_kinds(pipe_feed, Cuts),
    Cuts == [ cut(1, yield,       source_stage(fetch/4))
            , cut(2, edge_append, head(change_log/3)) ],
    chain_cut_kinds(pipe_cache, CacheCuts),
    CacheCuts == [ cut(1, yield, source_stage(fetch/4)) ] )).

% R5's claimed bonus, mechanised: every rule after the first has exactly one
% trigger atom, and it is the piped-in intermediate.
check(piped_atom_is_the_only_trigger,
  ( chain_rules(pipe_feed, [_ | Downstream]),
    forall(member((_ <+ Body), Downstream),
           ( trigger_atoms(Body, Triggers),
             Triggers = [Trigger],
             functor(Trigger, TriggerName, _),
             sub_atom(TriggerName, 0, 5, _, pipe_) )) )).

check(desugared_trace_equals_hand_written,
  ( run_named(feed_piped, _, PipedDeltas),
    run_named(feed_hand,  _, HandDeltas),
    rel_deltas(change_log/3, PipedDeltas, PipedFeed),
    rel_deltas(change_log/3, HandDeltas,  HandFeed),
    PipedFeed == HandFeed,
    PipedFeed == [ [], [], [], [], [ +change_log(cli, 42, alice) ], [] ] )).

% Two boundaries, two extra ticks. The response lands at tick 3; the append
% lands at tick 5. A pipe is not sequencing sugar, it is latency.
check(pipe_stage_costs_one_tick,
  ( run_named(feed_piped, _, Deltas),
    nth1(3, Deltas, ThirdTick),
    memberchk(+pipe_change_log_1(cli, fresh(tag_w1, body1)), ThirdTick),
    nth1(4, Deltas, FourthTick),
    memberchk(+pipe_change_log_2(cli, 42), FourthTick),
    nth1(5, Deltas, FifthTick),
    memberchk(+change_log(cli, 42, alice), FifthTick),
    rel_deltas(change_log/3, [ThirdTick, FourthTick], [[], []]) )).

check(keyed_head_chain_replaces,
  ( run_named(cache_replace, Final, Deltas),
    rel_deltas(cache/2, Deltas, CacheDeltas),
    CacheDeltas == [ [], [], [], [ -cache(cli, no_tag), +cache(cli, tag_w1) ] ],
    rel_rows(cache/2, Final, [ cache(cli, tag_w1) ]) )).

% ── (3) BINDING FLOW ───────────────────────────────────────────────────────

% Minimal arity: stage 1 binds four variables, two cross the boundary. The
% bucket and the previous tag are referenced nowhere downstream and are
% therefore not columns of the generated rel.
check(carried_columns_are_minimal,
  ( chain_rules(pipe_feed, [ (FirstInter <+ FirstStage) | _ ]),
    term_variables(FirstStage, StageVariables),
    length(StageVariables, 4),
    FirstInter =.. [pipe_change_log_1 | Carried],
    length(Carried, 2),
    FirstStage = ( watch(Endpoint), cache_tag(_, PrevTag), every_300(Bucket),
                   fetch(_, _, _, Result) ),
    Carried == [Endpoint, Result],
    \+ same_variable_in(PrevTag, Carried),
    \+ same_variable_in(Bucket,  Carried) )).

% Endpoint is bound in stage 1, absent from stage 2, and used in stage 3 and
% the head. It must ride through the second intermediate anyway.
check(variable_skipping_a_stage_still_flows,
  ( chain_rules(pipe_feed, [_, (SecondInter <+ (_, SecondStage)), _]),
    SecondInter =.. [pipe_change_log_2, CarriedEndpoint, _CarriedStars],
    term_variables(SecondStage, SecondStageVariables),
    \+ same_variable_in(CarriedEndpoint, SecondStageVariables),
    run_named(feed_piped, Final, _),
    rel_rows(change_log/3, Final, [ change_log(cli, 42, alice) ]) )).

% Reusing a name downstream is a JOIN, defined by unification, not a shadow.
% Endpoint appears in stage 1 and again in stage 3; bob subscribes to another
% endpoint and does not appear in the output.
check(name_reuse_across_stages_is_a_join,
  ( run_named(feed_piped, Final, _),
    rel_rows(change_log/3, Final, Rows),
    Rows == [ change_log(cli, 42, alice) ],
    \+ ( member(change_log(_, _, bob), Rows) ) )).

check(head_variable_bound_nowhere_is_rejected,
  ( catch(chain_rules(pipe_unsafe_head, _), Thrown, true),
    nonvar(Thrown),
    Thrown = unsafe_head_variable(out(_, _)) )).

% ── (4) THE BOUNDARY LAW ───────────────────────────────────────────────────

check(chain_without_cut_rejected,
  ( catch(chain_rules(pipe_no_cut, _), Thrown, true),
    nonvar(Thrown),
    Thrown = no_time_cut(1, Stage),
    Stage =@= ( watch(Endpoint), cache_tag(Endpoint, _Tag) ) )).

% The SAME chain text, one declaration different, and the verdict flips. The
% law needs the rel declarations in scope; it is not a property of the text.
check(cut_law_depends_on_declarations,
  ( chain_of(pipe_no_cut,       chain(_, _, ChainWithoutDecl)),
    chain_of(pipe_declared_cut, chain(_, _, ChainWithDecl)),
    ChainWithoutDecl =@= ChainWithDecl,
    catch(chain_rules(pipe_no_cut, _), Refused, true), nonvar(Refused),
    chain_cut_kinds(pipe_declared_cut, Cuts),
    Cuts == [ cut(1, edge_append, source_stage(cache_tag/2)) ],
    chain_rules(pipe_declared_cut, Accepted),
    length(Accepted, 2) )).

% Nesting parses. The desugarer is the thing that refuses.
check(nested_pipe_parses_but_desugar_rejects,
  ( read_shape('out(X) <+ watch(X) ~> (every_300(B) ~> fetch(X)) ~> true',
               Shape, Functor),
    Functor == (<+)/2,
    Shape = <+(out(hole), _),
    catch(chain_rules(pipe_nested, _), Thrown, true),
    nonvar(Thrown),
    Thrown = nested_pipe_in_stage(_) )).

% ── (5) COMMA INSIDE A STAGE ───────────────────────────────────────────────
% One stage holding a join, a comparison and a negation. All three are commas.
check(stage_holds_negation_and_comparison,
  ( run_named(guard_fires, FiresFinal, _),
    rel_rows(alert/2, FiresFinal, [ alert(cli, 420) ]),
    run_named(guard_muted, MutedFinal, _),
    rel_rows(alert/2, MutedFinal, []),
    run_named(guard_too_small, SmallFinal, _),
    rel_rows(alert/2, SmallFinal, []) )).

% ── (6) R5: does the pipe actually buy trigger control? ────────────────────
% Same schedule, same three rules. With the marker the late subscriber gets
% nothing. Without it, carol's arrival re-fires the last rule against the
% whole standing intermediate set: backlog replay, exactly R5.
check(trigger_marker_is_what_stops_backlog_replay,
  ( run_named(feed_piped,    _, MarkedDeltas),
    run_named(feed_unmarked, _, UnmarkedDeltas),
    rel_deltas(change_log/3, MarkedDeltas,   MarkedFeed),
    rel_deltas(change_log/3, UnmarkedDeltas, UnmarkedFeed),
    nth1(6, MarkedFeed,   []),
    nth1(6, UnmarkedFeed, [ +change_log(cli, 42, carol) ]),
    nth1(5, MarkedFeed,   [ +change_log(cli, 42, alice) ]),
    nth1(5, UnmarkedFeed, [ +change_log(cli, 42, alice) ]) )).

go :- run(check).

% ═══ trace printer (receipts for the .md) ══════════════════════════════════

report :-
    forall(chain_of(Program, Chain),
           ( format("chain in ~w~n", [Program]),
             (   catch(desugar(Program, Chain, Rules, Cuts), Thrown, true)
             ->  (   var(Thrown)
                 ->  forall(member(Cut, Cuts), format("  ~q~n", [Cut])),
                     forall(member(Rule, Rules), format("  ~q~n", [Rule]))
                 ;   format("  REFUSED ~q~n", [Thrown]) )
             ;   format("  no solution~n", []) ),
             nl )),
    forall(scenario(ScenarioName, Program, _, _),
           ( format("~w  (program ~w)~n", [ScenarioName, Program]),
             (   catch(run_named(ScenarioName, Final, DeltaTicks), Error, true)
             ->  (   var(Error)
                 ->  forall(nth1(Index, DeltaTicks, Deltas),
                            format("  tick ~w  ~q~n", [Index, Deltas])),
                     format("  final    ~q~n", [Final])
                 ;   format("  REJECTED ~q~n", [Error]) )
             ;   format("  no solution~n", []) ),
             nl )).
