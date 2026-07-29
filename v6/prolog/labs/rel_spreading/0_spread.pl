% 0_spread.pl : declaration-time COLUMN SPLICE prototype (case C1, C2, C5, C6).
%
% Follows the expand-module precedent of 0_enum_expand.pl and
% 0_match_expand.pl: ONE shared expansion over prog(Decls, Rules) that both the
% oracle engine and the compiler would consult, producing ORDINARY declaration
% entries that no downstream phase has to know about.
%
% Sugar term retained in the program:
%
%   spread_decl(Name, [spread(a), col(extra, int)])
%
% Consumers receive ordinary entries, in the spliced source order:
%
%   col_type(b/3, id, int)
%   col_type(b/3, name, text)
%   col_type(b/3, extra, int)
%
% STRUCTURAL CONSEQUENCE, found by writing this and not assumed: the sugar
% term CANNOT carry Name/Arity the way every other decl entry does, because
% the arity is what the splice computes. A spread rel's arity stops being
% syntactic. That is why the modifier entries (kind/keyed/keep) accept a bare
% Name for a spread rel and get their arity filled in here; an explicitly
% written Name/Arity is CHECKED, never trusted.

:- module(spread_expand,
          [ expand_spread_program/2,
            expand_spread_program_inheriting/2,
            spread_columns/3,
            spread_arity/3,
            declared_columns/3
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

% ═══ the expansion ══════════════════════════════════════════════════════════
% expand_spread_program(+prog(SugaredDecls, Rules), -prog(ExpandedDecls, Rules))
%
% 1. every spread_decl entry resolves its column list (recursively; a spread
%    source may itself be spread, and a cycle is a named refusal);
% 2. the resolved list is checked for column collisions;
% 3. the spread_decl entry is replaced IN PLACE by its col_type entries, so
%    declaration source order is preserved exactly (the roundtrip law);
% 4. modifier entries carrying a bare Name for a spread rel get the computed
%    arity; ones carrying Name/Arity are validated against it;
% 5. NOTHING else crosses the splice. kind, keyed, keep and the source's own
%    modifiers stay with the source (case C5).
expand_spread_program(prog(SugaredDecls, Rules), prog(ExpandedDecls, Rules)) :-
    spread_arity_map(SugaredDecls, ArityMap),
    expand_decls(SugaredDecls, SugaredDecls, ArityMap, no_inherit, ExpandedDecls).

% The graded NEGATIVE of case C5: the same expansion with plane and key
% inheritance switched on. Used only to show what the selected semantics
% avoids; never proposed.
expand_spread_program_inheriting(prog(SugaredDecls, Rules),
                                 prog(ExpandedDecls, Rules)) :-
    spread_arity_map(SugaredDecls, ArityMap),
    expand_decls(SugaredDecls, SugaredDecls, ArityMap, inherit, ExpandedDecls).

spread_arity_map(Decls, ArityMap) :-
    findall(Name-Arity,
            ( member(spread_decl(Name, _), Decls),
              spread_arity(Decls, Name, Arity)
            ),
            ArityMap).

spread_arity(Decls, Name, Arity) :-
    spread_columns(Decls, Name, Columns),
    length(Columns, Arity).

expand_decls([], _, _, _, []).
expand_decls([Decl | Rest], AllDecls, ArityMap, Mode, Expanded) :-
    expand_decl(Decl, AllDecls, ArityMap, Mode, ThisEntries),
    expand_decls(Rest, AllDecls, ArityMap, Mode, RestEntries),
    append(ThisEntries, RestEntries, Expanded).

expand_decl(spread_decl(Name, _Spec), AllDecls, _ArityMap, Mode, Entries) :-
    !,
    ( declared_columns(AllDecls, Name, _)
    -> throw(unsupported_construct(spread_and_explicit_columns(Name)))
    ;  true
    ),
    spread_columns(AllDecls, Name, Columns),
    length(Columns, Arity),
    Ref = Name/Arity,
    maplist(column_entry(Ref), Columns, ColumnEntries),
    inherited_entries(Mode, AllDecls, Name, Ref, InheritedEntries),
    append(ColumnEntries, InheritedEntries, Entries).
expand_decl(Decl, _AllDecls, ArityMap, _Mode, [Resolved]) :-
    modifier_ref(Decl, Ref),
    !,
    resolve_modifier_ref(Decl, Ref, ArityMap, Resolved).
expand_decl(Decl, _AllDecls, _ArityMap, _Mode, [Decl]).

column_entry(Ref, col(Column, Type), col_type(Ref, Column, Type)).

% C5 negative only: copy the FIRST spread source's plane and key words onto
% the target. Never selected.
inherited_entries(no_inherit, _, _, _, []).
inherited_entries(inherit, AllDecls, Name, Ref, Entries) :-
    ( member(spread_decl(Name, Spec), AllDecls),
      member(spread(Source), Spec)
    -> findall(Entry, inherited_entry(AllDecls, Source, Ref, Entry), Entries)
    ;  Entries = []
    ).

inherited_entry(AllDecls, Source, Ref, kind(Ref, Kind)) :-
    member(kind(Source/_, Kind), AllDecls).
inherited_entry(AllDecls, Source, Ref, keyed(Ref, Positions)) :-
    member(keyed(Source/_, Positions), AllDecls).
inherited_entry(AllDecls, Source, Ref, keep(Ref, Retention)) :-
    member(keep(Source/_, Retention), AllDecls).

modifier_ref(kind(Ref, _), Ref).
modifier_ref(keyed(Ref, _), Ref).
modifier_ref(keep(Ref, _), Ref).
modifier_ref(col_type(Ref, _, _), Ref).

resolve_modifier_ref(Decl, Ref, ArityMap, Resolved) :-
    ( atom(Ref)
    -> ( memberchk(Ref-Arity, ArityMap)
       -> Ref = Name, ResolvedRef = Name/Arity,
          replace_modifier_ref(Decl, ResolvedRef, Resolved)
       ;  throw(unsupported_construct(bare_ref_on_unspread_rel(Ref)))
       )
    ;  Ref = Name/WrittenArity,
       ( memberchk(Name-SplicedArity, ArityMap),
         SplicedArity \== WrittenArity
       -> throw(unsupported_construct(
                    spread_arity_conflict(Name, WrittenArity, SplicedArity)))
       ;  Resolved = Decl
       )
    ).

replace_modifier_ref(kind(_, Kind), Ref, kind(Ref, Kind)).
replace_modifier_ref(keyed(_, Positions), Ref, keyed(Ref, Positions)).
replace_modifier_ref(keep(_, Retention), Ref, keep(Ref, Retention)).
replace_modifier_ref(col_type(_, Column, Type), Ref, col_type(Ref, Column, Type)).

% ═══ column resolution ══════════════════════════════════════════════════════
% spread_columns(+Decls, +Name, -Columns) with Columns = [col(Name, Type)] in
% spliced source order.
spread_columns(Decls, Name, Columns) :-
    spread_columns(Decls, Name, [], Columns).

spread_columns(Decls, Name, Seen, Columns) :-
    (  memberchk(Name, Seen)
    -> reverse([Name | Seen], Cycle),
       throw(unsupported_construct(spread_cycle(Cycle)))
    ;  true
    ),
    (  member(spread_decl(Name, Spec), Decls)
    -> maplist(spec_columns(Decls, [Name | Seen]), Spec, ColumnLists),
       append(ColumnLists, Columns),
       check_column_collisions(Name, Columns)
    ;  declared_columns(Decls, Name, Columns)
    -> true
    ;  throw(unsupported_construct(spread_source_not_declared(Name)))
    ).

spec_columns(Decls, Seen, spread(Source), Columns) :-
    !,
    spread_columns(Decls, Source, Seen, Columns).
spec_columns(_, _, col(Column, Type), [col(Column, Type)]) :-
    atom(Column), atom(Type), !.
spec_columns(_, _, Item, _) :-
    throw(unsupported_construct(spread_spec_shape(Item))).

% Declared columns of a plain rel: its col_type entries in Decls source order.
% findall preserves that order, which is exactly the ordering authority the
% decl_column_spelling ruling gives the source text.
declared_columns(Decls, Name, Columns) :-
    findall(col(Column, Type),
            ( member(col_type(Ref, Column, Type), Decls),
              ref_name(Ref, Name)
            ),
            Columns),
    Columns \== [].

ref_name(Name/_, Name) :- !.
ref_name(Name, Name) :- atom(Name).

check_column_collisions(Name, Columns) :-
    findall(Column, member(col(Column, _), Columns), Names),
    duplicate_column(Names, Duplicate),
    !,
    throw(unsupported_construct(spread_column_collision(Name, Duplicate))).
check_column_collisions(_, _).

duplicate_column([Column | Rest], Column) :-
    memberchk(Column, Rest), !.
duplicate_column([_ | Rest], Duplicate) :-
    duplicate_column(Rest, Duplicate).
