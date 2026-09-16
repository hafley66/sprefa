# dl-rails — Claude Code plugin

The whole `dl` surface in one install, namespaced as a plugin instead of four
separate `dl setup` wirings. `plugin.json` declares:

| piece | what it gives Claude Code |
|---|---|
| **lspServers** (`dl --lsp`) | rails diagnostics on save, go-to-definition over `module_edge`, find-references over the ref spine |
| **skills/** (`sprefa-dl`) | the authoring skill — the constraints that bite, the extractor ladder, kwargs, effects — loaded on demand |
| **mcpServers** (`dl --mcp`) | an MCP query server (the bundled `mcp-server.dl`; extend it with tools that run dl queries) |
| **hooks/** (`dl --hook`) | the rail hook on UserPromptSubmit + Edit/Write, so a `diag`/`inject` rule can add context or block |

The skill and MCP program are symlinks to the repo's single sources
(`assets/sprefa-dl.skill.md`, `examples/mcp-server.dl`); a release step copies
them.

## Install

```sh
cargo install --path . --bin dl       # puts `dl` on PATH (~/.cargo/bin)
claude --plugin-dir editors/claude-plugin
```

The LSP needs the target repo to have a `.dl/` directory with at least one
`.dl` file, else the server exits loudly at startup (a typo'd setup must not
look like a clean one). The LSP learns its workspace from the client's
`rootUri` — there is no `--root`.

## The MCP server

`mcp-server.dl` ships the MCP lifecycle (initialize / tools/list / tools/call)
as datalog rules, with a `ping` tool as the worked example. Add a tool by
adding a rule that heads the `@out(rpc)` response for a `tools/call` — e.g. a
`callers_of` tool that runs a `call_edge` query and returns the rows. Keep tools
few and dense (return the answer, not a relation dump) so each call is cheap.

## Division of labor

Navigation + diagnostics come from the LSP; enforcement (block-and-feed-back)
comes from the `dl --hook` rail; on-demand structured queries come from MCP;
authoring guidance comes from the skill. One install wires all four. The
standalone `dl setup` remains for non-Claude-Code hosts and dynamic detection.
