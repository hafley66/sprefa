# v5 parity spelunk + git-fact diagnostics rail (lane/v5-parity-spelunk)

Two asks, both answered by writing `.dl6` programs that run rather than by an
agent reading v5 and writing down what it saw. Zero new syntax, zero compiler
edits, zero extractor edits.

| artifact | what it is |
|---|---|
| `v6/dl/fixtures/v5-parity.dl6` | task 1 — the parity table, derived every tick from v5's own catalog + v5's source + v6's registry |
| `v6/dl/fixtures/v5-git-diags.dl6` | task 2 — the git-fact diagnostics rail, three finding classes over `enumerate` minus `enumerate_at` |
| `v6/tsv2/scripts/parity-grep.py` | the ONE helper: a generic `<rev> <pathspec> <regex>` → JSONL line-regex host executable |
| `v6/tsv2/scripts/v5-parity.sh` | task 1 receipt; writes `plans/2026-07-30-v5-parity-table.tsv` |
| `v6/tsv2/scripts/v5-git-diags.sh` | task 2 receipt; scratch git repo, four commits, red→green |
| `plans/2026-07-30-v5-parity-table.tsv` | the committed parity table |

Everything runs hermetically: `DL_STATE_DIR` and `--db` point into a `mktemp`
directory for the v5 legs, the v6 leg is `:memory:`, and no daemon is contacted.

---

## 1. The scan() reconciliation, which is the headline number

The v5-utility review recorded **"scan() used in 105/129 examples"**. The
program computes **108/132**. Both are right; they are different file sets, and
the difference is a glob-semantics divergence this repo has already pinned once
(the flagship rail asserts the same thing about `src/**/*.rs`).

| spelling | files | using `scan(` | semantics |
|---|---|---|---|
| `:(glob)examples/*.dl` | 129 | **105** | `*` stops at `/` — what a shell glob selects |
| `examples/*.dl` | 132 | **108** | git pathspec default: `*` **crosses** `/` |

The three extra files are `examples/lints/rust.dl`, `examples/lints/ts.dl`,
`examples/trace-diag-demo/trace-diag.dl` — all three in subdirectories, and all
three use `scan`, so both the numerator and denominator move by exactly 3.

**Verdict: the review's number is correct and reproduces exactly**, under the
shell-glob reading of `examples/*.dl`. Nothing was stale. The program computes
both spellings on purpose and the receipt prints both, because a parity count
whose file set is ambiguous is not a number.

The shipped rails are a separate and stronger number: **`scan` appears in 29 of
the 33 `.dl/*.dl` rails**. Examples can be rewritten; the rails are what the
daemon actually runs, so that 29 is the real weight behind porting `scan`, and
it is why the program counts the two corpora separately instead of pooling
them.

---

## 2. The v5 inventory, and what a line regex does and does not see

v5 is **self-describing**: `op_catalog`, `fn_catalog` and `rel_catalog` are
built-in relations the engine fills from `op_docs()`, `fn_docs()` and
`all_builtin_decls()` (`src/rels/catalog.rs:44`). So the authoritative
inventory is obtained by *asking v5*, through an `sh` host running a one-line
`.dl` query. That is leg **L1** and everything else is graded against it.

**L1, v5's own answer: 28 ops · 16 scalar functions · 112 built-in relations.**

| leg | source | rows | vs L1 |
|---|---|---|---|
| L1 oracle | `? op_catalog(...)` | 28 | — |
| L2 regex | `op_docs()` table, `src/engine/decls.rs:222-254` | 28 | exact |
| L2 regex | parse dispatch ladder, `src/parse/mod.rs:596-637` | 14 | subset by design (source + graph ops only) |
| L3 doc | `docs/reference/syntax.md` | 28 | exact |
| L1 oracle | `? fn_catalog(...)` | 16 | — |
| L2 regex | `fn_docs()` table, `src/engine/decls.rs:165+` | 16 | exact |
| L3 doc | `docs/reference/functions.md` | 16 | exact |
| L1 oracle | `? rel_catalog(...)` | 112 | — |
| L2 regex | single-line `RelDecl { name: "…"` over `src/*.rs` | 92 | **−20** |
| L3 doc | `docs/reference/relations.md` | 108 | **−4** |

### Finding A — the line regex misses 20 of 112 built-in relations, for the reason v5 documents

`RelDecl` is constructed two ways: `RelDecl { name: "changed".into(), … }` on
one line, and `vec![RelDecl {` with `name:` on the *next* line. A line regex
sees only the first. This is not a defect in the regex; it is the exact
limitation v5's own `match_line` documentation states about itself:

> LINE REGEX over file content — for FLAT TEXT (ini/env/log/csv) only, never
> structured source code (a construct spanning more than one line will not
> match; use match_ast for source)

so the parity rail reproduces v5's own warning as a measured number. The lesson
transfers directly: **v6 has no `match_ast` equivalent** (`sg_pattern/3` is a
registry row with status `refused`, slot `slot_sg_metavariable_semantics`), so
a v6 rail that needs a multi-line structural match today has no spelling for it
short of shelling out to the fixed extractor.

### Finding B — a generated v5 doc has drifted from the engine, and it says not to hand-edit it

`docs/reference/relations.md` is headed *"Generated from the engine's
`rel_catalog` by examples/gen-reference.dl. Do not hand-edit."* It carries 108
of the live 112. Missing:

```
git_ref   rev_behind   type_lgg   type_shape
```

Nothing is present in the doc that is absent from the catalog, so this is a
one-directional staleness: the generator has not been re-run since those four
relations landed. `syntax.md` and `functions.md` are both current. This is a
real v5 defect found mechanically by a v6 program, and it is the kind of thing
the parity rail is worth having for — a doc that lies is worse than no doc, and
nothing else in either repo checks it.

### Finding C — op arity is not a number, and rel arity is not reachable in-program

The brief asked for "op names + arities". v5's `op_catalog` has **no arity
column**, and it should not: `scan` is 2-, 3-, 4- or 5-ary and `match_line`
takes up to three optional trailing arguments, so the catalog carries a
`syntax` sketch instead. The parity table therefore carries the syntax string,
not an arity.

For relations the arity *is* well defined (`cols.len()`), and
`docs/reference/relations.md` renders it as a parenthesised column list, e.g.
`(harness, session, idx, path)`. Turning that text into `4` needs a string
split — and **v6 has no string functions at all**. That is consistent with the
landed sqlite-udf-graft verdict (`@libsql/client` 0.17.4 exposes no UDF
registration API; `split`/`replace`/`replace_re` are three of v5's 16 and are
among the most used). So the column list stays text. **Named inexpressible.**

---

## 3. The parity table

`plans/2026-07-30-v5-parity-table.tsv`, regenerable with
`bash v6/tsv2/scripts/v5-parity.sh`.

Status is **derived, never stated**. A marker comment inside the program says
only *"this v5 thing corresponds to that v6 name"*; the word covered / partial /
absent then comes from which of four v6 catalogs the name resolves in and what
`registry.pl` says about it. A construct that gets refused in v6 tomorrow flips
its row by itself.

- **covered** — the bridged v6 name is a `live` registry construct, a shipped
  `sh`/`bind` declaration, a rel a shipped `.dl6` program declares, or core
  grammar
- **partial** — the bridged v6 name exists but the registry marks it `refused`
  or `reserved`
- **absent** — nothing claims a v6 counterpart

The `rel` family needs no markers at all: a v5 built-in relation is covered
exactly when a shipped v6 program declares a rel of the same name — a pure name
intersection, derived directly. Note what that means: **v6 has zero built-in
relations by the `spine_residency` ruling**, so a covered relation is covered
*by a program*, never by the engine.

**Self-inclusion, checked rather than waved off.** Two of the four v6 catalogs
are globbed out of `v6/dl/fixtures/*.dl6`, which now includes this lane's own
two programs, so a marker could in principle be satisfied by a name this lane
itself introduced. Measured: dropping both new files takes the host catalog
from 27 to 22 names and the program-rel catalog from 138 to 82, and **every one
of the 18 markers still resolves to exactly the same catalog and status** —
`enumerate` is `enumerate-hosts.dl6`'s and `diag_v5` is `diag-rail.dl6`'s. No
row in the table depends on this lane existing.

Receipt, run 2026-07-30 (`bash v6/tsv2/scripts/v5-parity.sh`, exit 0, 8m55s
wall). Settled vector, engine-side:
`v5_op:v5_fn:v5_rel:src_op:src_rel:doc_rel:parity_row:usage_count:bridge = 28:16:112:28:92:108:156:141:18`.

**156 rows — 28 ops, 16 functions, 112 built-in relations.**

| family | covered | partial | absent |
|---|---|---|---|
| op | 14 | 2 | 12 |
| fn | 0 | 2 | 14 |
| rel | 6 | 0 | 106 |
| **all** | **20** | **4** | **132** |

The ops, in full, by usage over the 129-file corpus:

| op | files | status | |
|---|---|---|---|
| `scan` | 105 | covered | `enumerate` host |
| `diag` | 33 | covered | `diag_v5` rel, shipped by `diag-rail.dl6` |
| `gen` | 24 | **absent** | no codegen sink in v6 |
| `comment` | 20 | **absent** | marker regions — the op this lane itself wanted |
| `match_line` | 19 | covered | via `sh_decl/4`, i.e. shelling out |
| `closure` | 17 | **absent** | graph-algo queue item |
| `jsonp` | 14 | covered | `decode/2` |
| `match_ast` | 14 | **partial** | `sg_pattern/3` is `refused` |
| `ast` | 12 | covered | `ts_query/1` |
| `json` | 9 | covered | `decode/2` |
| `scc` | 9 | **absent** | graph-algo queue item |
| `arith` `ast_yaml` `atom` `cmd` `comparison` `glob` `graph_edge` `graph_node` `hover_note` `match` `node2vec` `query` `regex` | ≤2 | mixed | |
| `aggregation` `negation` `sg` `strfn` | 0 | mixed | |

**The two numbers worth acting on: `gen` (24 files) and `comment` (20 files)
are the highest-usage constructs with no v6 spelling at all**, and `match_ast`
(14 files) is the highest-usage one that is refused rather than missing. Every
op above `match_line` in that table is either covered or one of those three.

Functions are the starkest column: **zero of sixteen are covered.** `replace_re`
(16 files) and `split` (14) lead, and `json_array`/`json_object` are `partial`
only because the registry carries them as *refused* aggregate heads. This is
the sqlite-udf-graft verdict showing up as a usage count.

Relations: 6 of 112 covered, all six by name-collision with a rel some shipped
`.dl6` program happens to declare (`file`, `call_site`, `df_node`, `df_arg`,
`df_edge`, `df_param`). The highest-usage absent relations are `diag` (33),
`type_entity` (22), `call_edge` (20), `call_def` (17), `type_edge` (15).

Note `diag` appears twice with different verdicts — **op `diag` covered, rel
`diag` absent** — and that is the family split doing its job. The v5 *sink op*
has a v6 answer (head an ordinary rel named `diag_v5`); the v5 *built-in
relation* does not, because v6 ships no built-in relations. Keying status on
the bare name would have collapsed those two true statements into one false
one, which is exactly what the first draft of this program did.

---

## 4. The git-fact diagnostics rail

`v6/dl/fixtures/v5-git-diags.dl6`, receipt `v6/tsv2/scripts/v5-git-diags.sh`.
Four commits in a scratch repository, row-set equality at every stage.

```
c0  src/existing.ts   one eval(, Owner header      -> the ratchet baseline
    src/clean.ts      no eval, Owner header
c1  + src/new_bad.ts  one eval(, NO Owner header   -> classes (a)+(b)+(c) RED
    + src/new_ok.ts   no eval, Owner header        -> the CONTROL: new, no finding
c2  - src/new_bad.ts                               -> (a),(b),(c) GREEN
c3  base advances to c2                            -> new_file empties
```

Receipt, run 2026-07-30 (`bash v6/tsv2/scripts/v5-git-diags.sh`, exit 0):

```
PASS  c0 built: 2 files, ratchet baseline measured at 1 hit
PASS  program loaded, hosts: "enumerate_at","enumerate_at_ws","grep_at"
PASS  stage 0  new_file empty (base == head)
PASS  stage 0  diag_v5 empty (GREEN)
PASS  defect   JSON host reads c0's two files as TWO ROWS (correct)
PASS  defect   whitespace host MIS-DECODES the same answer into 1 row, silently:
        src/clean.ts 5fa58c9e88f635d2dc9ddc88d8ee24b9d795781e
        (serve/1_hosts.ts parseWhitespace: lines.length === outputs.length)
PASS  stage 1  new_file = the two files c1 added
PASS  stage 1  class (a) banned pattern, at a REAL 0-based line
PASS  stage 1  class (b) missing Owner header, ONLY on the file lacking it
PASS  stage 1  class (c) ratchet rose above the baseline
RED   all three classes fire at c1; the control file src/new_ok.ts is new and carries ZERO findings
PASS  stage 2  new_file = the surviving new file only
PASS  stage 2  classes (a) and (b) RETRACTED
PASS  stage 2  class (c) ratchet RETRACTED (count back to the baseline)
GREEN all three classes retracted on the fixing commit
PASS  stage 3  new_file empty once the base advances to head
PASS  stage 3  diag_v5 empty (GREEN)
PASS  defect   at THREE files the two encodings agree again (the bug is data-dependent)
PASS  ratchet  lowering the committed ceiling to 0 fires the rail on an unchanged tree
GIT-FACT DIAGS RAIL HOLDS
```

The last assertion is the ratchet's other half: stage 1 proved it fires when the
*count rises*, and lowering the committed ceiling to 0 on an unchanged tree
proves the ceiling is read from the row rather than compiled in.

**Sabotage receipt, run and reverted 2026-07-30.** Replacing class (b)'s
`not(owner_marked(path))` with a bare `owner_marked(path)` inverts the rail:

```
FAIL  stage 1  class (b) missing Owner header, ONLY on the file lacking it
      expected:  new-file-missing-owner  src/new_bad.ts  0
      actual:    new-file-missing-owner  src/new_ok.ts   0
```

Both spellings emit exactly **one** row at stage 1, so a count assertion would
have passed the sabotage. That is why every assertion in the receipt compares
sorted row sets, and why the corpus carries a control file at all.

### The `new_file` shape the brief asked for, and why it is two steps

The brief writes it as one rule:

```
new_file(path) <- enumerate(...), not(enumerate_at('BASE_REV', ...)).
```

That exact line is **refused, correctly**. `not/1` in a level body lowers to
`NOT EXISTS` over one plain relation atom, and a host call is not one: a probe
expands to a demand atom plus a keyed response atom, so there is no single
table for the `NOT EXISTS`. Routing each side through its own rel first is the
whole fix and costs two rels. Not a workaround — the refusal is naming a real
distinction between a stored relation and a demanded one.

### Both sides are rev-pinned, and that is forced by the cache

`enumerate(glob)` takes its witness from the glob **alone**, so it answers once
and caches for the life of the db (its own declaration in
`fixtures/enumerate-hosts.dl6` says so; freshness there is the watcher's job).
A rail that must show a diagnostic *appear* and then *retract* across commits
cannot ask a worktree host twice — it would get the same cached answer both
times and the retraction would be invisible. So the current side is pinned too,
and advancing it is what a commit does. Every answer is immutable, each commit
is a new witness by construction, and the cache is exactly right rather than
worked around. The same reasoning is why `parity-grep.py` takes a rev and reads
blobs out of the object database.

### Line numbers are real here, unlike every other shipped v6 rail

`flagship-callgraph.dl6` and `diag-rail.dl6` both write `line = 0` because
`sprefa-extract` emits byte spans as a **nested** JSON object and the sh-host
decode is a projection over top-level keys. A line-regex host has no such
problem: the line number is a top-level int column. So class (a) reports a real
line, and the 1-based → 0-based conversion v5's `diag` schema wants is a `:=`
bind written in the open. This is the first v6 rail whose `diag_v5` rows carry a
position.

---

## 5. Named inexpressibles, refusals, and defects

Ordered by how much they cost a cold author.

### D1 — a backslash in a `.dl6` string constant is silently deleted (defect)

Two independent lossy passes:

1. **parse time.** `parse_dl.pl:301`'s catch-all `escape_code(C, C)` maps any
   unknown escape to the bare character, so `\s` becomes `s` and the backslash
   is gone.
2. **module load.** `lower.pl` splices the constant into an emitted TypeScript
   **template literal** and does not escape backslashes, so JavaScript's own
   escape processing eats a second one.

Measured end to end, reading the host's demand row back off `/idb`:

| written in the `.dl6` file | reaches the host |
|---|---|
| `\s` | `s` |
| `\\s` (the correct SWI spelling of one backslash) | `s` |
| `\\\\s` | `\s` |
| `\\\\\\\\s` | `\\s` |

No refusal, no diagnostic, no trace line. The emitter **does** escape `${` (an
emitted literal `dollar${process.env.HOME}end` renders as `dollar\${…}end` and
does not interpolate), so this is a *partial* escape rather than an absent one —
the injection hole is closed and the correctness hole beside it is open.

Consequence: a regex cannot be written normally in this language. Every pattern
in both programs is written with character classes instead — `[(]` for `\(`,
`[0-9]` for `\d`, `[ ]` for `\s`, `(?<![a-z_0-9])` for `\b` — which happens to
be readable, but is a workaround, not a style.

### D2 — `parseWhitespace` mis-decodes N rows as one row of N columns (defect, live, shipped)

`serve/1_hosts.ts`:

```js
if (outputs.length > 1 && lines.length === outputs.length) {
  return [outputs.map((column, index) => coerce(host, column, lines[index] ?? ""))];
}
```

The heuristic is deliberate and documented — ghcacher's `printf '%s\n%s\n%s'`
templates emit one row as one value per line. But it is **data-dependent**: a
two-column host answering with exactly two rows is indistinguishable from a
two-column host answering with one row, and the decoder picks the second
reading.

**The shipped `enumerate_at` is a two-column whitespace host**, so over a
revision holding exactly two matching files it returns one garbage row whose
`path` is the first whole line and whose `digest` is the second whole line.
Silently. Every downstream join then derives nothing.

This is why the rail's `base_file` was empty on the first run. The shipped
`enumerate.sh` receipt cannot see it: it runs against this repository, where the
glob matches hundreds of files, so `2 == 2` never holds.

Not patched — the runtime is not this lane's to edit. Instead the rail's own
`enumerate_at` emits a named JSON projection (which `decodeObjectItems` reads by
column name and no row count can confuse), and the whitespace twin is kept in
the program as `enumerate_at_ws` **purely as the defect's witness**, with the
receipt asserting both halves:

- at 2 files the two encodings disagree, the whitespace one collapsing to 1 row
- at 3 files they agree again — proving the bug is data-dependent, not constant

Suggested fix for whoever owns `1_hosts.ts`: make the one-row-per-invocation
reading opt-in from the declaration (a host that means it says so) rather than
inferred from a row count that the world controls.

### D3 — compile time is superlinear in program size

`bop check` on prefixes of `v5-parity.dl6` as first written (117 statements),
same machine, cold each time, each run to completion:

| statements | compile |
|---|---|
| 60 | 7.9s |
| 70 | 18.1s |
| 78 | 23.3s |
| 82 | 26.9s |
| 84 | 29.7s |
| 85 | 31.9s |
| 86 | 34.7s |

Roughly 4.4× the time for 1.4× the statements. Nothing loops — every prefix
terminates — but the whole 117-statement file ran past **232 seconds** without
finishing (that run was interrupted, so 232s is a lower bound, not a compile
time). This was initially mistaken for a hang, twice, and it is worth naming
exactly *because* it looks like one: a 30-second timeout that is generous for
every shipped fixture is not generous for a program of this size, so the first
diagnosis was wrong in a way that cost real time.

**It changed the shipped program.** `v5-parity.dl6` is 95 statements, not 117,
because the four-clause `bridge_via` resolution gate and the three separate
per-family usage aggregates were removed on compile-cost grounds — the gate
moved into `v5-parity.sh` (it is plumbing, not a finding) and the three
aggregates collapsed into one over a union rel. That is a language cost
showing up as a design decision, which is the honest way to report it.

The trimmed 95-statement program still takes **~6.5 minutes** to compile
server-side (measured inside the 8m55s receipt run, uncontended), so the trim
bought correctness of the table rather than a comfortable compile. For scale:
`examples/*.dl` in v5 routinely reaches this size, so any port of the rail
corpus meets this wall.

### D4 — `quote_in_literal`: a single quote cannot appear in a string constant

`unsupported_construct(quote_in_literal("…'…"))`, a *named* refusal, so it is
honest. But `registry.pl` spells operator functors as quoted atoms
(`surface('=='/2, …)`), so no pattern can name the quote it would need to strip.
The parity program captures the whole signature field (`'=='/2`) rather than
splitting name from arity, which costs nothing here but would block a rail that
needed to normalise quoted identifiers.

### D5 — `aggregate_group_not_delta_local`: an aggregate cannot be grouped by another rel's column

The natural spelling of a config-scoped count:

```
ratchet_count(code, count(line)) <- baseline(code, _), banned_hit(path, line).
```

is refused: the grouping key comes from a rel other than the one being
aggregated, so the group a delta belongs to is not decidable from the delta.
The refusal is right. The cost is that *"count, scoped by a config row"* becomes
two steps — a whole-rel count, then a join that brings the ceiling in at the
reading rule. Worth a worked example in SYNTAX.md, since the ratchet shape is
exactly what the standing no-new-eprintln law is made of.

### D6 — `not/1` cannot wrap a probe

Named above under the rail. Every negated host answer needs its own rel first.

### D7 — a program that reads its own marker comments must be git-tracked

The bridge markers are extracted with `git ls-files`, so an untracked program
yields an empty bridge and a table that scores everything `absent` — silently.
The receipt's first assertion is `git ls-files --error-unmatch` on the program
itself. Worth knowing before anyone else builds a self-describing rail.

### D8 — no string functions, so a rendered column list cannot become an arity

See finding C. Three of v5's 16 scalar functions are string splitters and they
are among the most used; v6 has none, and the UDF verdict says the current
driver cannot register any.

---

## 6. What this says about the porting backlog

The parity table's `absent` rows are the backlog, and they cluster:

- **structural matching** — `match_ast` / `sg` / `ast_yaml`. `sg_pattern/3` is
  `refused` in the registry, so this is `partial` at best and blocks exactly the
  multi-line-construct case finding A measured.
- **graph algorithms** — `closure`, `scc`, `node2vec` have no v6 spelling at
  all, and they are the standing graph-algo queue item.
- **codegen and the drawable sinks** — `gen`, `graph_node`, `graph_edge`,
  `hover_note`.
- **scalar functions** — 14 of 16 absent, gated on the UDF driver decision.
- **comment-marker regions** — `comment`, which is the one v5 op this lane
  actually wanted (marker extraction) and had to re-implement as a line regex.

The rows with high usage counts and `absent` status are the ones worth pricing
first; the table sorts by family, then status, then usage descending so that
read is the default one.
