:- begin_tests(type_relation_ir).

:- use_module('../../0_generic_expand',
              [ schema_member_rows/2, type_relation_rows/2 ]).
:- use_module('../../lower', [ catalog_type_relation_rows/3,
                               catalog_type_transport_rows/4 ]).
:- use_module('../../compile/typegen_export', []).
:- use_module('../../compile/8_emit_rust_types', [ rust_types_text/3 ]).

ordinary_schema_decls([
    type_decl(person, [col(id, int), col(name, text)]),
    col_type(person/2, id, int),
    col_type(person/2, name, text),
    keyed(person/2, [1])
]).

test(ordinary_members_keep_authored_and_value_type_ids) :-
    ordinary_schema_decls(Decls),
    schema_member_rows(Decls, Rows),
    once(member(schema_member(Id, Owner, 1, id, int, primitive(int), [key]), Rows)),
    once(member(schema_member(NameId, Owner, 2, name, text, primitive(text), []), Rows)),
    Id \== NameId,
    Owner = named(local, relation, person).

test(ordinary_relation_does_not_require_self) :-
    ordinary_schema_decls(Decls),
    type_relation_rows(Decls, Rows),
    member(type_relation(named(local, relation, person), none, Inputs,
                        none, Keys), Rows),
    Inputs = [member(named(local, relation, person), 1, id),
              member(named(local, relation, person), 2, name)],
    Keys = [member(named(local, relation, person), 1, id)].

test(ordinary_interface_implementation_does_not_require_self) :-
    Decls = [type_decl(document, [col(id, int)]),
             rel_is_implementation(document/1, [codec])],
    catch(type_relation_rows(Decls, Rows), Error, true),
    var(Error),
    once(member(type_relation(named(local, relation, document), none,
                              [_], none, []), Rows)), !.

test(anonymous_owner_role_retains_path) :-
    Decls = [type_decl(outer,
                       [col(value, anonymous_product(path, text))])],
    schema_member_rows(Decls, Rows),
    once(member(schema_member(_, _, 1, value, anonymous_product(path, text),
                         _, [anonymous_owner(path)]), Rows)), !.

self_schema_decls([
    type_decl('Convert', [col('Self', type), col('Input', text),
                          col(return, text)]),
    keyed('Convert'/3, [1, 2])
]).

test(trait_relation_projects_self_inputs_return_and_keys) :-
    self_schema_decls(Decls),
    type_relation_rows(Decls, Rows),
    once(member(schema_member(SelfId, Owner, 1, 'Self', type,
                         primitive(type), [self_subject, key]), Rows)),
    once(member(schema_member(_, Owner, 2, 'Input', text, primitive(text),
                         [key]), Rows)),
    once(member(schema_member(ReturnId, Owner, 3, return, text,
                         primitive(text), [return]), Rows)),
    once(member(type_relation(Owner, SelfId, [InputId], ReturnId,
                              [SelfId, InputId]), Rows)),
    InputId = member(Owner, 2, 'Input').

test(trait_self_missing_is_named,
     [throws(unsupported_construct(type_relation_self_missing(_)))]) :-
    Decls = [type_decl('Convert', [col('Input', type)]),
             rel_is_implementation('Convert'/1, [codec])],
    type_relation_rows(Decls, _).

test(trait_self_duplicate_is_named,
     [throws(unsupported_construct(type_relation_self_duplicate(_)))]) :-
    Decls = [type_decl('Convert', [col('Self', type), col('Self', type)])],
    type_relation_rows(Decls, _).

test(trait_self_not_first_is_named,
     [throws(unsupported_construct(type_relation_self_not_first(_)))]) :-
    Decls = [type_decl('Convert', [col('Input', text), col('Self', type)])],
    type_relation_rows(Decls, _).

test(trait_self_not_type_is_named,
     [throws(unsupported_construct(type_relation_self_not_type(_, _)))]) :-
    Decls = [type_decl('Convert', [col('Self', text), col('Input', text)])],
    type_relation_rows(Decls, _).

test(catalog_metadata_is_the_same_parallel_stream) :-
    ordinary_schema_decls(Decls),
    catalog_type_relation_rows(type_relation_fixture, Decls, CatalogRows),
    type_relation_rows(Decls, DirectRows),
    CatalogRows =@= DirectRows.

test(catalog_transport_uses_typed_child_rows) :-
    ordinary_schema_decls(Decls),
    CatalogRows = [ row(10, 0, 0, person, rel, 0, 0, 0, '', '', ''),
                    row(11, 10, 0, id, column, 1, 0, 0, '', '', ''),
                    row(12, 10, 1, name, column, 2, 0, 0, '', '', '') ],
    catalog_type_transport_rows(type_relation_fixture, CatalogRows, Decls,
                                TransportRows),
    once(member(schema_member_column(11, MemberId), TransportRows)),
    once(member(schema_member_role(MemberId, 1, key, ''), TransportRows)),
    once(member(type_relation_key(_, 1, MemberId), TransportRows)), !.

test(rust_omits_self_subject_from_struct_fields) :-
    Rows = [ row(1, 0, 0, text, primitive, 0, 0, 0, '', '', ''),
             row(2, 0, 0, doc, module, 0, 0, 0, 'hash', '', ''),
             row(3, 2, 0, convert, rel, 0, 0, 1, '', '', ''),
             row(4, 3, 0, 'Self', column, 1, 0, 1, '', '', ''),
             row(5, 3, 1, input, column, 1, 0, 1, '', '', ''),
             row(6, 2, 1, other, rel, 0, 0, 1, '', '', ''),
             row(7, 6, 0, 'Self', column, 1, 0, 1, '', '', ''),
             schema_member(member(convert, 1, 1, 'Self'),
                           named(local, relation, convert), 1, 'Self', type,
                           primitive(type), [self_subject]),
             schema_member(member(convert, 1, 2, input),
                           named(local, relation, convert), 2, input, text,
                           primitive(text), []),
             schema_member_column(4, member(convert, 1, 1, 'Self')),
             schema_member_column(5, member(convert, 1, 2, input)),
             schema_member_column(7, member(other, 1, 1, 'Self')),
             schema_member_role(member(convert, 1, 1, 'Self'), 1,
                                self_subject, '') ],
    once(rust_types_text(doc, Rows, Text)),
    sub_string(Text, _, _, _, 'pub struct Convert {'),
    sub_string(Text, _, _, _, 'pub input: String,'),
    sub_string(Text, _, _, _, 'pub struct Other {'),
    sub_string(Text, _, _, _, 'pub Self: String,'),
    \+ sub_string(Text, _, _, _, "pub struct Convert {\n    pub Self:"), !.

test(typegen_metadata_round_trips_as_terms) :-
    Rows = [ schema_member(
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1, 'Self', 'type',
                 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'),
             schema_member_role(
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 1, 'self_subject', ''),
             schema_member_column(
                 4,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),
             type_relation(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 ''),
             type_relation_input(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1,
                 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd'),
             type_relation_key(
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                 1,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa') ],
    Path = '/private/tmp/dl6-type-relation-roundtrip.jsonl',
    setup_call_cleanup(
        open(Path, write, Stream),
        forall(member(Row, Rows), typegen_export:write_row_line(Stream, Row)),
        close(Stream)),
    typegen_export:read_row_lines(Path, RoundTripped),
    RoundTripped =@= Rows, !.

:- end_tests(type_relation_ir).
