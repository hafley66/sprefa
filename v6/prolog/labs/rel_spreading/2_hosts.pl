% 2_hosts.pl : does the decl-time splice compose with the host declaration
% (case C7)? The host term forms are the ones the hosts+extraction verdict
% selected, plans/2026-07-29-hosts-extraction-verdict.md:
%
%   sh_decl(Name, InputColumns, OutputColumns, template(Text))
%   probe(Name, InputValues, OutputValues, SaltColumns)
%
% Surface under test:
%
%   sh fetch(...common, ep: text) -> (status: int, body: text) = `...`.
%
% The lab reimplements the minimal host-decl compiler (that lab died on
% landing per protocol) so the composition can be graded here, and reuses the
% column resolver from 0_spread.pl unchanged. That reuse IS the case C7
% answer: if the splice is decl-time, the host side needs no spread code of
% its own, only the same resolver over two column lists instead of one.

:- module(host_spread,
          [ expand_host_decl/3,
            compile_host_decl/2
          ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('0_spread', [spread_columns/3]).

% expand_host_decl(+Decls, +SugaredShDecl, -ExpandedShDecl)
%
% Each side is resolved INDEPENDENTLY through the same resolver the rel
% declaration uses. The input/output split therefore survives the splice: a
% spread on the input side can never move a column to the output side, which
% is the property the hosts verdict selected the explicit split for.
expand_host_decl(Decls, sh_decl(Name, InputSpec, OutputSpec, Template),
                 sh_decl(Name, InputColumns, OutputColumns, Template)) :-
    maplist(host_spec_columns(Decls), InputSpec, InputLists),
    append(InputLists, InputColumns),
    maplist(host_spec_columns(Decls), OutputSpec, OutputLists),
    append(OutputLists, OutputColumns).

host_spec_columns(Decls, spread(Source), Columns) :-
    !,
    spread_columns(Decls, Source, SourceColumns),
    maplist(as_host_column, SourceColumns, Columns).
host_spec_columns(_, col(Column, Type), [col(Column, Type)]) :-
    atom(Column), atom(Type), !.
host_spec_columns(_, Item, _) :-
    throw(unsupported_construct(spread_spec_shape(Item))).

as_host_column(col(Column, Type), col(Column, Type)).

% ═══ the host-decl checks the splice must keep firing ═══════════════════════
% compile_host_decl(+ExpandedShDecl, -host_plan(Name, Inputs, Outputs, Template))
%
% Refusal names are the ones the hosts+extraction verdict recorded, so a
% spliced column and a hand-written column produce the SAME refusal.
compile_host_decl(sh_decl(Name, Inputs, Outputs, template(Text)),
                  host_plan(Name, Inputs, Outputs, template(Text))) :-
    column_names(Inputs, InputNames),
    column_names(Outputs, OutputNames),
    check_duplicates(input, InputNames),
    check_duplicates(output, OutputNames),
    check_overlap(InputNames, OutputNames),
    template_references(Text, References),
    check_unreferenced_inputs(InputNames, References),
    check_outputs_not_referenced(OutputNames, References),
    check_unknown_references(References, InputNames).

column_names(Columns, Names) :-
    findall(Name, member(col(Name, _), Columns), Names).

check_duplicates(Role, Names) :-
    ( duplicate_name(Names, Duplicate)
    -> throw(unsupported_construct(column_mismatch(Role, duplicate(Duplicate))))
    ;  true
    ).

duplicate_name([Name | Rest], Name) :- memberchk(Name, Rest), !.
duplicate_name([_ | Rest], Duplicate) :- duplicate_name(Rest, Duplicate).

check_overlap(InputNames, OutputNames) :-
    ( member(Name, InputNames), memberchk(Name, OutputNames)
    -> throw(unsupported_construct(
                 column_mismatch(input_output_overlap(Name))))
    ;  true
    ).

check_unreferenced_inputs(InputNames, References) :-
    ( member(Name, InputNames), \+ memberchk(Name, References)
    -> throw(unsupported_construct(
                 template_mismatch(unreferenced_input(Name))))
    ;  true
    ).

check_outputs_not_referenced(OutputNames, References) :-
    ( member(Name, OutputNames), memberchk(Name, References)
    -> throw(unsupported_construct(
                 template_mismatch(output_used_as_input(Name))))
    ;  true
    ).

check_unknown_references(References, InputNames) :-
    ( member(Name, References), \+ memberchk(Name, InputNames)
    -> throw(unsupported_construct(template_mismatch(unknown_column(Name))))
    ;  true
    ).

% `{name}` references inside the template text.
template_references(Text, References) :-
    string_codes(Text, Codes),
    brace_names(Codes, References).

brace_names([], []).
brace_names([0'{ | Rest], [Name | More]) :-
    !,
    brace_name_codes(Rest, NameCodes, Tail),
    atom_codes(Name, NameCodes),
    brace_names(Tail, More).
brace_names([_ | Rest], References) :-
    brace_names(Rest, References).

brace_name_codes([], [], []).
brace_name_codes([0'} | Rest], [], Rest) :- !.
brace_name_codes([Code | Rest], [Code | More], Tail) :-
    brace_name_codes(Rest, More, Tail).
