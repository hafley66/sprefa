# Lane `lab/extract-ctf-sonnet` (sonnet): map a codebase you may only ever see through `extract`

First action: `git merge --ff-only f71d1ae5e3f62ebb7798e19444c88a1e24dce192`. Failure or missing tree = STOP and
beep the coordinator.

## The rule that defines this whole task

**`extract` is your ONLY window onto the source code.** You may never read a
source file by any other means.

BANNED, on any `.rs` file, no exceptions:
`cat`, `head`, `tail`, `sed`, `awk` reading a source file, `less`, `more`,
`grep`, `rg`, `ag`, `ack`, the Read tool, any editor, `ast-grep`/`sg` invoked
directly, `cargo doc`, `cargo expand`, `rustdoc`, `git show`/`git log -p` of
file contents, and any web search.

ALLOWED:
- `extract` with any flags. This is the point of the exercise.
- `extract --help` and `extract --schema`. Read both first; `--schema` names
  every record shape and field you can get.
- `find` and `ls` to list FILE PATHS only. `--resolve` takes files, never
  directories, so you need paths to feed it. Listing a path is not reading a
  file.
- `jq`, `sort`, `uniq`, `wc`, `cut`, `awk`, `head` applied to EXTRACT'S OWN
  STDOUT or to a file of extract output you saved. Piping extract's output
  anywhere is encouraged.
- Writing your report and notes.

If you find yourself wanting to peek at the source, that wanting is the
measurement. Write down what you wanted to see and what you did instead.

## The binary

```
/Users/chrishafley/.cache/boop/cargo-target/release/extract
```

Do not build it, do not `cargo` anything. That path is a working release build.

## The target

`v6/sprefa-engine-rs/src` in your worktree: 35 `.rs` files, 14,994 lines. A
datalog engine runtime. You know nothing else about it and you are not to look
it up.

## Deliverable

One file, `plans/extract-eval-2026-08-31/CTF.sonnet.REPORT.md`, committed.
Two parts.

### Part 1: the seven flags

Answer each with the value AND the exact command that produced it. A flag with
no reproducible command scores zero even when the value is right. If a flag is
ambiguous, say how you read it and answer the reading you chose.

```
F1  Which single function makes calls to the most DISTINCT callees?
    Give file::function and the count.
F2  Which callee NAME is called from the most DISTINCT files?
    Give the name and the file count.
F3  Which file defines the most DISTINCT functions that make at least one call?
    Give the path and the count.
F4  Which file owns the most resolved TYPE references? Path and count.
F5  Which type is the target of the most resolved type edges? Name and count.
F6  Which file has the most resolved imports? Path and count.
F7  Break down every unresolved record by its reason, with counts, descending.
```

Every count in this crate is reachable from `extract --resolve --family
call,type` over the 35 files plus `jq`. Nothing here needs a trick.

### Part 2: the architecture report

Written for someone who has never opened this crate, from facts only.

- What is the shape of the thing: which files are the spine, which are leaves.
  Say how the call and import graphs told you.
- The 5 most connected functions and what their edges suggest they do.
- The type plane: what the most-referenced types are and what that implies
  about the design.
- **A section named "What extract could not tell me."** List every question you
  formed and could not answer through this tool, and say what record or flag
  would have answered it. This section is worth as much as the rest of the
  report; do not pad it and do not skip it.
- **A section named "Where I nearly cheated."** Every moment you wanted to open
  a file, and what you did instead.

Tables and lists over prose. No em dashes. Never the words `provenance`,
`substrate`, `load-bearing`, `regime`. Say "oracle", never "ground truth".

## How you are graded

The coordinator holds an answer key computed independently. Flags are scored
right or wrong; the report is scored on whether its claims follow from facts
you actually printed. A confident wrong number scores worse than "I could not
determine this, here is why", so say when you do not know.

Do NOT ask the coordinator for hints about the flags. Beeping to report a
broken brief or a blocked command is fine.

## Working posture

- Save extract output to files under `/tmp/claude-501/` and query them
  repeatedly rather than re-running the extractor for every question.
- 10-second law: nothing foreground over 10s. The whole 35-file resolve takes
  a few seconds, so this should not bite, but background anything that does.
- **Do not end your turn until the report is written AND committed.** A turn
  that ends with an uncommitted deliverable is a failed run; three lanes did
  exactly that today.
- You are a lane: never spawn a subagent.

## Reaching the coordinator

```
boop beep --no-wait --as lab-extract-ctf-sonnet sprefa-coordinator "<one line>"
```
