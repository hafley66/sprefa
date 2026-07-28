% analyze.pl : structural analysis of a prog(Decls, Rules) term form. Reads
% relation kind (mirrors engine.pl:88-93 rel_kind/4, reimplemented here since
% that predicate is not exported and depends only on Decls), which refs are
% EDB (an arrival schedule may write them: never a rule head) vs derived
% (headed by a level or edge rule), and per-ref column names mined from the
% ORIGINAL surface variable names the caller recovers via
% read_term(Stream, Term, [variable_names(Bindings)]) -- column identity comes
% from the fixture source text, never invented here.
%
% Compiles the SUBSET engine.pl semantics that the two Phase B target
% fixtures use: edge rule bodies are `only(TriggerAtom)` alone (no extra
% joined goal, no pre/1, no departed/1, no guard); level rules are acyclic
% (no self-recursion), non-aggregate, with `not/1` allowed. Anything wider
% throws unsupported_construct(What) at analysis time -- a compiler finding,
% not a silent guess.

:- module(analyze,
          [ rel_kind/3, decl_key/3, decl_keep/3, declared_refs/2,
            program_refs/2, arrival_target_refs/2, derived_refs/2,
            edge_headed_refs/2, level_headed_refs/2,
            rule_head_ref/2, rule_is_edge/1, rule_is_level/1,
            body_ref_uses/2, rel_columns/4, snake_name/2,
            check_supported_subset/1 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../conformance/body', [rel_ref/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ rel kind (mirrors engine.pl declared_kind/rel_kind exactly) ════════════

declared_kind(Decls, Ref, Kind) :- memberchk(kind(Ref, Kind), Decls).

rel_kind(Decls, Ref, log) :- declared_kind(Decls, Ref, log), !.
rel_kind(Decls, Ref, set) :- declared_kind(Decls, Ref, set), !.
rel_kind(Decls, Ref, set) :- memberchk(keyed(Ref, _), Decls), !.
rel_kind(_, _, set).

decl_key(Decls, Ref, Positions) :- memberchk(keyed(Ref, Positions), Decls).

decl_keep(Decls, Ref, Bound) :- memberchk(keep(Ref, Bound), Decls), !.
decl_keep(_, _, all).

% ═══ rule shape ═════════════════════════════════════════════════════════════

rule_is_edge((_ <+ _)).
rule_is_level((_ <- _)).

rule_head_ref((Head <- _), Ref) :- rel_ref(Head, Ref).
rule_head_ref((Head <+ _), Ref) :- rel_ref(Head, Ref).

rule_head((Head <- _), Head).
rule_head((Head <+ _), Head).

rule_body((_ <- Body), Body).
rule_body((_ <+ Body), Body).

edge_headed_refs(Rules, Refs) :-
    findall(Ref, ( member(Rule, Rules), rule_is_edge(Rule), rule_head_ref(Rule, Ref) ), Refs0),
    sort(Refs0, Refs).

level_headed_refs(Rules, Refs) :-
    findall(Ref, ( member(Rule, Rules), rule_is_level(Rule), rule_head_ref(Rule, Ref) ), Refs0),
    sort(Refs0, Refs).

derived_refs(Rules, Refs) :-
    edge_headed_refs(Rules, EdgeRefs), level_headed_refs(Rules, LevelRefs),
    append(EdgeRefs, LevelRefs, All), sort(All, Refs).

% ═══ body walking ════════════════════════════════════════════════════════════
% body_ref_uses(Body, Uses): Uses = list of use(Ref, Args, Sign, Marked) for
% every relation atom the body reaches, Sign = pos|neg (neg = under not/1,
% which is a strictly-lower-stratum read, never a trigger source), Marked =
% marked|unmarked (only/1 wrapped or not). Recurses into not/1 (unlike
% body.pl's body_atoms/2, which the engine deliberately keeps shallow there
% because a negated atom is never a trigger candidate; here we want it for
% stratification and column-name mining, both of which DO care about a
% negated read).

body_ref_uses((Left, Right), Uses) :- !,
    body_ref_uses(Left, LeftUses), body_ref_uses(Right, RightUses),
    append(LeftUses, RightUses, Uses).
body_ref_uses(only(departed(Atom)), [use(Ref, Args, pos, marked)]) :- !,
    atom_ref_args(Atom, Ref, Args).
body_ref_uses(only(Atom), [use(Ref, Args, pos, marked)]) :- !,
    atom_ref_args(Atom, Ref, Args).
body_ref_uses(departed(Atom), [use(Ref, Args, pos, unmarked)]) :- !,
    atom_ref_args(Atom, Ref, Args).
body_ref_uses(pre(Atom), [use(Ref, Args, pos, unmarked)]) :- !,
    atom_ref_args(Atom, Ref, Args).
body_ref_uses(not(Goal), Uses) :- !,
    body_ref_uses(Goal, InnerUses),
    maplist(flip_to_neg, InnerUses, Uses).
body_ref_uses(true, []) :- !.
body_ref_uses(now(_), []) :- !.
body_ref_uses(_ := _, []) :- !.
body_ref_uses(_ is _, []) :- !.
body_ref_uses(decode(_, _), []) :- !.
body_ref_uses(json_each(_, _), []) :- !.
body_ref_uses(Goal, []) :- comparison_goal(Goal), !.
body_ref_uses(Atom, [use(Ref, Args, pos, unmarked)]) :-
    atom_ref_args(Atom, Ref, Args).

flip_to_neg(use(Ref, Args, _, Marked), use(Ref, Args, neg, Marked)).

atom_ref_args(Atom, Ref, Args) :- rel_ref(Atom, Ref), Atom =.. [_ | Args].

comparison_goal(_ < _). comparison_goal(_ =< _). comparison_goal(_ > _).
comparison_goal(_ >= _). comparison_goal(_ == _). comparison_goal(_ \== _).

% ═══ program-wide ref inventory ═════════════════════════════════════════════
% declared_refs/2: every kind(Ref, _) declaration, regardless of whether any
% rule ever mentions Ref. Sweeping the full fixture corpus (phase C) turned
% up EDB-only fixtures with an empty Rules list entirely (e.g.
% engine_core.pl's retention_count_prunes_oldest: `kind(event/1, log),
% keep(event/1, count(2))`, no rules at all) -- program_refs/2 alone walks
% only Rules, so a rel that is purely an arrival target with zero readers
% was silently absent from RelPlans, dropping its DDL/arrival handling from
% the emitted program entirely (a genuine compiler gap, not a
% supported-subset refusal: the program "compiled" into something that could
% never accept its own schedule). compile.pl:program_plan/2 unions this with
% program_refs/2's rule-derived set.

% A ref can be declared via kind/2, keyed/2, or keep/2 independently -- a
% fixture may declare ONLY keyed(Ref, Positions) with no kind/2 at all
% (rel_kind/3's own fallback already handles that: keyed implies Set), and
% the corpus has a real case with zero rule readers too
% (scopes.pl:zombie_scope_negative_case_a2b's `keyed(open_pane/2, [1])`,
% intentionally never read by any rule -- comment: "REJECTED READING
% dropped on purpose"). Scanning only kind/2 missed it, and its Schedule
% arrival then hit the emitted program's "arrival for undeclared rel" guard.
declared_refs(Decls, Refs) :-
    findall(Ref,
            ( member(Decl, Decls),
              ( Decl = kind(Ref, _) ; Decl = keyed(Ref, _) ; Decl = keep(Ref, _) )
            ), Refs0),
    sort(Refs0, Refs).

program_refs(Rules, Refs) :-
    findall(Ref,
            ( member(Rule, Rules),
              ( rule_head_ref(Rule, Ref)
              ; rule_body(Rule, Body), body_ref_uses(Body, Uses), member(use(Ref, _, _, _), Uses) )
            ), Refs0),
    sort(Refs0, Refs).

% Arrival targets = every referenced ref that is NEVER a rule head. A schedule
% (or the fixture's Initial rows) is the only source that can write these.
arrival_target_refs(Rules, ArrivalRefs) :-
    program_refs(Rules, AllRefs),
    derived_refs(Rules, DerivedRefs),
    subtract(AllRefs, DerivedRefs, ArrivalRefs).

% ═══ column naming from surface variable identity ═══════════════════════════
% rel_columns(Rules, Bindings, Ref, Columns): for each argument position of
% Ref (1..Arity), scan every occurrence of Ref across all rule heads and body
% atoms; the first occurrence whose argument at that position is, by ==/2
% identity, one of Bindings' bound variables gives the column its name
% (snake_case of the surface Prolog variable name). A position that is never
% a plain named variable anywhere falls back to col<N>.

% findall/bagof would COPY_TERM every solution's Args, which severs the
% shared variable identity this whole scheme depends on (the same hazard
% engine.pl's trigger_items comment calls out: "findall copies its template,
% which would sever the trigger atom from the body"). column_name_at/4
% therefore drives ref_occurrence_args/3 directly inside an if-then,
% backtracking over the ORIGINAL term without ever collecting it into a list.

rel_columns(Rules, Bindings, Name/Arity, Columns) :-
    numlist(1, Arity, Positions),
    maplist(column_name_at(Rules, Bindings, Name/Arity), Positions, Columns).

ref_occurrence_args(Rules, Ref, Args) :-
    member(Rule, Rules),
    ( rule_head(Rule, Head), rel_ref(Head, Ref), Head =.. [_ | Args]
    ; rule_body(Rule, Body), body_ref_uses(Body, Uses), member(use(Ref, Args, _, _), Uses) ).

column_name_at(Rules, Bindings, Ref, Position, ColumnName) :-
    ( ref_occurrence_args(Rules, Ref, Args),
      nth1(Position, Args, Arg),
      member(SurfaceName = BoundVar, Bindings),
      Arg == BoundVar
    -> snake_name(SurfaceName, ColumnName)
    ; format(atom(ColumnName), 'col~w', [Position]) ).

% CamelCase Prolog variable name -> snake_case column identifier.
snake_name(VarName, ColumnName) :-
    atom_codes(VarName, Codes),
    snake_codes(Codes, SnakeCodes0),
    ( SnakeCodes0 = [0'_ | Rest] -> SnakeCodes = Rest ; SnakeCodes = SnakeCodes0 ),
    atom_codes(ColumnName, SnakeCodes).

snake_codes([], []).
snake_codes([Code | Rest], Out) :-
    ( code_type(Code, upper)
    -> code_type(Lower, to_lower(Code)), Out = [0'_, Lower | More]
    ;  Out = [Code | More]
    ),
    snake_codes(Rest, More).

% ═══ supported-subset gate ═══════════════════════════════════════════════════
% Refuses (with a specific term, not a generic failure) any construct wider
% than what lower.pl knows how to emit: an edge rule body must be exactly
% `only(Atom)` (optionally `only(departed(Atom))`, still unsupported by
% lower.pl today and refused separately); a level rule must not carry an
% aggregate head (aggregate_head, level_eval.pl) and must not reference
% pre/1, now/1, decode/2 or json_each/2 (none of the two target fixtures need
% them; lower.pl has no SQL shape for them yet).

check_supported_subset(prog(Decls, Rules)) :-
    forall(( member(Rule, Rules), rule_is_edge(Rule) ), check_edge_rule_shape(Rule)),
    forall(( member(Rule, Rules), rule_is_level(Rule) ), check_level_rule_shape(Rule)),
    forall(( member(keyed(Ref, Positions), Decls), rel_kind(Decls, Ref, log) ),
           throw(unsupported_construct(keyed_log_rel(Ref, Positions)))).

check_edge_rule_shape((Head <+ Body)) :-
    ( Body = only(Atom), \+ ( Atom = departed(_) )
    -> true
    ; throw(unsupported_construct(edge_body_shape(Head, Body)))
    ),
    % Same hazard as a level head (head_arithmetic_shape/2's comment):
    % compile_head_expr renders every compound argument as a json1 tagged
    % term unconditionally, arithmetic functor or not. No fixture in the
    % current corpus hits this on an edge head, checked defensively anyway.
    ( head_arithmetic_shape(Head, ArithExpr)
    -> throw(unsupported_construct(head_arithmetic(Head, ArithExpr)))
    ; true ).

check_level_rule_shape((Head <- Body)) :-
    ( aggregate_head_shape(Head)
    -> throw(unsupported_construct(aggregate_head(Head)))
    ; true ),
    ( head_arithmetic_shape(Head, ArithExpr)
    -> throw(unsupported_construct(head_arithmetic(Head, ArithExpr)))
    ; true ),
    ( body_forbidden_goal(Body, Forbidden)
    -> throw(unsupported_construct(level_body_goal(Head, Forbidden)))
    ; true ).

aggregate_head_shape(Head) :-
    Head =.. [_ | Args],
    member(Arg, Args), nonvar(Arg), Arg =.. [Kind | _],
    memberchk(Kind, [count, sum, min, max, json_array, json_object]).

% compile_head_expr (lower.pl) renders EVERY compound head argument as a
% json1 "construct a tagged term" expression (json_object('fn', Functor,
% 'args', json_array(...))) -- correct for a genuine domain compound like
% route_data(RouteId), silently WRONG for an arithmetic expression like
% `LeftSize + RightSize - Shared`: nothing evaluates it, so the stored value
% is a json1-encoded expression TREE, never the computed number. Phase C
% sweep finding (fixtures/expressions.pl:head_expression_evaluates_derived_column,
% comparison_filters_rows): both "compiled" clean under the pre-existing
% gate and produced a wrong stored value, invisible to the tick-log diff
% here because both fixtures also have an empty Schedule (all grading is via
% `final(...)`, which this compiler's sweep does not check -- see
% SCOREBOARD.md findings). Refusing head arithmetic turns that silent wrong
% output into a named refusal until real arithmetic lowering lands (widening
% priority: "arithmetic + bind :=" in the phase C contract's own ordering).
head_arithmetic_shape(Head, ArithExpr) :-
    Head =.. [_ | Args],
    member(Arg, Args),
    contains_arithmetic_functor(Arg, ArithExpr).

contains_arithmetic_functor(Arg, Arg) :-
    compound(Arg), Arg =.. [Functor | SubArgs], SubArgs \== [],
    memberchk(Functor, ['+', '-', '*', '/', mod]), !.
contains_arithmetic_functor(Arg, Found) :-
    compound(Arg), Arg =.. [_ | SubArgs], member(SubArg, SubArgs), contains_arithmetic_functor(SubArg, Found).

body_forbidden_goal((Left, Right), Forbidden) :- !,
    ( body_forbidden_goal(Left, Forbidden) ; body_forbidden_goal(Right, Forbidden) ).
body_forbidden_goal(pre(Atom), pre(Atom)) :- !.
body_forbidden_goal(now(Tick), now(Tick)) :- !.
body_forbidden_goal(decode(Expr, Pattern), decode(Expr, Pattern)) :- !.
body_forbidden_goal(json_each(Expr, Elem), json_each(Expr, Elem)) :- !.
body_forbidden_goal(departed(Atom), departed(Atom)) :- !.
% Comparison operators (body_ref_uses/2 already returns zero Uses for these
% -- comparison_goal/1 -- meaning nothing downstream ever compiled them into
% a WHERE clause; a level rule that filters on one silently lost the filter)
% and `:=`/`is` binds (body_ref_uses/2 returns zero Uses for these too, and
% lower.pl's compile_pattern_arg never learns the bound variable, so any
% head reference to it already fails as unbound_head_var -- but a `:=`/`is`
% goal that is NEVER read by the head, only used to gate a later comparison,
% reached no failure at all and just silently vanished). Both are phase C
% sweep findings (fixtures/expressions.pl:comparison_filters_rows,
% range_join_over_arithmetic, bind_computes_derived_value_then_comparison_filters);
% refusing them cleanly until real lowering lands, rather than leaving the
% silent-drop behavior.
body_forbidden_goal(Left < Right, comparison(Left < Right)) :- !.
body_forbidden_goal(Left =< Right, comparison(Left =< Right)) :- !.
body_forbidden_goal(Left > Right, comparison(Left > Right)) :- !.
body_forbidden_goal(Left >= Right, comparison(Left >= Right)) :- !.
body_forbidden_goal(Left == Right, comparison(Left == Right)) :- !.
body_forbidden_goal(Left \== Right, comparison(Left \== Right)) :- !.
body_forbidden_goal(Var := Expr, bind(Var := Expr)) :- !.
body_forbidden_goal(Var is Expr, bind(Var is Expr)) :- !.
