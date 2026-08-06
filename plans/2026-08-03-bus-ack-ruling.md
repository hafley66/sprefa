# Bus ruling: message state + cass-verified acks (user-set, 2026-08-03 ~00:00)

Owner's words, near-verbatim: message passing requires cass. Messages are
from/to ids and from_timestamp/to_timestamp; to_timestamp is acked by a cass
query proving the to-session has the message in its history/context as a read.

## Envelope (supersedes the ts-only field in the 2026-08-02 belt design)

```json
{ "id": "m-...", "from": "<agent id>", "to": "<agent id>",
  "from_timestamp": "<iso, set at send>",
  "to_timestamp": null,
  "kind": "request|result|note", "reply_to": null,
  "body": "...", "ref": null }
```

## Ack semantics

- Send: `hail` appends the envelope (to_timestamp null) and injects the body
  into the recipient session.
- Ack: to_timestamp is filled ONLY when a cass query finds the message in the
  to-session's transcript: `cass search "<id or body prefix>" --robot` scoped
  to the recipient's session/source path. The harness transcript is the ack
  ledger; cass is the reader; no separate receipt channel exists.
- Consequence: at-least-once resend is safe and cheap. Unacked after resend +
  cass reindex = the injection leg is broken, which is a defect in the leg,
  never a reason to invent a receipt protocol.
- "Read" = present in the recipient's history/context. Nothing stronger is
  claimed (the model may not have attended to it; a `result` reply remains the
  only proof of handling, per the request/result correlation already ruled).

## Relational state

messages(id, from_id, to_id, from_timestamp, to_timestamp, kind, reply_to,
body, ref) — the mailbox NDJSON is the log; to_timestamp updates are appended
as ack rows (append-only law), latest row per id wins. Sessions join messages
via the registry (agent id -> harness + session id), same join the
harness-trace panel uses.

## Scope notes

- cass index freshness bounds ack latency; `cass index --watch` is the lever.
- Tonight's dock-strip lab fixtures predate this ruling (single `ts` field);
  its MailEnvelope type is optional-field tolerant, so no re-run needed. The
  belt build applies this ruling from the first line.
