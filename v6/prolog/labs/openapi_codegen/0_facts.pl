% 0_facts.pl : the PROPOSED canonical HTTP surface facts.
%
% Lab-local on purpose. On landing these rows move into
% v6/prolog/compile/registry.pl beside cli_command/3 and the existing
% http_route/3, and this file disappears (lab protocol: labs die on landing).
% Nothing in v6/prolog/compile/ or v6/tsv2/ is edited by this lab.
%
% ── why these shapes ─────────────────────────────────────────────────────────
%
% registry.pl already carries http_route(Method, Path, Summary) -- three
% columns, enough to generate a CLI inventory and detect route drift, not
% enough to generate a spec. What a spec needs on top is exactly: an operation
% NAME (stable across renames of the path), the PARAMETERS, the REQUEST body,
% and the RESPONSE set. Those become four more tables rather than four more
% columns, for the same reason registry.pl keeps surface/5 and expression/5
% apart: a route with no body should not carry a body column.
%
%   http_operation(OpId, Method, PathTemplate, Summary)
%   http_path_param(OpId, Name, Type, Description)
%   http_query_param(OpId, Name, Type, Required, Description)
%   http_request_body(OpId, MediaType, TypeExpr, Required, Description)
%   http_response(OpId, Status, MediaType, TypeExpr, Description)
%
% PathTemplate keeps the SERVER'S own `:rel` spelling (4_http.ts and the
% generated cli/0_inventory.ts both use it). Translating to OpenAPI's `{rel}`
% is the emitter's job, so the facts stay target-neutral -- the same rule the
% tsv2 plan term follows (target-neutral term, target-specific emission).
%
% ── the type vocabulary ──────────────────────────────────────────────────────
%
% TypeExpr reuses the existing column-type words wherever they say the right
% thing (0_type_plane.pl: int / text / bool / float / a rel name in column
% position) and adds only what a wire boundary needs that a stored column
% does not:
%
%   text | int | bool | float        the scalar words, unchanged
%   list(TypeExpr)                   JSON array
%   schema(Name)                     $ref to a named schema below
%   enum([Atom, ...])                closed string set
%   one_of([TypeExpr, ...])          union; OpenAPI 3.1 only (see plan doc)
%
% Deliberately ABSENT: null. rulings say null never (Option is variants or
% absence), and no route on the current surface returns one.
%
%   http_schema(Name, object, Description)
%   http_schema(Name, alias(TypeExpr), Description)
%   http_schema_field(Name, Field, TypeExpr, Required, Description)
%
% Field ORDER is source order and is significant in the emitted spec's
% property list, matching decl_column_spelling's "source order significant"
% ruling for rel columns.

:- module(openapi_facts,
          [ http_operation/4,
            http_path_param/4,
            http_query_param/5,
            http_request_body/5,
            http_response/5,
            http_schema/3,
            http_schema_field/5,
            api_info/3,
            api_server/2,
            dropped_operation/1
          ]).

% ── document-level facts ─────────────────────────────────────────────────────

api_info('sprefa tsv2 served engine',
         '6.2.0',
         'The served tsv2 datalog engine (v6/tsv2/serve). One program at a time; POST /program swaps it. Every response body is JSON except GET /ticks, which is an SSE stream of canonical tick-log lines.').

api_server('http://127.0.0.1:17500', 'bop serve default port').

% ── operations ───────────────────────────────────────────────────────────────
%
% Five, matching serve/4_http.ts's ROUTE_LIST and its dispatch branches. The
% catch-all 404 (`{error, routes}`) is NOT an operation: it is what the server
% answers for any path outside this table, so listing it would make the table
% claim a route it refuses.

http_operation(loadProgram, 'POST', '/program',
               'compile and load a DL6 program.').
http_operation(postArrivals, 'POST', '/edb/events',
               'submit signed EDB arrivals.').
http_operation(readRelation, 'GET', '/idb/:rel',
               'read one relation snapshot.').
http_operation(streamTicks, 'GET', '/ticks',
               'stream tick events as SSE.').
http_operation(readStats, 'GET', '/stats',
               'read process memory and SQLite storage statistics.').

% ── parameters ───────────────────────────────────────────────────────────────

http_path_param(readRelation, rel, text,
                'relation name; must be a rel the loaded program declares.').

http_query_param(readStats, tables, text, false,
                 'comma-separated table names to scope the dbstat pass. Omitted or empty means PRAGMA-only.').

% ── request bodies ───────────────────────────────────────────────────────────

http_request_body(loadProgram, 'text/plain', text, true,
                  'the .dl6 program source.').
http_request_body(postArrivals, 'application/json', schema('ArrivalBatch'), true,
                  'one tick''s ordered arrival list.').

% ── responses ────────────────────────────────────────────────────────────────

http_response(loadProgram, 200, 'application/json', schema('ProgramLoaded'),
              'program compiled, booted, and running.').
http_response(loadProgram, 400, 'application/json', schema('Error'),
              'compile failure or named refusal; the previously loaded program keeps running.').

http_response(postArrivals, 200, 'application/json', schema('TickBatch'),
              'the settle tick plus any drain ticks the batch caused.').
http_response(postArrivals, 400, 'application/json', schema('Error'),
              'batch names a rel that is not an arrival target, carries a bad sign, or a row of the wrong width.').
http_response(postArrivals, 409, 'application/json', schema('Error'),
              'no program loaded.').
http_response(postArrivals, 500, 'application/json', schema('Error'),
              'the request branch raised; the app keeps serving.').

http_response(readRelation, 200, 'application/json', schema('RelationRows'),
              'the rel''s current rows through the program''s own decode SELECT.').
http_response(readRelation, 404, 'application/json', schema('Error'),
              'no program loaded.').
http_response(readRelation, 500, 'application/json', schema('Error'),
              'the read raised (for example, an unknown rel name).').

http_response(streamTicks, 200, 'text/event-stream', text,
              'one `data: <tick log line>` frame per tick until the client closes the socket.').
http_response(streamTicks, 404, 'application/json', schema('Error'),
              'no program loaded.').

http_response(readStats, 200, 'application/json', schema('StatsSnapshot'),
              'process memory plus SQLite storage statistics for the running program''s seam.').
http_response(readStats, 404, 'application/json', schema('Error'),
              'no program loaded.').
http_response(readStats, 500, 'application/json', schema('Error'),
              'the stats read raised.').

% ── schemas ──────────────────────────────────────────────────────────────────
%
% One row per name in v6/tsv2/runtime/types.ts that actually crosses the HTTP
% boundary. Names match the TS interface names minus the `I` prefix (the
% prefix is a TS convention for interface-vs-object disambiguation and has no
% meaning on the wire).

% Schema rows are grouped by SCHEMA, not by predicate: reading the whole of
% `Error` in one place beats reading every http_schema/3 row in one place.
:- discontiguous http_schema/3.
:- discontiguous http_schema_field/5.

http_schema('Error', object, 'Every non-2xx body on this surface.').
http_schema_field('Error', error, text, true, 'human-readable failure text.').

http_schema('RowValue', alias(one_of([text, float, bool])),
            'One relation cell. runtime/types.ts IRowValue = string | number | boolean.').

http_schema('Row', alias(list(schema('RowValue'))),
            'One relation row, columns in the rel''s declared order. Positional: no column names cross this boundary.').

http_schema('ProgramLoaded', object, 'The 200 body of POST /program.').
http_schema_field('ProgramLoaded', loaded, bool, true, 'always true; a failure is a 400.').
http_schema_field('ProgramLoaded', rels, list(text), true, 'every rel the program declares, sorted.').
http_schema_field('ProgramLoaded', arrivalTargets, list(text), true, 'the rels POST /edb/events accepts.').
http_schema_field('ProgramLoaded', hosts, list(text), true, 'declared host names.').
http_schema_field('ProgramLoaded', binds, list(schema('BindSummary')), true, 'declared binds and their literals.').

http_schema('BindSummary', object, 'One bind plan as the load response reports it.').
http_schema_field('BindSummary', name, text, true, 'bind name.').
http_schema_field('BindSummary', literals, list(schema('RowValue')), true, 'the bind decl''s literal arguments.').

http_schema('ArrivalBatch', object, 'The POST /edb/events request body.').
http_schema_field('ArrivalBatch', batch, list(schema('ArrivalRow')), true,
                  'ordered and duplicate-preserving (rulings q1); an absent batch is treated as empty.').

http_schema('ArrivalRow', object, 'One signed outside-arrival row for one tick.').
http_schema_field('ArrivalRow', rel, text, true, 'must be one of the program''s arrivalTargets.').
http_schema_field('ArrivalRow', sign, enum([add, del]), true, '+row or -row; -row is never valid against a Log rel.').
http_schema_field('ArrivalRow', row, schema('Row'), true, 'exactly as wide as the rel''s declared column list.').

http_schema('TickBatch', object, 'The 200 body of POST /edb/events.').
http_schema_field('TickBatch', ticks, list(schema('TickReport')), true, 'the ticks THAT batch caused.').

http_schema('TickReport', object, 'One tick as the served engine reports it.').
http_schema_field('TickReport', tick, int, true, 'tick number.').
http_schema_field('TickReport', line, text, true,
                  'the canonical tick-log line -- itself a JSON document, carried as text so a client diffs it byte-for-byte against the oracle''s log (the item-9 cross-target log contract).').

http_schema('RelationRows', object, 'The 200 body of GET /idb/{rel}.').
http_schema_field('RelationRows', rows, list(schema('Row')), true, 'current rows, unsorted.').

http_schema('StatsSnapshot', object, 'The 200 body of GET /stats.').
http_schema_field('StatsSnapshot', memory, schema('ProcessMemory'), true, 'process.memoryUsage().').
http_schema_field('StatsSnapshot', sqlite, schema('SqliteStats'), true, 'PRAGMA + dbstat storage numbers.').

http_schema('ProcessMemory', object, 'The one memory number this driver can give (@libsql exposes no sqlite3_status).').
http_schema_field('ProcessMemory', rssBytes, int, true, 'resident set size.').
http_schema_field('ProcessMemory', heapUsedBytes, int, true, 'V8 heap in use.').
http_schema_field('ProcessMemory', externalBytes, int, true, 'external (off-heap) allocations.').

http_schema('SqliteStats', object, 'PRAGMA page_count/page_size/freelist_count plus one grouped dbstat pass.').
http_schema_field('SqliteStats', pageCount, int, true, 'PRAGMA page_count.').
http_schema_field('SqliteStats', pageSize, int, true, 'PRAGMA page_size.').
http_schema_field('SqliteStats', freelistCount, int, true, 'PRAGMA freelist_count.').
http_schema_field('SqliteStats', dbBytes, int, true, 'pageCount * pageSize.').
http_schema_field('SqliteStats', freelistBytes, int, true, 'freelistCount * pageSize.').
http_schema_field('SqliteStats', dbstatAvailable, bool, true,
                  'false when this SQLite build has no dbstat vtab; objectBytes is then empty, never a guess.').
http_schema_field('SqliteStats', objectBytes, list(schema('SqliteObjectBytes')), true, 'per-object page bytes for the requested tables.').

http_schema('SqliteObjectBytes', object, 'One sqlite_master object''s page bytes.').
http_schema_field('SqliteObjectBytes', name, text, true, 'table or index name.').
http_schema_field('SqliteObjectBytes', bytes, int, true, 'sum(pgsize) from dbstat.').

% ── sabotage hook ────────────────────────────────────────────────────────────
%
% The parity gate's red receipt needs a way to emit a LYING spec without
% editing a checked-in file mid-run. OPENAPI_LAB_DROP=<OpId> makes the emitter
% skip one operation; unset, nothing is dropped. This exists for the receipt
% and would not survive the move into registry.pl.
dropped_operation(OpId) :-
    getenv('OPENAPI_LAB_DROP', Text),
    Text \== '',
    atom_string(OpId, Text).
