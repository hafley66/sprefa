% 3_real_normalize.pl : assertions against the shipped parser normalization,
% analyzer, lowerer, and TypeScript emitter.
%
% Run:
%   swipl -q -l v6/prolog/labs/rel_value_unification/3_real_normalize.pl -g go -g halt

:- use_module('../../src/grader.pl').
:- use_module('../../compile/parse_dl.pl', [parse_dl_file/4]).
:- use_module('../../compile/compile.pl',
              [program_plan/2, compile_program/6]).
:- use_module('../../compile/lower.pl', [lower_program/2]).
:- use_module(library(readutil)).

:- dynamic lab_directory/1.
:- prolog_load_context(directory, Directory), assertz(lab_directory(Directory)).

go :- run(check).

lab_file(Name, Path) :-
    lab_directory(Directory),
    directory_file_path(Directory, Name, Path).

parsed(Name, Program, Bindings, Findings) :-
    lab_file(Name, Path),
    parse_dl_file(Path, Program, Bindings, Findings).

candidate(Program, Bindings) :-
    parsed('1_rel_candidate.dl6', Program, Bindings, []).

plan_for(Program, Bindings, Plan) :-
    program_plan(fixture(rel_value_lab, Program, [], [], [])-Bindings, Plan).

emitted_text(Program, Bindings, Text) :-
    tmp_file_stream(text, Path, Stream),
    close(Stream),
    setup_call_cleanup(
        true,
        ( with_output_to(string(_),
              compile_program(rel_value_lab,
                              fixture(rel_value_lab, Program, [], [], []),
                              Bindings, [], Path, emit_ts:emit_program)),
          read_file_to_string(Path, Text, []) ),
        delete_file(Path)).

check(referenced_rel_normalizes_to_existing_type_ir,
      ( candidate(prog(Decls, _), _),
        memberchk(type_decl(span,
                            [col(start, int), col(end, int)]),
                  Decls),
        \+ member(col_type(span/2, _, _), Decls) )).

check(real_analyzer_accepts_normalized_rel,
      ( candidate(Program, Bindings),
        plan_for(Program, Bindings, _Plan) )).

check(real_lowerer_accepts_normalized_rel,
      ( candidate(Program, Bindings),
        plan_for(Program, Bindings, Plan),
        lower_program(Plan, _Lowered) )).

check(real_emitter_accepts_normalized_rel,
      ( candidate(Program, Bindings),
        emitted_text(Program, Bindings, Text),
        string_length(Text, Length),
        Length > 0 )).
