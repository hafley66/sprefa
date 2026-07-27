# diag_emit: prolog checks into the editor, with zero new editor code

Run: `swipl -q -l v6/prolog/labs/diag_emit.pl -g go -g halt` (14 PASS, exit 0).

VERDICT: the self-programmable-LSP loop closes today, from prolog, with no rust
change and no extension change. A sqlite file with one table and one view is the
entire interface. The lab writes it by piping SQL text to the `sqlite3` CLI in
one `process_create` per emit, and v5's `dl --lsp --diag-db` picks the rows up on
its 500ms poll. The interesting part is not the plumbing, it is that the emitter
has to reimplement level semantics by hand (delete every row for the sources it
scanned, then reinsert), which is exactly the maintenance job the candidate
language promises to do for you. That hand-written refresh is the strongest
argument in this lab for the `<-` arrow existing at all.

## 1. The `diag_v5` schema, as found in src/lsp.rs

Nine columns, positional, this exact order. The reader treats them as the whole
interface (`src/lsp.rs:392-393`).

| # | column | SQL type | rust type | citation |
|---|---|---|---|---|
| 0 | `path` | TEXT | `String` | src/lsp.rs:400 |
| 1 | `line` | INTEGER | `i64` | src/lsp.rs:401 |
| 2 | `col` | INTEGER | `i64` | src/lsp.rs:402 |
| 3 | `end_line` | INTEGER | `i64` | src/lsp.rs:403 |
| 4 | `end_col` | INTEGER | `i64` | src/lsp.rs:404 |
| 5 | `severity` | TEXT | `String` | src/lsp.rs:405 |
| 6 | `code` | TEXT | `String` | src/lsp.rs:406 |
| 7 | `msg` | TEXT | `String` | src/lsp.rs:407 |
| 8 | `hint` | TEXT or NULL | `Option<String>` | src/lsp.rs:408 |

The statement the poller prepares, verbatim (`src/lsp.rs:545`):

```sql
SELECT path, line, col, end_line, end_col, severity, code, msg, hint FROM diag_v5
```

Other pinned behavior read off the same file:

- Positions are already 0 based. `diag_v5_to_diagnostic` does no adjustment
  (`src/lsp.rs:595-609`), unlike the engine's own 1 based `diag` relation.
  Negative values are clamped to 0 by `row.line.max(0)` (src/lsp.rs:600-601).
- `severity` mapping (`src/lsp.rs:585-593`): `error`, `warn`, `info`, `hint` map
  to ERROR, WARNING, INFORMATION, HINT. Every other string, including the empty
  one, silently becomes WARNING. So a typo in this column is invisible.
- `code` is dropped when empty, otherwise attached as a string code
  (src/lsp.rs:607).
- `hint`, when present and non empty, is appended to the message as a second
  line prefixed `hint: ` (src/lsp.rs:603-606).
- `source` is always the literal `"dl"` (src/lsp.rs:608).
- Relative `path` values resolve against the LSP process cwd, not the workspace
  root (src/lsp.rs:416-419, :620). Absolute paths pass through unchanged. This
  lab emits absolute paths.
- A missing `diag_v5` view reads as zero rows rather than an error
  (src/lsp.rs:564-567), and a missing db file is retried every 500ms
  (src/lsp.rs:519-527). Both mean the writer may start after the editor.
- Change detection is `PRAGMA data_version` on a persistent connection
  (src/lsp.rs:435-452). A commit from another process is what moves it.
- Retraction is by absence: a path published on a previous round and absent from
  the current SELECT gets an empty publish (src/lsp.rs:439-441, :488-495).

The DDL this lab writes is copied from the node side runtime
(`v6/dl/src/5_diag.ts:39-41`) so both writers agree on one shape:

```sql
CREATE TABLE IF NOT EXISTS rel_diag (
  path TEXT, line INTEGER, col INTEGER, end_line INTEGER, end_col INTEGER,
  severity TEXT, code TEXT, msg TEXT, hint TEXT);
CREATE VIEW IF NOT EXISTS diag_v5 AS SELECT path, line, col, end_line, end_col,
  COALESCE(severity,'warn') severity, code, msg, hint FROM rel_diag;
```

## 2. Wiring it up for real

Emit side, from the repo root:

```sh
swipl -q -l v6/prolog/labs/diag_emit.pl \
  -g "prolog_root(Root), checkable_files(Root, Paths), \
      out_db('diag.sqlite', Db), emit_over_files(Db, Paths)" -g halt
```

That writes `v6/prolog/labs/out/diag.sqlite`. Re run it after any edit; the
delete plus reinsert is what makes fixed findings vanish from the editor.

Server side (`src/cli/mod.rs:114-124`, `src/lsp.rs:89-99`):

```sh
dl --lsp --diag-db /Users/you/projects/sprefa/v6/prolog/labs/out/diag.sqlite
```

In this mode there is no engine boot, no tick, and no db write. The process is a
poller plus a shutdown handler. The db need not exist when the server starts.

VSCode side, from `editors/vscode-dl/README.md` and `src/extension.ts`:

- The extension resolves the `dl` binary (`dl.binaryPath`, else
  `~/.cargo/bin`, `/opt/homebrew/bin`, `/usr/local/bin`) and spawns it with
  `args = program ? [program, "--lsp"] : ["--lsp"]` (extension.ts:203), cwd set
  to the workspace root (extension.ts:205).
- There is no setting that appends `--diag-db`. See ambiguity 3 for the two
  ways around that, neither of which this lab performs.
- The client attaches by language id, not by extension glob:
  `dl, rust, typescript, typescriptreact, javascript, javascriptreact, python,
  go, kotlin, json, yaml, toml, shell` (extension.ts:211-224). `prolog` and
  `markdown` are absent, so the rows this lab writes about `.pl` and `.md` files
  would be published and then ignored by the client's document filter. See
  ambiguity 4.
- Activation is `onStartupFinished` plus `workspaceContains:**/*.dl`
  (package.json:16-19), so a workspace with no `.dl` file still activates on
  startup.

Everything above is documentation. No editor was launched by this lab.

## 3. The three checks, and what they look like in the candidate language

Implemented here as prolog over file text:

1. banned identifiers or prose, severity `error`, code `banned-word`. The word
   table is spelled in halves inside the lab so the lab does not trip its own
   rule (see limitation 1). A line carrying the waiver marker is skipped.
2. a `.pl` lab with no `go :-` clause, severity `warn`, code `no-go-entrypoint`,
   anchored at the top of the file.
3. an em dash in a `.md`, severity `warn`, code `em-dash`, at the real line and
   column.

In the candidate surface (prose only, nothing implemented):

- The spine is already relational: `file(path, content_hash)` and a text
  extraction rel giving `line(path, index, text)`. Check 3 is then one level
  rule: `diag(path, index, col, index, col+1, "warn", "em-dash", ...) <- line(path, index, text), contains(text, em_dash), ends_with(path, ".md");`
  and nothing else. Retraction is free: edit the line, the body atom leaves the
  current membership, the head row leaves with it, the editor clears the
  squiggle. That is the whole feature the manual delete plus reinsert in this
  lab is imitating.
- Check 1 is the same shape with the word table as a fact rel,
  `banned(word, replacement);`, so adding a word is a fact, not a code change,
  and the join does the fan out over words.
- Check 2 is the one that wants negation over a derived rel: a rel
  `has_entrypoint(path)` derived from a clause head fact, and the diagnostic
  heads on `lab_file(path)` with `not has_entrypoint(path)`. Under level
  semantics that retracts the moment someone adds the clause, which is the
  behavior a hand written emitter has to buy with an explicit DELETE.
- `diag` is a keyed rel candidate. Key `(path, line, col, code)` gives
  latest wins per finding site for free, and makes the per source refresh a
  consequence of the key rather than a step in the emitter.
- Emission itself is a bind, not program text: `bind diag_sink = sqlite { ... }`
  writing the same view. The program never names sqlite3, and the batching law
  (one process per tick) lives in the bind, where it can be enforced once.

Tier order finding: this lab needs nothing above `{ground_terms, rule,
external_rel}` plus one shell bind. It does not need keys, and it does not need
the effect envelope machinery, because the sink has no interesting reply. It
sits at the same tier as shell_stream's terminal case and can land before
register lowering. What it does need, and what it demonstrates the cost of
skipping, is the `<-` arrow: the entire delete plus reinsert dance in
`emit_diags/3` exists only because the emitter maintains membership by hand.

## 4. Limitations

1. Self reference. A lint whose rule table names the forbidden words cannot be
   written in plain text without flagging itself. Here the table is assembled
   from halves at load time, and a waiver marker skips a line. The live run does
   flag `v6/prolog/labs/LANG.md` lines 89 and 90 (1 based), which is the law text
   that defines the ban. A real deployment needs the waiver marker on those two
   lines, or a rule scoped to exclude the file that declares the rule. This lab
   does not modify LANG.md.
2. Check 2 is a substring test for `go :-`, not a clause head test. Measured
   consequence on the live tree: `src/grader.pl` passes only because line 2 of
   its comment block quotes the string. `src/checks.pl` and `src/kernel.pl` are
   flagged even though they are libraries and were never meant to be scored. The
   honest version reads clauses with `read_term/3` and asks whether a clause
   with head `go/0` exists, and scopes the rule to `labs/`.
3. One sqlite3 process per emit means the whole script goes to stdin before
   stdout is drained. That is deadlock free only because sqlite3 writes nothing
   while consuming DDL and INSERT statements. A future emit that mixes writes and
   large SELECTs in one script must read and write concurrently.
4. Query results are parsed by splitting sqlite3's default list output on `|`.
   A message containing a pipe character would corrupt the parse. Apostrophes are
   handled (doubled on the way in, graded by `apostrophe_round_trip`); pipes are
   not.
5. Poll latency is 500ms, fixed (src/lsp.rs:515). The editor cannot show a
   finding sooner, and the writer gets no signal that a poll happened.
6. Refresh granularity is the source path. An emit that scans one file cannot
   clear rows for a file it did not scan, which is correct, but it also means a
   deleted source file keeps its rows forever unless something explicitly emits
   an empty scan for that path.
7. Column offsets are prolog string offsets, which are code points. The LSP
   specification counts UTF-16 code units by default. The two agree for the em
   dash and for everything else on the basic plane, and diverge for astral
   characters such as emoji.

## 5. Numbered ambiguities

1. Line origin. The task brief said the missing entry point diagnostic is
   "anchored line 1", but `src/lsp.rs:395-398` and `:598-602` pin the view as
   0 based with no adjustment. This lab emits `line = 0`, which is the first line
   of the file as the editor draws it. If a future writer treats the view as
   1 based, every squiggle lands one line low and nothing errors.
2. `code` and `msg` are `String` in the rust struct (src/lsp.rs:406-407), not
   `Option<String>`, so a NULL in either column makes `row.get()` fail and the
   whole poll cycle error out, log, and drop the connection (src/lsp.rs:502-513).
   The view does not defend against this; only `severity` gets a COALESCE
   (v6/dl/src/5_diag.ts:41). Writers must never leave `code` or `msg` NULL. This
   lab always fills both.
3. The VSCode extension cannot pass `--diag-db`. `extension.ts:203` builds
   `args` from `dl.program` alone. Two options exist, neither performed here:
   add a `dl.diagDb` setting on the extension side, or run
   `dl --lsp --diag-db <path>` outside VSCode against another LSP client. The
   README documents neither, because the flag postdates it.
4. The client's document filter has no `prolog` and no `markdown` entry
   (extension.ts:211-224). Diagnostics for `.pl` and `.md` paths are published by
   the server and then filtered out client side. Whether the fix belongs in the
   selector list or in a diagnostics only client is unresolved.
5. The README describes only the engine backed mode ("Squiggles appear on save,
   the engine reads disk"). In `--diag-db` mode nothing is read on save, and the
   trigger is a sqlite commit by an unrelated process. Same extension, opposite
   causality, one undocumented flag apart.
6. Table name. `src/lsp.rs` names only the view. The base table name `rel_diag`
   comes from the node side runtime (v6/dl/src/5_diag.ts). A third writer is free
   to name its table anything as long as `diag_v5` exists, which means the table
   name is convention, not contract.

## 6. Deviations from LANG.md

- LANG.md says the sqlite lowering is described in the verdict, not mocked in
  code. This lab is exempt by that same sentence: it is specifically about
  emission, so the sqlite write is the subject rather than a mock.
- The lab holds two `nb_setval` cells (the lab directory captured at load time,
  and the process call counter). Global mutable state is not in the spec's spirit;
  the counter exists so `one_sqlite_process_per_emit` can grade the batching law
  rather than trust it.
