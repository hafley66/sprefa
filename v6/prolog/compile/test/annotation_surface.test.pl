:- begin_tests(annotation_surface).

:- use_module('../../compile/parse_dl_dcg', [parse_dl/4]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../0_annotation_expand',
              [elaborate_annotation/3]).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

test(empty_annotation_round_trips) :-
    parse_text("rel Revision(id: @(int, [])).", Program, Bindings),
    Program = prog([col_type('Revision'/1, id,
                            annotated_type(int, []))], []),
    print_dl_program(Program, Bindings, Text),
    atom_codes(Text, Codes),
    parse_dl(Codes, RoundTripped, _, []),
    Program =@= RoundTripped,
    Text == 'rel Revision(id: @(int, [])).\n'.

test(configured_annotation_round_trips) :-
    parse_text("rel Revision(id: @(int, [key(), min(Value: 1)])).",
               Program, Bindings),
    Program = prog([col_type('Revision'/1, id,
                            annotated_type(int,
                              [ key,
                                min(named('Value', 1)) ]))], []),
    print_dl_program(Program, Bindings, Text),
    atom_codes(Text, Codes),
    parse_dl(Codes, RoundTripped, _, []),
    Program =@= RoundTripped.

test(annotation_elaboration_adds_ordered_implicit_targets) :-
    elaborate_annotation(int,
      [key, min(named('Value', 1))],
      annotation_steps(int,
        [ annotation_step(1, int,
            key(named('Target', int)),
            annotation_result(1)),
          annotation_step(2, annotation_result(1),
            min(named('Target', annotation_result(1)), named('Value', 1)),
            annotation_result(2)) ] )).

:- end_tests(annotation_surface).
