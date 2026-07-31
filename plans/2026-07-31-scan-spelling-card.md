# The `scan()` design card

Base `009d8969`. Analysis only: no code, no fixtures, no justfile touched.

`v6/READINESS.md` and the v5-utility review both name `scan()` as the largest
v5-migration item (105 of 129 example files). This card does the census first,
maps every shape onto what v6 already ships, and turns only the residue into
ruling cards. Nothing below is a recommendation-as-fiat; every card ranks its
candidates against stated criteria and stops.

---

## 0. Summary table — shapes x counts x status

Census method: every `scan(` occurrence in `examples/*.dl` at this base, parsed
with a paren/quote-aware splitter, comment mentions excluded, arities normalised
to the 5-ary form `scan(repo, rev, glob, path, rev_out)` using the defaults
`README.md:320` documents (`repo="."`, `rev="WORK"`).

**242 code call sites across 105 of 129 files.** 9 further mentions are comment
prose and are excluded. Arity split: 4-ary 192, 2-ary 31, 3-ary 12, 5-ary 7.

| # | shape | v5 spelling (verbatim from the corpus) | sites | v6 status |
|---|---|---|---|---|
| A | worktree, `dir/**/` recursive | `scan("WORK", "src/**/*.rs", path, rev)` | 112 | **EXPRESSIBLE, two ways** — 3-line `watch` idiom or 5-line `enumerate` idiom |
| B | worktree, exact path | `scan("WORK", "README.md", path, rev)` | 69 | **EXPRESSIBLE** — same idioms, and the only bucket where both glob dialects agree |
| C | worktree, single `*` | `scan("WORK", "bin/*.mjs", path, rev)` | 30 | **EXPRESSIBLE-BUT-WRONG** — `git ls-files` `*` crosses `/`, v5's globset and node's matcher do not (card 2) |
| D | worktree, brace alternation | `scan("WORK", "src/**/*.{rs,ts,kt}", p, rev)` | 18 | **INEXPRESSIBLE via `enumerate`** — `git ls-files` returns ZERO rows for any brace glob (measured) |
| E | worktree, leading `**/` | `scan("WORK", "**/*.md", path, rev)` | 18 | **EXPRESSIBLE-BUT-WRONG** — git `**` excludes repo-root files, v5's does not (card 2) |
| F | pinned rev, literal | `scan("HEAD", "src/**/*.rs", path, rev)` | 9 | **EXPRESSIBLE** — `enumerate_at`, copy-pasted into 4 programs (card 4) |
| G | pinned rev from data | `pr_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev)` | 4 | **EXPRESSIBLE** — `want_at(rev, glob)` is exactly this |
| H | named repo coordinate | `repo(r,_,_), scan(r, "HEAD", "go.mod", p, rev)` | 6 | **PARTIAL** — host plane solved (`cd '{root}'`), watcher + server cwd are not (card 5) |
| I | fan over every configured repo | `scan("*", "HEAD", "**/*", p, rev)` | 1 | **INEXPRESSIBLE as one program** — this is the org fan-out gap (card 5) |

Buckets are exclusive and sum to 242. Slot occupancy behind them:

| slot | value | sites |
|---|---|---|
| `rev` | `"WORK"` 229 / `"HEAD"` 9 / variable 4 | 242 |
| `repo` | `"."` 235 / variable 6 / `"*"` 1 | 242 |
| `glob` | **literal in 242 of 242** (all three variable-glob mentions are comment prose) | 242 |
| `rev_out` | bound variable 210 / absent (2-ary) 31 / `_` 1 | 242 |

How the rules consume it, by companion source op in the same rule body:

| companion op in the same rule | rules | v6 mapping |
|---|---|---|
| none (pure file-set projection, `src(p) <- scan(...)`) | 95 (91 with an empty rest-of-body) | direct |
| `match_line` | 41 | `sh grep_at(...)` — shipped, `v5-git-diags.dl6` |
| `ast` | 38 | `sh` over `sprefa-extract` — shipped, spans still blocked (`compound_storage` arc) |
| `comment` | 35 | `sh comment_fact(...)` + python sidecar — shipped, 6 `comment-*-rail.dl6` fixtures |
| `match_ast` | 22 | same as `ast` |
| `jsonp` / `json` | 8 | `decode`/json family, partly refused |
| `ast_yaml` | 2 | no v6 spelling |
| `cmd` | 1 | `sh` host |

**The headline the census produced, which the READINESS row does not say:**
`scan` itself is not the blocker. Shapes A/B/F/G — 194 of 242 sites, 80% — are
expressible today with zero new constructs. The blocker is one live defect and
four spelling questions underneath it.

---

## 1. What v6 already ships

Three mechanisms, all landed, all in-language per ruling `spine_residency`.

**(i) `bind watch(glob, path, digest)`** — `v6/tsv2/serve/2_binds.ts`. Declared
identically in 13 fixture programs. It does *both* halves of `scan("WORK", …)`:
`GlobWatch.bootBatch` enumerates the tracked worktree through
`git ls-files -z -- <glob>` at subscribe (2_binds.ts:248, :281) and reconciles
against durable rows, then `batchFor` publishes live changes filtered with
`path.matchesGlob` (2_binds.ts:314). The glob must be a **literal in the program
text** — `lower.pl` collects literals into the plan and "a bind that reads no
literal gets `literals: []` and therefore no live source" (2_binds.ts:16).

**(ii) `sh enumerate(glob) -> (path, digest)`** — `v6/dl/fixtures/enumerate-hosts.dl6:45`.
One-shot, `git ls-files` + `git hash-object` per file. The glob arrives as a
**demand row** (`want(glob)`), not as program text. Its own header states the
cache story: "the witness is the glob alone, so the answer caches for the life
of the db. Worktree FRESHNESS is the watcher's job."

**(iii) `sh enumerate_at(rev, glob) -> (path, digest)`** — same file, line 60.
The rev pins the answer so the witness caches forever, correctly.

Worked cost, shape A, against v5's two lines
(`rel src(path: file, rev: text).` + `src(path, rev) <- scan("WORK", "src/**/*.rs", path, rev).`):

```dl6
# v6, via watch — 3 lines, live, glob in program text
bind watch(glob: text, path: text, digest: text).
rel file(path: text, digest: text).
file(path, digest) <- watch('src/**/*.rs', path, digest).
```

```dl6
# v6, via enumerate — 5 lines + one want() row posted to /arrivals out of band
sh enumerate(glob: text) -> (path: text, digest: text) =
  `git ls-files -- '{glob}' | while IFS= read -r entry; do printf '%s %s\n' "$entry" "$(git hash-object -- "$entry")"; done`.
rel want(glob: text).
rel file(path: text, digest: text).
file(path, digest) <- want(glob), enumerate(glob, path, digest).
```

rx lowering of the `watch` form, per the standing snippet law (this is the
shipped shape, quoted from `extraction-live.dl6:16`):

```ts
watchSource(root).pipe(
  bufferTime(coalesceMs),
  map(diffAgainstLastDigest),                 // -> IArrivalRow[] with signs
  concatMap(batch => engine.submit(batch)))
```

The `rev` column disappears and a `digest` column appears. That is not a loss:
v5's `rev_out` is bound in 210 of 242 sites but the corpus uses it almost
entirely as a pass-through into `match_line(p, rev, …)` / `ast(path, rev, …)`,
which in v6 is the content-addressed host witness. Same role, better identity.

---

## 2. THE LIVE DEFECT the census found

`bind watch` uses **two different glob dialects on its two halves**, and they
disagree on 170 of the corpus's 242 globs (70%).

Measured at this base, in this repo:

| glob | boot half (`git ls-files -- glob`) | live half (`path.matchesGlob`) | agree |
|---|---|---|---|
| `src/**/*.rs` on `src/lib.rs` | **NO** — 0 of 145 rows are direct children of `src/` | YES | ✗ |
| `src/**/*.rs` on `src/a/b.rs` | YES | YES | ✓ |
| `src/*.rs` on `src/lib.rs` | YES | YES | ✓ |
| `src/*.rs` on `src/a/b.rs` | **YES** — 145 of the 200 rows are nested | NO | ✗ |
| `v6/**/*.{ts,pl}` on `v6/tsv2/serve/x.ts` | **NO** — returns 0 rows total | YES | ✗ |
| `**/*.md` on `README.md` | **NO** — 0 root-level `.md` of 836 rows; `*.md` returns 845 | YES | ✗ |

Corpus exposure by bucket:

| bucket | sites | what goes wrong |
|---|---|---|
| `dir/**/…` | 104 | boot silently drops every direct child of `dir/`; editing one makes it appear |
| exact path | 72 | agree |
| single `*` | 30 | boot admits nested files the live half will never update or retract |
| brace `{a,b}` | 18 | boot returns zero rows for the whole glob |
| leading `**/` | 18 | boot drops repo-root files |

The last row is reproducible from `v6/GETTING-STARTED.md` itself, whose tutorial
glob is `**/*.md` (line 159): a reader who creates a root-level `.md` gets no row
at boot, a row on edit, and — because `bootBatch` reconciles durable rows against
the tracked set — a `del` on the next restart. The file flickers.

The v5 dialect is globset: `*` does not cross `/`, `{a,b}` works, `**` matches
zero directories. Node's `path.matchesGlob` agrees with v5 on **all six** rows
above. `git ls-files` pathspec agrees on one. The live half is already
v5-compatible; the boot half is the divergent one.

This is a defect, not a design question, and it is prior to every card below —
any answer that keeps `git ls-files` as the matcher inherits it. It is also
already half-known: `flagship-callgraph.dl6:73` states the globset-vs-pathspec
divergence and the rig asserts the two file sets equal before grading, which is
why no gate has caught this. That assertion is on the `enumerate_at` path, and
it compares v5's answer to v6's *boot* answer only.

---

## 3. The ruling cards

Criteria used to rank every card, stated once:

| C1 | construct budget | 0 new constructs > sugar over shipped ones > new construct |
| C2 | vocabulary law | rxjs / prolog / SQL words, with their rxjs / prolog / SQL meanings |
| C3 | migration reach | how many of the 242 sites the option unblocks |
| C4 | defect closure | does it close section 2 |
| C5 | `spine_residency` | git/fs stays hosted in-language, never kernel |
| C6 | line cost | per program, against v5's 2 lines |

---

### CARD 1 — `SLOT-SCAN-NAME`: is there a word `scan` at all?

The obvious migration aid is to give v6 a construct spelled `scan`. Two
collisions say no, both live in this tree:

- **rxjs `scan` is the accumulator operator**, and it is used in v6 —
  `v6/tsv2/serve/2_binds.ts:373`, inside the watch bind's own pipeline, and in
  the multirepo golden's documented lowering (`0_multirepo_crawl.dl6:51`,
  `mergeMap(g => g.pipe(scan(minMax, seed)))`). A construct named `scan` that
  enumerates files would mean the word has two unrelated meanings inside one
  file.
- **SQL `SCAN` is the full-table-scan verdict in `EXPLAIN QUERY PLAN`**, and
  "SEARCH-not-SCAN" is a standing repo law (`CLAUDE.md:894`), asserted in
  `v6/tsv2/tests/relationDepth.test.ts:187` and `scripts/1_p1-receipts.ts:268`.

This is finding B8 of the language design review (`combine`, `finalize`, `pre`,
`keep`) arriving before the word is spent rather than after.

| candidate | C1 | C2 | C3 | C6 | note |
|---|---|---|---|---|---|
| **(a) no word — keep `watch` + `enumerate`** | 0 new | clean (`watch` is not an rx/SQL word in conflict; it is the fs verb) | 194/242 today | 3 or 5 | status quo; cards 2-5 still apply |
| **(b) `scan` as an alias** | 0 new (sugar) | **violates** — two live conflicting meanings in-tree | 194/242 | 2 | migration-friendliest, vocabulary-worst |
| **(c) one name for the family, e.g. `enumerate` for all three** | 0 new | `enumerate` is neither rx nor SQL; it is the shipped word | 194/242 | 3-5 | collapses card 3 into a naming call |
| **(d) `ls` / `files`** | 0 new | `ls` is a shell word not an rx/prolog/SQL word | 194/242 | 2-3 | short, but a fourth vocabulary |

Ranking by C2 then C3: (a) ≈ (c) > (d) > (b). (b) ranks last on the only
criterion the standing law makes non-negotiable, and first on migration comfort.
That trade is the ruling.

---

### CARD 2 — `SLOT-GLOB-DIALECT`: one matcher, which one? (closes section 2)

| candidate | C4 | C3 | C1 | cost |
|---|---|---|---|---|
| **(a) node `matchesGlob` everywhere** — boot enumerates with a broad pathspec (the literal prefix before the first magic char, which `watchRootOf` already computes at 2_binds.ts:192) then filters with the same matcher the live half uses | **closes it** | 242/242, and every v5 glob ports byte-unmodified | 0 new constructs, one function call moves | one extra `git ls-files` breadth; `trackedPaths` already exists |
| **(b) git pathspec everywhere** — live half switches to a pathspec matcher | closes it | 72/242 port unmodified; 170 globs get rewritten by hand and 18 brace globs become N globs | 0 new | rewrites the corpus; contradicts `flagship-callgraph.dl6`'s own stated v5 semantics |
| **(c) keep both, document the seam** | no | 72/242 correct, 170 subtly wrong | 0 new | cheapest today, and section 2's flicker stays shipped |
| **(d) desugar braces at compile time into N rules, keep git for the rest** | partial — closes bucket D only | 90/242 | sugar (one expansion pass, the enum/match precedent) | leaves buckets C and E wrong |

C4 is the deciding column and only (a) and (b) satisfy it. Between them C3 is
decisive by 170 sites. (d) is a real option only as an addition to (b).

Note for whichever wins: `enumerate` and `enumerate_at` are `sh` templates
running `git ls-files` in program text, so (a) does not reach them — they stay on
pathspec unless card 4 moves them off copy-paste first. That coupling is why
card 4 is not independent.

---

### CARD 3 — `SLOT-FEED-RESIDENCY`: one feed or two?

`watch` and `enumerate` overlap. Both answer "which files match this glob, with
what digest". They differ on three axes and no document ranks them:

| axis | `bind watch` | `sh enumerate` |
|---|---|---|
| glob source | program-text literal | demand row (`want(glob)`) |
| freshness | boot-enumerates AND streams changes | one-shot, caches for the life of the db |
| rev | worktree only | worktree; `enumerate_at` for a pin |

| candidate | C1 | C3 | C6 | note |
|---|---|---|---|---|
| **(a) keep both, write the rule for which** | 0 new | 194/242 | 3 or 5 | zero work, one doc paragraph; two things a cold author must choose between with no stated criterion |
| **(b) collapse onto `watch`, delete `enumerate`** | 0 new (a deletion) | loses shape G's data-driven glob and every rev pin | 3 | `enumerate_at` cannot be deleted, so the family does not actually collapse |
| **(c) collapse onto hosts, `watch` becomes freshness-only** | 0 new | 194/242 | 5 + 3 | the honest split (pull vs push) at the highest line cost; no fixture does this union today |
| **(d) one name, two variants — `enumerate(glob)` live / `enumerate_at(rev, glob)` pinned**, the ruled rev shape (`enumerate-hosts.dl6:3`, "optionality is spelled as variants") applied to freshness too | 0 new; a rename plus wiring `watch` under the live variant | 194/242 | 3 | the variant pattern this repo already ruled for rev; costs the `watch` name |

(a) is free and (d) is the one that removes the choice rather than documenting
it. (b) does not survive its own arithmetic.

---

### CARD 4 — `SLOT-FEED-REUSE`: `enumerate_at`'s two-line shell template is copy-pasted verbatim into 4 programs

`enumerate-hosts.dl6`, `flagship-callgraph.dl6`, `v5-git-diags.dl6`,
`flagship-flow.dl6` each contain a byte-identical
`sh enumerate_at(rev: text, glob: text) -> (path: text, digest: text) = …` and
say so in their own headers ("copied verbatim from fixtures/enumerate-hosts.dl6").
The `bind watch(glob: text, path: text, digest: text).` line appears identically
13 times; `rel file(path: text, digest: text).` 16 times; the
`file(path, digest) <- watch(…)` rule 10 times.

There is **no import, include, module or prelude construct** in `registry.pl` —
checked, the registry has no such row. Every program restates the feed.

| candidate | C1 | C5 | note |
|---|---|---|---|
| **(a) leave it** | 0 new | fine | the shell template is the thing most likely to drift; nothing gates cross-file identity today, and the `gen_staleness_gate` class is what happens next |
| **(b) a shipped prelude `.dl6` the loader concatenates** | 0 new constructs, 1 new loader behaviour | fine — a prelude in program text IS in-language | invisible names in the program a reader sees; the `magic-rel` hazard the hosts lab already refused, one level up |
| **(c) an `include 'path'` construct** | +1 construct, registry row, parse/print/grammar | fine | the rel-spreading lab already rejected `include a` for decls on a different ground (splice position); nothing carries here |
| **(d) rel spreading extended to `sh` decls** | sugar over a landed (unwired) design | fine | rel spreading is design-record-only and unwired; this would be its first consumer |
| **(e) a gate: assert every copy is byte-identical, keep the copies** | 0 new | fine | closes the drift risk, not the line cost; smallest correct if the cost is judged acceptable |

C1 orders (a) = (e) > (b) > (d) > (c). C6 orders them the other way. The
question the user is actually settling is whether 5 restated lines per program is
a cost or a feature (every program reads standalone, which is what `dl6` has
optimised for so far).

---

### CARD 5 — `SLOT-ORG-FANOUT`: root as a column

`READINESS.md:149` prices this: the v6 crawl leg is capped at 8 of 250 repos
because `crawl-bench.sh` supplies fan-out as a shell `while` loop, one server and
one db per repo, keyed by a `DL_CRAWL_REPO` env var read inside the shell
template (`crawl-bench.sh:113`). v5 writes the same thing as one program:
`src(p, rev) <- scan(r, "HEAD", "**/*.{go,ts,tsx}", p, rev), repo(r, _, _).`

It is the same family as `scan` because it is the same argument slot: v5's
`repo` is `scan`'s first column.

**What is already solved:** the host plane. `0_multirepo_crawl.dl6:59` proves
root-as-input-column works with zero new constructs —
`sh repo_grep_at(root, rev, glob, pattern) -> … = \`cd '{root}' && …\`` — and
READINESS row 5 grades that program READY, 4/4 rels byte-identical to v5. The
repo set arrives as `want_repo(slug, root, rev)` EDB rows instead of ambient
config, which the program's header argues is the better shape.

**What is not:** two things, both ambient rather than columnar.

1. `bind watch` has no root column. Its root is `options.root`, a server
   constructor argument (2_binds.ts:340, :354), fed by `TSV2_WATCH_ROOT`.
2. `sh` hosts inherit the server process's cwd — `0_multirepo_crawl.dl6:20`
   states it ("run git in the SERVER'S cwd, so they can only ever see one
   repository") and `GETTING-STARTED.md:184` states it to readers ("The server's
   working directory is the watch root").

So a v6 program can *pull* from N repos and cannot *watch* N repos, and shape I
(`scan("*", …)`, fan over every configured repo) has no spelling because
"which repos exist" is neither config nor a host anyone has written.

| candidate | C1 | C3 | C5 | note |
|---|---|---|---|---|
| **(a) `bind watch(root, glob, path, digest)`** — 4 columns, root from the program's own rules | 0 new constructs, +1 column on a shipped bind | shapes H+I, 7 sites | fine | needs N watcher instances from one bind plan; the plan already carries a literal list, so this is "literals become pairs" |
| **(b) root column on every fs host + server cwd stops being ambient** | 0 new | 7 sites, and removes an invisible input from every program | strongest — nothing is ambient | biggest blast radius; every shipped fixture gains a column or a default |
| **(c) status quo: N servers, shell federation** | 0 new | 0 | fine | what is shipped; the 84x parity gap keeps its stated cause |
| **(d) `sh repos(dir) -> (slug, root)` over `find -name .git`, plus (a) or (b)** | 0 new | shape I, and makes the repo set data | fine — literally `spine_residency` | the missing half of I; useless alone |

(a)+(d) is the smallest pair that answers both H and I. (b) is (a) generalised
and is the one that removes ambient state rather than adding one more column to
it. (c) is the honest do-nothing and keeps the READINESS number where it is.

---

### CARD 6 — `SLOT-BRACE-ALTERNATION` (dependent on card 2)

18 sites, all `src/**/*.{rs,ts,kt}`-shaped. Dies automatically under card 2 (a).
Under card 2 (b) or (c) it needs its own answer: **(i)** compile-time expansion
into N rules (the enum/match shared-expansion precedent, one pass in
`1_expansion.pl`), **(ii)** N explicit pathspecs on one host, **(iii)** a named
refusal so the current silent zero-row answer becomes loud. (iii) is strictly
better than the status quo at zero cost and is compatible with (i) and (ii).

---

## 4. Dependency order

Cards are not independent. The order forced by their own contents:

```
CARD 2 (glob dialect)  ← closes the live defect; every other card inherits it
   └─ CARD 6 (braces)  ← dies under 2(a), lives under 2(b)/2(c)
CARD 1 (the word)      ← independent, pure vocabulary
CARD 3 (one feed/two)  ← must be answered before CARD 4 knows what to share
   └─ CARD 4 (reuse)   ← what gets shared depends on how many feeds exist
CARD 5 (fan-out)       ← independent of 1-4; its own arc
```

## 5. What is NOT in this card

- **Byte spans / `line` columns.** `flagship-callgraph.dl6:33` and
  `diag-rail.dl6` both write `line = 0`. That is the `compound_storage =
  struct_as_rows` arc, in flight, and it is orthogonal to file selection.
- **`ast_yaml`** (2 rules) has no v6 spelling and no card here; it is a
  companion-op gap, not a `scan` gap.
- **The `finding` / `scanwork` / lazy-heads half of stopping-point program 4.**
  `READINESS.md:120` records that those do not appear in the parity table at all.
- **`commit_ms`.** The other named cause of the 84x parity gap. Perf, not
  spelling.

## 6. Reproducing the census

The numbers above came from parsing `examples/*.dl` with a paren- and
quote-aware argument splitter, normalising arities against `README.md:320`, and
excluding comment mentions by column position. The glob-dialect table came from
running `git ls-files` and `node -e "path.matchesGlob(...)"` against this
checkout directly. No script was left in the tree; the method is four steps and
the counts are stated per bucket so any of them can be re-derived by grep.
