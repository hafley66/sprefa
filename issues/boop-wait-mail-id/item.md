---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: high
labels:
- area:boop
---

# boop wait: block until a reply or the next mail, and tell the sender how

## Description

User 2026-08-16: every agent can background a process, so the universal push
is a blocking wait. Two spellings:

- `boop wait <message-id>`: block until a bus row with `reply_to == <id>`
  (or any mail from the recipient back to the sender) lands; print it; exit 0.
- `boop wait --me`: block until the next unread mail addressed to the caller
  (identity via `boop whoami`); print it; exit 0.

And the send side advertises it: after `boop beep hail ...` queues a message,
the CLI prints one more line:

    queued m-691bc40e -> sprefa-coordinator
    to await the reply: boop wait m-691bc40e   (or: boop wait --me &)

Agents run it under a background shell; the exit re-invokes them, no
keystrokes into any pane. Reference implementation of `--me` is the polling
script at sprefa/.claude/hooks/boop-inbox-wait.sh (5s poll, unread = to==me,
kind!=ack, id not in the drained set). Native impl should use file-watch on
bus.ndjson, not sleep-poll, and honor a --timeout.

Related: boop-parent-broadcast-easy-tell (least-args tell-parent),
boop-mail-hook-inbox (hook drain for claude coordinators). Together: send with
zero route knowledge, receive with zero keystrokes, wait with one command.

## Acceptance Criteria

- [ ] `boop wait <id>` and `boop wait --me` exist, block, print the mail, exit 0;
      `--timeout <secs>` exits 124 with nothing printed.
- [ ] `boop beep hail` prints the wait hint line after `queued`.
- [ ] Marks the printed mail as delivered (to_timestamp) so a second wait does
      not replay it.
- [ ] e2e over a real bus file: send, wait in background, reply, wait exits.

## Comments

### 2026-08-17T03:47:29Z · @coordinator

USER ADDENDUM 2026-08-16: wait has a DEFAULT timeout (pick one under the Bash 10-min cap, e.g. 540s) and a shortcut arg --wait-timeout <secs> usable on wait AND on hail (hail --wait-timeout = send then block). On timeout: exit nonzero (124), and BOTH stdout and stderr print the exact re-run line, e.g. 'timed out after 540s waiting for reply to m-691bc40e; re-run: boop wait m-691bc40e --wait-timeout 540'. Same on any other exit: the last line is always the next command to run, so an agent (or a tired human) reads what to do next without thinking. Add to AC: default timeout documented in --help; timeout exit prints re-run line on both streams; hail --wait-timeout accepted.
