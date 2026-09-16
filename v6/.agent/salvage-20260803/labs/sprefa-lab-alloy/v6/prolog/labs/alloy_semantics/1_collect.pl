% 1_collect.pl : materialize decl/ref/import facts from 0_facts, per target.
%
% The refkey/binder analogy: 0_facts carries raw schema (tables + FK columns).
% This pass resolves which symbol IDs exist (decl) and which files reference
% them (ref), then derives the minimal cross-file import set (import). The
% rendered NAME is still absent here -- naming is the check+render concern.
%
% Materialized per target into dynamic facts so 2_check and 3_render share one
% snapshot per target:
%   col_decl(Target, Id, Kind, File)   a symbol exists in some file
%   col_ref (Target, FromFile, Id)     a file references a symbol
%   col_imp (Target, FromFile, ToFile, Id)  cross-file ref (FromFile /= ToFile)
%
% File assignment (the ts virtual-file split and the rust module split):
%   ts:   strings -> core.ts ; node, edge -> graph.ts   (graph imports core)
%   rust: strings -> core     ; node, edge -> graph      (graph `use` core)

:- module(alloy_collect,
          [ collect/1,
            decl/4,
            ref/3,
            import_needed/4,
            rendered_name/3,
            decl_table/2,
            target_file/3,
            target_kind/2
          ]).

:- use_module('0_facts').

:- dynamic col_decl/4.
:- dynamic col_ref/3.
:- dynamic col_imp/4.

% ---- target config ----------------------------------------------------------

target_file(ts,   strings, 'core.ts').
target_file(ts,   node,    'graph.ts').
target_file(ts,   edge,    'graph.ts').
target_file(rust, strings, 'core').
target_file(rust, node,    'graph').
target_file(rust, edge,    'graph').

target_kind(ts,   interface).
target_kind(rust, struct).

% ---- sabotage hooks (lab protocol, mirrors OPENAPI_LAB_DROP in emit_openapi) -
%
% ALLOW_LAB_SABOTAGE_UNRESOLVED=1  -> node gets NO decl, edge still refs it.
% ALLOW_LAB_SABOTAGE_DUPLICATE=1   -> a rendered name is asserted twice.
sabotage_unresolved :-
    getenv('ALLOW_LAB_SABOTAGE_UNRESOLVED', V), V \== ''.
sabotage_duplicate :-
    getenv('ALLOW_LAB_SABOTAGE_DUPLICATE', V), V \== ''.

% ---- the collect pass -------------------------------------------------------

collect(Target) :-
    retractall(col_decl(Target, _, _, _)),
    retractall(col_ref(Target, _, _)),
    retractall(col_imp(Target, _, _, _)),
    forall(base_decl(Target, Id, Kind, File),
           assertz(col_decl(Target, Id, Kind, File))),
    forall(base_ref(Target, FromFile, Id),
           assertz(col_ref(Target, FromFile, Id))),
    forall((col_ref(Target, FromFile, Id),
            col_decl(Target, Id, _, ToFile),
            FromFile \== ToFile),
           assertz(col_imp(Target, FromFile, ToFile, Id))).

base_decl(Target, Id, Kind, File) :-
    table(Name, _),
    \+ unresolved_drop(Name),
    target_file(Target, Name, File),
    target_kind(Target, Kind),
    atom_concat(s_, Name, Id).

unresolved_drop(Name) :-
    sabotage_unresolved,
    Name == node.

base_ref(Target, FromFile, Id) :-
    column(Table, _, _, _, RefTable, _, _, _),
    RefTable \== none,
    target_file(Target, Table, FromFile),
    atom_concat(s_, RefTable, Id).

% ---- readers for the later passes -------------------------------------------

decl(Target, Id, Kind, File) :- col_decl(Target, Id, Kind, File).
ref(Target, FromFile, Id)    :- col_ref(Target, FromFile, Id).
import_needed(Target, FromFile, ToFile, Id) :- col_imp(Target, FromFile, ToFile, Id).

% ---- rendered-name policy (the name key) ------------------------------------
%
% ts:   PascalCase(table)+"Row"  -> StringsRow / NodeRow / EdgeRow (the real
%       spine.ts uses the Row suffix for entity row types).
% rust: PascalCase(table)        -> Strings / Node / Edge (the real struct).
rendered_name(Target, Id, Name) :-
    decl_table(Id, Table),
    target_name(Target, Table, Name).

target_name(ts, Table, Name) :-
    words_pascal(Table, P),
    atom_concat(P, 'Row', Name).
target_name(rust, Table, Name) :-
    words_pascal(Table, Name).

decl_table(Id, Table) :-
    atom_concat(s_, Table, Id).

words_pascal(Base, Pascal) :-
    atomic_list_concat(Parts, '_', Base),
    maplist(capitalize, Parts, CapParts),
    atomic_list_concat(CapParts, Pascal).

capitalize(Part, Cap) :-
    atom_codes(Part, [C|Cs]),
    code_type(U, to_upper(C)),
    atom_codes(Cap, [U|Cs]).
capitalize(Part, Cap) :-
    Part == '',
    Cap = ''.
