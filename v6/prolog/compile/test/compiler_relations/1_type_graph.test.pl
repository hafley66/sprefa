:- begin_tests(compiler_type_graph).

:- use_module('../../../0_generic_expand',
              [ expand_generic_program_with_bindings/3 ]).
:- use_module('../../parse_dl_dcg', [parse_dl/4]).
:- use_module('../../../use_resolve', [expand_uses/8]).

:- op(1150, xfx, <-).

expand_type_graph_source(Source, Decls) :-
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])),
    once(expand_generic_program_with_bindings(Program, Bindings,
                                              prog(Decls, []))).

test(dotted_node_edge_and_path_sources_query_canonical_rows) :-
    Source = "rel Box(T)(value: T).\n\c
              rel Item(Value: Box(int)).\n\c
              rel seen_edge(Owner: type, Edge: semantic, Role: text, Position: semantic, Label: semantic, Target: semantic).\n\c
              rel seen_node(Node: type, Kind: text, Label: semantic).\n\c
              rel seen_path(Node: type, Path: semantic).\n\c
              seen_edge(Owner, Edge, Role, Position, Label, Target) <- type.edge(Edge, Owner, Role, Position, Label, Target).\n\c
              seen_node(Node, Kind, Label) <- type.node(Node, Kind, Label).\n\c
              seen_path(Node, Path) <- type.path(Node, Path).\n",
    expand_type_graph_source(Source, Decls),
    member(compiler_type_metadata(_, Closure), Decls),
    Item = named(local, relation, 'Item'),
    ItemMember = member(Item, 1, 'Value'),
    Box = named(local, relation, 'Box'),
    BoxInt = application(Box, [primitive(int)]),
    memberchk(seen_edge(Item, ItemMember, member, 1, 'Value', _), Closure),
    memberchk(seen_edge(BoxInt, constructor_edge(BoxInt), constructor, 0, '',
                        Box), Closure),
    memberchk(seen_node(BoxInt, application, ''), Closure),
    memberchk(seen_node(primitive(int), primitive, int), Closure),
    memberchk(seen_path(Item, ['Item']), Closure),
    \+ member(col_type(type__node/3, _, _), Decls),
    \+ member(col_type(type__edge/6, _, _), Decls),
    \+ member(col_type(type__path/2, _, _), Decls).

test(dotted_members_and_projection_default_to_the_logical_plane) :-
    Source = "rel Status(ready(); failed()).\n\c
              rel Address(city: text).\n\c
              rel Item(maybe: option(text), status: Status, home: Address).\n\c
              rel seen_member(Member: type, Owner: type, Plane: semantic, Position: int, Name: text, Target: type).\n\c
              rel seen_project(Owner: type, Name: text, Target: type).\n\c
              seen_member(Member, Owner, Plane, Position, Name, Target) <- type.member(Member, Owner, Plane, Position, Name, Target).\n\c
              seen_project(Owner, Name, Target) <- type.project(Owner, Name, Target).\n",
    expand_type_graph_source(Source, Decls),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    Item = named(local, relation, 'Item'),
    Maybe = member(Item, 1, maybe),
    OptionText = application(named(local, relation, option),
                             [primitive(text)]),
    Address = named(local, relation, 'Address'),
    memberchk(seen_member(Maybe, Item, logical, 1, maybe, OptionText),
              Closure),
    memberchk(seen_member(member(Item, 3, home), Item, logical, 3, home,
                          Address), Closure),
    memberchk(seen_project(Item, maybe, OptionText), Closure),
    \+ member(seen_member(_, _, storage(_), _, _, _), Closure),
    \+ member(col_type(type_member/6, _, _), Decls),
    \+ member(col_type(type__project/3, _, _), Decls).

test(nested_and_enum_edges_have_distinct_roles) :-
    Source = "rel Outer() { rel Child(id: int). }.\n\c
              rel Choice(left(); right()).\n\c
              rel seen(Owner: type, Role: text, Position: semantic, Label: semantic, Target: semantic).\n\c
              seen(Owner, Role, Position, Label, Target) <- type.edge(_, Owner, Role, Position, Label, Target).\n",
    expand_type_graph_source(Source, Decls),
    member(compiler_type_metadata(_, Closure), Decls),
    Outer = named(local, relation, 'Outer'),
    Child = named(local, relation, 'Outer__Child'),
    Choice = named(local, enum, 'Choice'),
    memberchk(seen(Outer, nested, 0, 'Child', Child), Closure),
    findall(Position-Label-Target,
            member(seen(Choice, variant, Position, Label, Target), Closure),
            Variants),
    Variants = [1-left-Left, 2-right-Right],
    Left = named(local, relation, _),
    Right = named(local, relation, _),
    Left \== Right.

test(annotation_sites_have_structural_node_and_edge_ids) :-
    Source = "rel operation(Target: type, Method: text) -> Target.\n\c
              rel Pet(id: int).\n\c
              rel route(first: operation(Pet, Method: 'GET')).\n\c
              rel seen(Member: type, Site: semantic, Annotator: semantic).\n\c
              rel seen_node(Node: type, Kind: text, Label: semantic).\n\c
              seen(Member, Site, Annotator) <- type.edge(_, Member, 'annotation', _, Annotator, Site).\n\c
              seen_node(Node, Kind, Label) <- type.node(Node, Kind, Label).\n",
    expand_type_graph_source(Source, Decls),
    member(compiler_type_metadata(_, Closure, _), Decls),
    Route = named(local, relation, route),
    First = member(Route, 1, first),
    Site = annotation_site(First, [first], 1),
    Operation = named(local, relation, operation),
    memberchk(seen(First, Site, Operation), Closure),
    memberchk(type__node(Site, annotation_site, [first]), Closure),
    memberchk(type__edge(annotation_edge(Site), First, annotation, 1,
                         Operation, Site), Closure).

test(module_qualified_declarations_survive_the_node_view) :-
    predicate_property(plunit_compiler_type_graph:expand_type_graph_source(_, _),
                       file(ThisFile)),
    file_directory_name(ThisFile, TestDir),
    absolute_file_name('../../../../dl/fixtures/0_type-reflection.dl6', Fixture,
                       [relative_to(TestDir), access(read)]),
    once(expand_uses(Fixture, [], [], _, prog(Decls0, Rules0), _, Bindings, [])),
    append(Decls0,
           [ col_type(node_seen/3, id, type),
             col_type(node_seen/3, kind, text),
             col_type(node_seen/3, label, semantic) ],
           Decls1),
    append(Rules0,
           [node_seen(Id, Kind, Label) <- type__node(Id, Kind, Label)],
           Rules1),
    once(expand_generic_program_with_bindings(prog(Decls1, Rules1), Bindings,
                                              prog(Decls, []))),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    findall(Id,
            member(node_seen(Id, declaration, 'Imported'), Closure),
            [Imported]),
    Imported = named(Module, relation, 'Imported'),
    Module \== local.

test(type_graph_rows_are_visible_after_a_construction_refreeze) :-
    Source = "rel Source(value: int).\n\c
              rel seed(Value: type).\n\c
              rel request(Value: type).\n\c
              rel observed(Value: type).\n\c
              seed(Source).\n\c
              request(Application) <- seed(Owner), type.edge(_, Owner, 'member', 1, value, Target), type.apply(list, [Target], Application).\n\c
              observed(Application) <- request(Application), type.node(Application, 'application', '').\n",
    expand_type_graph_source(Source, Decls),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    ListInt = application(named(local, relation, list), [primitive(int)]),
    memberchk(request(ListInt), Closure),
    memberchk(observed(ListInt), Closure),
    memberchk(semantic_type_rows(SemanticRows), Decls),
    memberchk(application(ListInt, named(local, relation, list)), SemanticRows).

test(structural_type_patterns_construct_heads_and_match_bodies) :-
    Source = "rel Item(value: option(text)).\n\c
              rel Choice(left(); right()).\n\c
              rel scalar(Name: text, Type: type).\n\c
              scalar('int', int).\n\c
              rel primitive_head(Type: type).\n\c
              rel primitive_body(Type: type, Name: text).\n\c
              primitive_head(primitive(Name)) <- scalar(Name, _).\n\c
              primitive_body(int, Name) <- primitive_head(primitive(Name)).\n\c
              rel named_seed(Type: type).\n\c
              rel named_body(Type: type, Module: semantic, Kind: text, Name: text).\n\c
              named_seed(Item).\n\c
              named_body(Item, Module, Kind, Name) <- named_seed(named(Module, Kind, Name)).\n\c
              rel application_seed(Type: type).\n\c
              rel application_body(Constructor: type, Argument: type).\n\c
              application_seed(option(text)).\n\c
              application_body(Constructor, Argument) <- application_seed(application(Constructor, [Argument])).\n\c
              rel member_seed(Node: type).\n\c
              rel member_body(Owner: type, Position: int, Name: text).\n\c
              rel rebuilt_member(Node: type).\n\c
              member_seed(Node) <- type.member(Node, _, logical, _, _, _).\n\c
              member_body(Owner, Position, Name) <- member_seed(member(Owner, Position, Name)).\n\c
              rebuilt_member(member(Owner, Position, Name)) <- member_body(Owner, Position, Name).\n\c
              rel variant_seed(Node: type).\n\c
              rel variant_body(Owner: type, Position: int, Name: text).\n\c
              variant_seed(Node) <- type.edge(Node, _, variant, _, _, _).\n\c
              variant_body(Owner, Position, Name) <- variant_seed(variant(Owner, Position, Name)).\n",
    expand_type_graph_source(Source, Decls),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    Item = named(local, relation, 'Item'),
    Choice = named(local, enum, 'Choice'),
    Option = named(local, relation, option),
    memberchk(primitive_head(primitive(int)), Closure),
    memberchk(primitive_body(primitive(int), int), Closure),
    memberchk(named_body(Item, local, relation, 'Item'), Closure),
    memberchk(application_body(Option, primitive(text)), Closure),
    memberchk(member_body(Item, 1, value), Closure),
    memberchk(rebuilt_member(member(Item, 1, value)), Closure),
    memberchk(variant_body(Choice, 1, left), Closure),
    \+ member(col_type(type__named/4, _, _), Decls),
    \+ member(col_type(type__node/3, _, _), Decls),
    \+ member(col_type(type__edge/6, _, _), Decls).

test(grouped_count_construction_refreezes_and_erases) :-
    Source = "rel Box(T)(value: T).\n\c
              rel source_member(Owner: type, Position: int).\n\c
              source_member(int, 1).\n\c
              rel normalized_member(Owner: type, Position: int).\n\c
              normalized_member(Owner, Position) <- source_member(Owner, Position).\n\c
              rel member_count(Owner: type, Total: int).\n\c
              member_count(Owner, count(Position)) <- normalized_member(Owner, Position).\n\c
              rel request(Type: type).\n\c
              request(Box(Owner)) <- member_count(Owner, Total), Total >= 1.\n\c
              rel observed(Type: type).\n\c
              observed(Type) <- request(Type), type.node(Type, 'application', '').\n",
    expand_type_graph_source(Source, Decls),
    memberchk(compiler_type_metadata(_, Closure), Decls),
    BoxInt = application(named(local, relation, 'Box'), [primitive(int)]),
    memberchk(member_count(primitive(int), 1), Closure),
    memberchk(request(BoxInt), Closure),
    memberchk(observed(BoxInt), Closure),
    memberchk(semantic_type_rows(SemanticRows), Decls),
    memberchk(application(BoxInt, named(local, relation, 'Box')), SemanticRows),
    \+ member(col_type(source_member/2, _, _), Decls),
    \+ member(col_type(member_count/2, _, _), Decls),
    \+ member(col_type(request/1, _, _), Decls),
    \+ member(col_type(observed/1, _, _), Decls).

test(non_ground_structural_type_fact_has_named_diagnostic,
     [throws(unsupported_construct(
         compiler_relation_non_ground_type_pattern(primitive(_))))]) :-
    Source = "rel bad(Type: type).\nbad(primitive(Name)).\n",
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])),
    expand_generic_program_with_bindings(Program, Bindings, _).

test(surface_constructor_body_pattern_arity_has_named_diagnostic,
     [throws(unsupported_construct(
         type_pattern_arity_mismatch(named(local, relation, option), 1, 2)))]) :-
    Source = "rel seed(Type: type).\n\c
              rel seen(Type: type).\n\c
              seed(option(int)).\n\c
              seen(Type) <- seed(option(Type, text)).\n",
    string_codes(Source, Codes),
    once(parse_dl(Codes, Program, Bindings, [])),
    expand_generic_program_with_bindings(Program, Bindings, _).

:- end_tests(compiler_type_graph).
