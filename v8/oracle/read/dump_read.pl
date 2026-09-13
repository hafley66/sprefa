% Freeze v7 read_dl7/5 output as JSON, one file per input.
%   V7_DIR=<abs v7> swipl v8/oracle/read/dump_read.pl --

% Paths handed to read_dl7/5 are v7-relative so node ids carry no absolute path.
:- use_module(library(json)).
:- use_module(library(filesex)).
:- prolog_load_context(directory, Here),
   assertz(here_dir(Here)),
   atomic_list_concat([Here, '/../eval/json_terms'], Helpers),
   consult(Helpers).
:- initialization(main, main).

main(_) :-
    here_dir(Here),
    v7_dir(V7),
    atomic_list_concat([V7, '/src/0_reader/0_parser'], ParserPath),
    use_module(ParserPath, [read_dl7/5]),
    atom_concat(Here, '/cases', CaseDir),
    findall(V7-Rel, dl7_under(V7, Rel), V7Jobs),
    findall(CaseDir-Rel, case_under(CaseDir, Rel), CaseJobs),
    append(V7Jobs, CaseJobs, Jobs),
    forall(member(Root-Rel, Jobs), dump_one(Here, Root, Rel)),
    length(Jobs, Count),
    format("wrote ~d oracle files~n", [Count]).

v7_dir(V7) :-
    (   getenv('V7_DIR', V7)
    ->  true
    ;   here_dir(Here),
        atomic_list_concat([Here, '/../../../v7'], V7)
    ).

dl7_under(Root, Rel) :-
    directory_member(Root, Abs,
                     [ recursive(true), extensions([dl7]),
                       file_type(regular), exclude_directories(['.git']) ]),
    relative_name(Root, Abs, Rel).

case_under(CaseDir, Rel) :-
    directory_member(CaseDir, Abs,
                     [ extensions([dl7]), file_type(regular) ]),
    file_base_name(Abs, Base),
    atom_concat('cases/', Base, Rel).

relative_name(Root, Abs, Rel) :-
    atom_concat(Root, '/', Prefix),
    atom_concat(Prefix, Rel, Abs).

dump_one(Here, Root, Rel) :-
    source_path(Root, Rel, Abs),
    read_file_to_string(Abs, Text, [encoding(utf8)]),
    read_dl7(Rel, Text, Forms, SourceRows, Diagnostics),
    maplist(term_json, Forms, FormJson),
    maplist(term_json, SourceRows, RowJson),
    maplist(term_json, Diagnostics, DiagnosticJson),
    slug(Rel, Slug),
    atomic_list_concat([Here, '/', Slug, '.json'], Out),
    Dict = _{ path: Rel, input: Text,
              expected: _{ forms: FormJson, source_rows: RowJson,
                           diagnostics: DiagnosticJson } },
    setup_call_cleanup(
        open(Out, write, Stream, [encoding(utf8)]),
        json_write_dict(Stream, Dict, [width(0)]),
        close(Stream)).

source_path(Root, Rel, Abs) :-
    (   atom_concat('cases/', Base, Rel)
    ->  atomic_list_concat([Root, '/', Base], Abs)
    ;   atomic_list_concat([Root, '/', Rel], Abs)
    ).

slug(Rel, Slug) :-
    atomic_list_concat(Segments, '/', Rel),
    atomic_list_concat(Segments, '_', Joined),
    atomic_list_concat(Stems, '.', Joined),
    atomic_list_concat(Stems, '_', Slug).
