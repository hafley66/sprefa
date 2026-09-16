% Emit the spine row interfaces from 3a_spine_schema_facts.pl into two
% generated marker zones:
%   spine_ts -> sprefa-store/js/src/engine/spine.ts (7 entity rows, packed)
%   types_ts -> sprefa-store/js/src/engine/types.ts (NodeRow/EdgeRow,
%               blank-line separated)
% rows_ts_text/2 must equal each zone's file body byte-for-byte; the
% staleness gate is v6/tsv2/tests/spineSchema.test.ts.
%
% Run:
%   swipl -q -l v6/prolog/compile/3_emit_spine_schema.pl \
%     -g emit_spine_schema -g halt

:- module(emit_spine_schema,
          [ emit_spine_schema/0,
            rows_ts_text/2
          ]).

% (table)/2 parenthesized: `table` is SWI's tabling prefix operator.
:- use_module('3a_spine_schema_facts', [(table)/2, table_symbol/2, column/6]).

:- dynamic(compile_dir/1).
:- prolog_load_context(directory, Here), assertz(compile_dir(Here)).

zone_file(spine_ts, '../../sprefa-store/js/src/engine/spine.ts').
zone_file(types_ts, '../../sprefa-store/js/src/engine/types.ts').

zone_tables(spine_ts, [strings, repos, roots, repo_revs, files, revs_files, file_bytes], '\n').
zone_tables(types_ts, [node, edge], '\n\n').

zone_markers(spine_ts,
    "// BEGIN GENERATED spine row interfaces (v6/prolog/compile/3_emit_spine_schema.pl)",
    "// END GENERATED spine row interfaces").
zone_markers(types_ts,
    "// BEGIN GENERATED node/edge row interfaces (v6/prolog/compile/3_emit_spine_schema.pl)",
    "// END GENERATED node/edge row interfaces").

emit_spine_schema :-
    forall(zone_file(Zone, RelativePath), emit_zone(Zone, RelativePath)).

emit_zone(Zone, RelativePath) :-
    compile_dir(Here),
    directory_file_path(Here, RelativePath, Path),
    read_file_to_string(Path, Existing, []),
    rows_ts_text(Zone, Text),
    zone_markers(Zone, BeginMarker, EndMarker),
    replace_generated_section(Existing, BeginMarker, EndMarker, Text, Updated),
    setup_call_cleanup(
        open(Path, write, Stream),
        format(Stream, '~s', [Updated]),
        close(Stream)).

rows_ts_text(Zone, Text) :-
    zone_tables(Zone, Tables, Separator),
    findall(InterfaceText,
            ( member(Table, Tables),
              table_interface_text(Table, InterfaceText)
            ),
            InterfaceTexts),
    atomic_list_concat(InterfaceTexts, Separator, Joined),
    format(string(Text), '~w~n', [Joined]).

table_interface_text(Table, Text) :-
    interface_name(Table, InterfaceName),
    findall(Line,
            ( column(Table, ColumnName, BaseType, Nullable, _Pk, _ScipSymbol),
              column_line(ColumnName, BaseType, Nullable, Line)
            ),
            Lines),
    atomic_list_concat(Lines, '', Body),
    format(atom(Text), 'export interface ~w {~n~w}', [InterfaceName, Body]).

column_line(ColumnName, BaseType, Nullable, Line) :-
    ts_type(BaseType, TsType),
    (   Nullable == true
    ->  format(atom(Line), '  ~w: ~w | null;~n', [ColumnName, TsType])
    ;   format(atom(Line), '  ~w: ~w;~n', [ColumnName, TsType])
    ).

ts_type(integer, number).
ts_type(int32,   number).
ts_type(text,    string).
ts_type(blob,    'Uint8Array').

interface_name(Table, InterfaceName) :-
    atomic_list_concat(Parts, '_', Table),
    maplist(capitalize_part, Parts, CapitalizedParts),
    atomic_list_concat(CapitalizedParts, '', PascalName),
    atom_concat(PascalName, 'Row', InterfaceName).

capitalize_part(Part, Capitalized) :-
    sub_atom(Part, 0, 1, _, FirstChar),
    sub_atom(Part, 1, _, 0, Rest),
    upcase_atom(FirstChar, UpperFirst),
    atom_concat(UpperFirst, Rest, Capitalized).

replace_generated_section(Existing, BeginMarker, EndMarker, Text, Updated) :-
    string_length(BeginMarker, BeginLength),
    sub_string(Existing, BeginAt, BeginLength, _, BeginMarker),
    PrefixLength is BeginAt + BeginLength,
    sub_string(Existing, 0, PrefixLength, _, Prefix),
    string_length(EndMarker, EndLength),
    sub_string(Existing, EndAt, EndLength, _, EndMarker),
    sub_string(Existing, EndAt, _, 0, Suffix),
    format(string(Updated), '~s~n~s~s', [Prefix, Text, Suffix]).
