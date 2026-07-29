% 1_rows.pl : ROW spread inside rules (case C3) and the width check that
% catches the wider-row-where-narrower-wanted hazard (case C4).
%
% Surface shape under test:
%
%   c(...ARow, 5) <- a(...ARow), guard(...)
%
% Term shape:
%
%   (c(spread(ARow), 5) <- a(spread(ARow)))
%
% where ARow is an ordinary Prolog variable used as a ROW GROUP MARKER. It is
% never a value. What it binds is decided here: one FRESH VARIABLE PER SPLICED
% COLUMN, shared across every occurrence of the marker, so the splice is an
% ordinary positional join and nothing downstream sees a new construct.
%
% Width comes from the DECLARED arity of the relation the marker is spread
% into, minus the explicit slots written beside it. A marker that never
% appears in a body atom has no width source and is refused.
%
% The term is walked in place. findall/copy_term would sever the variable
% identity the whole scheme depends on, the same hazard analyze.pl:rel_columns
% calls out for column naming.

:- module(row_spread,
          [ expand_row_spread_rules/3,
            row_spread_widths/3
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('0_spread', [declared_columns/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% expand_row_spread_rules(+Decls, +Rules, -ExpandedRules)
expand_row_spread_rules(Decls, Rules, Expanded) :-
    maplist(expand_row_spread_rule(Decls), Rules, Expanded).

expand_row_spread_rule(Decls, Rule, Expanded) :-
    rule_parts(Rule, Arrow, Head, Body),
    body_atom_list(Body, BodyAtoms),
    collect_widths(Decls, BodyAtoms, [], Widths),
    mint_groups(Widths, Groups),
    check_head_markers_bound(Head, Groups),
    splice_atom(Groups, Head, SplicedHead),
    check_head_arity(Decls, SplicedHead),
    maplist(splice_atom(Groups), BodyAtoms, SplicedBodyAtoms),
    conjoin_atoms(SplicedBodyAtoms, SplicedBody),
    rule_parts(Expanded, Arrow, SplicedHead, SplicedBody).

% row_spread_widths(+Decls, +Rules, -Widths) exposes the width table for
% grading without performing the splice.
row_spread_widths(Decls, Rules, Widths) :-
    foldl(rule_widths(Decls), Rules, [], Widths).

rule_widths(Decls, Rule, Acc, Widths) :-
    rule_parts(Rule, _, _, Body),
    body_atom_list(Body, BodyAtoms),
    collect_widths(Decls, BodyAtoms, Acc, Widths).

rule_parts((Head <- Body), (<-), Head, Body).
rule_parts((Head <+ Body), (<+), Head, Body).

body_atom_list(Body, Atoms) :-
    ( nonvar(Body), Body = (Left, Right)
    -> body_atom_list(Left, LeftAtoms),
       body_atom_list(Right, RightAtoms),
       append(LeftAtoms, RightAtoms, Atoms)
    ;  Atoms = [Body]
    ).

conjoin_atoms([Atom], Atom) :- !.
conjoin_atoms([Atom | Rest], (Atom, Conjoined)) :-
    conjoin_atoms(Rest, Conjoined).

% ═══ width collection ═══════════════════════════════════════════════════════
collect_widths(_, [], Widths, Widths).
collect_widths(Decls, [Atom | Rest], Acc, Widths) :-
    atom_markers(Atom, Markers),
    ( Markers == []
    -> Acc1 = Acc
    ;  Markers = [Marker]
    -> atom_marker_width(Decls, Atom, Width),
       add_width(Marker, Width, Acc, Acc1)
    ;  throw(unsupported_construct(multiple_row_spreads_in_atom(Atom)))
    ),
    collect_widths(Decls, Rest, Acc1, Widths).

atom_markers(Atom, Markers) :-
    ( compound(Atom)
    -> Atom =.. [_ | Args],
       marker_args(Args, Markers)
    ;  Markers = []
    ).

marker_args([], []).
marker_args([Arg | Rest], Markers) :-
    ( nonvar(Arg), Arg = spread(Marker)
    -> Markers = [Marker | More]
    ;  Markers = More
    ),
    marker_args(Rest, More).

atom_marker_width(Decls, Atom, Width) :-
    functor(Atom, Name, WrittenArity),
    ( declared_columns(Decls, Name, Columns)
    -> length(Columns, DeclaredArity)
    ;  throw(unsupported_construct(row_spread_width_unknown(Name)))
    ),
    ExplicitSlots is WrittenArity - 1,
    Width is DeclaredArity - ExplicitSlots,
    ( Width < 0
    -> throw(unsupported_construct(
                 row_spread_overfills(Name, WrittenArity, DeclaredArity)))
    ;  true
    ).

add_width(Marker, Width, Acc, Acc1) :-
    ( member(w(Seen, SeenWidth), Acc), Seen == Marker
    -> ( SeenWidth =:= Width
       -> Acc1 = Acc
       ;  throw(unsupported_construct(
                    row_spread_width_conflict(SeenWidth, Width)))
       )
    ;  append(Acc, [w(Marker, Width)], Acc1)
    ).

mint_groups([], []).
mint_groups([w(Marker, Width) | Rest], [g(Marker, Vars) | More]) :-
    length(Vars, Width),
    mint_groups(Rest, More).

% ═══ the splice ═════════════════════════════════════════════════════════════
check_head_markers_bound(Head, Groups) :-
    atom_markers(Head, Markers),
    forall(member(Marker, Markers),
           ( member(g(Bound, _), Groups), Bound == Marker
           -> true
           ;  throw(unsupported_construct(row_spread_unbound_in_head))
           )).

splice_atom(Groups, Atom, Spliced) :-
    ( compound(Atom)
    -> Atom =.. [Name | Args],
       splice_args(Groups, Args, SplicedArgs),
       Spliced =.. [Name | SplicedArgs]
    ;  Spliced = Atom
    ).

splice_args(_, [], []).
splice_args(Groups, [Arg | Rest], Spliced) :-
    ( nonvar(Arg), Arg = spread(Marker)
    -> ( member(g(Bound, Vars), Groups), Bound == Marker
       -> true
       ;  throw(unsupported_construct(row_spread_unbound_in_head))
       ),
       append(Vars, More, Spliced)
    ;  Spliced = [Arg | More]
    ),
    splice_args(Groups, Rest, More).

% ═══ head arity totality (case C3 second half, case C4 check) ═══════════════
% After the splice the head must fill its declared width EXACTLY. A wider row
% spliced where a narrower relation is declared lands here, and so does a
% narrower one; the splice never coerces and never truncates.
check_head_arity(Decls, Head) :-
    functor(Head, Name, SplicedArity),
    ( declared_columns(Decls, Name, Columns)
    -> length(Columns, DeclaredArity),
       ( SplicedArity =:= DeclaredArity
       -> true
       ;  throw(unsupported_construct(
                    head_arity_mismatch(Name, SplicedArity, DeclaredArity)))
       )
    ;  throw(unsupported_construct(head_width_unknown(Name)))
    ).
