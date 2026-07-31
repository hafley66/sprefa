% expand.pl -- the point-free lab's LOCAL expander for the three candidate
% moves. Nothing here touches the compiler, the oracle, the registry or the
% parser: this module reads a `prog(Decls, Rules)` term that spells the SUGAR
% and writes a `prog(Decls, Rules)` term that spells only shipped constructs.
% print_dl.pl then renders that to ordinary `.dl6` text, and the existing two
% doors grade the text. The sugar therefore never enters the language; it is
% priced by what its output costs.
%
% ── the glyph ──────────────────────────────────────────────────────────────
% `|>` is unlexable in a prolog term file (`|` is the reader's own), which the
% 2026-07-27 aggregate analysis already recorded ("`|>` in the shipped surface,
% DCG-owned; labs use `~>` stand-in"). This lab writes `~>` for the same reason
% and the glyph question stays where that verdict left it.
%
% ── expansion order, and why ───────────────────────────────────────────────
% pipe -> seq -> scan, declared here the way 1_expansion.pl declares the
% shipped order. It is not arbitrary: `seq` emits a cursor rule whose head
% carries a `scan`, so seq must run BEFORE scan or the emitted fold never
% expands. That dependency is the Q5 minimality receipt in executable form --
% seq is scan plus a minted rel plus two read arms, and this file is where you
% can see it.
%
% ── refusals ───────────────────────────────────────────────────────────────
% Every refusal below throws point_free_refusal(Term). They are the lab's
% break rules, not defensive coding: each one exists because the unrefused
% expansion was written, run, and produced a DIFFERENT tick log than the
% program it claims to be sugar for. expand_point_free/3 with `unsafe(true)`
% skips them so the receipts can show that diverging log.

:- module(point_free_expand,
          [ expand_point_free/2,
            expand_point_free/3,
            expand_point_free/4,
            expand_pipe/2,
            expand_seq/2,
            expand_scan/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(1100, xfy, ~>).

% ═══ entry ═══════════════════════════════════════════════════════════════════

expand_point_free(In, Out) :- expand_point_free(In, Out, []).

expand_point_free(In, Out, Options) :- expand_point_free(In, Out, Options, _Minted).

% Minted is the Name=Var list for every variable this expander created. The
% shipped compiler would not need it -- a fresh variable needs no name to be
% lowered -- but print_dl.pl renders an unnamed variable as `_`, and two `_` in
% one rule are two DIFFERENT variables, so an unnamed minted variable prints a
% program that is not the program that was expanded. The names below are fixed
% words, not a counter, so re-running the expander produces the same text.
expand_point_free(prog(Decls0, Rules0), prog(Decls, Rules), Options, Minted) :-
    b_setval(point_free_options, Options),
    b_setval(point_free_minted, []),
    expand_pipe(prog(Decls0, Rules0), prog(Decls1, Rules1)),
    expand_seq(prog(Decls1, Rules1), prog(Decls2, Rules2)),
    expand_scan(prog(Decls2, Rules2), prog(Decls, Rules)),
    b_getval(point_free_minted, Reversed),
    reverse(Reversed, Minted).

% One minted variable, carrying the name the printer will give it. Within a
% single rule every call site below uses a DISTINCT base word, which is what
% keeps two minted variables in one rule from printing as one identifier;
% across rules the same word repeats freely, exactly as golden-flex.dl6 reuses
% `TreeId` in thirteen rules.
% b_setval, never nb_setval: nb_setval COPIES the term it stores, so the
% variable that came back was a copy and the printer never matched it against
% the one in the rule. Measured -- every minted variable printed as `_`, and
% two `_` in one rule are two different variables, so the emitted program was
% not the expanded program.
mint_var(Name, Var) :-
    b_getval(point_free_minted, Minted),
    b_setval(point_free_minted, [Name=Var | Minted]).

unsafe :-
    catch(b_getval(point_free_options, Options), _, Options = []),
    memberchk(unsafe(true), Options).

refuse(_Term) :- unsafe, !.
refuse(Term)  :- throw(point_free_refusal(Term)).

% ═══ M1: scan(Acc, Seed, Expr) in an edge head ═══════════════════════════════
%
%   total(Counter, scan(Prev, 0, Prev + Delta)) <+ tick_event(Counter, Delta).
%
% becomes the shipped two-arm fold:
%
%   total(Counter, Next) <+ tick_event(Counter, Delta),
%                           not(total(Counter, _)), Next := 0 + Delta.
%   total(Counter, Next) <+ tick_event(Counter, Delta),
%                           pre(total(Counter, Prev)), Next := Prev + Delta.
%
% `Prev` is the PREVIOUS accumulated value inside Expr, exactly as rx's
% scan((acc, value) => ...) binds its first parameter; the head column receives
% Expr's value. The base arm is Expr with Prev replaced by Seed, which is why
% the seed cannot be a separate emission: rx's scan does not emit its seed
% either.

expand_scan(prog(Decls, Rules0), prog(Decls, Rules)) :-
    foldl(expand_scan_rule(Decls), Rules0, [], Reversed),
    reverse(Reversed, Rules).

expand_scan_rule(Decls, Rule, Acc0, Acc) :-
    ( scan_rule_parts(Rule, Name, Args, Positions, Body)
    -> check_scan_head_keyed(Decls, Name, Args, Positions),
       scan_arms(Decls, Name, Args, Positions, Body, BaseRule, StepRule),
       Acc = [StepRule, BaseRule | Acc0]
    ;  Acc = [Rule | Acc0] ).

% ALL scan positions in one head are ONE fold, not N folds. rx spells the same
% thing with an array accumulator -- scan(([sum, count], value) => [sum + value,
% count + 1], [0, 0]) -- and the reason is identical in both languages: the two
% accumulators advance on the same event and each step expression may read the
% other's previous value. Two independent folds would need two `pre` reads of
% the same rel in one arm and would let the columns drift apart.
scan_rule_parts((Head <+ Body), Name, Args, Positions, Body) :-
    compound(Head),
    Head =.. [Name | Args],
    findall(Index, ( nth1(Index, Args, Arg), nonvar(Arg), Arg = scan(_, _, _) ), Positions),
    Positions \== [],
    !.
scan_rule_parts((Head <- _Body), _, _, _, _) :-
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args), nonvar(Arg), Arg = scan(_, _, _),
    !,
    % A level rule has no occurrences to fold over: its head is recomputed from
    % the current world every tick, so `pre` of itself is either empty or a
    % cycle. Receipt R-M1-c.
    refuse(scan_in_level_rule),
    fail.

% findall/3 COPIES its template, which silently unshares every body variable a
% step expression reads (`Folded := _ + _` instead of `Folded := Prev + Delta`).
% Measured here, and the prolog org-refactor journal records being bitten by
% the same thing twice. Every collection below that carries a VARIABLE is a
% plain recursion for that reason; the two that collect integers still use
% findall, where copying is harmless.
scan_arms(Decls, Name, Args, Positions, Body, (BaseHead <+ BaseBody), (StepHead <+ StepBody)) :-
    collect_triples(Positions, Args, Triples),
    length(Args, Arity),

    % base arm: the guard wildcards every accumulator column, and every step
    % expression reads Seed where it would read the previous value.
    mint_head_vars(Decls, Name/Arity, Positions, Args, BaseArgs, BaseVars),
    BaseHead =.. [Name | BaseArgs],
    wildcard_positions(Positions, Args, GuardArgs),
    GuardAtom =.. [Name | GuardArgs],
    seed_substituted(Triples, SeedExprs),
    binds(BaseVars, SeedExprs, BaseBinds),
    conj_append(Body, (not(GuardAtom)), BaseGuarded),
    conj_append(BaseGuarded, BaseBinds, BaseBody),

    % step arm: ONE `pre` read binds every accumulator at once.
    mint_head_vars(Decls, Name/Arity, Positions, Args, StepArgs, StepVars),
    StepHead =.. [Name | StepArgs],
    accumulator_positions(Positions, Args, PreArgs),
    PreAtom =.. [Name | PreArgs],
    triple_exprs(Triples, StepExprs),
    binds(StepVars, StepExprs, StepBinds),
    conj_append(Body, (pre(PreAtom)), StepGuarded),
    conj_append(StepGuarded, StepBinds, StepBody).

mint_head_vars(_Decls, _Ref, [], Args, Args, []).
mint_head_vars(Decls, Ref, [Index | Rest], Args0, Args, [Var | Vars]) :-
    % One minted name per HEAD COLUMN, taken from that column's own declared
    % name. Two accumulators in one head would otherwise both print as `Folded`
    % and the printer would render them as ONE identifier -- measured: the
    % running-average expansion emitted `running(Sensor, Folded, Folded)` and
    % two contradictory binds of the same variable.
    accumulator_var_name(Decls, Ref, Index, VarName),
    mint_var(VarName, Var),
    replace_nth1(Index, Args0, Var, Args1),
    mint_head_vars(Decls, Ref, Rest, Args1, Args, Vars).

accumulator_var_name(Decls, Ref, Index, VarName) :-
    (  declared_columns(Decls, Ref, Columns),
       nth1(Index, Columns, Column)
    -> capitalized(Column, VarName)
    ;  atomic_list_concat(['Folded', Index], VarName) ).

declared_columns(Decls, Ref, Columns) :-
    collect_columns(Decls, Ref, Columns),
    Columns \== [].

collect_columns([], _Ref, []).
collect_columns([col_type(Ref, Column, _Type) | Rest], Ref, [Column | Columns]) :- !,
    collect_columns(Rest, Ref, Columns).
collect_columns([_Other | Rest], Ref, Columns) :- collect_columns(Rest, Ref, Columns).

capitalized(Atom, Capitalized) :-
    sub_atom(Atom, 0, 1, _, First),
    upcase_atom(First, Upper),
    sub_atom(Atom, 1, _, 0, Rest),
    atom_concat(Upper, Rest, Capitalized).

wildcard_positions([], Args, Args).
wildcard_positions([Index | Rest], Args0, Args) :-
    replace_nth1(Index, Args0, _Wild, Args1),
    wildcard_positions(Rest, Args1, Args).

accumulator_positions([], Args, Args).
accumulator_positions([Index | Rest], Args0, Args) :-
    nth1(Index, Args0, scan(Acc, _Seed, _Expr)),
    replace_nth1(Index, Args0, Acc, Args1),
    accumulator_positions(Rest, Args1, Args).

collect_triples([], _Args, []).
collect_triples([Index | Rest], Args, [Acc-Seed-Expr | Triples]) :-
    nth1(Index, Args, scan(Acc, Seed, Expr)),
    collect_triples(Rest, Args, Triples).

triple_exprs([], []).
triple_exprs([_Acc-_Seed-Expr | Rest], [Expr | Exprs]) :- triple_exprs(Rest, Exprs).

% Every accumulator variable in every step expression is replaced by that
% accumulator's own seed, so a cross-reading base arm ("start the average at
% the seed count") stays correct rather than referring to an unbound variable.
seed_substituted(Triples, SeedExprs) :- seed_substituted(Triples, Triples, SeedExprs).

seed_substituted([], _All, []).
seed_substituted([_Acc-_Seed-Expr | Rest], All, [SeedExpr | SeedExprs]) :-
    foldl(substitute_seed, All, Expr, SeedExpr),
    seed_substituted(Rest, All, SeedExprs).

substitute_seed(Acc-Seed-_Expr, In, Out) :- subst_var(In, Acc, Seed, Out).

binds([Var], [Expr], (Var := Expr)) :- !.
binds([Var | Vars], [Expr | Exprs], ((Var := Expr), Rest)) :- binds(Vars, Exprs, Rest).

% BREAK RULE M1-1. The two arms discriminate on `not(head-with-every-
% accumulator-wildcarded)`, so the head's OTHER columns have to identify
% exactly one row -- they must be the declared key. On an unkeyed or log head
% the guard is a whole-relation emptiness test and the fold restarts, or never
% restarts, at the wrong moments. Receipt R-M1-b shows the diverging log.
check_scan_head_keyed(Decls, Name, Args, Positions) :-
    length(Args, Arity),
    Ref = Name/Arity,
    findall(Index, ( between(1, Arity, Index), \+ memberchk(Index, Positions) ), GroupColumns),
    (  memberchk(keyed(Ref, Declared), Decls),
       msort(Declared, Sorted), msort(GroupColumns, Sorted)
    -> true
    ;  refuse(scan_head_not_keyed_on_group(Ref, GroupColumns))
    ).

% ═══ M2: Ordinal := seq(Partition) ═══════════════════════════════════════════
%
%   stream(Name, Ordinal, Payload) <+ event(Name, Payload),
%                                     Ordinal := seq(Name).
%
% mints ONE cursor rel per (head rel, ordinal column) and emits the shipped
% four-rule block. The cursor's own rule is written with `scan`, so M1 finishes
% the job -- that composition IS the answer to Q5 for this move.
%
% slot_seq_scope answers itself from the argument: `seq(Name)` with a VARIABLE
% partitions the order by that variable's value; `seq('q')` with an atom is one
% global order named q. There is no third choice to make and no per-rel-versus-
% per-name switch to add, because the argument already says which was meant.

expand_seq(prog(Decls0, Rules0), prog(Decls, Rules)) :-
    foldl(expand_seq_rule, Rules0, decls_rules(Decls0, []), decls_rules(Decls, Reversed)),
    reverse(Reversed, Rules).

expand_seq_rule(Rule, decls_rules(Decls0, Acc0), decls_rules(Decls, Acc)) :-
    (  seq_rule_parts(Rule, Name, Args, Position, OrdinalVar, Partition, Body)
    -> cursor_ref(Name, Position, CursorName),
       CursorRef = CursorName/2,
       % `Carried`, not `At`: the cursor's own accumulator column is DECLARED
       % `at`, so M1 names the head variable `At` from that decl, and a second
       % `At` in the same rule prints as one identifier. Measured -- the first
       % emission read `seq_...('q', At) <+ ..., pre(seq_...('q', At)),
       % At := At + 1`, a self-referential rule that is not what expanded.
       % A real compiler minting fresh variables never has this problem; a
       % PRINTED expansion does, which is a cost of grading through text.
       mint_var('Carried', CursorAt),
       CursorHead =.. [CursorName, Partition, scan(CursorAt, 0, CursorAt + 1)],
       CursorRule = (CursorHead <+ Body),

       mint_var('Ordinal', FirstOrdinal),
       replace_nth1(Position, Args, FirstOrdinal, FirstArgs),
       FirstHead =.. [Name | FirstArgs],
       EmptyGuard =.. [CursorName, Partition, _AnyAt],
       conj_append(Body, (not(EmptyGuard), FirstOrdinal := 1), FirstBody),

       mint_var('Ordinal', NextOrdinal),
       replace_nth1(Position, Args, NextOrdinal, NextArgs),
       NextHead =.. [Name | NextArgs],
       mint_var('At', PreAt),
       PreCursor =.. [CursorName, Partition, PreAt],
       conj_append(Body, (pre(PreCursor), NextOrdinal := PreAt + 1), NextBody),

       % The minted cursor carries its OWN column types. Without them the
       % printer names its columns `col1`/`col2` and the compiler has no
       % declared type to check the ordinal against; measured, the first
       % emission read `rel seq_numbered_1(col1, folded2)`.
       ( memberchk(keyed(CursorRef, _), Decls0)
       -> Decls = Decls0
       ;  infer_value_type(Partition, Decls0, Body, PartitionType),
          append(Decls0,
                 [ col_type(CursorRef, partition, PartitionType),
                   col_type(CursorRef, at, int),
                   keyed(CursorRef, [1]) ],
                 Decls) ),
       Acc = [ (NextHead <+ NextBody), (FirstHead <+ FirstBody), CursorRule | Acc0 ],
       ignore(OrdinalVar = OrdinalVar)
    ;  Decls = Decls0,
       Acc = [Rule | Acc0] ).

seq_rule_parts((Head <+ Body0), Name, Args, Position, OrdinalVar, Partition, Body) :-
    compound(Head),
    Head =.. [Name | Args],
    conj_list(Body0, Items0),
    select(Item, Items0, Rest),
    nonvar(Item), Item = (OrdinalVar := SeqCall),
    nonvar(SeqCall), SeqCall = seq(Partition),
    !,
    nth1(Position, Args, HeadArg),
    HeadArg == OrdinalVar,
    list_conj(Rest, Body).
% `nonvar(Right)` BEFORE the unification, not after: `Value := Shifted` with
% `Shifted` still unbound unified happily with `_ := seq(_)` and BOUND the
% program's own variable to `seq(_)`. That is how the sensor pipeline, which
% contains no `seq` at all, earned a seq_in_level_rule refusal.
seq_rule_parts((Head <- Body), _, _, _, _, _, _) :-
    conj_list(Body, Items),
    member(Item, Items), nonvar(Item), Item = (_ := Right),
    nonvar(Right), Right = seq(_),
    !,
    % Same reason as scan: a level rule has no occurrence to advance a cursor
    % on. Receipt R-M2-c.
    ignore(Head = Head),
    refuse(seq_in_level_rule),
    fail.

% MEASURED, probe/underscore_rel.dl6: a rel name beginning with `__` -- the
% engine's own minting convention for `__host_*`, `__pre_*`,
% `__departure_frontier_*` -- does not PARSE. A leading underscore is the
% variable marker, so `bop check` returns broken: parse_failed. That convention
% is reachable only by term-level expansion, never by hand. This lab therefore
% mints ordinary legal names so the desugared twin is a program a person could
% have written, which is what the grading law needs; the naming question that
% leaves open is slot_stage_naming in the verdict.
% A literal says its own type; a variable takes the type of the declared column
% it is read from in this rule's body. Nothing here is a guess: if neither
% applies the expansion refuses rather than defaulting, because a wrong column
% type is the `edge_head_column_type_mismatch` class the corpus has already
% been bitten by twice.
infer_value_type(Value, _Decls, _Body, text) :- atom(Value), !.
infer_value_type(Value, _Decls, _Body, int)  :- integer(Value), !.
infer_value_type(Value, Decls, Body, Type) :-
    var(Value),
    conj_list(Body, Items),
    member(Item, Items),
    compound(Item),
    Item =.. [ItemName | ItemArgs],
    nth1(Index, ItemArgs, Arg),
    Arg == Value,
    length(ItemArgs, ItemArity),
    declared_columns(Decls, ItemName/ItemArity, Columns),
    nth1(Index, Columns, Column),
    memberchk(col_type(ItemName/ItemArity, Column, Type), Decls),
    !.
infer_value_type(Value, _Decls, _Body, _Type) :-
    refuse(seq_partition_type_unknown(Value)),
    fail.

cursor_ref(Name, Position, CursorName) :-
    atomic_list_concat(['seq_', Name, '_', Position], CursorName).

% ═══ M3: anonymous stages, `~>` (the lab's stand-in for `|>`) ═════════════════
%
%   alert(Sensor, Value) <-
%       reading(Sensor, Raw), Doubled := Raw * 2
%    ~> Shifted := Doubled + 10
%    ~> Shifted > 50, Value := Shifted.
%
% Every cut mints one rel. Its columns are the variables bound at or before the
% cut that are still READ after it (head included) -- nothing else crosses, so
% the minted width is a property of the program, not of the author's typing.
%
% slot_stage_naming: the minted name is `__stage_<head name>_<head arity>_<k>`,
% a function of the rule's own head and the cut's ordinal position. It is
% stable across recompiles because nothing in it is a counter, a gensym or a
% hash of formatting. Two rules heading the SAME rel are the one case that
% collides, and that collision is refused by name below rather than
% disambiguated, because a silent `_2` suffix would make the tick log depend on
% clause order in the file.

expand_pipe(prog(Decls0, Rules0), prog(Decls, Rules)) :-
    foldl(expand_pipe_rule, Rules0, decls_rules(Decls0, []), decls_rules(Decls, Reversed)),
    reverse(Reversed, Rules).

expand_pipe_rule(Rule, decls_rules(Decls0, Acc0), decls_rules(Decls, Acc)) :-
    (  pipe_rule_parts(Rule, Arrow, Head, Stages)
    -> pipe_check(Arrow, Head, Stages, Decls0, Rules0Names),
       ignore(Rules0Names = Rules0Names),
       stage_rules(Head, Stages, Decls0, Decls, Emitted),
       reverse(Emitted, RevEmitted),
       append(RevEmitted, Acc0, Acc)
    ;  Decls = Decls0,
       Acc = [Rule | Acc0] ).

pipe_rule_parts((Head <- Body), '<-', Head, Stages) :-
    nonvar(Body), Body = (_ ~> _), !,
    pipe_list(Body, Stages).
pipe_rule_parts((Head <+ Body), '<+', Head, Stages) :-
    nonvar(Body), Body = (_ ~> _), !,
    pipe_list(Body, Stages).

pipe_list(Body, [First | Rest]) :-
    nonvar(Body), Body = (First ~> Tail), !,
    pipe_list(Tail, Rest).
pipe_list(Stage, [Stage]).

% BREAK RULE M3-1. Every cut in an EDGE rule turns the next stage's source into
% a DERIVED trigger, which costs one tick per stage and changes the head's
% arrival tick. Sugar that moves a row by N ticks is not sugar. Receipt R-M3-a
% shows the tick shift.
pipe_check('<+', _Head, _Stages, _Decls, []) :- !,
    refuse(pipe_in_edge_rule).
% BREAK RULE M3-2. Aggregate head. The group key of an aggregate is exactly the
% head's non-aggregate columns; under `~>` those are computed by liveness, so
% inserting a later stage that stops reading a column silently REGROUPS the
% aggregate. Refused rather than defined. Receipt R-M3-c.
pipe_check('<-', Head, _Stages, _Decls, []) :-
    compound(Head),
    Head =.. [_ | Args],
    member(Arg, Args), nonvar(Arg), compound(Arg),
    functor(Arg, AggName, AggArity),
    aggregate_functor(AggName, AggArity),
    !,
    refuse(pipe_head_is_aggregate(AggName/AggArity)).
pipe_check('<-', _Head, _Stages, _Decls, []).

aggregate_functor(count, 1).
aggregate_functor(sum, 1).
aggregate_functor(min, 1).
aggregate_functor(max, 1).
aggregate_functor(avg, 1).
aggregate_functor(json_group_array, 1).
aggregate_functor(json_group_array, 2).
aggregate_functor(group_concat, 2).
aggregate_functor(group_concat, 3).

stage_rules(Head, Stages, Decls0, Decls, Rules) :-
    length(Stages, Count),
    Last is Count - 1,
    functor(Head, HeadName, HeadArity),
    stage_carry_sets(Head, Stages, CarrySets),
    stage_rules_walk(1, Last, HeadName, HeadArity, Head, Stages, CarrySets,
                     _Previous, Rules),
    stage_decls(1, Last, HeadName, HeadArity, CarrySets, Decls0, Decls).

stage_rules_walk(Index, Last, HeadName, HeadArity, Head, Stages, CarrySets,
                 Previous, [Rule | Rest]) :-
    Index =< Last, !,
    nth1(Index, Stages, Stage),
    nth1(Index, CarrySets, Carry),
    stage_atom(HeadName, HeadArity, Index, Carry, StageHead),
    ( Index =:= 1
    -> Body = Stage
    ;  conj_append(Previous, Stage, Body) ),
    Rule = (StageHead <- Body),
    Next is Index + 1,
    stage_rules_walk(Next, Last, HeadName, HeadArity, Head, Stages, CarrySets,
                     StageHead, Rest).
stage_rules_walk(Index, Last, _HeadName, _HeadArity, Head, Stages, _CarrySets,
                 Previous, [(Head <- Body)]) :-
    Index =:= Last + 1,
    nth1(Index, Stages, Final),
    ( var(Previous) -> Body = Final ; conj_append(Previous, Final, Body) ).

stage_atom(HeadName, HeadArity, Index, Carry, StageHead) :-
    atomic_list_concat(['stage_', HeadName, '_', HeadArity, '_', Index], Name),
    StageHead =.. [Name | Carry].

% The carry set at cut k: variables that OCCUR in stages 1..k and also occur in
% stages k+1..n or in the head. Order is first-occurrence order in stages 1..k,
% which makes the column order a function of the program text.
stage_carry_sets(Head, Stages, CarrySets) :-
    length(Stages, Count),
    Last is Count - 1,
    carry_sets(1, Last, Head, Stages, CarrySets).

carry_sets(Index, Last, Head, Stages, [Carry | Rest]) :-
    Index =< Last, !,
    carry_at(Head, Stages, Index, Carry),
    Next is Index + 1,
    carry_sets(Next, Last, Head, Stages, Rest).
carry_sets(_Index, _Last, _Head, _Stages, []).

% length/2 + append/3 rather than findall: findall would COPY the stages and
% every carried variable would then be a fresh one, which is how the first
% emission produced `stage_alert_2_1` with no columns at all.
carry_at(Head, Stages, Index, Carry) :-
    length(Before, Index),
    append(Before, After, Stages),
    term_variables(Before, BeforeVars),
    term_variables(After-Head, AfterVars),
    include(var_member(AfterVars), BeforeVars, Carry).

var_member(Set, Var) :- \+ \+ ( member(Other, Set), Other == Var ), !.

stage_decls(Index, Last, HeadName, HeadArity, CarrySets, Decls0, Decls) :-
    Index =< Last, !,
    nth1(Index, CarrySets, Carry),
    length(Carry, Width),
    atomic_list_concat(['stage_', HeadName, '_', HeadArity, '_', Index], Name),
    Ref = Name/Width,
    ( memberchk(kind(Ref, _), Decls0)
    -> % BREAK RULE M3-3. Two rules heading one rel would mint the same stage
       % name. Refused; a silent suffix makes the tick log clause-order
       % dependent.
       refuse(pipe_stage_name_collision(Ref)),
       Decls1 = Decls0
    ;  Decls1 = Decls0 ),
    Next is Index + 1,
    stage_decls(Next, Last, HeadName, HeadArity, CarrySets, Decls1, Decls).
stage_decls(_Index, _Last, _HeadName, _HeadArity, _CarrySets, Decls, Decls).

% ═══ shared term utilities ═══════════════════════════════════════════════════

replace_nth1(1, [_ | Tail], New, [New | Tail]) :- !.
replace_nth1(Index, [Head | Tail], New, [Head | Rest]) :-
    Index > 1, Next is Index - 1,
    replace_nth1(Next, Tail, New, Rest).

conj_list(Term, List) :-
    ( nonvar(Term), Term = (Left, Right)
    -> conj_list(Left, LeftList), conj_list(Right, RightList),
       append(LeftList, RightList, List)
    ;  List = [Term] ).

list_conj([Single], Single) :- !.
list_conj([Head | Tail], (Head, Rest)) :- list_conj(Tail, Rest).

conj_append(Left, Right, Conjunction) :-
    conj_list(Left, LeftList),
    conj_list(Right, RightList),
    append(LeftList, RightList, All),
    list_conj(All, Conjunction).

% Replace every occurrence of one VARIABLE (by identity, never by unification)
% with a replacement term. copy_term/2 cannot be used here: it would also copy
% the body variables the arm has to keep sharing with.
subst_var(Term, Var, Replacement, Out) :-
    ( Term == Var        -> Out = Replacement
    ; var(Term)          -> Out = Term
    ; compound(Term)     -> Term =.. [Functor | Args],
                            maplist(subst_var_arg(Var, Replacement), Args, NewArgs),
                            Out =.. [Functor | NewArgs]
    ;  Out = Term ).

subst_var_arg(Var, Replacement, Term, Out) :- subst_var(Term, Var, Replacement, Out).
