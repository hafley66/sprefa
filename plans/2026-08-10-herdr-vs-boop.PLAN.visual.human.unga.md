# herdr vs boop, plain words

## Table of contents

1. [The call, one screen](#1-the-call-one-screen)
2. [What herdr is](#2-what-herdr-is)
3. [What boop is](#3-what-boop-is)
4. [Where they overlap](#4-where-they-overlap)
5. [The one thing tmux does better](#5-the-one-thing-tmux-does-better)
6. [The four things herdr does better](#6-the-four-things-herdr-does-better)
7. [Why not fork](#7-why-not-fork)
8. [Why not raw pty libraries](#8-why-not-raw-pty-libraries)
9. [The interface you asked for](#9-the-interface-you-asked-for)
10. [The plan](#10-the-plan)
11. [Your calls](#11-your-calls)

---

## 1. The call, one screen

**Write the interface. Keep tmux. Add herdr later as a shell-out. Do not fork.**

```
       today                      step 1                      step 3
  +-------------+           +-------------+           +-------------+
  |    boop     |           |    boop     |           |    boop     |
  |             |           |             |           |             |
  |  tmux.rs    |  ---->    | TerminalHost|  ---->    | TerminalHost|
  |  (9 fns)    |           |   trait     |           |   trait     |
  +------|------+           +------|------+           +---|-----|---+
         |                         |                      |     |
       tmux                     TmuxHost               TmuxHost HerdrHost
                                   |                      |     |
                                 tmux                   tmux  herdr api
                                                              (JSON socket)
```

Step 1 changes zero behavior. It is a move, and every `boop beep` output stays
byte-identical. That is the whole point: it is safe, and it answers your ask.

Step 3 is where you find out if herdr is actually better, using a real lane
instead of reading. If it wins, switching hosts is a flag flip.

## 2. What herdr is

A tmux replacement built for coding agents. Rust. Apache-2.0. 27,118 stars,
1,903 forks, 75 contributors, pushed today. Version 0.8.0. 222,000 lines.

It runs its own pty (portable-pty), draws its own terminal (ratatui + ghostty),
and exposes everything over a JSON socket with a published schema.

The headline feature: it knows whether the agent in a pane is **idle, working,
blocked, or done**. tmux has no idea. That is the real product.

## 3. What boop is

Four layers. Only one of them overlaps herdr.

```
+--------------------------------------------------+
|  boop db      SQLite, transcripts, tokens, cost   |  herdr: nothing
+--------------------------------------------------+
|  mailbox      bus.ndjson, registry.json, hails    |  herdr: nothing
+--------------------------------------------------+
|  worktree     git worktree at base sha, ff-only   |  herdr: partial
+--------------------------------------------------+
|  session      tmux: spawn, send, capture, kill    |  herdr: YES, this one
+--------------------------------------------------+
```

Only the bottom layer is in play. It is 9 functions in one file, called from 29
places. That is small.

## 4. Where they overlap

| What | boop today | herdr | Winner |
|---|---|---|---|
| Spawn a session with a command | tmux exec's it | herdr types it into a shell | **tmux** |
| Send a line of text in | send-keys | send-text / send-keys / prompt | tie |
| Read the screen | capture-pane | read, with ansi + 3 view modes | **herdr** |
| Is it alive | yes/no | idle/working/blocked/done/unknown | **herdr** |
| Kill it | kill-session | pane close | tie |
| Get the pid | pane_pid | full argv, tty, process group | **herdr** |
| Wait for output to match | nothing | built in, regex or substring, timeout | **herdr** |
| Push events instead of polling | nothing (polls 1s) | subscribe | **herdr** |
| Exit code of the job | nothing | nothing | tie, see below |
| Mailbox / hails | yes | nothing | **boop** |
| Transcripts, tokens, cost | yes | nothing | **boop** |

## 5. The one thing tmux does better

herdr does not exec your command. It **types** it into a shell prompt and hits
Enter.

```
tmux:   tmux new-session -d -s lane -c /path 'opencode run "..."; hail rc'
        \_ the command IS the session. no shell prompt involved.

herdr:  pane.split (cwd, env)      -> you get a bare shell
        pane.run  (pane_id, text)  -> types the text, presses Enter
        \_ depends on the shell being at a clean prompt
```

For a fire-and-forget lane that runs for 20 minutes and reports an exit code,
exec is the sturdier of the two.

Related, and fine: **neither** gives you the job's exit code. boop already
solves that by wrapping the command in `; __rc=$?; hail rc=$__rc; exit $__rc`
and reading the mailbox. That trick works under either host, unchanged. So it
is not a reason to pick one.

## 6. The four things herdr does better

1. **Agent state.** idle / working / blocked / done / unknown, per pane, for 20+
   agent kinds. boop only knows alive or dead.
2. **Wait for output.** Block until a regex or substring shows up, with a
   timeout. boop polls its mailbox once a second.
3. **Event stream.** Subscribe instead of poll.
4. **Reads.** Visible screen, recent scrollback, or the detection view, with
   ansi preserved if you want it.

All four come through the JSON socket. **None of them needs linked code.** That
is the crux of the recommendation.

## 7. Why not fork

herdr is an app. It exposes no library, so nothing in it is importable.

```
what a Cargo dependency needs        what herdr has
-----------------------------        --------------
src/lib.rs                           absent
[lib] in Cargo.toml                  absent
public types                         pty types are pub(crate)
clean dependency tree                vendored portable-pty patch,
                                     ghostty bindings, ratatui, tokio
```

To fork it you add lib.rs and widen visibility across 222,000 lines, then
rebase that forever against a repo with 75 contributors that pushed today.

The fix belongs upstream, and it is small: two crates, `herdr-protocol` and
`herdr-client`. The client is **208 lines**. Every herdr plugin author needs the
same thing. File the issue; do not carry the fork.

## 8. Why not raw pty libraries

You could take `portable-pty` (the same crate herdr uses) and own the pty
yourself. Then this happens:

```
own a pty
   |
   +--> raw bytes come out. "capture the screen" needs a vt100 parser.
   |
   +--> the pane dies when your CLI exits. you need a daemon.
   |
   +--> you need reattach, persistence, restore.
   |
   = you just rebuilt tmux
```

boop has no daemon today. Every verb is a one-shot process asking someone
else's server. Owning a pty means growing a daemon, and that is a much bigger
question than the herdr one. "Infra is bought, never built" says use someone
else's multiplexer, which is exactly what boop already does.

Libraries surveyed and why each is or is not the answer:

| Crate | Verdict |
|---|---|
| portable-pty 0.9 | what herdr uses. Only if boop grows a daemon. |
| tmux_interface 0.4 | already a boop dep. Keep. Cannot do control mode or literal send-keys. |
| pty-process 0.5 | smaller portable-pty. Same daemon trap. |
| expectrl 0.9 | expect-style wait-for-pattern. Worth a look on its own merits. |
| vt100 0.16 | you need this the moment you own a pty. |
| zellij | another app with no library target. Same problem as herdr. |
| herdr on crates.io | published as a binary. `cargo add` gets you nothing importable. |

## 9. The interface you asked for

"i want tmux behind interface so we know what we need from our session/pty
handlers." Nine methods, every one of them derived from a call site that
already exists:

```
TerminalHost
  |
  +-- open(name, cwd, command, env, endpoint) -> handle
  +-- send_line(handle, text)
  +-- capture(handle, lines) -> screen text
  +-- alive(handle) -> Live | Dead | HostUnreachable
  +-- list() -> Option<set of names>
  +-- root_pid(handle) -> Option<pid>
  +-- close(handle)
  +-- owner(handle) -> Option<session name>
  +-- extras() -> what this host can do beyond the required set
```

Two details that are deliberate:

- **`alive` has three answers.** "the host is unreachable" is a
  different fact from "the session is dead". boop's tmux code already gets this
  right and the comment says so. Do not lose it.
- **There is no `exit_rc()`.** The rc comes from the shell epilogue and the
  mailbox. Asking a host for something both tmux and herdr decline to give
  would be inventing a method nobody can implement.

`extras()` is where herdr's wins live: agent state, wait-for-output, event
stream, worktree. tmux answers false to all four. That is the honest shape.

## 10. The plan

| Step | Do | Done when |
|---|---|---|
| 1 | Trait lands, tmux is the only impl | every `beep` output identical, tests green |
| 2 | Upstream issue: please publish herdr-protocol + herdr-client | issue posted |
| 3 | HerdrHost impl, shells `herdr api`, behind `--host herdr` | one lane spawned, hailed, captured, waited, end to end |
| 4 | Maybe adopt herdr's state vocabulary in `beep lane list` | your call |

Step 1 is safe and small. Step 3 is where the real decision gets made, with a
running lane instead of a reading session.

## 11. Your calls

1. Should boop lanes show up in your herdr window, or hide on their own herdr
   session name?
2. Second impl: herdr, zellij, or both? Two impls prove the trait shape. Three
   is a tax.
3. Does boop ever get a daemon? That is the gate on the pty-library route, and
   it is a bigger question than herdr.
4. File the upstream herdr-protocol issue under your name?
5. Adopt idle/working/blocked/done in `beep lane list` even on tmux?

---

Footnote you may care about: you already probed this in a codex session on
2026-08-09 ("herdr eclipse boop not the other way around u dingus", "herdr could
be an impl with shellouts or some shit, zellij etc."). That session reached the
same structural conclusion, and nothing was implemented. Your shell-out instinct
was right.
