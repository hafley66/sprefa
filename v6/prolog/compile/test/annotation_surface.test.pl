:- begin_tests(annotation_surface).

:- use_module('../../compile/parse_dl_dcg', [parse_dl/4]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../0_annotation_expand',
              [elaborate_annotation/3]).
:- use_module('../../0_anonymous_expand', [expand_anonymous_decls/2]).
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

test(annotation_carriers_print_and_reparse_byte_identically) :-
    parse_text(
      "rel Shapes(empty: @(int, []), one: @(text, [mark()]), configured: @(int, [min(Value: 1)]), composed: @(int, [first(), second()]), product: @((x: int), [mark()]), sum: @((ok(value: int); err()), [mark()]), listed: @(list(int), [mark()]), optional: @(option(int), [mark()])).",
      Program, Bindings),
    print_dl_program(Program, Bindings, Text1),
    atom_codes(Text1, Codes),
    parse_dl(Codes, RoundTripped, RoundTrippedBindings, []),
    print_dl_program(RoundTripped, RoundTrippedBindings, Text2),
    Program =@= RoundTripped,
    Text2 == Text1.

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

test(annotation_phase_handoff_preserves_carriers_and_source_order) :-
    parse_text(
      "rel Shapes(empty: @(int, []), one: @(text, [mark()]), configured: @(int, [min(Value: 1)]), composed: @(int, [first(), second()]), product: @((x: int), [mark()]), sum: @((ok(value: int); err()), [mark()]), listed: @(list(int), [mark()]), optional: @(option(int), [mark()])).",
      Program, Bindings),
    handoff_annotations_before_runtime(Program, Bindings, Decls),
    member(compiler_annotation_requests(Requests), Decls),
    length(Requests, 8),
    annotation_request_for(Requests, [], EmptyInput, []),
    EmptyInput == primitive(int),
    annotation_request_for(Requests, [mark], OneInput,
                           [annotation_step(1, OneInput,
                                            mark(named('Target', OneInput)),
                                            annotation_result(1))]),
    OneInput == primitive(text),
    annotation_request_for(Requests, [min(named('Value', 1))], _,
                           [annotation_step(1, _,
                             min(named('Target', _), named('Value', 1)),
                             annotation_result(1))]),
    annotation_request_for(Requests, [first, second], ComposedInput,
                           [annotation_step(1, ComposedInput,
                                            first(named('Target', ComposedInput)),
                                            annotation_result(1)),
                            annotation_step(2, annotation_result(1),
                                            second(named('Target', annotation_result(1))),
                                            annotation_result(2))]),
    annotation_request_for(Requests, [mark], _, _, ProductRequest),
    ProductRequest = annotation_request(_, _, [product],
                                        annotated_type(ProductName, [mark]), _),
    sub_atom(ProductName, 0, _, _, '__anon_'),
    annotation_request_for(Requests, [mark], _, _, SumRequest),
    SumRequest = annotation_request(_, _, [sum],
                                    annotated_type(SumName, [mark]), _),
    sub_atom(SumName, 0, _, _, '__anon_'),
    member(col_type('Shapes'/8, listed, annotated_type(list(int), [mark])), Decls),
    member(col_type('Shapes'/8, optional, annotated_type(option(int), [mark])), Decls),
    !.

test(annotation_phase_handoff_follows_concrete_generic_substitution) :-
    parse_text("rel mark(Target: type) -> type. mark(int, int). rel Box(T)(value: @(T, [mark()])). rel use(box: Box(int)).",
               Program, Bindings),
    expand_generic_program_with_bindings(Program, Bindings, prog(Decls, [])),
    member(type_decl(Concrete, [col(value, int)]), Decls),
    sub_atom(Concrete, 0, _, _, '__gen__Box'),
    member(compiler_type_metadata(_, _, [Evidence]), Decls),
    Evidence = annotation_evidence(MemberId, [value], 1, primitive(int),
                                   mark(named('Target', primitive(int))),
                                   primitive(int)),
    MemberId = member(named(local, relation, Concrete), 1, value),
    !.

annotation_request_for(Requests, Applications, Input, Steps) :-
    annotation_request_for(Requests, Applications, Input, Steps, _).

annotation_request_for(Requests, Applications, Input, Steps, Request) :-
    member(Request, Requests),
    Request = annotation_request(_, _, _, annotated_type(_, Applications),
                                 annotation_steps(Input, Steps)).

test(annotation_phase_handoff_walks_nested_carriers_from_canonical_members) :-
    parse_text(
      "rel Nested(listed: list(@(int, [mark()])), optional: option(@(text, [mark()])), generic: Box(@(int, [mark()])), product: (field: @(int, [mark()])), sum: (ok(value: @(int, [mark()])); err())).",
      Program, Bindings),
    handoff_annotations_before_runtime(Program, Bindings, Decls),
    member(compiler_annotation_requests(Requests), Decls),
    length(Requests, 5),
    request_at(Requests, [listed, 1], int, [mark]),
    request_at(Requests, [optional, 1], text, [mark]),
    request_at(Requests, [generic, 1], int, [mark]),
    request_at(Requests, [product, field], int, [mark]),
    request_at(Requests, [sum, ok, value], int, [mark]),
    forall(member(annotation_request(Owner, Member, _, _, _), Requests),
           ( Owner == named(local, relation, 'Nested'),
             Member = member(Owner, _, _) )),
    !.

test(annotation_phase_handoff_distinguishes_nested_sites_and_deduplicates) :-
    parse_text(
      "rel Deep(value: list(list(@(int, [first(), second()]))), pair: (left: @(text, [mark()]), right: @(int, [tag()]))).",
      Program, Bindings),
    handoff_annotations_before_runtime(Program, Bindings, Decls),
    member(compiler_annotation_requests(Requests), Decls),
    Requests = [ annotation_request(named(local, relation, 'Deep'),
                                    member(named(local, relation, 'Deep'), 1, value),
                                    [value, 1, 1],
                                    annotated_type(int, [first, second]),
                                    annotation_steps(primitive(int),
                                      [ annotation_step(1, primitive(int),
                                          first(named('Target', primitive(int))),
                                          annotation_result(1)),
                                        annotation_step(2, annotation_result(1),
                                          second(named('Target', annotation_result(1))),
                                          annotation_result(2)) ])),
                 annotation_request(named(local, relation, 'Deep'),
                                    member(named(local, relation, 'Deep'), 2, pair),
                                    [pair, left],
                                    annotated_type(text, [mark]), _),
                 annotation_request(named(local, relation, 'Deep'),
                                    member(named(local, relation, 'Deep'), 2, pair),
                                    [pair, right],
                                    annotated_type(int, [tag]), _) ],
    !.

request_at(Requests, Site, Type, Applications) :-
    member(annotation_request(_, _, Site, annotated_type(Type, Applications), _),
           Requests).

handoff_annotations_before_runtime(prog(Decls0, []), _, Decls) :-
    expand_anonymous_decls(Decls0, AnonymousDecls),
    generic_expand:handoff_annotation_requests(AnonymousDecls, Decls).

:- end_tests(annotation_surface).
