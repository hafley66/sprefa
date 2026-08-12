# BRIEF: boop, a multiplexer trait behind its own crate + agent-reading as a feature

## Base
- Branch: `refactor/boop-mux-trait-features`.
- Base sha: `91c5ea6e` (origin/main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.
- FIRST action after the worktree exists: `git merge --ff-only 91c5ea6e`.
  Failure = STOP AND REPORT. Do not work around it.

## User decision 2026-08-11, verbatim
"bc i want to use sprefa-extract and boop with rust linking for 1 binary hehe,
and boop should have its agent reading as its own module/feature, and it should
be crate separate from a trait that tmux impl's for kosherness"

Three asks. Two are yours to BUILD. The third is yours to REPORT ONLY.

---

## PART A, BUILD: the multiplexer trait in its own crate

Today `v6/boop/src/tmux.rs` is 648 lines with 13 public functions, and callers
reach it directly. Measured call sites of `tmux::` by file:

| file | `tmux::` call sites |
|---|---:|
| `src/main.rs` | 19 |
| `src/lane.rs` | see below, grep it |
| `src/ident.rs`, `src/query.rs`, `src/harness.rs`, `src/bus.rs`, `src/usage.rs`, `src/identity.rs`, `src/rows.rs` | grep each |

Note before you panic at raw grep counts: `main.rs` mentions the string `tmux`
87 times, but only 19 are calls into the module. The rest are field names,
clap flag names, and help text. Leave those alone; a field called `tmux_session`
keeps its name.

### What to build
1. A NEW crate at `v6/boop-mux/`, package name `boop-mux`, its own
   `[workspace]` table. Copy the reason comment from `v6/boop/Cargo.toml:1-3`
   verbatim in shape: a standalone `[workspace]` keeps cargo from walking up
   into the v5 root workspace.
2. In it, one trait, `Multiplexer`. The name is decided; do not rename it.
   Its methods are the CURRENT public functions of `tmux.rs` that callers
   actually use. Derive that set by grepping call sites, not by copying all 13.
   A public function with zero external callers stays a free function in the
   tmux implementation and does NOT enter the trait.
3. The tmux implementation moves into the new crate as the one impl. Keep the
   `tmux_interface` dependency with it.
4. `v6/boop` depends on `boop-mux`. `v6/boop/src/tmux.rs` either disappears or
   becomes a thin re-export; pick one and say which in your report.
5. Call sites go through the trait.

### Constraints
- The trait is object-safe OR you state in the report exactly which method
  prevents it and why generic dispatch is used instead. Do not silently pick.
- `socket: Option<&str>` threads through nearly every current function. Decide
  whether it becomes trait state (a field on the implementation) or stays a
  per-call argument, and say which and why. Trait state is the smaller surface;
  per-call keeps the current call sites unchanged. Either is acceptable, the
  choice must be stated.
- Behavior does not change. This is a move, not a redesign. No new capability,
  no removed capability.
- Do NOT invent a second implementation. A trait with one implementation is the
  point; the user asked for the seam, not for a screen/zellij backend.

---

## PART B, BUILD: agent reading behind a cargo feature

`v6/boop/src/lib.rs:1` calls boop "the relational store over `~/.agent/boop.db`
plus the harness adapters that fill it". The transcript-reading half is these
modules, by their own headers:

| module | lines | header says |
|---|---:|---|
| `src/ident.rs` | 2317 | Layer 3, the relational conversation store |
| `src/chat.rs` | 287 | Layer 2 projection, the chat-repr door, one NDJSON line per turn |
| `src/tail.rs` | 174 | offset-resume tailing of a transcript |
| `src/query.rs` | 747 | the `boop db` read surface |
| `src/usage.rs` | 704 | token analytics over the store |

Add a cargo feature, name it `agent-read`, DEFAULT ON. It gates the transcript
reading and its store-facing surface. Building with `--no-default-features`
must still produce a working `boop` binary for the lane/bus/mux verbs.

### How to decide what goes under the feature
Do NOT guess from the table above. Build the dependency graph: for each of the
five modules, list what else in the crate uses it. A module that `lane.rs` or
`bus.rs` needs cannot go under the feature without also gating them, and
gating the lane verbs defeats the purpose. If the graph says the split is not
clean, say so with the specific `use` lines and gate the LARGEST clean subset,
reporting what you left out and which `use` line blocked it.

`cargo build --no-default-features` and `cargo build` must BOTH be green, and
both go in your report.

---

## PART C, REPORT ONLY: the one-binary link with sprefa-extract

Do NOT attempt this. Report on it.

The obstacle is already written down in both manifests. `v6/boop/Cargo.toml:1-3`
and `v6/sprefa-extract/Cargo.toml:1-4` each declare a standalone `[workspace]`
table, each with a stated reason: keeping `cargo test` isolated from the v5
root workspace, and for extract specifically "prove the v6 extraction leaf with
no v5 tree in the build graph". The repo root `Cargo.toml:7-10` is the v5
workspace and excludes `.claude` so nested git worktrees do not break cargo.

Answer, with file:line, in a section of your report:
1. What exactly breaks if both crates join one workspace. Try it in a scratch
   copy OUTSIDE your worktree if you need the error text; do not commit it.
2. Whether a multicall binary (one executable dispatching on `argv[0]` or a
   subcommand) is the shape here, and what already exists in the tree that
   resembles it.
3. The dependency collision surface: list every crate both `boop` and
   `sprefa-extract` depend on, with each one's version in each manifest.
   Version disagreement is the thing that decides feasibility.
4. What a follow-up arc would have to do, as a numbered list.

Build-vs-buy law applies to part C's recommendation: a multicall binary is a
common shape with existing crates. If you name one, name at least two and
compare them. If you name none, say "no library research done, out of scope
for this lane" rather than asserting a hand-rolled answer is right.

---

## Files you own
| path | part |
|---|---|
| `v6/boop-mux/**` (new) | A |
| `v6/boop/src/tmux.rs`, `src/lib.rs`, `src/main.rs`, `src/lane.rs`, `Cargo.toml` | A, B |
| `v6/boop/src/{ident,chat,tail,query,usage,bus,harness,identity,rows}.rs` | B, only for feature attributes and `use` fixes |
| `plans/2026-08-11-boop-one-binary.md` | C |

Forbidden: `v6/prolog/**`, `v6/tsv2/**`, `v6/labs/**`, `.github/**`,
`v6/sprefa-extract/**`, `chat_log/**`. Four other lanes are live in this repo.

## Gates, every commit
```bash
cd v6/boop && cargo build
cd v6/boop && cargo build --no-default-features
cd v6/boop && cargo test
cd v6/boop && cargo clippy --all-targets -- -D warnings
cd v6/boop && cargo fmt --check
cd v6/boop-mux && cargo test
```
Baseline on `91c5ea6e` is 161 tests, 0 failures, clippy clean, fmt clean.
Test count may only go UP. A test you had to delete is a finding you report,
never a silent removal.

Also run the real binary before you report done:
```bash
./target/debug/boop beep lane list
./target/debug/boop config show
./target/debug/boop beep lane create --branch chore/probe --brief <abs path> --dry-run
```
The dry-run must print the same `cmd:` line as before your change. Paste both.

## Known fatal
- `boop beep lane create` is how every lane in this repo spawns. Breaking it
  strands work in flight. The dry-run comparison above is not optional.
- Four lanes are running RIGHT NOW through the installed release binary. Do NOT
  run `cargo build --release` in the main tree and do NOT touch
  `~/projects/claude-research/bin/boop`. Build only inside your worktree.
- `opencode run` takes its prompt from ARGV; a mid-flight hail reaches nothing.
  Nothing you do should change that behavior.
- The store is plain SQLite at `~/.agent/boop.db` and `boop db "<sql>"` is the
  query surface. No bespoke query-flag DSLs. Do not add one while refactoring.
- Do not touch `~/.agent/boop.db` or `~/.agent/mail/`. Tests use temp dirs.

## Deliverable
A final report with four sections: part A (the trait's final method list, the
object-safety call, the socket call, the file:line of each converted call
site), part B (the module dependency graph you built, what went under the
feature, what did not and which `use` line blocked it, both build outputs),
part C (the four numbered answers), and the gate output verbatim including the
two dry-run `cmd:` lines.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  or identifiers.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Every new type name says what the thing is on first reading.
- Follow the existing style of each file you edit, even where it differs from
  anything stated here.
- Tables and lists over prose. Numbers come from tool output only.
