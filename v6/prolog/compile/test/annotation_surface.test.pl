:- begin_tests(annotation_surface).

:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../0_generic_expand', [expand_generic_program/2]).
:- use_module('../parse_dl_dcg', [parse_dl/4]).

parse_text(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    parse_dl(Codes, Program, Bindings, []).

test(direct_type_calls_parse_and_print) :-
    parse_text("rel Revision(id: key(int), configured: configure(int, Value: 7, Enabled: true, Ratio: 1.5), composed: second(first(int)), plain: int).", Program, Bindings),
    print_dl_program(Program, Bindings, Text),
    Text == 'rel Revision(id: key(int), configured: configure(int, Value: 7, Enabled: true, Ratio: 1.5), composed: second(first(int)), plain: int).\n'.

test(empty_annotations_are_plain_types) :-
    parse_text("rel Revision(id: int).", Program, Bindings),
    print_dl_program(Program, Bindings, Text),
    Text == 'rel Revision(id: int).\n'.

test(annotation_surface_has_named_refusal,
     [throws(unsupported_construct(annotation_surface_removed))]) :-
    parse_text("rel Revision(id: @(int, [])).", _, _).

test(direct_call_handoff_retains_the_member_site) :-
    parse_text("rel key(Target: type) -> type. rel Revision(id: key(int)).", Program, _),
    expand_generic_program(Program, prog(Decls, _)),
    member(keyed('Revision'/1, [1]), Decls).

:- end_tests(annotation_surface).
