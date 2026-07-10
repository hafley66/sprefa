---
name: project_dl_ports_channel_class
description: "FINAL port design 2026-07-01 — @in(class)/@out(class) decl qualifiers on user-named rels, class=rpc|stream|duplex contract, transport only in CLI binding profiles (--mcp = rpc->stdio-jsonrpc); ambient recv/send REJECTED"
metadata: 
  node_type: memory
  type: project
  originSessionId: 33b32503-a5e3-4068-bf31-2f4638b4145e
---

Port design for `dl --mcp` and beyond, SETTLED 2026-07-01 (Chris: "im sold"):
user-named rels carry decl qualifiers `@in(class)` / `@out(class)` riding the
`key(...)`/`merge(...)` qualifier seam. class = the contract (rpc = Promise
shape, envelope id/method/params in, id/result out, one reply closes; stream =
Observable, complete sentinel; duplex later). The `.dl` NEVER names a
transport; binding is a CLI profile (`--mcp` = rpc ports -> stdio x jsonrpc,
later `--bind rpc=http:jsonrpc`). Drain law 1 (retire the answered @in row);
law 2 (digest sent-log, generalizes pending_effect) later. Completion:
quiescence-detection KILLED; producer-complete sentinel and consumer-gone are
distinct signals. VecDeque in-proc wire = permanent test transport.

**Why:** ambient reserved `recv`/`send` globals (built half-way, then torn out)
made MCP the only event runtime and squatted generic words; hex port/codec/wire
split (recovered from pre-compaction transcript, grep "hexagonal" in session
33b32503) keeps handler programs transport-portable. Mercury @in/@out conflict
dissolved: dl's future mode surface is the `?` sigil family, never in/out
words. SIGIL LAW: `@` marks axis 2 (tick boundary: @next/@async/@stream/@in/
@out), bare decl qualifiers mark axis 1 (lattice: key/merge). @next/@async are
literally Dedalus's two primitives; dl = single-node Dedalus + code facts +
rev second clock. Resend trap: an @out rel derived from persistent state
re-fires every rebuild — hence law 1/law 2.

**How to apply:** LANDED 2026-07-01: feature commit 0ef19f7 on
feat/dl-mcp-lattice, local main ff'd to it (NOT pushed; chat_log commit
7d02b6e stays on the branch — main's dirty LATEST.md blocked that ff).
ast.rs Port{dir,class} + Port::envelope; parse.rs qualifier loop takes
Tok::At("in"/"out"); engine declare() validates the envelope by column
name; inject_rpc/drain_rpc/retire_rpc are rel-parameterized; tick.rs bails
on any rule heading an @in port; mcp.rs rpc_ports() resolves the pair
(exactly one each, bails on ambiguity). e2e tests/it/mcp.rs (8), suite lib
189/0/1 it 412/0/4. STRING-ID GAP CLOSED same day: rpc envelope id is now
TEXT holding the request id's raw JSON (`1` / `"abc"`), int+string ids
round-trip; notifications = no id key, silent. examples/mcp-server.dl =
registerable MCP server (initialize/ping/tools/list as literal-JSON rules,
tools/call via term-form jsonp over params w/ extract/derived split);
tests/it/mcp_lifecycle.rs = the repeatable handshake harness (drives the
real example through initialize -> initialized -> tools/list -> tools/call
over stdio; replaces manual claude-mcp-add re-testing). Suite it 416/0/4.
Transport-default design discussed: bind("rpc","stdio","jsonrpc") FACTS in
a profile file, cascade CLI > bind facts > class default; decl-level codec
kwargs rejected (transport would leak into the program). DAEMON-FIRST PUMP
landed (c57065c): --mcp mirrors --hook — .dl/ present + no --db = thin
adapter over daemon RPCs mcp_request (inject+tick+drain under the daemon
lock) + mcp_retire, both validating the rel is a port in the DAEMON's
loaded program (drift guard); mcp.rs Pump{Local,Daemon}; --no-daemon = the
hermetic CI path only. e2e mcp_daemon.rs (tick-counter proof + non-port
refusal). Do not re-propose recv/send globals or transport names in decls. Innovation seats if language work resumes: type-the-boundary
(gradual Blazes: monotone/crosses-stage/needs-idempotence as checked quals)
and two-clock (tx vs rev) rel typing. Session log:
chat_log/20260701.1.dl-ports-channel-class-mcp-rung1.md. See
[[project_sh_effect_runtime]], [[feedback_rx_operators_when_sensible]].
