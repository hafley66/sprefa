% emit_openapi.pl : facts -> OpenAPI 3.1 document.
%
% Run:
%   swipl -q -l v6/prolog/labs/openapi_codegen/emit_openapi.pl -g emit_openapi -g halt
%   swipl -q -l v6/prolog/labs/openapi_codegen/emit_openapi.pl \
%         -g "openapi_json_text(T),format('~s',[T])" -g halt
%
% Same shape as compile/2_emit_cli_inventory.pl, which is the mechanism this
% arc generalizes: a `*_text/1` predicate that builds the artifact in memory
% (so a test can diff it against the checked-in file without running the
% writer) plus a `emit_*/0` that writes it.
%
% ── what this file owns, and what it deliberately does not ───────────────────
%
% Owns: the OpenAPI *dialect* -- `{rel}` path templating, the
% `#/components/schemas/` ref prefix, the 3.1 `type: [..]` union spelling,
% media-type nesting. Every one of those is target-specific, so it lives here
% and not in 0_facts.pl, exactly as lower.pl owns SQL text while the plan term
% stays target-neutral.
%
% Does not own: any route, parameter, or schema. Adding a route means adding a
% fact, never editing this file. The one exception is `path_template/2` below,
% which knows the server's `:param` spelling.

:- module(emit_openapi,
          [ emit_openapi/0,
            openapi_json_text/1,
            openapi_document/1,
            spec_operations/1
          ]).

:- use_module(library(http/json)).
:- use_module('0_facts').

:- dynamic(lab_dir/1).
:- prolog_load_context(directory, Here), assertz(lab_dir(Here)).

% ── entry points ─────────────────────────────────────────────────────────────

emit_openapi :-
    openapi_json_text(Text),
    ( getenv('OPENAPI_LAB_OUT', OutPath) -> true
    ; lab_dir(Here), directory_file_path(Here, 'openapi.json', OutPath)
    ),
    setup_call_cleanup(open(OutPath, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

openapi_json_text(Text) :-
    openapi_document(Doc),
    with_output_to(string(Body), json_write_dict(current_output, Doc, [width(78), step(2), tab(200)])),
    format(string(Text), '~w\n', [Body]).

% ── the document ─────────────────────────────────────────────────────────────

openapi_document(Doc) :-
    api_info(Title, Version, Description),
    findall(Url-Desc, api_server(Url, Desc), ServerPairs),
    maplist(server_object, ServerPairs, Servers),
    paths_object(Paths),
    schemas_object(Schemas),
    Doc = _{ openapi: '3.1.0',
             info: _{ title: Title, version: Version, description: Description },
             servers: Servers,
             paths: Paths,
             components: _{ schemas: Schemas }
           }.

server_object(Url-Desc, _{url: Url, description: Desc}).

% ── paths ────────────────────────────────────────────────────────────────────
%
% One dict key per distinct path template, each carrying one key per method.
% Two operations on the same path merge rather than one shadowing the other.

paths_object(Paths) :-
    findall(Template-Method-OpId,
            ( http_operation(OpId, Method, Path, _),
              \+ dropped_operation(OpId),
              path_template(Path, Template)
            ),
            Triples),
    findall(Template, member(Template-_-_, Triples), TemplatesDup),
    sort(TemplatesDup, Templates),
    maplist(path_item(Triples), Templates, Pairs),
    dict_pairs(Paths, _, Pairs).

path_item(Triples, Template, Template-Item) :-
    findall(MethodKey-Operation,
            ( member(Template-Method-OpId, Triples),
              downcase_atom(Method, MethodKey),
              operation_object(OpId, Operation)
            ),
            Pairs),
    dict_pairs(Item, _, Pairs).

/** The server spells a path parameter `:rel` (node's own URL parsing plus a
 *  segment compare, no router library); OpenAPI spells it `{rel}`. This is
 *  the whole of the path dialect. */
path_template(Path, Template) :-
    atomic_list_concat(Segments, '/', Path),
    maplist(template_segment, Segments, Converted),
    atomic_list_concat(Converted, '/', Template).

template_segment(Segment, Converted) :-
    ( atom_concat(':', Name, Segment)
    -> format(atom(Converted), '{~w}', [Name])
    ;  Converted = Segment
    ).

% ── one operation ────────────────────────────────────────────────────────────

operation_object(OpId, Operation) :-
    http_operation(OpId, _, _, Summary),
    findall(P, operation_parameter(OpId, P), Parameters),
    responses_object(OpId, Responses),
    Base = _{ operationId: OpId, summary: Summary, responses: Responses },
    ( Parameters == [] -> WithParams = Base ; WithParams = Base.put(parameters, Parameters) ),
    ( request_body_object(OpId, Body)
    -> Operation = WithParams.put(requestBody, Body)
    ;  Operation = WithParams
    ).

operation_parameter(OpId, Param) :-
    http_path_param(OpId, Name, Type, Description),
    schema_object(Type, Schema),
    Param = _{ name: Name, in: path, required: true, description: Description, schema: Schema }.
operation_parameter(OpId, Param) :-
    http_query_param(OpId, Name, Type, Required, Description),
    schema_object(Type, Schema),
    Param = _{ name: Name, in: query, required: Required, description: Description, schema: Schema }.

request_body_object(OpId, Body) :-
    http_request_body(OpId, MediaType, Type, Required, Description),
    schema_object(Type, Schema),
    dict_pairs(Content, _, [MediaType-_{schema: Schema}]),
    Body = _{ required: Required, description: Description, content: Content }.

responses_object(OpId, Responses) :-
    findall(StatusKey-Response,
            ( http_response(OpId, Status, MediaType, Type, Description),
              atom_number(StatusKey, Status),
              response_object(MediaType, Type, Description, Response)
            ),
            Pairs),
    ( Pairs == [] -> throw(openapi_operation_without_response(OpId)) ; true ),
    dict_pairs(Responses, _, Pairs).

response_object(MediaType, Type, Description, Response) :-
    schema_object(Type, Schema),
    dict_pairs(Content, _, [MediaType-_{schema: Schema}]),
    Response = _{ description: Description, content: Content }.

% ── schemas ──────────────────────────────────────────────────────────────────

schemas_object(Schemas) :-
    findall(Name-Object,
            ( http_schema(Name, Kind, Description),
              named_schema(Name, Kind, Description, Object)
            ),
            Pairs),
    dict_pairs(Schemas, _, Pairs).

named_schema(Name, object, Description, Object) :-
    findall(Field-FieldSchema,
            ( http_schema_field(Name, Field, Type, _, FieldDescription),
              schema_object(Type, Bare),
              described(Bare, FieldDescription, FieldSchema)
            ),
            FieldPairs),
    dict_pairs(Properties, _, FieldPairs),
    findall(Field, http_schema_field(Name, Field, _, true, _), Required),
    Object = _{ type: object,
                description: Description,
                properties: Properties,
                required: Required,
                additionalProperties: false }.
named_schema(_, alias(Type), Description, Object) :-
    schema_object(Type, Bare),
    described(Bare, Description, Object).

/** OpenAPI 3.1 is JSON Schema 2020-12, where a `$ref` may carry sibling
 *  keywords -- so a field that is a ref still gets its own description
 *  instead of the 3.0 `allOf: [$ref]` workaround. Named here because it is
 *  one of the two places this emitter is 3.1-only (the other is the union
 *  `type: [..]` in schema_object/2). */
described(Bare, Description, Described) :-
    Described = Bare.put(description, Description).

% ── TypeExpr -> JSON Schema ──────────────────────────────────────────────────

schema_object(text,  _{type: string}).
schema_object(int,   _{type: integer}).
schema_object(float, _{type: number}).
schema_object(bool,  _{type: boolean}).
schema_object(list(Item), _{type: array, items: ItemSchema}) :-
    schema_object(Item, ItemSchema).
schema_object(schema(Name), Ref) :-
    format(atom(Pointer), '#/components/schemas/~w', [Name]),
    dict_pairs(Ref, _, ['$ref'-Pointer]).
schema_object(enum(Values), _{type: string, enum: Values}).
schema_object(one_of(Types), Schema) :-
    ( maplist(scalar_type_name, Types, Names)
    -> Schema = _{type: Names}          % 3.1 union spelling; 3.0 cannot say this
    ;  maplist(schema_object, Types, Members),
       Schema = _{oneOf: Members}
    ).
schema_object(Type, _) :-
    \+ known_type(Type),
    throw(unsupported_construct(openapi_type_unknown(Type))).

known_type(text). known_type(int). known_type(float). known_type(bool).
known_type(list(_)). known_type(schema(_)). known_type(enum(_)). known_type(one_of(_)).

scalar_type_name(text,  string).
scalar_type_name(int,   integer).
scalar_type_name(float, number).
scalar_type_name(bool,  boolean).

% ── inventory, for the parity gate's prolog leg ──────────────────────────────

spec_operations(Operations) :-
    findall(Method-Template,
            ( http_operation(OpId, Method, Path, _),
              \+ dropped_operation(OpId),
              path_template(Path, Template)
            ),
            Pairs),
    sort(Pairs, Operations).
