% 3_emit_spine_schema.pl: regenerate the ts row-interface zones from the
% spine catalog facts (3a_spine_schema_facts.pl). Two marker zones, one per
% file: spine.ts keeps its 7 row interfaces, types.ts keeps NodeRow/EdgeRow,
% each wrapped in its own begin/end marker. span_row is out of scope (no spine
% table backs it).
%
% Byte-equality law: the emitted text must equal the hand-written interfaces
% byte-for-byte. A mismatch means the facts are wrong, never that the ts
% should change.
%
% Run:
%   swipl -q -l v6/prolog/compile/3_emit_spine_schema.pl -g emit_spine_schema -g halt

:- module(emit_spine_schema,
          [ emit_spine_schema/0,
            rows_ts_text/2
          ]).

:- use_module('3a_spine_schema_facts.pl', ['table'/2, column/6]).

:- use_module(library(lists)).

:- dynamic(compile_dir/1).
:- prolog_load_context(directory, Here), assertz(compile_dir(Here)).

% ---- zone layout ------------------------------------------------------------

spine_zone_tables([strings, repos, roots, repo_revs, files, revs_files, file_bytes]).
types_zone_tables([node, edge]).

% Interfaces in spine.ts sit flush against each other; in types.ts NodeRow and
% EdgeRow are separated by a blank line. Keep that per-zone join.
zone_gap(spine, '').
zone_gap(types, '\n').

zone_markers(spine, "// BEGIN GENERATED spine row interfaces",
                     "// END GENERATED spine row interfaces").
zone_markers(types, "// BEGIN GENERATED node/edge row interfaces",
                     "// END GENERATED node/edge row interfaces").

zone_tables(spine, Tables) :- spine_zone_tables(Tables).
zone_tables(types, Tables) :- types_zone_tables(Tables).

% ---- rows_ts_text/2 ---------------------------------------------------------

rows_ts_text(Zone, Text) :-
    zone_gap(Zone, Gap),
    zone_tables(Zone, Tables),
    findall(Row, (member(Table, Tables), interface_text(Table, Row)), Rows),
    atomic_list_concat(Rows, Gap, Text).

interface_text(Table, Text) :-
    interface_name(Table, Name),
    findall(Line,
            ( column(Table, Col, BaseType, Nullable, _, _),
              column_line(Col, BaseType, Nullable, Line) ),
            ColumnLines),
    atomic_list_concat(ColumnLines, ColumnText),
    format(string(Text), 'export interface ~w {\n~w}\n', [Name, ColumnText]).

column_line(Col, BaseType, Nullable, Line) :-
    ts_column_type(BaseType, Nullable, Type),
    format(atom(Line), '  ~w: ~w;~n', [Col, Type]).

ts_column_type(BaseType, Nullable, Type) :-
    base_ts_type(BaseType, Base),
    ( Nullable == true
      -> format(atom(Type), '~w | null', [Base])
      ;  Type = Base ).

base_ts_type(integer, number).
base_ts_type(int32,   number).
base_ts_type(text,    string).
base_ts_type(blob,    'Uint8Array').

% snake_case table name -> PascalCase interface name + "Row".
% strings -> StringsRow, repo_revs -> RepoRevsRow.
interface_name(Table, Name) :-
    atom_string(Table, TableStr),
    split_string(TableStr, "_", "", Parts),
    maplist(capitalize_part, Parts, CapParts),
    atomics_to_string(CapParts, "", PascalStr),
    atom_string(PascalStr, Pascal),
    atom_concat(Pascal, 'Row', Name).

capitalize_part(Part, Out) :-
    atom_codes(Part, [First | Rest]),
    ( First >= 0'a, First =< 0'z -> UpFirst is First - 32 ; UpFirst = First ),
    atom_codes(Out, [UpFirst | Rest]).

% ---- emit_spine_schema/0 ----------------------------------------------------

emit_spine_schema :-
    compile_dir(Here),
    directory_file_path(Here, '../../../v6/sprefa-store/js/src/engine/spine.ts', SpinePath),
    directory_file_path(Here, '../../../v6/sprefa-store/js/src/engine/types.ts', TypesPath),
    emit_zone(SpinePath, spine),
    emit_zone(TypesPath, types).

emit_zone(Path, Zone) :-
    read_file_to_string(Path, Existing, []),
    rows_ts_text(Zone, Body),
    zone_markers(Zone, Begin, End),
    replace_generated_section(Existing, Begin, End, Body, Updated),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, '~s', [Updated]), close(Stream)).

% The generated body lives on its own lines between the two marker comment
% lines: head runs through the newline after the begin marker, body is emitted
% verbatim, tail starts at the end marker line.
replace_generated_section(Existing, Begin, End, Body, Updated) :-
    string_length(Begin, BeginLength),
    sub_string(Existing, BeginAt, BeginLength, _, Begin),
    AfterBegin is BeginAt + BeginLength,
    sub_string(Existing, AfterBegin, 1, _, Newline),
    Newline == "\n",
    HeadLength is AfterBegin + 1,
    sub_string(Existing, 0, HeadLength, _, Head),
    string_length(End, EndLength),
    sub_string(Existing, EndAt, EndLength, _, End),
    sub_string(Existing, EndAt, _, 0, Tail),
    string_concat(Head, Body, HeadBody),
    string_concat(HeadBody, Tail, Updated).
