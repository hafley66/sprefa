# Transports as rels — http/ws/LSP/shell with zero effect syntax (F4/F10 working note, 2026-07-23)

The whole answer is one law about direction:

> **Outbound** (the program wants something from the world): a **host rel** — probe it
> with bound inputs, the host proves it, rows come back. Demand-driven, tabled, cold.
> **Inbound** (the world wants something from the program): a **fact/response rel
> pair** — the transport writes request rows at the one ingest site, rules derive the
> response rel, the transport's standing subscription drains it. Correlation is a
> plain id column, never a callback.

Nothing else exists. No sigils, no @in/@out, no arrow, no send. Both directions cross
at the store; the rx lowering is the literal-rx spec's vocabulary unchanged.

## Outbound: shell (the important one) and http callers

A host rel is a foreign predicate: mode on the decl says which columns form the call
key (bound going in), the executor tail says who proves it, and the store's
call-keyed table (v5's `pending_effect` digest, already built) gives at-most-once.

```prolog
% candidate grammar — F4, human-reviewed; the SHAPE is the decision, not the spelling
host fetch(endpoint: text in, previous_etag: text in,
           status: int, etag: text, body: term) = http.

host lines(command: text in,
           line_number: int, line_text: text) = sh`{command}`.

% usage: an ordinary body predicate. Bound `in` columns = the call key.
response(endpoint, tick, status, etag, body) <-
    watch(endpoint), tick(300, tick),
    etag(endpoint: endpoint, tick: tick - 1, value: previous_etag),
    fetch(endpoint, previous_etag, status, etag, body).
```

- v5's `sh name(args) -> (outs) = \`template\`` maps 1:1: the template moves into the
  executor tail; `->` dissolves because outs are just columns after the `in` block.
- v5's `ShellKind {Read, Mutate, Stream}` collapses: Read = tabled host rel; Mutate =
  host rel where the table key IS the at-most-once guard (same mechanism, the digest
  refuses a re-fire); Stream = a host rel whose rows arrive over time with a `seq`
  column (the transport appends; ordering is data).
- rx lowering: a host rel is a cold Observable keyed by its bound columns;
  `switchMap` (cancel-stale) or `concatMap` (ordered) is a property declared on the
  host rel once. Prior art: SWI foreign predicates + modes; SQLite virtual tables
  (`json_each` is a host rel the engine already uses); SLG tabling = the call cache.

## Inbound: http handlers

The transport is dumb plumbing owned by the server process. Per registered route:

```prolog
rel http_request(request_id: int, path: text, method: text, body: term).   % transport writes
rel http_response(request_id: int, status: int, body: term).               % transport drains

http_response(request_id, 200, {stars: star_count}) <-
    http_request(request_id: request_id, path: "/stars"),
    stars(star_count: star_count).
```

Flow: request arrives -> transport INSERTs a `http_request` row (this IS the one
`.next()` site) -> tick -> rules derive `http_response` -> the transport, holding a
standing subscription on `http_response`, replies to the socket whose `request_id`
matches, then retracts or lets retention age the pair out. The standing subscription
is what keeps the route's cone warm (the demand law); deregister the route and the
cone goes cold. `--inline` is the same machine with zero transports and one query
subscription.

## Websockets

The http pair with ordering and lifecycle as columns:

```prolog
rel ws_connection(connection_id: int, opened_tick: int).                    % transport writes
rel ws_inbound(connection_id: int, seq: int, frame: term).                  % transport writes
rel ws_outbound(connection_id: int, seq: int, frame: term).                 % transport drains, in seq order
```

Both directions are streams of rows; `seq` is the order; a close is a fact. The
transport's drain subscription pushes each new `ws_outbound` row as it lands — that
is `tail -f` on a rel, which is what a standing subscription already is.

## LSP

LSP is jsonrpc over a stream = the ws shape plus method dispatch, which is just a
column to match on:

```prolog
rel lsp_request(request_id: int, method: text, params: term).               % transport writes
rel lsp_response(request_id: int, result: term).                            % transport drains
rel diagnostic(uri: text, line: int, severity: text, message: text).        % push: no request_id

lsp_response(request_id, {contents: hover_text}) <-
    lsp_request(request_id: request_id, method: "textDocument/hover",
                params: {textDocument: {uri: uri}, position: {line: line}}),
    hover_at(uri: uri, line: line, text: hover_text).
```

tower-lsp (Rust) / the node shim (prototype) owns framing and the stdio<->socket
proxy; a didOpen mints a standing subscription on that document's diagnostic cone
(the demand plan, unchanged); didClose drops it. Diagnostics need no request_id —
push rels are drained rels without correlation, the transport decides the wire shape.

## Who hosts all of this

The server process (node:http prototype; tokio+axum+rmcp+tower-lsp at
productionization, per the frozen-bindings ruling the data plane is Rust by then)
owns: the transports, the ingest writes, and one standing subscription per registered
interface. Process lifetime = sum of held subscriptions, so a server with routes
lives, an `--inline` run dies at take(1), and there is no daemon concept anywhere.

## What this kills from v5

| v5 | replaced by |
|---|---|
| `@in(class)` / `@out(class)` + `PortDir` | fact rel the transport writes / rel the transport drains |
| `sh name(...) -> (...) = template` + `ThinArrow` | `host` decl: mode columns + executor tail |
| `ShellKind::{Read,Mutate,Stream}` | tabling key semantics + a seq column |
| per-protocol Rust serve loops (mcp.rs, lsp.rs mirrors) | one generic write/drain loop per transport, registry as data |
| `@async` / `@stream` on rules | nothing — asynchrony is the host rel's private business |

Open (F4, human-reviewed): the mode spelling (`in` keyword vs prolog `+`/`-` vs
column-order convention), the executor-tail spelling, and the serve/route
registration form (a decl vs plain facts in a `serve` rel).
