% emit.pl -- read a sugar term file, run expand.pl over it, render the result
% as ordinary `.dl6` text with the SHIPPED printer (compile/print_dl.pl,
% unedited). The output file is then an ordinary program the existing two doors
% grade; nothing downstream knows a sugar existed.
%
%   swipl -q -l emit.pl -g "emit('sugar/counter.sugar.pl','out/counter.dl6')" -g halt
%   swipl -q -l emit.pl -g "emit_unsafe('sugar/x.sugar.pl','out/x.dl6')" -g halt
%   swipl -q -l emit.pl -g "show_refusal('sugar/x.sugar.pl')" -g halt
%
% A sugar file holds exactly one term, `sugar(prog(Decls, Rules))`, written
% with ordinary named variables. read_term/3's variable_names is what lets the
% printer keep those names, so the emitted `.dl6` reads like something a person
% wrote rather than `_G123`.

:- use_module(library(lists)).
:- use_module(expand).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(1100, xfy, ~>).

lab_dir(Dir) :- source_file(lab_dir(_), File), file_directory_name(File, Dir).

load_printer :-
    lab_dir(Dir),
    atomic_list_concat([Dir, '/../../compile/print_dl.pl'], PrintPath),
    use_module(PrintPath).

read_sugar(File, Program, Bindings) :-
    setup_call_cleanup(
        open(File, read, Stream),
        read_term(Stream, sugar(Program), [variable_names(Bindings)]),
        close(Stream)).

emit(SugarFile, OutFile) :- emit_with(SugarFile, OutFile, []).
emit_unsafe(SugarFile, OutFile) :- emit_with(SugarFile, OutFile, [unsafe(true)]).

emit_with(SugarFile, OutFile, Options) :-
    load_printer,
    read_sugar(SugarFile, Sugar, Bindings),
    expand_point_free(Sugar, Expanded, Options, Minted),
    append(Bindings, Minted, AllBindings),
    print_dl:print_dl_to_file(Expanded, AllBindings, OutFile).

% Print the refusal a sugar file earns, or `none` when it expands clean. This
% is how the break-rule receipts read the expander instead of grepping it.
show_refusal(SugarFile) :-
    load_printer,
    read_sugar(SugarFile, Sugar, _Bindings),
    (  catch(expand_point_free(Sugar, _Expanded, []), point_free_refusal(Reason), true)
    -> ( var(Reason) -> format("none~n") ; format("~q~n", [Reason]) )
    ;  format("expansion_failed~n") ).

% Rule counts, for the census: how many rules the author writes in the sugar
% file against how many rules the expansion produces. `produced` is what the
% same program costs written out by hand today, which is the census's
% "rules today" column.
counts(SugarFile) :-
    read_sugar(SugarFile, prog(SugarDecls, SugarRules), _Bindings),
    catch(expand_point_free(prog(SugarDecls, SugarRules), prog(ExpandedDecls, ExpandedRules),
                            [unsafe(true)]),
          _, ( ExpandedRules = [], ExpandedDecls = [] )),
    length(SugarRules, Written),
    length(ExpandedRules, Produced),
    minted_rel_count(SugarDecls, ExpandedDecls, MintedRels),
    format("~w written=~w produced=~w minted_rels=~w~n",
           [SugarFile, Written, Produced, MintedRels]).

minted_rel_count(Before, After, Count) :-
    findall(Ref, ( member(keyed(Ref, _), After), \+ memberchk(keyed(Ref, _), Before) ), Refs),
    length(Refs, Count).
