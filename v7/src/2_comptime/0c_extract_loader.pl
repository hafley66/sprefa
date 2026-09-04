:- module(dl7_extract_loader,
          [ load_tsi_stream/3,
            load_tsi_text/3,
            accepted_rows/2,
            install_tsi_graph/6,
            tsi_expression_environment/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(readutil), [read_line_to_string/2]).

% The relation table of `v6/sprefa-extract/src/tsi/registry.rs`. A relation
% absent here is a named stop, never a silent skip.
tsi_relation('tsi.type', 1).
tsi_relation('tsi.name', 2).
tsi_relation('tsi.symbol', 1).
tsi_relation('tsi.denotes', 2).
tsi_relation('tsi.scip_symbol', 2).
tsi_relation('tsi.value', 2).
tsi_relation('tsi.value_argument', 3).
tsi_relation('tsi.has_type', 2).
tsi_relation('tsi.origin', 3).
tsi_relation('tsi.product', 1).
tsi_relation('tsi.sum', 1).
tsi_relation('tsi.callable', 1).
tsi_relation('tsi.primitive', 2).
tsi_relation('tsi.edge', 5).
tsi_relation('tsi.parameter', 4).
tsi_relation('tsi.called', 3).
tsi_relation('tsi.argument', 3).
tsi_relation('tsi.input', 3).
tsi_relation('tsi.output', 3).
tsi_relation('tsi.subtype', 3).
tsi_relation('tsi.assignable', 3).
tsi_relation('tsi.conforms', 3).
tsi_relation('tsi.equivalent', 3).
tsi_relation('ts.interface', 1).
tsi_relation('ts.conditional', 5).
tsi_relation('ts.mapped', 4).
tsi_relation('ts.readonly', 1).
tsi_relation('ts.optional', 1).
tsi_relation('rust.trait', 1).
tsi_relation('rust.impl', 3).
tsi_relation('rust.lifetime', 2).
tsi_relation('rust.ownership', 2).
tsi_relation('rust.assoc', 3).
tsi_relation('go.interface', 1).
tsi_relation('go.type_set', 2).
tsi_relation('go.embedding', 2).

% Relations the graph consumes. Every other registry relation reaches the
% basement as a comptime relation carrying its rows unchanged.
graph_relation('tsi.type').
graph_relation('tsi.product').
graph_relation('tsi.sum').
graph_relation('tsi.edge').
graph_relation('tsi.primitive').
graph_relation('tsi.symbol').
graph_relation('tsi.value').
graph_relation('tsi.value_argument').
graph_relation('tsi.called').
graph_relation('tsi.argument').
graph_relation('tsi.origin').

% The cap bounds a stream whose application ids never settle, as
% `v6/sprefa-extract/src/tsi/ingest.rs:14` bounds its own renumbering.
identity_passes(16).

% A protocol other than 1 voids the stream; a malformed line is reported and
% the remaining lines still load.

%% load_tsi_stream(+JsonlPath, -Rows, -Diagnostics) is det.
load_tsi_stream(JsonlPath, Rows, Diagnostics) :-
    must_be(ground, JsonlPath),
    setup_call_cleanup(
        open(JsonlPath, read, Stream, [encoding(utf8)]),
        stream_rows(Stream, JsonlPath, 1, Rows0, Diagnostics0),
        close(Stream)),
    finish_stream_rows(Rows0, Diagnostics0, JsonlPath, Rows, Diagnostics).

%% load_tsi_text(+JsonlText, -Rows, -Diagnostics) is det.
load_tsi_text(JsonlText, Rows, Diagnostics) :-
    must_be(string, JsonlText),
    setup_call_cleanup(
        open_string(JsonlText, Stream),
        stream_rows(Stream, memory, 1, Rows0, Diagnostics0),
        close(Stream)),
    finish_stream_rows(Rows0, Diagnostics0, memory, Rows, Diagnostics).

finish_stream_rows(Rows0, Diagnostics0, JsonlPath, Rows, Diagnostics) :-
    (   member(extract_protocol(Version), Rows0),
        Version =\= 1
    ->  Rows = [],
        Diagnostics = [diagnostic(extract, stream(JsonlPath),
                                  tsi_protocol(Version))]
    ;   Rows = Rows0,
        Diagnostics = Diagnostics0
    ).

stream_rows(Stream, JsonlPath, LineNumber, Rows, Diagnostics) :-
    read_line_to_string(Stream, Line),
    (   Line == end_of_file
    ->  Rows = [],
        Diagnostics = []
    ;   NextLineNumber is LineNumber + 1,
        stream_rows(Stream, JsonlPath, NextLineNumber,
                    RestRows, RestDiagnostics),
        line_result(Line, JsonlPath, LineNumber, Result),
        combine_line_result(Result, RestRows, RestDiagnostics,
                            Rows, Diagnostics)
    ).

combine_line_result(skip, Rows, Diagnostics, Rows, Diagnostics).
combine_line_result(ok(Row), Rows, Diagnostics, [Row | Rows], Diagnostics).
combine_line_result(error(Diagnostic), Rows, Diagnostics,
                    Rows, [Diagnostic | Diagnostics]).

line_result(Line, JsonlPath, LineNumber, Result) :-
    (   blank_line(Line)
    ->  Result = skip
    ;   catch(atom_json_dict(Line, Dict, [value_string_as(string)]), _, fail)
    ->  decode_result(Dict, JsonlPath, LineNumber, Result)
    ;   Result = error(diagnostic(extract, stream(JsonlPath),
                                  tsi_line(JsonlPath, LineNumber, not_json)))
    ).

blank_line(Line) :-
    normalize_space(string(Trimmed), Line),
    Trimmed == "".

decode_result(Dict, JsonlPath, LineNumber, Result) :-
    (   is_dict(Dict),
        get_dict(record, Dict, RecordText),
        string(RecordText),
        atom_string(Record, RecordText)
    ->  decode_known_record(Record, Dict, JsonlPath, LineNumber, Result)
    ;   Result = error(diagnostic(extract, stream(JsonlPath),
                                  tsi_line(JsonlPath, LineNumber,
                                           no_record_key)))
    ).

%% foreign_record(?Record) is nondet.
%  types.rs:2976 FlatFact less the TSI six; astgrep.rs:97; 1_ast_rule.rs:395.
foreign_record(arg).
foreign_record(ast_rule).
foreign_record(capture).
foreign_record(cfg_scope).
foreign_record(const).
foreign_record(data_doc).
foreign_record(data_value).
foreign_record(df_allocates).
foreign_record(df_field).
foreign_record(df_lit).
foreign_record(df_loop).
foreign_record(df_nest).
foreign_record(doc).
foreign_record(doc_node).
foreign_record(doc_tag).
foreign_record(edge).
foreign_record(file).
foreign_record(file_edge).
foreign_record(file_unresolved).
foreign_record(flow_edge).
foreign_record(macro_site).
foreign_record(method_owner).
foreign_record(node).
foreign_record(package_edge).
foreign_record(param).
foreign_record(projectedge).
foreign_record(reference).
foreign_record(resolved_edge).
foreign_record(resolved_import).
foreign_record(resolved_type_edge).
foreign_record(scip_callee_type).
foreign_record(scip_def).
foreign_record(scip_diagnostic).
foreign_record(scip_document).
foreign_record(scip_documentation).
foreign_record(scip_edge).
foreign_record(scip_fn_edge).
foreign_record(scip_impl).
foreign_record(scip_index).
foreign_record(scip_local).
foreign_record(scip_metadata).
foreign_record(scip_name).
foreign_record(scip_occurrence).
foreign_record(scip_occurrence_doc).
foreign_record(scip_ref).
foreign_record(scip_relationship).
foreign_record(scip_signature).
foreign_record(scip_signature_occurrence).
foreign_record(scip_skip).
foreign_record(scip_symbol).
foreign_record(sig).
foreign_record(site).
foreign_record(size_skip).
foreign_record(specifier).
foreign_record(test_only_call).
foreign_record(unresolved).

% A TSI record whose body does not decode stays malformed; only a record name
% outside decode_record/3 reaches the skip.
decode_known_record(Record, Dict, JsonlPath, LineNumber, Result) :-
    (   decode_record(Record, Dict, Row)
    ->  Result = ok(Row)
    ;   foreign_record(Record)
    ->  Result = skip
    ;   Result = error(diagnostic(extract, stream(JsonlPath),
                                  tsi_line(JsonlPath, LineNumber,
                                           malformed_record(Record))))
    ).

decode_record(protocol, Dict, extract_protocol(Version)) :-
    get_dict(version, Dict, Version),
    integer(Version).
decode_record(run, Dict, extract_run(Run, Mode, Tool, Version, Scope)) :-
    get_dict(run, Dict, Run),
    integer(Run),
    dict_atom(Dict, mode, Mode),
    memberchk(Mode, [syntax, semantic]),
    dict_atom(Dict, tool, Tool),
    dict_atom(Dict, version, Version),
    get_dict(scope, Dict, ScopeTexts),
    is_list(ScopeTexts),
    maplist(scope_digest, ScopeTexts, Scope).
decode_record(fact, Dict, extract_fact(Fact, Relation, Arguments)) :-
    get_dict(fact, Dict, Fact),
    integer(Fact),
    dict_atom(Dict, relation, Relation),
    get_dict(args, Dict, ArgumentDicts),
    is_list(ArgumentDicts),
    maplist(decode_argument, ArgumentDicts, Arguments).
decode_record(witness, Dict, extract_witness(Fact, Run, Method)) :-
    get_dict(fact, Dict, Fact),
    integer(Fact),
    get_dict(run, Dict, Run),
    integer(Run),
    dict_atom(Dict, method, Method).
decode_record(coverage, Dict, extract_coverage(Run, Relation, Coverage)) :-
    get_dict(run, Dict, Run),
    integer(Run),
    dict_atom(Dict, relation, Relation),
    dict_atom(Dict, coverage, Coverage),
    memberchk(Coverage, [partial, complete]).
decode_record(diagnostic, Dict, extract_diagnostic(Run, Relation, Detail)) :-
    get_dict(run, Dict, Run),
    integer(Run),
    dict_atom(Dict, relation, Relation),
    dict_atom(Dict, detail, Detail).

dict_atom(Dict, Key, Value) :-
    get_dict(Key, Dict, Text),
    string(Text),
    atom_string(Value, Text).

scope_digest(Text, Digest) :-
    string(Text),
    atom_string(Digest, Text).

decode_argument(Dict, Argument) :-
    is_dict(Dict),
    decode_argument_shape(Dict, Argument).

decode_argument_shape(Dict, id(Id)) :-
    get_dict(id, Dict, Id),
    integer(Id).
decode_argument_shape(Dict, span(Digest, Start, End)) :-
    get_dict(span, Dict, [DigestText, Start, End]),
    string(DigestText),
    integer(Start),
    integer(End),
    atom_string(Digest, DigestText).
decode_argument_shape(Dict, text(Text)) :-
    get_dict(text, Dict, Text),
    string(Text).
decode_argument_shape(Dict, int(Value)) :-
    get_dict(int, Dict, Value),
    integer(Value).
decode_argument_shape(Dict, atom(Value)) :-
    get_dict(atom, Dict, Text),
    string(Text),
    atom_string(Value, Text).

% Fact ordinals are the key across every stream in one call, which is what
% `extract --ingest` over the concatenation of those streams guarantees.

%% accepted_rows(+Rows, -Accepted) is det.
accepted_rows(Rows, Accepted) :-
    must_be(ground, Rows),
    findall(extract_fact(Fact, Relation, Arguments),
            ( member(extract_fact(Fact, Relation, Arguments), Rows),
              accepted_fact(Rows, Fact, Relation)
            ),
            Accepted0),
    sort(Accepted0, Accepted).

accepted_fact(Rows, Fact, Relation) :-
    member(extract_witness(Fact, Run, _), Rows),
    member(extract_run(Run, semantic, _, _, Scope), Rows),
    newest_complete_run(Rows, Scope, Relation, Run),
    !.
accepted_fact(Rows, Fact, Relation) :-
    member(extract_witness(Fact, Run, _), Rows),
    member(extract_run(Run, syntax, _, _, Scope), Rows),
    \+ semantic_complete(Rows, Scope, Relation),
    !.

%% tsi_expression_environment(+Rows, +Importers, -Environment) is det.
%
% Give source modules callable names for the ordinary TSI relations present in
% one accepted stream. Graph relations continue to enter through graph rows.
tsi_expression_environment(Rows, Importers,
                           expression_environment(Reservations,
                                                  Relations, [])) :-
    accepted_rows(Rows, Accepted),
    stream_owner(Rows, OwnerResult),
    tsi_environment_for_owner(OwnerResult, Accepted, Importers,
                              Reservations, Relations).

tsi_environment_for_owner(owner(Owner), Accepted, Importers,
                          Reservations, Relations) :-
    !,
    relation_names(Accepted, Names, _),
    findall(relation(tsi_relation(Owner, Name), Arity, []),
            ( member(Name, Names),
              tsi_relation(Name, Arity)
            ),
            Relations),
    findall(reservation(Importer, Name,
                        target(tsi_relation(Owner, Name)), product),
            ( member(Importer, Importers),
              member(Name, Names)
            ),
            Reservations).
tsi_environment_for_owner(_, _, _, [], []).

% With no complete claim on the relation every semantic run stands; with one or
% more, the highest run number is the only one left.
newest_complete_run(Rows, Scope, Relation, Run) :-
    complete_semantic_runs(Rows, Scope, Relation, CompleteRuns),
    (   CompleteRuns == []
    ->  true
    ;   last(CompleteRuns, Run)
    ).

semantic_complete(Rows, Scope, Relation) :-
    complete_semantic_runs(Rows, Scope, Relation, [_ | _]).

complete_semantic_runs(Rows, Scope, Relation, Runs) :-
    findall(Run,
            ( member(extract_run(Run, semantic, _, _, Scope), Rows),
              member(extract_coverage(Run, Relation, complete), Rows)
            ),
            Runs0),
    sort(Runs0, Runs).

% Unlike `install_project_graph/6` a diagnostic does not void the install: the
% row it names is skipped and the rest of the stream still becomes a basement.

%% install_tsi_graph(+Rows, +Basements0, +Origins0,
%%                   -Basements, -Origins, -Diagnostics) is det.
install_tsi_graph(Rows, Basements0, Origins0, Basements, Origins,
                  Diagnostics) :-
    must_be(ground, Rows),
    must_be(ground, Basements0),
    accepted_rows(Rows, Accepted),
    stream_owner(Rows, OwnerResult),
    install_for_owner(OwnerResult, Accepted, Basements0, Origins0,
                      Basements, Origins, Diagnostics).

install_for_owner(none, _, Basements, Origins, Basements, Origins, []).
install_for_owner(missing_run, _, Basements, Origins, Basements, Origins,
                  [diagnostic(extract, none, tsi_stream_without_run)]).
install_for_owner(owner(Owner), Accepted, Basements0, Origins0,
                  [module_basement(Owner, Basement) | Basements0],
                  [module_origins(Owner, NodeOrigins) | Origins0],
                  Diagnostics) :-
    identity_map(Owner, Accepted, Basements0, Identities, IdentityDiagnostics),
    graph_nodes(Owner, Accepted, Identities, TypeNodes),
    graph_edges(Accepted, Identities, TypeEdges, EdgeDiagnostics),
    comptime_relations(Owner, Accepted, Identities, RelationNames,
                       Relations, Seeds, RelationDiagnostics),
    relation_edges(Owner, RelationNames, 0, RelationEdges),
    append(RelationEdges, TypeEdges, Edges),
    node_origins(Accepted, Identities, NodeOrigins),
    Basement = basement_program(
                   root_graph([node(Owner), module(Owner), product(Owner)
                              | TypeNodes],
                              Edges),
                   datalog_program(Relations, Seeds, [])),
    append([IdentityDiagnostics, EdgeDiagnostics, RelationDiagnostics],
           Diagnostics0),
    sort(Diagnostics0, Diagnostics).

% The newest run names the owner: newest-run-wins is already the tie-break
% every accepted row was picked under.
stream_owner(Rows, Result) :-
    findall(Run-owner(Tool, Scope),
            member(extract_run(Run, _, Tool, _, Scope), Rows),
            Runs0),
    sort(Runs0, Runs),
    (   Runs == []
    ->  (   Rows == []
        ->  Result = none
        ;   Result = missing_run
        )
    ;   last(Runs, _-owner(Tool, Scope)),
        Result = owner(module(tsi(Tool, Scope)))
    ).

% One `identity(Id, Term)` row per wire id the graph can name.

%% identity_map(+Owner, +Accepted, +Basements,
%%              -Identities, -Diagnostics) is det.
identity_map(Owner, Accepted, Basements, Identities, Diagnostics) :-
    primitive_identities(Accepted, Basements, PrimitiveIdentities,
                         PrimitiveDiagnostics),
    base_identities(Owner, Accepted, BaseIdentities),
    append(PrimitiveIdentities, BaseIdentities, Identities0),
    identity_passes(Passes),
    application_identities(Accepted, Identities0, Passes, Identities),
    unresolved_diagnostics(Accepted, Identities, UnresolvedDiagnostics),
    value_diagnostics(Accepted, ValueDiagnostics),
    application_diagnostics(Accepted, Identities, ApplicationDiagnostics),
    append([PrimitiveDiagnostics, UnresolvedDiagnostics, ValueDiagnostics,
            ApplicationDiagnostics],
           Diagnostics0),
    sort(Diagnostics0, Diagnostics).

primitive_identities(Accepted, Basements, Identities, Diagnostics) :-
    findall(Class-Id,
            member(extract_fact(_, 'tsi.primitive', [id(Id), atom(Class)]),
                   Accepted),
            Claims0),
    sort(Claims0, Claims),
    primitive_claims(Claims, Basements, Identities, Diagnostics).

primitive_claims([], _, [], []).
primitive_claims([Class-Id | Claims], Basements, Identities, Diagnostics) :-
    primitive_claims(Claims, Basements, RestIdentities, RestDiagnostics),
    (   prelude_primitive(Basements, Class, Identity)
    ->  Identities = [identity(Id, Identity) | RestIdentities],
        Diagnostics = RestDiagnostics
    ;   Identities = RestIdentities,
        Diagnostics = [diagnostic(extract, none,
                                  tsi_primitive_class_absent(Class))
                      | RestDiagnostics]
    ).

prelude_primitive(Basements, Class, Identity) :-
    memberchk(module_basement(module(prelude),
                              basement_program(root_graph(_, Edges), _)),
              Basements),
    primitive_label(Class, Label),
    memberchk(pending_edge(module(prelude), Label, target(Identity), _),
              Edges).

primitive_label(unit, '()') :- !.
primitive_label(Class, Class).

% Declaring positions, in the order a shared id space resolves them: a symbol,
% an edge, an argument list, then an ordinary type node.
base_identities(Owner, Accepted, Identities) :-
    findall(identity(Id, Identity),
            ( declared_id(Accepted, Id, Kind),
              base_identity(Kind, Owner, Id, Identity)
            ),
            Identities0),
    sort(Identities0, Identities).

declared_id(Accepted, Id, symbol) :-
    member(extract_fact(_, 'tsi.symbol', [id(Id)]), Accepted).
declared_id(Accepted, Id, symbol) :-
    member(extract_fact(_, 'rust.impl', [id(Id), _, _]), Accepted).
declared_id(Accepted, Id, edge) :-
    member(extract_fact(_, 'tsi.edge', [id(Id), _, _, _, _]), Accepted).
declared_id(Accepted, Id, arguments) :-
    member(extract_fact(_, 'tsi.called', [_, _, id(Id)]), Accepted).
declared_id(Accepted, Id, type) :-
    member(extract_fact(_, 'tsi.type', [id(Id)]), Accepted).

base_identity(symbol, Owner, Id, tsi_symbol(Owner, Id)).
base_identity(edge, Owner, Id, tsi_edge(Owner, Id)).
base_identity(arguments, Owner, Id, tsi_arguments(Owner, Id)).
base_identity(type, Owner, Id, tsi_node(Owner, Id)).

% `intern/3` names a call result `application(Callee, Arguments)`
% (`1_libtime/0_evaluator.pl:499`); that term replaces the result's type node.
application_identities(_, Identities, 0, Identities) :- !.
application_identities(Accepted, Identities0, Passes, Identities) :-
    resolved_applications(Accepted, Identities0, Applications),
    (   Applications == []
    ->  Identities = Identities0
    ;   replace_identities(Applications, Identities0, Identities1),
        NextPasses is Passes - 1,
        application_identities(Accepted, Identities1, NextPasses, Identities)
    ).

resolved_applications(Accepted, Identities, Applications) :-
    findall(identity(Result, application(Callee, Arguments)),
            ( member(extract_fact(_, 'tsi.called',
                                  [id(Result), id(CalleeId), id(ListId)]),
                     Accepted),
              identity_of(Identities, CalleeId, Callee),
              argument_identities(Accepted, Identities, ListId, Arguments),
              \+ memberchk(identity(Result, application(Callee, Arguments)),
                           Identities)
            ),
            Applications0),
    sort(Applications0, Applications).

argument_identities(Accepted, Identities, ListId, Arguments) :-
    \+ member(extract_fact(_, 'tsi.value_argument', [id(ListId), _, _]),
              Accepted),
    findall(Position-ArgumentId,
            member(extract_fact(_, 'tsi.argument',
                                [id(ListId), int(Position), id(ArgumentId)]),
                   Accepted),
            Slots0),
    sort(Slots0, Slots),
    dense_positions(Slots, 0),
    findall(Argument,
            ( member(_-ArgumentId, Slots),
              identity_of(Identities, ArgumentId, Argument)
            ),
            Arguments),
    length(Slots, SlotCount),
    length(Arguments, SlotCount).

dense_positions([], _).
dense_positions([Position-_ | Slots], Position) :-
    NextPosition is Position + 1,
    dense_positions(Slots, NextPosition).

replace_identities([], Identities, Identities).
replace_identities([identity(Id, Term) | Replacements],
                   Identities0, Identities) :-
    exclude(identity_for(Id), Identities0, Identities1),
    replace_identities(Replacements, [identity(Id, Term) | Identities1],
                       Identities).

identity_for(Id, identity(Id, _)).

identity_of(Identities, Id, Identity) :-
    memberchk(identity(Id, Identity), Identities).

% A `tsi.value` id is reported by its own diagnostic rather than twice.
unresolved_diagnostics(Accepted, Identities, Diagnostics) :-
    findall(diagnostic(extract, none, tsi_id_unresolved(Relation, Id)),
            ( member(extract_fact(_, Relation, Arguments), Accepted),
              member(id(Id), Arguments),
              \+ identity_of(Identities, Id, _),
              \+ member(extract_fact(_, 'tsi.value', [id(Id), _]), Accepted)
            ),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

% A call result whose argument list did not close carries no application term,
% so the result is named but its identity is not the one `intern/3` would mint.
application_diagnostics(Accepted, Identities, Diagnostics) :-
    findall(diagnostic(extract, none, tsi_called_unresolved(Result)),
            ( member(extract_fact(_, 'tsi.called', [id(Result), _, _]),
                     Accepted),
              \+ identity_of(Identities, Result, application(_, _))
            ),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

% A v7 value node is `application(Literal, [Primitive, Value])`
% (`0_lowerer.pl:269`) and the wire carries a value's type, never its literal.
value_diagnostics(Accepted, Diagnostics) :-
    findall(diagnostic(extract, none, tsi_value_lacks_literal(Id)),
            member(extract_fact(_, 'tsi.value', [id(Id), _]), Accepted),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

% A primitive class contributes no node: its identity is the prelude product.

%% graph_nodes(+Owner, +Accepted, +Identities, -Nodes) is det.
graph_nodes(Owner, Accepted, Identities, Nodes) :-
    findall(Node,
            ( node_claim(Accepted, Relation, Id),
              identity_of(Identities, Id, Identity),
              loader_identity(Owner, Identity),
              node_row(Relation, Identity, Node)
            ),
            Nodes0),
    sort(Nodes0, Nodes).

node_claim(Accepted, Relation, Id) :-
    member(Relation, ['tsi.type', 'tsi.product', 'tsi.sum']),
    member(extract_fact(_, Relation, [id(Id)]), Accepted).

node_row('tsi.type', Identity, node(Identity)).
node_row('tsi.product', Identity, product(Identity)).
node_row('tsi.sum', Identity, sum(Identity)).

loader_identity(Owner, tsi_node(Owner, _)).
loader_identity(Owner, tsi_symbol(Owner, _)).
loader_identity(Owner, tsi_edge(Owner, _)).
loader_identity(Owner, tsi_arguments(Owner, _)).
loader_identity(_, application(_, _)).

% Wire positions order the edges; the dense run per owner the checker wants
% (`1_checker.pl:229-241`) is assigned from that order.

%% graph_edges(+Accepted, +Identities, -Edges, -Diagnostics) is det.
graph_edges(Accepted, Identities, Edges, Diagnostics) :-
    findall(claim(OwnerIdentity, Position, Label, TargetIdentity),
            ( member(extract_fact(_, 'tsi.edge',
                                  [ id(_), id(OwnerId), text(LabelText),
                                    id(TargetId), int(Position)
                                  ]),
                     Accepted),
              identity_of(Identities, OwnerId, OwnerIdentity),
              identity_of(Identities, TargetId, TargetIdentity),
              atom_string(Label, LabelText)
            ),
            Claims0),
    sort(Claims0, Claims),
    unique_edge_claims(Claims, [], UniqueClaims, LabelDiagnostics),
    indexed_edge_claims(UniqueClaims, none, 0, Edges),
    unplaced_edge_diagnostics(Accepted, Identities, UnplacedDiagnostics),
    append(LabelDiagnostics, UnplacedDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

unique_edge_claims([], _, [], []).
unique_edge_claims([Claim | Claims], Seen, UniqueClaims, Diagnostics) :-
    Claim = claim(OwnerIdentity, _, Label, _),
    (   memberchk(seen(OwnerIdentity, Label), Seen)
    ->  UniqueClaims = RestClaims,
        Diagnostics = [diagnostic(extract, none,
                                  tsi_duplicate_edge_label(OwnerIdentity,
                                                           Label))
                      | RestDiagnostics]
    ;   UniqueClaims = [Claim | RestClaims],
        Diagnostics = RestDiagnostics
    ),
    unique_edge_claims(Claims, [seen(OwnerIdentity, Label) | Seen],
                       RestClaims, RestDiagnostics).

indexed_edge_claims([], _, _, []).
indexed_edge_claims([claim(OwnerIdentity, _, Label, TargetIdentity) | Claims],
                    PreviousOwner, PreviousIndex,
                    [pending_edge(OwnerIdentity, Label,
                                  target(TargetIdentity), Index)
                    | Edges]) :-
    next_owner_index(OwnerIdentity, PreviousOwner, PreviousIndex, Index),
    indexed_edge_claims(Claims, OwnerIdentity, Index, Edges).

next_owner_index(Owner, Owner, PreviousIndex, Index) :-
    !,
    Index is PreviousIndex + 1.
next_owner_index(_, _, _, 0).

unplaced_edge_diagnostics(Accepted, Identities, Diagnostics) :-
    findall(diagnostic(extract, none, tsi_edge_unplaced(EdgeId)),
            ( member(extract_fact(_, 'tsi.edge',
                                  [ id(EdgeId), id(OwnerId), _,
                                    id(TargetId), _
                                  ]),
                     Accepted),
              (   \+ identity_of(Identities, OwnerId, _)
              ;   \+ identity_of(Identities, TargetId, _)
              )
            ),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

% Relation name is the wire name with its dot; arity is the registry's.

%% comptime_relations(+Owner, +Accepted, +Identities,
%%                    -Names, -Relations, -Seeds, -Diagnostics) is det.
comptime_relations(Owner, Accepted, Identities, Names, Relations, Seeds,
                   Diagnostics) :-
    relation_names(Accepted, Names, Diagnostics),
    findall(relation(tsi_relation(Owner, Name), Arity, []),
            ( member(Name, Names),
              tsi_relation(Name, Arity)
            ),
            Relations),
    findall(call(name(Owner, Name), Arguments),
            ( member(extract_fact(_, Name, WireArguments), Accepted),
              memberchk(Name, Names),
              maplist(seed_argument(Identities), WireArguments, Arguments)
            ),
            Seeds0),
    sort(Seeds0, Seeds).

relation_names(Accepted, Names, Diagnostics) :-
    findall(Name,
            ( member(extract_fact(_, Name, _), Accepted),
              tsi_relation(Name, _),
              \+ graph_relation(Name)
            ),
            Names0),
    sort(Names0, Names),
    findall(diagnostic(extract, none, tsi_unknown_relation(Unknown)),
            ( member(extract_fact(_, Unknown, _), Accepted),
              \+ tsi_relation(Unknown, _)
            ),
            Diagnostics0),
    sort(Diagnostics0, Diagnostics).

seed_argument(Identities, id(Id), ref(Identity)) :-
    !,
    identity_of(Identities, Id, Identity).
seed_argument(_, span(Digest, Start, End), const(span(Digest, Start, End))) :-
    !.
seed_argument(_, text(Text), const(Text)) :- !.
seed_argument(_, int(Value), const(Value)) :- !.
seed_argument(_, atom(Value), const(Value)).

relation_edges(_, [], _, []).
relation_edges(Owner, [Name | Names], Index,
               [pending_edge(Owner, Name, target(tsi_relation(Owner, Name)),
                             Index)
               | Edges]) :-
    NextIndex is Index + 1,
    relation_edges(Owner, Names, NextIndex, Edges).

node_origins(Accepted, Identities, Origins) :-
    findall(origin(node(Identity), extract(Language, Digest, Start, End)),
            ( member(extract_fact(_, 'tsi.origin',
                                  [ id(Id), atom(Language),
                                    span(Digest, Start, End)
                                  ]),
                     Accepted),
              identity_of(Identities, Id, Identity)
            ),
            Origins0),
    sort(Origins0, Origins).
