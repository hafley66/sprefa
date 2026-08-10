:- module(emit_openapi,
          [ openapi_text/3,
            emit_openapi/3,
            openapi_document/3
          ]).

:- use_module(library(http/json)).
:- use_module(library(lists)).
:- use_module('4_emit_jsonschema', [ module_defs/4, entry_module_details/4 ]).

openapi_text(Name, Rows, Text) :-
    openapi_document(Name, Rows, Doc),
    with_output_to(string(Body),
                   json_write_dict(current_output, Doc, [width(78), step(2), tab(200)])),
    format(string(Text), '~w\n', [Body]).

emit_openapi(Name, Rows, Path) :-
    openapi_text(Name, Rows, Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

openapi_document(Name, Rows, Doc) :-
    api_info(Title, Version, Description),
    entry_module_details(Name, Rows, ModuleId, _Hash),
    module_defs(ModuleId, Rows, '#/components/schemas/', SchemaPairs),
    dict_pairs(Schemas, schemas, SchemaPairs),
    findall(PathTemplate,
            ( api_route(_, _, Path, _, _),
              path_template(Path, PathTemplate) ),
            TemplateDup),
    sort(TemplateDup, Templates),
    maplist(path_item, Templates, PathPairs),
    dict_pairs(PathsDict, paths, PathPairs),
    Doc = _{ openapi: '3.1.0',
             info: _{ title: Title, version: Version, description: Description },
             servers: [ _{ url: 'http://127.0.0.1:17500',
                           description: 'bop serve default port' } ],
             paths: PathsDict,
             components: _{ schemas: Schemas } }.

path_item(Template, Template-PathItem) :-
    findall(MethodKey-Operation,
            ( api_route(OpId, Method, Path, Summary, PathParam),
              path_template(Path, Template),
              downcase_atom(Method, MethodKey),
              operation_object(OpId, Summary, PathParam, Operation) ),
            Pairs),
    dict_pairs(PathItem, _, Pairs).

operation_object(OpId, Summary, PathParam, Operation) :-
    responses_object(OpId, Responses),
    Base = _{ operationId: OpId, summary: Summary, responses: Responses },
    (   PathParam = none
    ->  Operation = Base
    ;   Operation = Base.put(parameters, [_{ name: PathParam, in: path, required: true, schema: _{ type: string } }])
    ).

responses_object(OpId, Responses) :-
    operation_responses(OpId, Statuses),
    maplist(response_pair, Statuses, Pairs),
    dict_pairs(Responses, _, Pairs).

response_pair(Status, Key-Response) :-
    format(atom(Key), '~w', [Status]),
    format(atom(Description), 'HTTP ~w', [Status]),
    Response = _{ description: Description }.

operation_responses(loadProgram, [200, 400]).
operation_responses(postArrivals, [200, 400, 409]).
operation_responses(readRelation, [200, 404]).
operation_responses(streamTicks, [200, 404]).
operation_responses(readStats, [200, 404]).
operation_responses(readOpenapi, [200]).

path_template(Path, Template) :-
    atomic_list_concat(Segments, '/', Path),
    maplist(template_segment, Segments, Converted),
    atomic_list_concat(Converted, '/', Template).

template_segment(Segment, Converted) :-
    (   atom_concat(':', Name, Segment)
    ->  format(atom(Converted), '{~w}', [Name])
    ;   Converted = Segment
    ).


api_route(loadProgram, 'POST', '/program',
          'compile and load a DL6 program', none).
api_route(postArrivals, 'POST', '/edb/events',
          'submit signed EDB arrivals', none).
api_route(readRelation, 'GET', '/idb/:rel',
          'read one relation snapshot', rel).
api_route(streamTicks, 'GET', '/ticks',
          'stream tick events as SSE', none).
api_route(readStats, 'GET', '/stats',
          'read process memory and SQLite storage statistics', none).
api_route(readOpenapi, 'GET', '/openapi.json',
          'read the loaded program OpenAPI document', none).

api_info('sprefa tsv2 served engine',
         '6.2.0',
         'The served tsv2 datalog engine (v6/tsv2/serve). One program at a time; POST /program swaps it. Every response body is JSON except GET /ticks, which is an SSE stream of canonical tick-log lines.').
