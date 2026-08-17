---
created: 2026-08-16
updated: 2026-08-16
type: improvement
reporter: chris
status: done
priority: high
labels:
- area:boop
- size:med
closed: 2026-08-16
---

# concatmap rework: one pass per window, chat-2 semantic, no fixed-point

## Description

## Comments

### 2026-08-16T15:59:14Z · @fable

THE SEMANTIC (user, 2026-08-16): copy conversation 1 into a second chat; chat 2 reads a surface-windowed view of it as INPUT and produces ITS OWN response. One model call per window. No refinement, no convergence checking.

STRIP: passes_until_fixed (hafley-rs crates/boop/src/concatmap.rs:707-725) and the --cap flag (:42,:144). The fixed-point agreement loop re-sends the same prompt up to 3x and doubled tonight's wall time for zero semantic gain. User: "i dont want passes".

FOUR DEFECTS from tonight's live run (sprefa session d0cb575b, 16-turn emotion-radar transform):
1. cursor seeds at the NEWEST ts on first run, so --session against an existing conversation maps nothing; backfill required hand-writing 0 into state/cursor. Backfill must be a spelling, not a hack (--from-start or --cursor 0).
2. default coalesce (QUEUE_CAP, concatmap.rs:17) silently DROPPED the whole backlog; only tail turns mapped until rules set coalesce:0. Dropping input is opt-in, never a default a caller discovers by absence.
3. a hung one-shot has no per-pass timeout: one opencode run sat 8+ min on one bundle until externally killed; the chat feed has a 600s turn bound, the oneshot feed has none.
4. retry ladder and the pre-built queue never re-check done/ markers, so planting a marker cannot skip a poisoned bundle mid-flight.
Also observed: turns whose text is command scaffolding (<command-name>/clear, local-command-caveat blocks) hung the worker repeatedly; the windower needs a way to filter or the runner needs to survive them.

NORTH STAR (user, same message): boop components assimilate with dl6 — the store schema/session graph declared as dl6 rels, dl6 rust typegen emitting the types, proving the rust codegen door on a real consumer. The rework keeps the store SQLite-plain and the window a caller-owned SQL SELECT precisely so that day needs no unwinding.

Sizing: size:med, one pro4 lane in hafley-rs, crates/boop/** ownership; receipts = a --from-start run over a pinned fixture conversation mapping every window exactly once, plus a poisoned-bundle test proving timeout+skip.

### 2026-08-16T16:22:47Z · @fable

SPEC CORRECTION (user, 2026-08-16, code verified): the earlier framing (one-shot per window as THE semantic) is wrong. Both feeds stay, as an explicit choice:

- feed:oneshot = concatMap(window => ONE call). Today it is not this: Rewriter::rewrite routes OneShot into passes_until_fixed (concatmap.rs:193,707-725). Strip the fixed-point + --cap; oneshot means exactly one model call per window.
- feed:chat = concatScan, KEEP IT. Resident conversation, goal turn seeds the accumulator, history carries state across windows. This arm is already the accum semantic the user wants (concatmap.rs:56-60) and was never the problem.

- The window is an incremental subscription to a templated SQL view over the store (window_rows binds :session/:session_id/:cursor, query.rs:46-79). This is deliberate proto-dl6 mechanics; keep the store SQLite-plain and the view caller-owned so a dl6 replay can take over later.
- Cursor lifecycle is the real defect: seeds at newest ts (load_or_seed_cursor, concatmap.rs:615) and advances from turn_rows ts even in window mode (poll_once computes max_seen from turn rows, not window ids). Required spelling: replay from 0 (--from-start / --cursor N), advance the cursor from the window rows the SQL returned, run until the view is exhausted, then keep tailing.

Receipts update: fixture run proving (a) oneshot maps every window exactly once with exactly one model call each, (b) chat feed carries state across two windows (second output references first), (c) --from-start replays a finished conversation fully.

