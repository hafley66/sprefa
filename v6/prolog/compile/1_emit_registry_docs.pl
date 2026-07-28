% 1_emit_registry_docs.pl: regenerate SYNTAX.md's surface/5 table.
%
% Run:
%   swipl -q -l v6/prolog/compile/1_emit_registry_docs.pl \
%     -g emit_registry_docs -g halt

:- module(emit_registry_docs, [emit_registry_docs/0]).

:- use_module(registry, [surface/5]).

:- dynamic(compile_dir_fact/1).
:- prolog_load_context(directory, Here), assertz(compile_dir_fact(Here)).

begin_marker("<!-- BEGIN GENERATED surface/5 TABLE -->").
end_marker("<!-- END GENERATED surface/5 TABLE -->").

emit_registry_docs :-
    compile_dir_fact(CompileDir),
    atomic_list_concat([CompileDir, '/SYNTAX.md'], SyntaxPath),
    read_file_to_string(SyntaxPath, Existing, []),
    generated_table(Table),
    replace_generated_section(Existing, Table, Updated),
    setup_call_cleanup(
        open(SyntaxPath, write, Stream),
        format(Stream, '~s', [Updated]),
        close(Stream)).

generated_table(Table) :-
    findall(Line,
            ( surface(Signature, Axis, AnalyzeRole, LowerRole, Status),
              registry_row(Line, Signature, Axis, AnalyzeRole, LowerRole, Status)
            ),
            RowLines),
    atomic_list_concat(
        [ '| signature | axis | analyze role | lower role | status |\n',
          '|---|---|---|---|---|\n'
        | RowLines
        ],
        Table).

registry_row(Line, Functor/Arity, Axis, AnalyzeRole, LowerRole, Status) :-
    format(atom(SignatureText), '~w/~w', [Functor, Arity]),
    format(atom(AnalyzeText), '~q', [AnalyzeRole]),
    format(atom(LowerText), '~q', [LowerRole]),
    format(atom(Line), '| `~w` | `~w` | `~w` | `~w` | `~w` |~n',
           [SignatureText, Axis, AnalyzeText, LowerText, Status]).

replace_generated_section(Existing, Table, Updated) :-
    begin_marker(BeginMarker),
    end_marker(EndMarker),
    string_length(BeginMarker, BeginLength),
    sub_string(Existing, BeginAt, BeginLength, _, BeginMarker),
    PrefixLength is BeginAt + BeginLength,
    sub_string(Existing, 0, PrefixLength, _, Prefix),
    string_length(EndMarker, EndLength),
    sub_string(Existing, EndAt, EndLength, _, EndMarker),
    sub_string(Existing, EndAt, _, 0, Suffix),
    format(string(Updated), '~s~n~s~s', [Prefix, Table, Suffix]).
