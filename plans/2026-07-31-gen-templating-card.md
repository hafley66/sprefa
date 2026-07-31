# The string-templating design card (v5 `gen()` -> v6)

Base `e1f26676`, branch `codex/rel-ref-file-span-lab`. Analysis only: no code, no
fixtures, no justfile touched.

User question: *"what is string templating story"*. Amendment recorded mid-work:
**the word `gen` is BANNED for the construct** (ruling `gen_word_banned`, same
status as `scan` under `files_naming`). `gen()` below always names the *v5*
construct being censused; card 1 prices the new spelling.

Method, same as the `scan` card: census first, map every shape onto what v6
already ships, turn only the residue into ruling cards. Every card ranks its
candidates against stated criteria and stops.

---

## 0. Summary table — shapes x counts x v6 status

Census method: every `gen(` occurrence in `examples/*.dl` plus the one
non-symlinked `.dl/*.dl` copy, parsed with a paren/quote/backtick-aware argument
splitter, comment mentions excluded by column position. The other 60 `.dl/`
occurrences are **symlinks into `examples/`** (`.dl/gen-reference.dl`,
`gen-doc-indexes.dl`, `gen-skill-ref.dl`, `builtin-rels.dl`, `op-table.dl`) and
are not double-counted.

**109 code call sites across 25 files.** Arity forms per `src/parse/mod.rs:674-804`.

| # | shape | v5 spelling (verbatim from the corpus) | sites | v6 status |
|---|---|---|---|---|
| A | **file-append** | `gen(:append, "docs/reference/relations.md", "\| \`{name}\` \| {group} \|") <- …` | 47 | **EXPRESSIBLE** — `group_concat(line, '\n', ordinal)` + one `sh` write host |
| B | **block splice** (line span from a rel) | `gen(p, l0, l1, "\| \`{op}\` \| {kind} \|") <- op_block(p, l0, l1), …` | 39 | **INEXPRESSIBLE** — v6 has no way to write *into* an existing file at coordinates |
| C | **named zone** | `gen(:zone, "README.md", "cli", "{row}") <- flag(name), readme_flag_row(name, row).` | 14 | **INEXPRESSIBLE** — same reason; also no `comment()`-located marker pair |
| D | **whole file** | `gen("examples/interface-soup.d2", "{impl} -> {iface}") <- impl_of(impl, iface).` | 9 | **EXPRESSIBLE** — same as A |
| | | **total** | **109** | 56 expressible / 53 not |

Slot occupancy behind the shapes:

| slot | value | sites |
|---|---|---|
| template literal form | double-quoted single-line **103** / backtick multi-line **6** | 109 |
| **holes per template** | **0 holes: 51** / 1: 11 / 2: 17 / 3: 16 / 4: 13 / 5: **1** | 109 |
| distinct hole names | 51 (`name` 15, `f` 11, `slug` 10, `t` 8, `number` 8, `title` 7, `doc` 6) | — |
| target path arg | string literal **70** / bound variable **39** (`p` 27, `dc` 4, `summary` 4, `readme` 2, `path` 1, `readme`/`summary` from block rels) | 109 |
| mustache escape `{{x}}` used | **1 site** (`gen-js-html.dl:72`) | 109 |
| `$${id}` dollar escape used | **1 site** (`gen-js-html.dl:85`) | 109 |

Target file kinds, resolved through the block rels' own `scan()` globs:

| kind | example targets | note |
|---|---|---|
| markdown | `docs/reference/{relations,functions,syntax,examples,magic-rels}.md`, `README.md`, `PLANS.md`, `book/{README,SUMMARY}.md`, `book/tutorial/README.md`, `.agents/skills/*/SKILL.md`, `examples/_auto-doc/*.md` | the bulk |
| **rust source** | `src/docs_cmd.rs` — 4 sites emit `const CHAPTER_{n}: &str = include_str!("../book/{slug}.md");` into a `comment()`-located block | the block form's real job |
| d2 diagrams | `examples/interface-soup.d2`, `examples/_npm/graph.d2`, `examples/typeports.d2`, `examples/typegraph-anim.d2` | |
| html / json | `page.html`, `data.json`, `json-shape.json` | the 6 backtick multi-line templates live here |
| **`.dl` itself** | `examples/chaos-soak.dl` splices into its own source | |

### The two headlines the census produced

**(1) Half of v5's templating is not templating.** 51 of 109 sites (47%) have
**zero holes** — table headers, `\|---\|---\|` separators, ` ``` ` fences, prose
paragraphs. They are constant lines carried by a rule with an empty body
(`<- true().`, 44 uses across the corpus). The maximum hole count anywhere in
the corpus is **5, at exactly one site**; 4 holes appears 13 times. The
templating is shallow and the *ordering + sink* half is what does the work.

**(2) v5 and v6 are exact complements, with no overlap.**

| | v5 | v6 |
|---|---|---|
| string assembly in a rule | **none** — `grep -c "concat(" examples/*.dl` = **0**; `fn_docs()` (`src/engine/decls.rs:165-189`) lists 16 scalars and none of them joins strings | `concat([…])` -> SQL `\|\|` chain (`lower.pl:442-444`) |
| ordered fold | **none** — aggregates are `count sum min max json_group_array json_group_object` (`decls.rs:246`) | `group_concat/2,3` with an explicit ordinal column |
| template with holes | `gen`'s `"{var}"`, the ONLY interpolation in the language | **none on the rule plane** — `{col}` and `$col` exist only inside `sh` host command text |
| write to a file | `gen`, four sink shapes, convergent, journalled | one `sh` host per program |

v5 templates and cannot concat. v6 concats and cannot template. The card is
about whether v6 buys the other half, and if so, in which spelling.

---

## 1. The v6 baseline, priced

Two shipped rails build entire markdown documents this way and are the whole
evidence base: `v6/dl/fixtures/self-map.dl6` (695 lines, emits the 649-line
`v6/ARCH-MAP.md`) and `v6/dl/fixtures/devlog.dl6` (202 lines, emits the 206-line
`DEVLOG.md`).

| measure | self-map.dl6 | devlog.dl6 |
|---|---|---|
| `concat([…])` call sites | **31** | **6** |
| `group_concat(…)` call sites | **22** | **1** |
| `rel` declarations | 100 | 15 |
| longest `concat` list | **11 pieces** (`:464`, `:467`) | 6 pieces (`:175`, `:193`) |
| next longest | 10 (`:424`, `:688`), 9 (`:651`) | 4 |
| constant lines paying the seed tax | **39 of 66 `map_line` clauses** end `<- map_seed(_).` | 5 |

The 11-piece worst case, verbatim (`self-map.dl6:464`):

```dl6
line_text := concat(['| `', axis, '` | ', total, ' | ', live, ' | ', refused, ' | ', reserved, ' | yes |'])
```

Its v5 twin is one string with five holes. Six of the eleven pieces are
separator literals whose only job is to sit between two columns.

### 1a. The seed tax

v5 spells a constant line `gen(:append, "…", "\| relation \| group \|") <- true().`
— `true()` is a nullary body atom and the line costs one word. v6 refuses a
level rule with no positive atom (`lower.pl:2491`,
`level_rule_no_positive_body`), and `true/0` is a **guard**, not a positive use
(`registry.pl:144`), so it does not satisfy the check. Every constant line in
`self-map.dl6` therefore hangs off a synthetic `map_seed(_)` rel that is itself
derived from a watched source row (`self-map.dl6:302-303`). 39 map_line clauses
plus 33 escape-table clauses = **72 clauses whose whole body is `<- map_seed(_).`**

### 1b. The apostrophe workaround — measured

`lower.pl:211-223`:

```prolog
sql_literal(Atom, Literal) :-
    …
    ;  ( sub_atom(Atom, _, _, _, '\'')
       -> throw(unsupported_construct(quote_in_literal(Atom)))
       ; format(atom(Literal), '\'~w\'', [Atom]) )
```

**Measured, this base, `swipl` against the real modules:**

| spelling | `parse_dl/4` | `lower:sql_literal/2` |
|---|---|---|
| `note('It''s here')` — doubled quote | **OK, findings `[]`** | — |
| `note('It\'s here')` — backslash | **OK, findings `[]`** | — |
| `note("It's here")` — double-quoted | **OK, findings `[]`** | — |
| atom `'hello'` | — | `'hello'` |
| atom `'It\'s'` | — | **throws `unsupported_construct(quote_in_literal('It\'s'))`** |

All three surface spellings **already parse clean**, carrying the apostrophe into
the term form. `ruling(string_quote, both_parse)` (`rulings.pl:432`) already
settled that both delimiters are legal. The oracle has **zero** occurrences of
`quote_in_literal` (`grep -c` on `conformance/engine.pl` = 0), so `engine.pl`
runs these programs. The refusal is **compiler-only, one predicate deep, and
downstream of every door.**

The cost, in the shipped output:

- **`v6/ARCH-MAP.md` contains ZERO apostrophes across 649 lines.** So does
  `DEVLOG.md` across 206.
- Four English sentences in `self-map.dl6` are grammatically broken to get past
  it, and each one switched from `'…'` to `"…"` first (which does not help,
  because the guard reads the atom's *content*, not its delimiter):
  - `:369` `"… \| this program own rel graph \|"` (wanted *program's*)
  - `:552` `"## 3. The build DAG open frontier"` (wanted *DAG's*)
  - `:662` `"## 4. A compiled program rel dataflow: this program own"`
  - `:663` `"… \`origin\` is the analyzer own partition …"` (wanted *analyzer's*)

**Scope limit, stated:** world data is safe. Arrival and host-answer values
enter through parameter binds (`runtime/1_incremental.ts:31 bindArgs`,
`2_boot.ts:38 args:`), never through `sql_literal`, so an apostrophe **in data**
already round-trips. Only program-text literals refuse.

### 1c. Braces are already spoken for, twice

`serve/1_hosts.ts:132-139` states the shipped contract for `sh` templates:

> `{col}` splices the value into the command line, ESCAPED for the quoting
> context it lands in; `$col` is left in the text and exported as an environment
> variable so the child's own shell expands it.

23 of the shipped `.dl6` fixtures carry `sh` decls and use `{col}` holes. 3 use
`{…}` json braces (`decode(args, spread({position: position: int, …}))`). The
escape rule for a literal brace in an `sh` template is **"a `{name}` that is not
an input column is left alone"** (`1_hosts.ts:188-189`) — pass-through, no
doubling. v5's `gen` uses the **opposite** rule: mustache doubling, `{{x}}` ->
literal `{x}` (`src/engine/gen.rs:737-756, 778-781`), used at exactly one corpus
site. Two live escape policies for one syntax is the collision card 2 has to
price.

### 1d. Ordering: v6 already wins

v5 orders a file's rows by `ORDER BY 1, 2, …` over **every template-hole column
in template-appearance order** (`src/lower.rs:1174-1181`). You reorder the output
by reordering the holes in the string. `src/engine/decls.rs:250` says so:
*"file form renders body rows in output-text order (there is no order-by
column)"*. v6's `group_concat(line, '\n', ordinal)` takes an explicit ordinal
column. Whatever card 5 decides, **this half should not be ported**.

---

## 2. The ruling cards

Criteria used to rank every card, stated once:

| C1 | construct budget | 0 new constructs > sugar over shipped ones > new construct |
| C2 | vocabulary law | rxjs / prolog / SQL words, `vocabulary_tiebreak = sqlite_first_then_sql_standard` |
| C3 | migration reach | how many of the 109 sites the option unblocks |
| C4 | smallest correct | the standing "turbo mid" directive: least code that is actually right |
| C5 | `spine_residency` | fs writing stays hosted in-language, never kernel |
| C6 | line cost | per document, against the v6 baseline in section 1 |

---

### CARD 1 — `SLOT-TEMPLATE-NAME`: what the construct is called

`gen` is banned. Candidates from the rx / prolog / SQL pool only, priced for
collisions the way the `scan` card priced rx-`scan` and SQL-`SCAN`.

| candidate | pool | live in-tree collisions | C2 | note |
|---|---|---|---|---|
| **(a) `printf`** | **SQL — sqlite's own** (`printf()` is core; verified on sqlite 3.43.2: `printf('%s-%d','a',3)` -> `a-3`) | **29 occurrences of `printf` as shell text inside `sh` templates across 12 `.dl6` fixtures** — not an identifier collision, but `printf(…)` as a dl6 construct would sit on the same page as `printf '%s'` shell text meaning something adjacent-but-different | strongest under `vocabulary_tiebreak` | the word carries format-string semantics for free (`%s`/`%d`/`%5.2f`), which is a *different* hole syntax from `{col}` — see card 2 |
| **(b) `format`** | **SQL AND prolog** — sqlite `format()` is an alias of `printf()` (verified: `format('%s-%d','a',3)` -> `a-3`); prolog `format/2,3` | **740 uses of `format(` inside `v6/prolog/**/*.pl`** — implementation, not surface, so no surface collision, but every compiler author reads `format(` 740 times a day meaning the prolog one | doubly blessed; the only candidate that is simultaneously a SQL word and a prolog word | sqlite's docs name the function `printf()` and `format()` second, so `sqlite_first` slightly prefers (a) |
| **(c) `write`** | **prolog** (`write/1`) | `write_arch_map`, `write_devlog` are already **user-chosen `sh` host names** in the two shipped rails; **5** uses of `write(` in the prolog impl. Reserving `write` as a construct word forces those two hosts to rename or makes the word mean two things one line apart | prolog-only; loses the sqlite tiebreak | names the **sink**, not the templating — pairs naturally with card 5 rather than card 2 |
| **(d) no word — extend `concat`** | already live | v6's `concat` takes a **list** (`concat([a,b,c])`); sqlite's `concat()` is variadic and **does not exist on sqlite 3.43.2** (verified: `no such function: concat`, added 3.44). So v6's `concat` is already a borrowed word that this sqlite build cannot honour | neutral | zero new words; card 2 (a)/(d) live here |

Reading of the criteria, not a fiat: **(a) > (b) > (c) > (d)** on C2 under
`vocabulary_tiebreak`, with the honest caveat that (a)'s collision is a *reading*
collision on the same page rather than a semantic one, and (b) is the only word
that satisfies both halves of the vocabulary law at once. (c) is answering a
different question and belongs in card 5. (d) is what happens if card 2 lands on
"no template construct at all".

**Decisive fact for this card:** `printf` and `format` are the same sqlite
function; `concat` is not available on the sqlite this repo actually links.

---

### CARD 2 — `SLOT-TEMPLATE-SPELLING`: holes in a literal, or no holes at all

| candidate | shape | C1 | C2 | C6 | C4 |
|---|---|---|---|---|---|
| **(a) status quo — `concat([…])` only** | `line := concat(['\| `', axis, '` \| ', total, ' \|'])` | 0 new | clean | **11 pieces worst case, 31 sites in one file** | already correct, just verbose |
| **(b) sqlite `printf` / `format` format-string** | `line := printf('\| `%s` \| %d \|', axis, total)` | 0 new constructs if it enters as an **expression function** beside `concat`; it is one `expression/5` registry row and a direct SQL call (`lower.pl` emits `printf(…)` instead of a `\|\|` chain) | perfect — sqlite's own function, sqlite's own `%` syntax, no new hole dialect invented | 1 call, 3 args vs 11 pieces | **the smallest correct thing measured**: nothing is invented, the escape question dissolves (`%` is the only metacharacter, `%%` its own escape, both already sqlite's), and the emitted SQL shortens |
| **(c) inline `{col}` holes in a rule-plane literal** | `line := '\| `{axis}` \| {total} \|'` | 0 new constructs, but a **new scoping rule**: a literal now captures variables from the enclosing rule | **collides twice**: (i) `sh` templates already own `{col}` with pass-through-unknown escaping, and `$col` with env-var semantics — a third policy for the same braces; (ii) `{…}` is json-object syntax in `decode`/`spread` (3 fixtures). Measured: no fixture uses both in one file *today*, so the collision is latent not live | shortest of all | invents a dialect the language does not have; a bare `'{x}'` in a rule would silently stop being a constant |
| **(d) v5's exact `"{var}"` inside a new sink construct only** | holes legal only in the template argument of the sink, never in an ordinary literal | 1 new construct (the sink) | same brace collision, contained | short | the literal port; card 5 decides whether the sink exists at all |

C2 is decisive between (b) and (c): (b) borrows a spelling sqlite already ships
and this repo already links; (c) mints a third meaning for `{`. C6 separates (b)
from (a) by roughly 3x on the worst case.

**One measured caveat against (b):** `printf('%d', …)` needs the argument's type
at emit time. The type plane exists (`col_type/3`, `ruling(type_gate_widening,
arrival_gate_all_types_all_positions)`), and `%s` accepts anything, so `%s`-only
is a strictly-safe subset if type-directed `%d`/`%f` is judged too much.

**Decisive fact for this card:** the same `{col}` braces already carry two
different, incompatible policies in this tree (`sh` splice-with-shell-escaping vs
`$col` env-var, `1_hosts.ts:132-139`) and a third in v5 (`{{x}}` mustache
doubling, one corpus use). `printf`'s `%` costs zero new dialect.

---

### CARD 3 — `SLOT-QUOTE-ESCAPE`: how an apostrophe gets into a literal

The gap is real, named, and priced in section 1b. Four ways to close it.

| candidate | what changes | C1 | C4 | correctness |
|---|---|---|---|---|
| **(a) double the quote at lowering** — `sql_literal` replaces `'` with `''` instead of throwing | **one predicate, `lower.pl:220-222`** | 0 new | **smallest correct by a wide margin** | This is *literally what SQLite does* — verified: `sqlite3 :memory: "select quote('It''s')"` -> `'It''s'`, and `replace('a''b','''','''''')` -> `a''b`. Prolog uses the identical doubling rule, which is why `parse_dl` already accepts it. Zero surface change: all three spellings already parse |
| **(b) emit the literal as a bind parameter** | `sql_literal` sites become `?` + an arg | 0 new | larger — literals are currently spliced into statement text at compile time, and the emitted-module goldens are byte-graded, so every affected module regenerates | strictly correct, and closes any future escaping class at once |
| **(c) `char(39)` concatenation** — split the atom and emit `… \|\| char(39) \|\| …` | one predicate, uglier SQL | 0 new | worse than (a) for identical correctness | verified working; produces unreadable emitted SQL, against the standing "predictable emitted SQL" defence |
| **(d) keep the refusal, document it** | nothing | 0 new | zero work | the status quo is four broken English sentences in a **release-gated** artifact (`ruling(release_gate_v620, arch_from_single_dl6_file)`) |

Independent sub-questions this card should answer at the same time:

- **braces**: nothing refuses a `{` in a rule-plane literal today (`self-map.dl6`
  writes `'#123;#125;'` as a *mermaid* escape, not a language one). If card 2
  takes (c) or (d), braces need an escape; if it takes (b), they do not.
- **backslash**: `parse_dl.pl:441-458` already defines exactly four escapes
  (`\n \t \r \\`) plus quote-self, and an **unknown escape keeps its backslash**
  (`escape_codes(_, Other, [0'\\, Other \| More], More)`, with a header comment
  recording that an earlier version silently dropped it). No gap.

C4 and correctness both point at (a). (b) is the more general answer at
regeneration cost. (d) is the only option that leaves a shipped, release-gated
document ungrammatical.

**Decisive fact for this card:** all three surface spellings **already parse
clean with zero findings** (measured above) and the oracle already runs these
programs; the entire gap is one `throw` at `lower.pl:221`, and the fix is the
doubling rule that both SQLite and Prolog already use.

---

### CARD 4 — `SLOT-REGEN-HYGIENE`: staleness and hand-edits

**Correction to the brief's premise:** `.dl/verified-sha` is *not* a gen pairing.
It is `scripts/verify.sh:19`'s tree stamp (HEAD sha + tracked diff + untracked
inventory) that lets a full green run skip re-running itself on an unchanged
tree. It has nothing to do with generated files.

What v5 actually does, four mechanisms:

1. **Convergent write** — `gen.rs:189-196` compares bytes and skips the write,
   emitting `{"wrote": false}` to the eventlog. A settled tick touches nothing.
2. **Machine-owned marker pairs** — `:zone` finds a `BEGIN: <name>` / `END:` line
   pair by name, any comment prefix (`gen.rs:556, 592-593`); the block form finds
   its span through the `comment()` extraction op
   (`gen-doc-indexes.dl:164-165`). Text outside the markers is hand-owned and
   survives.
3. **Clobber refusal** — two rules writing one file bail loudly rather than
   last-wins (`gen.rs:180-184`).
4. **Run-twice convergence** — `scripts/regen-docs.sh:40-46` runs each generator
   twice with a fresh db and fails if `git diff` moved on the second pass, with
   the standing law in its header: *"never hand-patch the rendered output
   (BEGIN/END zones are machine-owned; fix the generator)"*. Then `git diff
   --stat` for a human to read.

**There is no automated gate that fails when a checked-in generated file is
stale.** `regen-docs.sh` regenerates and reports; it only *fails* on
non-convergence.

What v6 has: `v6/tools/staleness-gate.sh` (ARCH `gen_staleness_gate`, done),
which covers **emitted TypeScript modules and release binaries**. Measured at
this base:

- `v6/justfile:327` `green` and `:330` `green-all` — **neither includes
  `self-map` nor `devlog`.** The two document-producing rails have **zero**
  staleness gate today, and `ARCH-MAP.md` is a release gate.
- The class has **five recorded sightings** (`chat_log/20260730.1:377` calls the
  `door-handwritten.ts` regeneration the "5th sighting"; `ARCH.pl:824` files
  `gen/scale_generated.ts` as the same class).

| candidate | who owns staleness | C1 | C5 | note |
|---|---|---|---|---|
| **(a) shell gate — add `self-map`/`devlog` to `staleness-gate.sh`** | the gate script | 0 new | fine | matches the shipped answer for TS modules; **one line each**, and `self-map.sh:23` already states the file is byte-stable "so a staleness gate can diff it" — the gate was designed for and never written |
| **(b) the language owns it — a `check` mode on the sink** | the program | +1 flag on whatever card 5 lands | fine | v5's shape (`--check` never writes); needs the sink to exist first, and the CLI already has `bop check` with a 0/1/2 exit contract |
| **(c) marker pairs + hand-edit refusal ported** | the target file | rides card 5's block/zone shapes | fine | only meaningful if card 5 takes shapes B/C; the 53 inexpressible sites are exactly the ones that need it |
| **(d) status quo** | nobody | 0 | fine | a release-gated document with no gate; the class already bit five times |

C4 orders (a) > (b) > (c) > (d): (a) is two lines and closes the measured hole
today, and it does not prejudge card 5. (b) is the honest long answer and is
free once a sink construct exists. (c) is not independent.

**Decisive fact for this card:** `self-map.sh:23` says the output is byte-stable
*"so a staleness gate can diff it"*, and no such gate exists — `self-map` and
`devlog` are in neither `green` nor `green-all`.

---

### CARD 5 — `SLOT-SINK-SHAPE`: a construct, or a write host?

`ruling(spine_residency)` says the fs spine is hosted in-language, never kernel.
Both shipped rails already write files with an ordinary `sh` host and no new
construct — so the "expressible" 56 sites of section 0 are **already done**. The
question is only whether the remaining 53 (block splice 39 + named zone 14) and
the two rails' rough edges justify a construct.

**Two shipped write hosts, two different mechanisms for the same job:**

```dl6
# self-map.dl6:118 — value arrives as an ENVIRONMENT VARIABLE
sh write_arch_map(path: text, document: text) -> (status: text) =
  `printf '%s' "$document" > "$path"; printf '%s' written`.

# devlog.dl6:197 — value is SPLICED INTO THE COMMAND LINE
sh write_devlog(document: text) -> (written: text) =
  `printf '%s\n' "{document}" > DEVLOG.md; printf '%s' DEVLOG.md`.
```

Measured consequence: `DEVLOG.md` is **105,401 bytes** and `ARG_MAX` on this
machine is **1,048,576**. `devlog`'s `{document}` splice sits at **10% of
`ARG_MAX`** and the document grows monotonically with every session ledger.
`self-map`'s `$document` form has no such ceiling. Two rails, one job, one of
them on a dated fuse — and nothing names the difference.

| candidate | C1 | C3 | C5 | C6 | note |
|---|---|---|---|---|---|
| **(a) status quo — per-program `sh` write host** | 0 new | 56/109 | ideal | ~3 lines | shipped and working; inherits the `{document}` vs `$document` inconsistency and the copy-paste class the `scan` card's CARD 4 already filed (there is **no import/include/prelude row in `registry.pl`**, checked) |
| **(b) one blessed `write` host shape, stated once** | 0 new constructs, 1 doc/lint decision | 56/109 | ideal | ~3 lines | picks `$document` over `{document}`, kills the `ARG_MAX` fuse, and is card 1(c)'s natural home. Does **not** need a construct |
| **(c) a sink construct with A/D semantics only** (whole-file + append) | +1 construct, registry row, parse/print/grammar/text-door | 56/109 — **no new sites unblocked** | needs a kernel write path or a blessed host underneath | ~1 line | buys line count and convergent-write for free; adds a construct that expresses nothing the host cannot |
| **(d) a sink construct with B/C semantics** (block splice + named zone) | +1 construct **and** a coordinate-addressed write path **and** a `comment()`-equivalent to locate markers | **+53 sites, 109/109** | the write path is genuinely new machinery | ~1 line | the only option that reaches the block/zone half; the biggest thing on this card by a wide margin, and it depends on byte-span/`comment()` extraction reaching v6 |

C1/C4 order (a) = (b) > (c) > (d). C3 inverts it completely. The real question
the user is settling: **are the 53 block/zone sites a migration target at all?**
They are the ones that write into `src/docs_cmd.rs`, `README.md`, `book/SUMMARY.md`
and the SKILL pages — files with hand-written content around a machine-owned
region. Under (a)/(b)/(c) those programs do not port; under (d) they do, at the
price of the largest single construct on the open list.

**Decisive fact for this card:** the two shipped rails already prove the sink
needs no construct for 56 of 109 sites — and they disagree with each other on
how, with `devlog`'s command-line splice at 10% of `ARG_MAX` and growing.

---

## 3. Ranking across the cards

Not independent. The order forced by their own contents:

```
CARD 3 (quote escape)   ← one predicate, closes a release-gated defect. Independent of every other card.
CARD 4 (regen hygiene)  ← two justfile lines for option (a). Independent.
CARD 1 (the word)       ← pure vocabulary; must precede card 2's spelling
   └─ CARD 2 (template) ← 2(b) printf makes card 1 nearly moot; 2(c)/(d) make card 3's brace half live
CARD 5 (sink shape)     ← 5(d) is the only card that unblocks new sites, and is the largest item here
```

Ranked by (smallest correct) x (measured harm today):

| rank | card | option | why |
|---|---|---|---|
| 1 | **CARD 3** | (a) double the quote | one predicate; SQLite's and Prolog's own rule; both doors already parse it; closes 4 broken sentences in a release-gate artifact |
| 2 | **CARD 4** | (a) shell gate | two lines; the script it belongs in exists; the class has bitten 5 times |
| 3 | **CARD 5** | (b) bless one host shape | zero constructs; kills a measured `ARG_MAX` fuse and an inconsistency between two shipped rails |
| 4 | **CARD 2** | (b) `printf`/`format` | 0 new constructs, sqlite's own function and syntax, ~3x shorter than the 11-piece worst case, invents no dialect |
| 5 | **CARD 1** | (a)/(b) | decided by 4; a naming call with no behaviour attached |
| 6 | **CARD 5** | (d) block/zone sink | +53 sites, and by far the most machinery; genuinely a separate arc |

---

## 4. What is NOT in this card

- **Byte spans / `comment()` in v6.** Card 5(d) depends on both. That is the
  `compound_storage = struct_as_rows` arc plus a `comment`-extraction host, and
  it is where the block-splice half actually lives.
- **`true()` in a level body.** Section 1a's seed tax is a real 72-clause cost
  in one file but it is the `level_rule_no_positive_body` check's question, not
  templating's.
- **String functions.** `plans/2026-07-30-v5-parity-spelunk.md` D8 already files
  the split/replace_re/lines gap and the UDF verdict says the current driver
  cannot register any. `printf` under card 2(b) is core sqlite, so it does not
  ride that blocker.
- **The `sh` copy-paste class.** Filed by the `scan` card as `SLOT-FEED-REUSE`;
  the write host is a second instance of the same shape, not a new question.

## 5. Reproducing the census

`examples/*.dl` plus `.dl/rusqlite-coupling.dl` (the only non-symlinked `.dl/`
file containing the construct) parsed with a paren-, quote- and backtick-aware
argument splitter, comment mentions excluded by column position, arities mapped
to the four forms in `src/parse/mod.rs:674-804`. The v6 numbers came from
counting balanced `concat([…])` bodies in the two fixtures. The escape table came
from running `parse_dl/4` and `lower:sql_literal/2` under `swipl 10.0.2` against
the real modules; the SQL rows from `sqlite3 3.43.2` on this machine; `ARG_MAX`
from `getconf`. No script was left in the tree.
