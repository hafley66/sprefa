# 13. A server made of rules

> `@in`/`@out` ports and `--mcp`: a JSON-RPC server as three routing rules and a lattice.

**Goal:** serve JSON-RPC from a datalog program, and see that "a server" is
just two port declarations around the dispatch relation you built in lesson 8.

A request/response server is a fact loop: requests arrive as rows, rules derive
answers, answers leave as rows. `dl` makes that literal. You declare which
relation requests arrive *in* and which relation answers leave *out*; the
binary supplies the transport.

## The program

Save as `13.dl`:

```dl
rel req(id: text, method: text, params: text) @in(rpc).
rel resp(id: text, result: text) @out(rpc).

rel route(id: text, result: text, prio: int) key(id) merge(MaxBy(prio)).
route(id, "pong", 100)         <- req(id, "ping", _).
route(id, params, 100)         <- req(id, "echo", params).
route(id, "unknown method", 1) <- req(id, _, _).

resp(id, result) <- route(id, result, _).
```

The `@in(rpc)` / `@out(rpc)` qualifiers are **ports**. `rpc` is a contract
class, one reply per id, and says nothing about *how* bytes arrive; transport
is chosen at the command line, never in the program. The rest is lesson 8's
dispatch lattice verbatim: every request gets a fallback row, specific methods
outrank it.

One law comes with `@in`: the serving loop is the only writer. Head `req` with
a rule or a fact and the program is rejected at declare time. Requests are
world input, like file contents; deriving them would make the loop a liar.

## Run it

`--mcp` binds `rpc` ports to stdio JSON-RPC (the Model Context Protocol
framing, newline-delimited). That means you can drive it with `printf`:

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"ping"}' \
  '{"jsonrpc":"2.0","id":"abc","method":"echo","params":{"text":"hello dl"}}' \
  '{"jsonrpc":"2.0","id":3,"method":"frobnicate"}' \
  | dl 13.dl --mcp --no-daemon
```

## Expected output

```
{"jsonrpc":"2.0","id":1,"result":"pong"}
{"jsonrpc":"2.0","id":"abc","result":{"text":"hello dl"}}
{"jsonrpc":"2.0","id":3,"result":"unknown method"}
```

Three envelope behaviors worth noticing:

- **The id round-trips as raw JSON text.** `1` stayed a number, `"abc"` stayed
  a string. Inside the program both are just `text` in the `id` column.
- **`params` arrives as the raw JSON text of the params member**, and because
  `echo` put it in `result` unchanged, it left as JSON too. To dissect it,
  use the term-form `json`/`jsonp` from lesson 8's reading list:
  `jsonp(params, "text", inner_text)` binds `"hello dl"`.
- A request with no id (a notification) is answered with silence, per spec.

Delete the fallback rule and rerun the `frobnicate` request to see the
engine's own floor:

```
{"jsonrpc":"2.0","id":9,"error":{"code":-32601,"message":"no rule answered method `frobnicate`"}}
```

An id nobody answers gets a proper JSON-RPC "method not found" error, so a
half-written server fails loudly instead of hanging clients.

## Where this goes

A real MCP server, one an agent can call tools on, is the same two ports plus
rules for the `initialize` / `tools/list` / `tools/call` methods, and those are
ordinary dispatch rules too: `dl examples --show mcp-server` is a complete,
registerable one (`claude mcp add notes -- dl mcp-server.dl --mcp`). The
serving process is daemon-first like everything else, so your server shares
the warm engine, and its `req`/`resp` rows are ordinary relations you can
join, count, or rail against while it serves.

The deeper point of this lesson is the composition. Lesson 8's lattice, lesson
5's negation, lesson 12's effects: any of them can sit between `@in` and
`@out`. A server that answers `"stats"` by counting `call_edge` rows over your
repo is a three-line change, and nothing about the transport knows or cares.

## Exercise

Add a `"version"` method that answers `"13.0"`, and a `"loc"` method that
answers with the number of files in the fixture: put a `scan` rule and a
`count` aggregate in the program (lesson 3 and lesson 5 shapes), run it from
inside `notes-app`, and drive it with `printf`. You have now joined a live
JSON-RPC response against a parsed codebase in a 20-line file.
