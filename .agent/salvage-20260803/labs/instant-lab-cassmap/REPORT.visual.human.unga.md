# cass vs harness.rs, plain words

location: worktree instant-lab-cassmap
asked: which parts of harness.rs fold into cass

## big thing first

the brief said harness.rs parses four history formats to feed a
"who ran for whom" tree, with parent ids, status, tmux joins.

that is not what the file does.

harness.rs is 166 lines. it answers one small question:
give me the newest list of resume-able session ids for an exact
folder, so the UI can type:

    claude --resume <id>
    opencode --session <id>

it pulls only the id and which one is newest. nothing else.
no parent, no status, no tmux. those live in ledger.rs, not here.

so the four legs below are the four "list ids for this folder"
functions.

## the four legs

    claude    reads  ~/.claude/projects/<folder codes>/  ids = file names
    opencode  reads  opencode.db sqlite  by exact folder   ids = session rows
    codex     reads  ~/.codex/sessions/**/*.jsonl  by cwd   ids = rollout id
    kimi      reads  ~/.kimi-code/sessions/<ws>/session_<id>/state.json

all four do the same: exact folder match, newest first.

## what cass can do (probed, real outputs)

cass version 0.6.22. index present but STALE.

    last indexed: 2026-08-03 01:58
    now:          ~10.4 hours later

cass has these commands:

    cass resume <path>   builds the resume command
        claude  ->  claude --resume <id>
        codex   ->  codex resume <id>
        opencode->  opencode --session <id>

    cass sessions --workspace <folder>   list sessions for a folder
    cass search "text" --agent <name>    search the transcripts

so the resume command bit lines up perfectly.

## where cass fails

### 1. stale. a session running right now is missing.

asked cass for this exact folder:

    cass sessions --current   -> returned a 2026-07-30 claude session
    cass search "cassmap" --agent opencode  -> 0 hits

this lane's own session is running in this folder right now.
cass does not see it. the index is 10 hours old.

harness.rs reads disk directly. always fresh. no gap.

### 2. wrong folder scoping.

asked cass for this worktree folder -> every row came back
scoped to the parent folder /projects, not the worktree.

asked cass for a folder that owns exactly one session
-> it returned that one PLUS the parent's sessions (superset).

asked cass search for that same folder -> 0 hits.

so sessions filter gives too much (parent), search gives too little.
harness.rs wants the exact folder. cass can not do exact.

### 3. kimi reads the wrong tree.

cass kimi paths look like:

    ~/.kimi/sessions/<id>/.../wire.jsonl

harness.rs kimi reads:

    ~/.kimi-code/sessions/<ws>/session_<id>/state.json

different place, different file. cass does not cover the tree
harness.rs reads.

## verdict table

    leg        cass resume  exact folder  live now      keep?
    claude     yes          no (parent)   no (stale)    KEEP DIRECT
    opencode   yes          no (0 hits)   no (stale)    KEEP DIRECT
    codex      yes          no            no (stale)    KEEP DIRECT
    kimi       no           no            no            KEEP DIRECT

net: none of the four folds into cass.

cass resume matches the shape. but the two walls,
stale index and parent-scoped folders, hit every leg.
so all four stay direct disk reads.

cass is still the right tool for the separate discovery
panel (ledger.rs), which already talks to it.

## what cass structurally can not answer

    a running session  (not indexed until a pass runs)
    anything not on disk yet
    an exact folder    (workspace = parent bucket)
    the kimi tree this repo reads

for the richer panel (outside this bound file):
no parent links, no live tmux state.

## housekeeping

no commits. no source edits. no writes to cass.
index is stale here; refresh would need `cass index` which is a
write, so not run. plain text, no citations above.
