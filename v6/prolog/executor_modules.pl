% @comment-ok: the import form's single documentation site, and the one place
% that says which term shapes carry a rel name.
%
% executor_modules.pl : an executor family is a MODULE a file imports.
%
%   use soopy.            files(glob: key(text)) -> (path: text, digest: text)
%   use soopy as s.       s.files(glob: key(text)) -> (...)
%   rel /soopy/files(...) the path spelling, still legal, still the same rel
%
% All three land on the registry's canonical atom `soopy__files`, so hosts.rs,
% the emitted SQL identifiers and every sidecar see one name. The import is a
% RENAME over the importing file's own program: the local name the file wrote
% is replaced by the canonical one in its decls, rules and queries, and nothing
% below the parser learns a new spelling.
%
% A declaration is what binds. `use soopy.` plus `rel files(...)` makes that
% declaration soopy's `files`; a file that wants a `files` of its own aliases
% the module instead. Two used modules exporting one leaf, both unaliased,
% stop at ambiguous_executor_leaf rather than picking a winner.
%
% A rel name reaches a term in four shapes and no other: the Name/Arity ref a
% decl carries, the functor of a plain relation atom, argument 1 of the three
% rel_name_argument/2 terms, and the segment list of a rel_path. An atom
% anywhere else is a value, so a string or enum arm spelling the same word
% does not move.

:- module(executor_modules,
          [ split_use_specs/3,
            executor_family_export/3,
            bind_executor_modules/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs), [pairs_keys/2]).
:- use_module('next/registry', [arrival_executor/2]).

%! executor_family_export(?Family, ?Segments, ?Canonical) is nondet.
%  A roster row with no `__` names no module and exports nothing.
executor_family_export(Family, Segments, Canonical) :-
    arrival_executor(Canonical, _),
    atomic_list_concat(Parts, '__', Canonical),
    Parts = [Family | Segments],
    Segments \== [].

%! split_use_specs(+UseSpecs, -FileSpecs, -ModuleSpecs) is det.
%  A bare `use soopy` never reaches the path resolver.
split_use_specs([], [], []).
split_use_specs([Spec | Rest], FileSpecs, ModuleSpecs) :-
    (   module_use_spec(Spec, Module)
    ->  FileSpecs = MoreFiles,
        ModuleSpecs = [Module | MoreModules]
    ;   FileSpecs = [Spec | MoreFiles],
        ModuleSpecs = MoreModules
    ),
    split_use_specs(Rest, MoreFiles, MoreModules).

% `pub` re-exports a FILE's rels; a family's names are the registry's and
% already reach every file that imports the family, so pub adds nothing.
module_use_spec(use_mod(Family),            mod(Family, none)).
module_use_spec(use_mod(Family, Alias),     mod(Family, alias(Alias))).
module_use_spec(pub_use_mod(Family),        mod(Family, none)).
module_use_spec(pub_use_mod(Family, Alias), mod(Family, alias(Alias))).

%! bind_executor_modules(+ModuleSpecs, +parts(D,R,Q), -parts(D,R,Q)) is det.
%  Term-identical out when no declaration is claimed by an import.
bind_executor_modules([], Parts, Parts) :- !.
bind_executor_modules(ModuleSpecs, parts(Decls0, Rules0, Queries0), Parts) :-
    maplist(check_known_module, ModuleSpecs),
    findall(Local-Family-Canonical,
            ( member(mod(Family, AliasOrNone), ModuleSpecs),
              executor_family_export(Family, Segments, Canonical),
              local_name(AliasOrNone, Segments, Local) ),
            Candidates),
    findall(Name, file_declared_name(Decls0, Name), Names0),
    sort(Names0, Names),
    findall(Local-Canonical,
            ( member(Local, Names),
              claimed_by(Candidates, Local, Canonical),
              Local \== Canonical ),
            Pairs0),
    sort(Pairs0, Map),
    (   Map == []
    ->  Parts = parts(Decls0, Rules0, Queries0)
    ;   maplist(rename_term(Map), Decls0, Decls),
        maplist(rename_term(Map), Rules0, Rules),
        maplist(rename_term(Map), Queries0, Queries),
        Parts = parts(Decls, Rules, Queries)
    ).

check_known_module(mod(Family, _)) :-
    (   executor_family_export(Family, _, _)
    ->  true
    ;   throw(unsupported_construct(unknown_executor_module(Family)))
    ).

local_name(none, Segments, Local) :-
    atomic_list_concat(Segments, '__', Local).
local_name(alias(Alias), Segments, Local) :-
    atomic_list_concat([Alias | Segments], '__', Local).

claimed_by(Candidates, Local, Canonical) :-
    findall(Family-Full, member(Local-Family-Full, Candidates), Claims0),
    sort(Claims0, Claims),
    Claims \== [],
    (   Claims = [_-Only]
    ->  Canonical = Only
    ;   pairs_keys(Claims, Families),
        throw(unsupported_construct(ambiguous_executor_leaf(Local, Families)))
    ).

% Already `__`-joined by the parser for a dotted or slash-rooted declaration,
% which is why a claim is matched on the joined atom and not on segments.
file_declared_name(Decls, Name) :- member(rel_path_decl(Name/_, _), Decls).
file_declared_name(Decls, Name) :- member(sh_decl(Name, _, _, _), Decls).
file_declared_name(Decls, Name) :- member(col_type(Name/_, _, _), Decls).
file_declared_name(Decls, Name) :- member(kind(Name/_, _), Decls).
file_declared_name(Decls, Name) :- member(keyed(Name/_, _), Decls).
file_declared_name(Decls, Name) :- member(keep(Name/_, _), Decls).

rename_term(Map, Term0, Term) :-
    (   var(Term0)
    ->  Term = Term0
    ;   atomic(Term0)
    ->  Term = Term0
    ;   Term0 = Name0/Arity,
        atom(Name0),
        integer(Arity)
    ->  mapped(Map, Name0, Name),
        Term = Name/Arity
    ;   Term0 = rel_path(Segments0, Args0)
    ->  rename_segments(Map, Segments0, Segments),
        maplist(rename_term(Map), Args0, Args),
        Term = rel_path(Segments, Args)
    ;   functor(Term0, Functor, Arity),
        rel_name_argument(Functor, Arity)
    ->  Term0 =.. [Functor, Name0 | Rest0],
        mapped(Map, Name0, Name),
        maplist(rename_term(Map), Rest0, Rest),
        Term =.. [Functor, Name | Rest]
    ;   Term0 =.. [Functor0 | Args0],
        mapped(Map, Functor0, Functor),
        maplist(rename_term(Map), Args0, Args),
        Term =.. [Functor | Args]
    ).

% Arrival rels: the declaration, its identity columns, and every reference.
rel_name_argument(sh_decl, 4).
rel_name_argument(arrival_identity, 2).
rel_name_argument(probe, 4).

mapped(Map, Name, Mapped) :-
    (   atom(Name),
        memberchk(Name-New, Map)
    ->  Mapped = New
    ;   Mapped = Name
    ).

rename_segments(Map, Segments0, Segments) :-
    (   atomic_list_concat(Segments0, '__', Joined),
        memberchk(Joined-New, Map)
    ->  atomic_list_concat(Segments, '__', New)
    ;   Segments = Segments0
    ).
