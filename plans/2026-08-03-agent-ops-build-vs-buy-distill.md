# Agent-ops build-vs-buy verdict (distilled 2026-08-04 overnight)

Distilled from the 2026-08-03 coordinator session (chat_log/20260803.7, opus
research lane relayed in-chat). Chat-level candidate detail is compressed;
verdict lines are verbatim from the session record. Re-run the research lane
before betting anything large on a compressed row.

| component | verdict | reason recorded |
|---|---|---|
| message bus (bus.ts + bus.ndjson + registry) | KEEP BESPOKE | cass transcript acks (ack = envelope id found in recipient transcript) are unique in the ~144-project field surveyed; no candidate offers read-proof |
| hcom | NAMED FUTURE SWAP | for the injection leg specifically; adopt if/when injection outgrows tmux send-keys |
| tmux control mode (-C) | THE SEAM for a rust agentd | verified locally; a daemon speaking -C owns spawn/watch/inject without scraping panes |
| registry.json | REPLACE with SQLite | real lost-update race between concurrent writers; half-day, ruling-shaped |
| message broker (mqtt/nats/redis) | NOT EARNED | volume and topology do not justify one; revisit only at multi-machine |

2026-08-04 amendments from live use (user verdict: "nothing about instant and
bus lanes was ready" -- observability failures, work products all correct):
- Claude-model workers: native subagents, never bus/tmux claude lanes (law
  landed in the agent-bus skill 2026-08-04; inherited permissions, completion
  notifications, in-UI visibility).
- External lanes get pre-seeded worktree permission rules + a mandatory
  done-hail line in the contract; tmux lanes emit no completion events.
- The [bus m-*] stamp needs a render-side consumer (attribution) and the strip
  needs a liveness poll; both are instant-fable's queue with the proof run as
  gate.
