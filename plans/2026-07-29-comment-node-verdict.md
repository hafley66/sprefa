# comment-node dogfood verdict (2026-07-29)

Contract: `plans/2026-07-29-comment-node-dogfood-header.md`. Lab base sha
`4531d087` (ff-only, clean). Lab files: `v6/prolog/labs/comment_node/`,
deleted on landing; the last full copy is recorded at the bottom.

## Headline

**The seven golden comment techniques come back to v6 with ZERO new
constructs, ZERO extractor change, and one policy-free helper.** The graded
route reproduces v5's `comment_node` **byte for byte, 745 / 745 rows**, on a
pinned corpus of this repository's own Rust sources, and reproduces
`std/arch.dl`'s `arch_node` **4 / 4**. Both dogfood programs run live on the
served engine.

What did NOT come back is text manipulation. v5's rails make **57 text-operation
call sites** (`=~`, `replace_re`, `trim`, `split`, `json`, `match_line`,
`match_ast`); v6's whole writable expression surface is **eleven rows**, all
`both_int` or `same_type`, with **zero text operations**. Every one of those 57
sites had to move into a host template. That is the cost of parity, and it is
the only cost.

One thing came back BETTER than the port asked for: the byte-span flattener
gives v6 rails **real line numbers**, which no prior v6 rail had.
`flagship-callgraph.dl6` dropped `line`; `diag-rail.dl6` shipped whole-file
zeros; `receipt.sh` phase 1 reads `["src/a.ts", 2, ...]`.

---

## 1. Text acquisition: the route, graded

Corpus for every number below: `v6/prolog/**/*.pl`, 58 files, 1,025,619 bytes
(the dogfood target the directive named). Receipt: `route-cost.sh`.

| route | rows crossing the rel boundary | stdout bytes | wall ms | string-safe |
|---|---|---|---|---|
| **a** extractor grows a `comment` family | 5,939 | 2,053,458 | n/a (not built) | yes |
| **b1** whole cst stream crosses, program filters | 230,096 | 24,479,804 | 620 | yes |
| **b2** host template pre-filters to comments | 5,939 | 597,770 | 494 | yes |
| **c** `grep -n`, no grammar join | 5,943 | 455,657 | 86 | **NO** |

`b1 / a` boundary amplification = **38.7x**. Every crossed row is an EDB
arrival the engine writes, diffs and refCounts, so b1 is not a style choice; it
is 38.7x the engine's per-file work for the same answer.

### VERDICT: route (b2), with route (c)'s scanner joined to it

The measured shape that wins is neither of the header's clean alternatives. It
is **(b2) for the grammar fact + (c) for the marker hit + an in-language join
between them**:

```
suppressed(path, line, code) <-
  directive_line(path, digest, line, code),          # route (c): grep, cheap
  comment_node(path, digest, line, kind, comment_text).  # route (b2): grammar
```

Rx lowering (the snippet law):

```js
const suppressed$ = combineLatest([directiveLine$, commentNode$]).pipe(
  map(([directives, comments]) => {
    const onLine = new Set(comments.map((row) => `${row.path}:${row.line}`));
    return directives.filter((row) => onLine.has(`${row.path}:${row.line}`));
  }),
  distinctUntilChanged(sameRowSet));
```

This is not an invention. `examples/gen-lang-skill.dl:24-28` already writes it
in v5, with the comment "*A junction = a text match the grammar lexed as a
comment*". The lab rediscovered the v5 convention by measurement and then found
v5 had written it down.

**Route (c) alone is dead**, and the witness is in our own sources, not a
synthetic file. `string-safety.sh` differences every naive-scanner hit against
what the parser calls a comment:

| language | scanner-flagged lines | FALSE POSITIVES |
|---|---|---|
| `%` over `v6/prolog/**/*.pl` | 5,943 | **4** |
| `//` over `v6/tsv2/**/*.ts` | 2,250 | **5** |

Witnesses (first of each):

```
v6/prolog/compile/lower.pl:468    format(atom(Sql), '(((~w % ~w) + ~w) % ~w)',
v6/tsv2/cli/bop.ts:172            fetch(`http://127.0.0.1:${event.port}/program`, ...
```

`http://` inside a template literal, in the CLI we shipped this week. Nine real
false positives across 8,193 flagged lines is a 0.11% error rate, which is
exactly the rate that makes a suppression rail untrustworthy rather than
obviously broken.

The **live** form of the same property is `receipt.sh` phase 3: the identical
`dl-disable-line no-eval` text, placed inside a string literal, suppresses
nothing. Sabotage receipt, run then reverted: deleting the `comment_node` atom
from `suppressed`'s two clauses passes phases 1, 2, 4, 5 and **fails phase 3**.

### Content-addressed re-extraction

Identical across all four routes, because it is not a property of the route: the
salt is `@ salt(digest: digest)` and the witness cache is
`__host_witness`. `receipt.sh` phase 2 (edit -> re-extract -> finding retracted)
and `extraction-live.sh` phase 3 (identical bytes -> zero ticks) both already
grade it. What DOES differ per route is the cost of one re-extraction, measured
above per file: b1 10.7 ms, b2 8.5 ms, c 1.5 ms.

### Statement counts at repo scale

`receipt-scale.sh`, real served engine, this repository's own 58 compiler
sources, `DL_PERF_LOG`:

```
comment_node rows landed: 5939
kinds:                    [["line", 5939]]
statements/tick:          ticks=57  first=63  last=92  peak=92
growth factor             1.46x while the corpus grew 57x
offline route rows:       5939   ==   engine rows 5939
COMMENT DOGFOOD SCALE HOLDS
```

Stated as **bounded, not flat**, because 63 -> 92 is what the numbers say. The
growth is the emitter's per-rel statement families as tables fill, not a
function of file count; 57x corpus for 1.46x statements is the assertion.

Disclosed receipt bug, found and fixed inside the lab: the first draft counted
`[` characters in the HTTP JSON and read 6,107 where the program's own aggregate
read 5,939. A comment body legitimately contains `[`. It would have been reported
as an engine defect. The script now reads the count from `comment_kind_count`.

---

## 2. Marker capture: SLOT-MARKER-CAPTURE

`gap-inventory.sh`, counted from the seven v5 rails:

| v5 rail | operations used |
|---|---|
| `std/arch.dl` | `json` x9, `replace_re` x4, `=~` x3, `split` x1, `jsonp` x1, `int` x1 |
| `std/suppress.dl` | `replace_re` x10, `=~` x7, `trim` x6 |
| `examples/gen-readme.dl` | `replace_re` x3, `trim` x2, `match_line` x1, `=~` x1 |
| `examples/gen-lang-skill.dl` | `match_line` x3 |
| `examples/gen-plans-index.dl` | `replace_re` x6, `=~` x3, `trim` x2, `match_line` x2, `count` x1 |
| `examples/gen-zone-info.dl` | `trim` x1, `replace_re` x1, `=~` x1 |
| `examples/lint-unwrap.dl` | `match_ast` x1 |
| **total call sites** | **57** |
| **v6 text operations in `registry.pl expression/5`** | **0** |

Pricing, `marker-price.sh`, same 58-file corpus:

| shape | wall | spawns |
|---|---|---|
| one marker host, all conventions in one grep | 133 ms | 58 |
| seven marker hosts, as the seven techniques declare them | 497 ms | 406 |
| the cst comment host (the grammar witness) | 340 ms | 58 |

Route (a) marker capture costs **1.46x the grammar host's wall and 7x its
spawns**; collapsing the seven conventions into one host saves 3.7x wall and is
the obvious optimization once more than two conventions ship.

**ANSWER: (a) per-marker sh host.** (b) extractor-side splitting is refused on
principle before cost: `std/suppress.dl`'s own header states the law -- *"policy
lives HERE, never in Rust -- the engine only produces grammar-accurate comment
facts; which directives mean what, and how a block pairs, is all datalog"* --
and baking `ARCH` / `dl-disable` / `LANG-JUNCTION` / `README(` / `todo(` /
`BEGIN:` into a fixed extractor makes rust the place a convention is added.

### (c) a text/regex construct: NOT PROVEN by these seven techniques

Under the extraction-lab discipline the honest answer is that the seven
techniques do **not** prove the gap. Every one of their 57 text operations runs
on bytes that originate in **one file**, and a per-file host can therefore
perform it. The lab looked for a counterexample and did not find one:

- multi-code peeling (`dl-disable-line a, b -- reason`, `std/suppress.dl`'s
  recursive `code_rest` tail) -- pushable, the host emits one row per code;
- `arch_parent` / `arch_last` / `arch_order` -- pushable, measured, shipped in
  `arch-rail.dl6`'s template;
- the ARCH hierarchy above them (`arch_edge`, `arch_root`, `arch_child_count`,
  and transitive ancestry) -- already ordinary datalog, needs nothing.

**The program that WOULD prove it** (stated, not built, per the discipline): a
rule that must split a string whose value does not exist until after a
**cross-file join** -- so no per-file host can see it. Concretely: resolving a
`README(anchor)` prose reference against a symbol whose fully-qualified name is
assembled from a module path in file A and a member name in file B, then
splitting that assembled name to match the anchor. Technique 3 does not
currently need this (`gen-readme.dl` anchors are per-file), so the construct
stays unbuilt and the slot stays open with the trigger written down.

What IS lost by pushing, and should be recorded as debt rather than denied:
the convention stops being incremental datalog and becomes an opaque shell
string; it is not typed, not composable with the rest of the program, and a
typo in the `sed` is silent -- it reads as "a host that answers zero rows",
which is exactly the trace obligation `serve/1_hosts.ts:261-270` already names.

---

## 3. Payload destructure through struct-as-rows: BLOCKED, receipt attached

`probes/p1_struct_host_output.dl6` declares the shape the ruling wants:

```
type arch_payload(url: text, role: text).
sh arch_marker(path: text) -> (line: int, payload: arch_payload) = `...`.
arch_node(path, line, url) <- marker(path, line, payload), decode(payload, {url: url}).
```

The real compiler's answer, verbatim:

```
compiler refusal unsupported_construct(surface_findings([
  unsupported_surface(column_type_wrapper(arch_marker, payload, none))]));
reason=surface_findings; location=rule-index unavailable
```

Root cause, read not guessed: `parse_dl.pl decl_b_column_type/5` (:453-457) --
the production `sh_decl` uses for BOTH its input and output columns -- accepts
`int`, `text`, `json` and nothing else; a bare identifier falls through to the
wrapper arm. `decl_a`'s `typed_column_type/3` (:305-313) accepts a bare
identifier as a type ref, which is why `rel` columns can be struct-typed and
host columns cannot. This is exactly ARCH row `struct_host_output_seam`, which
is `in_flight` and already blocking `flow_parity_upgrade`; this lab is the
second arc to hit it, and it now blocks technique 1's faithful spelling too.

Second half of the same wall, for when the compiler opens: `serve/1_hosts.ts
coerce/3` (:186-202) has no struct arm -- a non-string, non-number value is
`JSON.stringify`d into a TEXT column, so a struct-typed output would arrive as
text even if it compiled.

Also note `decode/2` refuses a `json`-typed source: `decode_source_type/6`
(lower.pl:845-853) resolves the source's type from a `ref(Type)` column binding
and throws `decode_source_not_struct` otherwise. There is no untyped-json escape
hatch, so "declare the payload `json` and decode it" is not an alternative.

**The graded ARCH marker therefore destructures its JSON in the host template**,
and `arch-rail.dl6`'s header says so in the program text rather than in a
comment nobody reads. It still runs end to end (`receipt.sh` phases 6-7) and
still matches v5 4/4 (section 5).

---

## 4. The two receipt programs

`receipt.sh`, one served tsv2 process, node's own `fs.watch` behind the bind
seam, the in-tree release extractor, real files, rows read back through the
programs' own emitted SELECTs. **7 / 7 phases PASS.**

```
PASS  phase 1  no-eval diag at LINE 2 (a real line number, not 0):
               [["src/a.ts",2,0,2,0,"error","no-eval","eval() is banned...",""]]
PASS  phase 2  dl-disable-line suppressed the live diag
PASS  phase 3  a directive INSIDE A STRING LITERAL suppresses nothing
PASS  phase 4  dl-disable-next-line suppressed line+1 (int arithmetic in the rule)
PASS  phase 5  unused suppression warned (the antijoin): [["src/b.ts",2,"no-eval"]]
PASS  phase 6  3 markers -> 3 nodes, the quoted-atom marker antijoined
PASS  phase 7  hierarchy: edges 2, roots 1, children [["sprefa/compile/01-lower",2]]

COMMENT DOGFOOD RECEIPTS HOLD
```

Phase 1's `2` is the first real line number a v6 rail has ever produced. It
comes from the byte-span **flattener**, and that is the reusable half of this
lab: `sprefa-extract` writes `"span":{"start":..,"end":..}` while
`decodeObjectItems` projects TOP-LEVEL declared columns only, so a nested field
can never reach a declared `int` column. Lifting `span` to flat
`line`/`col`/`end_line`/`end_col` **in the host template** unblocks line numbers
for **every family at once** -- call sites, defs, comments -- and touches
neither the extractor nor the struct seam. `diag-rail.dl6`'s whole-file zeros
and `flagship-callgraph.dl6`'s dropped `line` column are both closable by this
one-line template change.

Both programs compile clean through the **text door**
(`compile/scripts/compile_dl6.sh`), so they are real `.dl6`, not term-form
sketches.

### fixture/5 candidates (graded, not promoted)

Four candidates in `fixtures.pl`, run through the SAME
`engine:fixture_expectations_hold/2` that `conformance/go.pl` runs, via
`grade_fixtures.pl`. **4 / 4 PASS**, each with its own red-then-green sabotage:

| name | pins | sabotage that flips it red |
|---|---|---|
| `comment_witness_gates_a_scanner_hit` | the grammar witness join | drop the `comment_node` atom -> 2 rows incl. the string-literal false positive |
| `disable_next_line_shifts_the_effect_by_one` | `+ 1` is the LANGUAGE's arithmetic | `Line + 1` -> `Line + 0` |
| `unused_suppression_antijoins_the_finding` | eslint's reportUnusedDisableDirectives | `not(rail_finding(...))` -> `true` |
| `arch_hierarchy_from_decomposed_marker_rows` | edges / roots / child counts as joins | `not(arch_url(Parent))` -> `true` |

Disclosed: dropping F2's witness atom leaves F2 green. F2 pins the arithmetic
and F1 owns the witness; neither substitutes for the other, and the fixture
header says so.

They are **candidates**, not promotions: promoting inside a lab commit that is
about to be deleted would move the conformance count for a file that vanishes.
The precedent is consumption-arms and update-arm (candidates + a recoverable
hash). Conformance stays at 159 in this landing.

---

## 5. Parity vs the v5 rail

`parity.sh`. Pinned corpus: `src/main.rs`, `src/lower.rs`, `src/cst.rs`,
`src/parse/mod.rs`, `src/engine/strata.rs` -- listed literally, copied into a
scratch tree both engines use as cwd. `std/arch.dl` and `std/suppress.dl` are
copied **byte-for-byte** (sha asserted in the script); only the importer, which
those files' own headers require the importer to supply, is written by the rig.
The v5 leg runs `target/release/dl` with `DL_STATE_DIR` and `--db` both inside
the scratch tree: **nothing under `~/.local/state/sprefa` is read or written and
the daemon is untouched.**

| artifact | v5 | v6 | shared | only-v5 | only-v6 |
|---|---|---|---|---|---|
| `comment_node` (path, line, col, end_line, end_col, text, kind) | 745 | 745 | **745** | 0 | 0 |
| `arch_node` (path, line, url) | 4 | 4 | **4** | 0 | 0 |

**UNCLASSIFIED 0.** Not because there is nothing to classify -- because there is
no diff.

Sabotage receipt proving the rig discriminates, run then reverted: shifting the
column convention by one (`col` 0-based -> 1-based) flips **745 of 745** rows,
and the classifier names the bucket correctly (`p position-convention`) with
zero unclassified. A rig that cannot go red is not a grade.

Two convention facts had to be read out of `src/cst.rs` rather than assumed, and
both were wrong in the first draft:

1. **line 1-based, col 0-BASED** (`walk_comments` normalizes tree-sitter's
   0-based row); `end_row`/`end_col` are the node's END position, after the last
   byte. SLOT-SPAN-UNITS, settled by the v5 contract.
2. **a comment is a LEAF.** `walk_comments` stops descending the moment a kind
   contains "comment", so v5 emits ONE row for `/// x` where the rust grammar
   nests a `doc_comment` child inside the `line_comment`. The cst family is
   lossless and reports both. Measured before the fix: **430 v6 rows against 254
   v5 rows**, the entire gap being nested `doc_comment` children.

### The seven techniques: coverage after this lab

| # | technique | status | what it still needs |
|---|---|---|---|
| 1 | ARCH JSON markers -> nodes/hierarchy | **GRADED 4/4 vs v5** | faithful payload destructure needs `struct_host_output_seam`; url split is in the template |
| 2 | dl-disable suppression + pairing + unused rail | **GRADED live 5/5** | BLOCK pairing (`dl-disable` .. `dl-enable`, the argmax `span_beaten` nearest-enable rule) is not ported. It is ordinary datalog (`<=`, `<`, `not`) over rows the host already emits, so it is WORK, not a gap |
| 3 | README(anchor) doc prose | not ported | anchor resolution is per-file; a host + join. No new construct. The cross-file assembled-name case is the (c)-proving program above |
| 4 | LANG-JUNCTION(slug) registry | not ported | `match_line` with named captures -> one host per side + the same witness join. Drift rails are antijoins |
| 5 | todo(category)/TODO/FIXME plans index | not ported | markdown. v5 has `walk_md_comments` (html_block filter); `sprefa-extract` has no markdown grammar in the cst family. **This is the one technique with a real extractor-side hole** |
| 6 | BEGIN: gen zone ownership | not ported | zone ranges are the same start/end pairing as technique 2's blocks |
| 7 | lint finding governed by suppression | **GRADED live** (phases 1-5) | nothing |

Also measured, and it matters for the dogfood directive: **v5 cannot see prolog
at all.** `src/cst.rs lang_label_for_path` maps 17 extensions and `pl` is not
one of them. `sprefa-extract` drives ast-grep's grammar registry and prolog is
in it (95 `line_comment` nodes in `0_body_walk.pl` alone, 5,939 across the 58
compiler sources). Dogfooding our own compiler sources with comment facts is a
**v6-only capability**; there is no v5 leg to grade it against, which is why
`receipt-scale.sh` is a v6 assertion and not a diff.

---

## 6. Named slots

| slot | answer | basis |
|---|---|---|
| **SLOT-EXTRACTOR-WAIVER** | **NOT NEEDED for six of seven techniques. Recommend NO waiver now.** Route (b2) reaches byte parity with v5 (745/745) at 8.5 ms/file with zero extractor change. Route (a) would save one pipe and one file re-read per file and change nothing else about the answer. The ONE thing that does need the extractor is **markdown comments** (technique 5): v5 has `walk_md_comments`, the cst family has no markdown grammar, and no host template can recover it. **USER CALL, scoped to markdown only.** | route-cost.sh, parity.sh |
| **SLOT-MARKER-CAPTURE** | **(a) per-marker sh host.** (b) refused by `std/suppress.dl`'s policy law. (c) not proven by these seven techniques; the proving program is written down in section 2 and is not one of them. Recommend collapsing multiple conventions into ONE host (3.7x wall) once more than two ship. | gap-inventory.sh, marker-price.sh |
| **SLOT-COMMENT-KIND-VOCAB** | **In the language side of the seam, off the TOKEN, not the tree-sitter node name.** Measured reason: `doc` is not a kind in the grammar -- rust spells `/// x` as a `line_comment` node with a `doc_comment` CHILD, so doc-ness is a parent/child relation, while v5 reads it off the `///`/`//!`/`/**` prefix and is language-independent for free. `block` comes off the node name (`block_comment`), `line` is the default. Two functions, ~10 lines, and it reaches 745/745 parity. | cn.py `comment_kind`/`doc_kind`, parity.sh |
| **SLOT-SPAN-UNITS** | **Both, and the mapping lives in the host template.** Byte spans stay the transport (they are what the extractor emits and what content-addressing hashes); `line`/`col` are computed at the seam by the FLATTENER, in v5's exact convention (line 1-based, col 0-based, end = position after the last byte). Parity grading uses line/col because that is v5's output shape. The flattener is the generally useful artifact: it unblocks line numbers for every extractor family, not just comments. | parity.sh 745/745, receipt.sh phase 1 |
| **SLOT-TOKEN-STRIP** | **In the tool, same place v5 puts it, and it is NOT a policy violation.** Token stripping is LEXICAL -- a property of each grammar's comment syntax (`//`, `%`, `/* */`, leading `*`) -- while policy is which markers mean what. `cn.py` strips tokens and contains the string `ARCH` / `dl-disable` / `TODO` exactly zero times. The parity run proves the strip is faithful: 745/745 including block and doc bodies. | cn.py, parity.sh |

---

## 7. Open items this lab created or sharpened

1. **`struct_host_output_seam` now blocks a second arc.** It was escalated to
   blocker for `flow_parity_upgrade`; technique 1's faithful payload
   destructure is the second caller. The seam has TWO halves and both are
   located: `parse_dl.pl decl_b_column_type/5` (compiler) and
   `serve/1_hosts.ts coerce/3` (runtime).
2. **The byte-span flattener is a one-line fix for two shipped honesty gaps.**
   `diag-rail.dl6`'s whole-file zeros and `flagship-callgraph.dl6`'s dropped
   `line` column are both closable by piping the extractor through a flattening
   step in the host template. Neither needs the struct seam. Worth a small
   dispatch on its own; the flattener is recoverable from this lab's `cn.py`.
3. **Markdown comments are the one real extractor hole** (technique 5), and it
   is a grammar gap, not a policy gap.
4. **Block-range suppression (`dl-disable` .. `dl-enable`) is unported work,
   not a gap.** `std/suppress.dl:135-149`'s nearest-enable argmax is `<`, `<=`
   and one `not(...)` over rows the host already emits. It should be the next
   thing written, and it is the piece that makes the port complete rather than
   representative.
5. **`v6/prolog` has ZERO ARCH markers today.** The dogfood directive asks for
   the technique to return; the markers themselves still have to be authored.
   `arch-rail.dl6` is ready for them.

## Lab death

Lab files deleted in the landing commit. The last full copy of
`v6/prolog/labs/comment_node/` (cn.py, both `.dl6` programs, the four fixture
candidates, and all six receipt scripts) is at commit **`24f7cc7c`**; recover
any file with `git show 24f7cc7c:v6/prolog/labs/comment_node/<file>`.
