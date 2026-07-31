# Getting started with v6

Your first thirty minutes: install, run a program, make it react to a file on
disk, query it, and read an error.

Every command block below is **executed** by `just getting-started`
(`v6/tsv2/scripts/getting-started.sh`), and its output is diffed against the
text printed here. If the engine's behaviour moves and this page does not, that
receipt goes red. The page is a graded artifact, not a description of one.

The receipt replays the blocks in a fresh temporary directory. Do the same:

```
mkdir -p ~/bop-demo && cd ~/bop-demo
```

Volatile values are written as placeholders — `<digest>` for a content hash,
`<epoch>` for a wall-clock bucket, and `<n>` in the one block (`bop stats`)
whose numbers are machine state rather than engine behaviour. Those are exactly
the substitutions the receipt makes before diffing; everything else on the page,
including every row of every relation, is compared literally.

---

## 1. Setup

Three prerequisites, all of them real dependencies of the pipeline rather than
conveniences: **node 24+** (the runtime runs `.ts` directly under
`--experimental-transform-types`), **pnpm**, and **SWI-Prolog 10** (the compiler
front is prolog — it parses `.dl6` and emits the TypeScript program the runtime
executes). Section 3 also pipes one tick log through `jq`, which the repo's
other receipts use too.

<!-- gs:run nodiff -->
```console
$ export SPREFA="${SPREFA:-$HOME/projects/sprefa}"
$ export BOP="$SPREFA/v6/tsv2/cli/bop.ts"
$ export NODE_NO_WARNINGS=1
$ node --version && pnpm --version && swipl --version | head -1
```

`SPREFA` points at your clone (the `${VAR:-default}` form leaves an existing
setting alone). `NODE_NO_WARNINGS=1` only silences node's experimental-features
banner on stderr; nothing else depends on it.

Install per package. Each of the three has its own `node_modules`; a symlink to
a parent directory's is not a supported layout here.

<!-- gs:run nodiff -->
```console
$ (cd "$SPREFA/v6/tsv2" && pnpm install) > /dev/null
$ (cd "$SPREFA/v6/dl" && pnpm install) > /dev/null
$ (cd "$SPREFA/v6/sprefa-store/js" && pnpm install) > /dev/null
```

`bop` is the CLI. It is an executable `.ts` file with its own shebang, so no
build step stands between you and it:

<!-- gs:run -->
```console
$ "$BOP" --help | sed -n '1p'
Usage: bop [options] [command]
```

Seven verbs: `serve` `run` `check` `load` `q` `stats` `ticks`. There is no
daemon — `run` and `check` boot a server in-process for exactly one job and
tear it down.

---

## 2. Your first program

A `.dl6` program is declarations and rules. Write this as `hello.dl6`:

<!-- gs:write hello.dl6 -->
```dl6
bind interval(period: int, bucket: int).

rel beat(bucket: int).
beat(bucket) <- interval(1, bucket).

rel beats(total: int).
beats(count(bucket)) <- beat(bucket).
```

Three things are happening.

- `bind interval(...)` says the world pushes rows into a relation called
  `interval`. Cadence is not a language construct here; it arrives as ordinary
  rows, and the `1` in the rule body is the period in seconds.
- `beat(bucket) <- interval(1, bucket).` is a rule. `<-` reads "is derived
  from". Bare identifiers are variables; constants are numbers or
  single-quoted atoms.
- `beats(count(bucket)) <- beat(bucket).` is an aggregate over the whole
  relation.

Check it before running it. `check` compiles through the text door and boots
nothing; silence plus exit 0 is the answer:

<!-- gs:run -->
```console
$ "$BOP" check hello.dl6; echo "exit $?"
exit 0
```

Now run it for three ticks:

<!-- gs:run -->
```console
$ "$BOP" run hello.dl6 --ticks 3
{"tick":1,"deltas":{"beat":{"add":[[<epoch>]],"del":[]},"beats":{"add":[[1]],"del":[]},"interval":{"add":[[1,<epoch>]],"del":[]}}}
{"tick":2,"deltas":{"beat":{"add":[[<epoch>]],"del":[]},"beats":{"add":[[2]],"del":[[1]]},"interval":{"add":[[1,<epoch>]],"del":[]}}}
{"tick":3,"deltas":{"beat":{"add":[[<epoch>]],"del":[]},"beats":{"add":[[3]],"del":[[2]]},"interval":{"add":[[1,<epoch>]],"del":[]}}}
```

That output is the whole execution model in nine lines. One JSON line per tick;
each line is the **delta** on every relation that moved, not a snapshot. `beats`
does not re-emit a row and quietly overwrite it — it emits `add [[2]]` and
`del [[1]]`, because a derived row that stopped holding is retracted. Nothing
recomputes from scratch between ticks.

`bucket` is `floor(epoch_seconds / period)`, which is why it survives a restart
and why the receipt normalizes it to `<epoch>`.

---

## 3. Making it react to files

Two more constructs and the program becomes a live rail over a directory.

- `bind watch(glob, path, digest)` — the file watcher, as ordinary arriving
  rows. A save that changed bytes arrives as a delete of the old row plus an
  add of the new one; a save that changed nothing arrives as nothing at all,
  because the digest is the same row. A deleted file arrives as a bare delete.
  There is no "event kind" column and no null.
- <code>sh name(inputs) -> (outputs) = \`command\`</code> — a host: a shell
  command the engine runs on demand, whose stdout becomes rows. `{path}`
  splices a column into the command line (shell-quoted, not pasted raw).

Start from a directory holding one note:

<!-- gs:write notes/plan.md -->
```markdown
# plan

nothing to do yet
```

And this program, `todos.dl6`:

<!-- gs:write todos.dl6 -->
```dl6
bind watch(glob: text, path: text, digest: text).

sh scan_todos(path: text, digest: text) -> (line: int) =
  `grep -n 'TODO:' {path} | cut -d: -f1   # re-runs when {digest} moves`.

rel file(path: text, digest: text).
file(path, digest) <- watch('**/*.md', path, digest).

rel todo(path: text, line: int).
todo(path, line) <- file(path, digest), scan_todos(path, digest, line).

rel todo_count(path: text, total: int).
todo_count(path, count(line)) <- todo(path, line).
```

The `{digest}` in that shell comment is not decoration. A host's inputs form its
cache key, and the compiler refuses an input the command never mentions
(`template_mismatch`) rather than let a column silently do nothing. Naming the
digest is what makes the command re-run when the file's content changes and not
when it merely gets touched.

Start a server. In real use you leave this in its own terminal; here it goes to
a log so the rest of the page can keep reading it:

<!-- gs:run -->
```console
$ "$BOP" serve --port 17593 > serve.log 2>&1 &
$ sleep 2 && cat serve.log
tsv2 serving on 17593 (db :memory:)
```

The server's working directory is the watch root, so globs and emitted paths are
relative to wherever you started it. Load the program into it:

<!-- gs:run -->
```console
$ "$BOP" load todos.dl6 --port 17593
{"loaded":true,"rels":["__host_demand_scan_todos","__host_response_scan_todos","file","todo","todo_count","watch"],"arrivalTargets":["__host_response_scan_todos","watch"],"hosts":["scan_todos"],"binds":[{"name":"watch","literals":["**/*.md"]}]}
```

The two `__host_*` relations are the host's own machinery, made of the same
stuff as your relations: a demand row per `(path, digest)` the program wants
answered, and a response row per line of stdout that came back.

Now write a file with two TODOs in it — this is the part you would do in an
editor:

<!-- gs:write notes/todo.md -->
```markdown
# notes

TODO: ship the doc
TODO: wire the receipt script
```

<!-- gs:run -->
```console
$ sleep 3 && "$BOP" q todo --port 17593
notes/todo.md	3
notes/todo.md	4
```

The watcher saw the write, `file` gained a row, that row created demand for
`scan_todos`, the command ran once, and its two stdout lines became two `todo`
rows on lines 3 and 4. Nobody polled anything and nothing rebuilt.

Edit the file again, resolving one of them:

<!-- gs:write notes/todo.md -->
```markdown
# notes

TODO: ship the doc
done: wire the receipt script
```

<!-- gs:run -->
```console
$ sleep 3 && "$BOP" q todo --port 17593
notes/todo.md	3
$ "$BOP" q todo_count --port 17593
notes/todo.md	1
```

The row for line 4 is gone. Look at what the last two ticks actually said:

<!-- gs:run -->
```console
$ grep '"tick"' serve.log | tail -2 | jq -c '.deltas.todo, .deltas.todo_count'
{"add":[],"del":[["notes/todo.md",3],["notes/todo.md",4]]}
{"add":[],"del":[["notes/todo.md",2]]}
{"add":[["notes/todo.md",3]],"del":[]}
{"add":[["notes/todo.md",1]],"del":[]}
```

Retraction first, then re-derivation: the edit killed the old digest's rows, and
the new digest's answer put one back. That is the same delta discipline as
section 2, now driven by a real file on disk through a real subprocess.

---

## 4. Reading relations

`bop q <rel>` prints one relation's current rows, tab separated, positional —
no column names cross the HTTP boundary, so none are printed:

<!-- gs:run -->
```console
$ "$BOP" q file --port 17593
notes/todo.md	<digest>
```

`--json` gives you the raw body instead:

<!-- gs:run -->
```console
$ "$BOP" q todo --port 17593 --json
{"rows":[["notes/todo.md",3]]}
```

`bop stats` reports process memory and SQLite storage for the running engine —
the numbers a soak test watches. Every figure here is `<n>` because the receipt
pins the payload's shape, not one machine's RSS:

<!-- gs:run norm=bytes -->
```console
$ "$BOP" stats --port 17593
{"memory":{"rssBytes":<n>,"heapUsedBytes":<n>,"externalBytes":<n>},"sqlite":{"pageCount":<n>,"pageSize":<n>,"freelistCount":<n>,"dbBytes":<n>,"freelistBytes":<n>,"dbstatAvailable":true,"objectBytes":[]}}
```

`bop ticks --port 17593` streams tick events as they happen (server-sent events,
until you interrupt it). Stop the server when you are done — Ctrl-C in its own
terminal, or, since it is a background job here:

<!-- gs:run -->
```console
$ kill %1
```

Note what is missing from `notes/plan.md`: it never appears in `file`, even
though it matches `**/*.md`. Two things decide what a `watch` glob sees, and
only one of them is the glob:

- at subscribe, the bind reconciles against the **tracked** worktree
  (`git ls-files`), so a committed file that matches is already there on the
  first tick, before anything changes;
- after that it reports **changes**.

This scratch directory is not a Git repository at all, so the tracked set is
empty and only the second rule ever fires — which is why `plan.md` stays absent
while `todo.md` appears the moment it is written. Run the same page inside a
repo with `plan.md` committed and it is in `file` from the start.

Membership is decided by node's `path.matchesGlob` on both of those paths, never
by Git's pathspec rules (ruling `glob_dialect`), so `**/*.md` includes
repo-root files and `src/**/*.rs` includes `src/lib.rs` — the way the same glob
reads in v5. Enumerating a tree on demand instead of watching it is a separate
host (`enumerate`, backed by `git ls-files` as a pathspec); see `just
enumerate`.

---

## 5. Reading an error

Three exit codes, and the distinction between them is the point: `0` clean, `1`
broken, `2` a named refusal. A refusal is not a crash — it is the compiler
declining to compile a construct it will not silently get wrong.

Start with broken. Drop the closing period off a declaration:

<!-- gs:write typo.dl6 -->
```dl6
rel beat(bucket: int)

beat(bucket) <- interval(1, bucket).
```

<!-- gs:run -->
```console
$ "$BOP" check typo.dl6; echo "exit $?"
broken: parse error at line 3, column 1: statement
exit 1
```

Now a program that parses perfectly and still will not compile:

<!-- gs:write broken.dl6 -->
```dl6
bind interval(period: int, bucket: int).

rel beat(bucket: int) log keep(all).
beat(bucket) <- interval(1, bucket).
```

<!-- gs:run -->
```console
$ "$BOP" check broken.dl6; echo "exit $?"
refusal: <workdir>/broken.dl6:4: unsupported_construct: compiler refused rule 'log_on_level_headed_rel' for rel 'beat/1' (log_on_level_headed_rel)
exit 2
```

`log` declares an append-only relation, and `beat` is also the head of a
derived rule (`<-`). Those two cannot both be true: a derived view is
recomputed from its inputs, so there is no append for the log plane to record.
The refusal names the file and the line (`broken.dl6:4`, the rule), the check
(`log_on_level_headed_rel`), the relation (`beat/1`), and the functor you can
grep the compiler for — that check lives in `v6/prolog/0_program_check.pl`
with the one-sentence reason above it. Dropping `log keep(all)` from line 3 is
the fix.

The location comes from the compile door itself, so running the same file
through the compile script directly reports the same line:

<!-- gs:run -->
```console
$ bash "$SPREFA/v6/prolog/compile/scripts/compile_dl6.sh" broken.dl6 /dev/null; echo "exit $?"
ERROR: [Thread main] -g compile_dl6('broken.dl6', '/dev/null'): broken.dl6:4: unsupported_construct: compiler refused rule 'log_on_level_headed_rel' for rel 'beat/1' (log_on_level_headed_rel)
exit 2
```

The two spellings of the path are the two callers, not two answers: `bop check`
resolves what you type to a full path before handing it to the compiler, and
the script passes it through as typed.

---

## 6. Where to go next

| you want | read |
|---|---|
| every construct, with the generated table of what is live vs refused | [`prolog/compile/SYNTAX.md`](prolog/compile/SYNTAX.md) |
| what a tick *is* — the delta semantics behind sections 2 and 3 | [`prolog/compile/TICK-MODEL.md`](prolog/compile/TICK-MODEL.md) |
| how far the real programs actually got, per program, with receipts | [`READINESS.md`](READINESS.md) |
| the system's own map of itself, regenerated by a dl6 program | [`ARCH-MAP.md`](ARCH-MAP.md) |
| what happened and why, in order | [`../DEVLOG.md`](../DEVLOG.md) |

Every gate in this repo is a `just` recipe, and each carries a header saying
what it proves and what to expect. `cd v6 && just` lists them. The ones closest
to this page:

- `just getting-started` — replays this page and diffs the output.
- `just extraction-live` — section 3's shape at production scale: the real
  rust extractor, atomic saves, deletions, and `kill -9` mid-extraction.
- `just golden-flex` — one program exercising every live construct, graded six
  ways.
- `just green` — the full review gate.
