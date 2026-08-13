% xref.pl : go-to-reference over a .dl6 program's module graph. A rel's module
% is the one whose FILE declared it, never the entry that spliced it.

:- module(dl6_xref,
          [ dl6_reference_rows/2,
            dl6_report/1,
            dl6_report/2
          ]).

:- use_module(library(lists)).
:- use_module('../prolog/use_resolve', [expand_uses/6]).
:- use_module('../prolog/analyze', [body_ref_uses/2]).
:- use_module('../prolog/0_dot_expand', [declared_path/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

%! dl6_reference_rows(+EntryPath, -Rows) is det.
%   Rows = reference(FromModule, FromRef, ToModule, ToRef, Crossing).
dl6_reference_rows(EntryPath, Rows) :-
    expand_uses(EntryPath, [], [], _Loaded, Program, _ModuleTable),
    program_decls_rules(Program, Decls, Rules),
    declaring_modules(Decls, DeclaringModules),
    findall(Row,
            ( member(Rule, Rules),
              rule_reference_row(Rule, DeclaringModules, Row) ),
            Rows0),
    sort(Rows0, Rows).

program_decls_rules(prog(Decls, Rules), Decls, Rules).
program_decls_rules(program(Decls, Rules, _Queries), Decls, Rules).

% rel_module_decl/2 is minted per FILE by use_resolve, mounts excluded.
declaring_modules(Decls, DeclaringModules) :-
    findall(Name-Module,
            ( member(rel_module_decl(Name, Hash), Decls),
              module_name_for_hash(Decls, Hash, Module) ),
            Pairs),
    sort(Pairs, DeclaringModules).

module_name_for_hash(Decls, Hash, Module) :-
    ( member(module_decl(Module, Hash), Decls)
    -> true
    ;  Module = Hash
    ).

rule_reference_row(Rule, DeclaringModules, Row) :-
    rule_head_body(Rule, HeadName, Body),
    body_ref_uses(Body, Uses),
    member(Use, Uses),
    use_ref_name(Use, ToName),
    module_of(DeclaringModules, HeadName, FromModule),
    module_of(DeclaringModules, ToName, ToModule),
    ( FromModule == ToModule -> Crossing = same ; Crossing = crosses ),
    Row = reference(FromModule, HeadName, ToModule, ToName, Crossing).

rule_head_body((Head <- Body), Name, Body) :- !, atom_functor_name(Head, Name).
rule_head_body((Head <+ Body), Name, Body) :- !, atom_functor_name(Head, Name).

atom_functor_name(Atom, Name) :- functor(Atom, Name, _).

use_ref_name(use(Name/_Arity, _Args, _Sign, _Marking), Name) :- !.
use_ref_name(use(Name, _Args, _Sign, _Marking), Name) :- atom(Name).

module_of(DeclaringModules, Name, Module) :-
    ( memberchk(Name-Found, DeclaringModules)
    -> Module = Found
    ;  Module = unknown
    ).

%! dl6_report(+EntryPath) is det.
dl6_report(EntryPath) :- dl6_report(EntryPath, _).

dl6_report(EntryPath, Rows) :-
    dl6_reference_rows(EntryPath, Rows),
    forall(member(reference(FromModule, FromRef, ToModule, ToRef, Crossing), Rows),
           format("~w~t~12| ~w~t~26| -> ~w~t~40| ~w~t~54| ~w~n",
                  [FromModule, FromRef, ToModule, ToRef, Crossing])),
    include([reference(_, _, _, _, crosses)] >> true, Rows, Crossings),
    length(Rows, Total),
    length(Crossings, CrossingCount),
    format("DL6_XREF references=~w crossings=~w~n", [Total, CrossingCount]).
