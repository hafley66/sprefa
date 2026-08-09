# LANE boop — PASS 3 seed: the chat-repr door (user word, 2026-08-08)

User, verbatim: "we should also be able to do zipf analysis with boop by asking
for json stream or file of chat repr from boop."

## The feature

One subcommand, two modes over the SAME record type:

```
boop chat --session <id> [--json]      # snapshot: NDJSON to stdout, then exit
boop chat --session <id> --follow      # stream: tail, one NDJSON line per new turn
boop chat --all [--since <ts>]         # every session the registry knows
```

Record, one line per TURN (assistant text turn, user turn, tool call collapsed
to its name + primary arg):

```json
{"session":"<id>","harness":"claude","seq":41,"ts":1786230000000,
 "role":"user|assistant|tool","text":"...","tool":null,"branch":"main"}
```

- `text` for role=tool is empty; `tool` carries `{"name":"Read","arg":"<file_path>"}`.
- Reuse the pass-1 tailer and the pass-2 ident/session machinery; this is a
  projection of records you already parse, never a second parser.
- `--follow` is the tailer door; plain is read-to-EOF. Same serializer.

## Why (so the shape is right)

First consumer is zipf/word-frequency analysis: compare USER text vs ASSISTANT
text stem distributions (this is how `refusal` was caught: thousands of agent
uses, zero user uses). So role separation must be lossless and `text` must be
the human-visible words only: no tool payloads, no base64, no file contents
inside tool results. A consumer must be able to do
`boop chat --all | jq -r 'select(.role=="user") | .text'` and get pure human
words.

## Gates

- `cargo build`, `cargo test`, `cargo clippy -- -D warnings` green.
- One test: fixture jsonl in `v6/boop/tests/fixtures/`, assert the NDJSON
  projection line count and that a tool result's file content does NOT appear
  in any `text` field.
- Round-trip receipt in REPORT.md: run `boop chat` against a real
  `~/.claude/projects/...` transcript, show 3 output lines.

Done-report law unchanged: end with
`bus hail --to fable-main --kind result --body "boop pass3 done: <one line>"`.
