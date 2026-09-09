:- begin_tests(annotation_surface).

:- use_module('../../compile/parse_dl_dcg', [parse_dl/4]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../0_annotation_expand',
              [elaborate_annotation/3]).
:- use_module('../../0_generic_expand',
              [expand_generic_program_with_bindings/3]).

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

expand_text(Text, prog(Decls, Rules)) :-
    parse_text(Text, Program, Bindings),
    expand_generic_program_with_bindings(Program, Bindings,
                                         prog(Decls, Rules)).

test(annotation_phase_path_empty_and_wrapped_forms) :-
    expand_text("rel Revision(a: @(int, []), b: list(@(text, []))).",
                prog(Decls, [])),
    \+ ( member(Decl, Decls), sub_term(annotated_type(_, _), Decl) ),
    member(col_type('Revision'/2, a, int), Decls),
    member(col_type('Revision'/2, b, list(text)), Decls).

test(annotation_phase_path_configured_reaches_compiler_boundary) :-
    expand_text("rel key(Target: type) -> type.\n"
                "key(int, int).\n"
                "rel Revision(id: @(int, [key()])).",
                prog(Decls, [])),
    member(compiler_type_metadata(Evidence, Closure), Decls),
    memberchk(key(primitive(int), primitive(int)), Closure),
    memberchk(type_annotation_evidence('Revision'/1-id, [], 1, int, key, int),
              Evidence),
    member(keyed('Revision'/1, [1]), Decls).

test(annotation_phase_path_nested_anonymous_and_sum_forms) :-
    expand_text("rel Revision(a: @((x: int, y: text), []), "
                "b: list(@((Ok(value: int); Err(message: text)), []))).",
                prog(Decls, [])),
    \+ ( member(Decl, Decls), sub_term(annotated_type(_, _), Decl) ),
    member(type_decl(_, [col(x, int), col(y, text)]), Decls),
    member(enum_decl(_, _), Decls).

:- end_tests(annotation_surface).
