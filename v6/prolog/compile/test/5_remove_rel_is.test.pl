:- begin_tests(remove_rel_is).

:- use_module('../../7_lower/parse_dl_dcg', [parse_dl/4]).
:- use_module('../../print_dl', [print_dl_program/3]).
:- use_module('../../1_expansion/0_generic_expand',
              [ expand_generic_program/2,
                generic_type_ir/2
              ]).
:- use_module('../../compile', [program_plan/3]).
:- use_module('../../7_lower/lower', [catalog_decl_rows/6]).
:- use_module('../../0_dot_expand/registry', [surface/5]).

parse_remove_rel_is(Text, Program, Bindings) :-
    string_codes(Text, Codes),
    once(parse_dl(Codes, Program, Bindings, [])).

bounded_source("interface json_encodable. rel box(T: json_encodable)(value: T). rel holder(value: box(int)).").

test(the_removed_relation_suffix_has_a_pinned_parse_error) :-
    catch(parse_remove_rel_is(
              "rel file(path: text) is addressable.", _, _),
          Error,
          true),
    Error == dl_parse_error(statement, position(1, 22)).

test(ordinary_relations_emit_no_implementation_term) :-
    parse_remove_rel_is(
        "interface addressable. rel file(path: text).",
        prog(Decls, []),
        []),
    Decls == [ interface_decl(addressable, []),
               col_type(file/1, path, text) ],
    \+ sub_term(rel_is_implementation(_, _), Decls).

test(interface_bounds_print_and_reparse) :-
    bounded_source(Source),
    parse_remove_rel_is(Source, Program, Bindings),
    once(print_dl_program(Program, Bindings, Printed)),
    Printed == 'interface json_encodable.\nrel box(T: json_encodable)(value: T).\nrel holder(value: box(int)).\n',
    parse_remove_rel_is(Printed, Reparsed, _),
    Program =@= Reparsed.

test(type_rows_keep_constraints_without_implementation_rows) :-
    Decls = [ interface_decl(json_encodable, []),
              rel_template([box],
                           [type_parameter('T', [json_encodable])],
                           [column(value, 'T')]) ],
    generic_type_ir(Decls, Rows),
    memberchk(declaration(InterfaceId, root, json_encodable, interface,
                          compile_time), Rows),
    memberchk(parameter(ParameterId, _, 1, 'T'), Rows),
    memberchk(constraint(_, ParameterId, InterfaceId), Rows),
    \+ member(implementation(_, _, _), Rows).

test(catalog_keeps_interface_and_constraint_rows_without_implementation_rows) :-
    bounded_source(Source),
    parse_remove_rel_is(Source, Program, Bindings),
    once(program_plan(fixture(remove_rel_is, Program, [], [], [])-Bindings,
                      [intern(direct)],
                      plan(_, prog(Decls, Rules), _, RelPlans,
                           _, _, _, _, _))),
    once(catalog_decl_rows(remove_rel_is, Rules, RelPlans, Decls, Rows, _)),
    memberchk(row(_, _, _, json_encodable, interface, _, _, _, _, _, _), Rows),
    memberchk(row(_, _, _, json_encodable, constraint, _, _, _, _, _, _), Rows),
    \+ member(row(_, _, _, _, implementation, _, _, _, _, _, _), Rows).

test(applicative_key_annotation_still_elaborates) :-
    parse_remove_rel_is(
        "rel key(Target: type) -> Target. rel Revision(id: key(int)).",
        Program,
        _),
    once(expand_generic_program(Program, prog(Decls, []))),
    memberchk(col_type('Revision'/1, id, int), Decls),
    memberchk(keyed('Revision'/1, [1]), Decls).

test(expression_is_two_remains_a_live_bind_operator) :-
    once(surface(is/2, bind, no_refs, infix(lower), live)).

:- end_tests(remove_rel_is).
