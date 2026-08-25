:- begin_tests(annotation_surface).

:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../1_expansion/0_generic_expand', [expand_generic_program/2]).
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
    parse_text("rel key(Target: type) -> Target. rel Revision(id: key(int)).", Program, _),
    expand_generic_program(Program, prog(Decls, _)),
    member(keyed('Revision'/1, [1]), Decls).

test(return_alias_prints_and_reparses) :-
    parse_text("rel key(Target: type) -> Target.", Program, Bindings),
    print_dl_program(Program, Bindings, Text),
    Text == 'rel key(Target: type) -> Target.\n',
    parse_text(Text, Reparsed, _),
    Program =@= Reparsed.

% go.pl's fixture/5 battery hands the reference interpreter a pre-parsed AST
% (engine.pl:711) and never reaches parse_dl_dcg.pl, so an arrow on a
% parameterized rel can only be pinned from the text door. Fail-first: all
% three throw dl_parse_error at the arrow before parse_dl_dcg.pl:522.
test(unbounded_generic_template_takes_an_arrow) :-
    parse_text("rel edge_of(Node)(node: Node) -> list(Node).", prog(Decls, _), _),
    member(rel_template([edge_of], _, Specs), Decls),
    Specs == [column(node, 'Node'), column(return, list('Node'))].

test(bounded_generic_template_takes_an_arrow) :-
    parse_text("interface json_encodable. rel mapper(In: json_encodable, Out: json_encodable)(input: In) -> Out.", prog(Decls, _), _),
    member(rel_template([mapper], _, Specs), Decls),
    Specs == [column(input, 'In'), column(return, 'Out')].

test(generic_template_arrow_collides_with_an_explicit_return,
     [throws(unsupported_construct(arrow_return_column_collision(edge_of/3)))]) :-
    parse_text("rel edge_of(Node)(node: Node, return: int) -> list(Node).", _, _).

:- end_tests(annotation_surface).
