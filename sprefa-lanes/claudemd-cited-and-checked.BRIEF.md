# Lane: rewrite CLAUDE.md so every claim carries a citation, and build the rail that checks them

## Base
`git merge --ff-only e70417d9` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/chore/claudemd-cited-and-checked`.

## The problem, stated by the user

Every number a coordinator quotes from `CLAUDE.md` gets revised by the next
measurement. The user's words: "are we correctly interpreting any numbers
correctly at all, we are always refining them and you always go 'i didnt see
this'. well fuck off and start knowing and write it down somewhere."

And: "i want fucking references to the code in this file bc this tool is
SUPPOSED TO BE marking citations and checking them in markdown".

So: `CLAUDE.md` becomes small, every factual claim carries a citation, and a
rail fails when a citation goes stale.

## Measured proof the file is wrong RIGHT NOW

Do not take these on faith. Re-measure each one and cite what you find.

| CLAUDE.md claims | measured 2026-08-12 |
|---|---|
| "`v6/prolog/compile/parse_dl.pl` is the real surface (text door)" | `v6/prolog/use_resolve.pl:28` sets `dl_parser` to `dcg` unless `DL_PARSER=classic`, so `compile_dl6` runs `parse_dl_dcg.pl`. An agent edited only `parse_dl.pl` and its probe still failed. |
| "manifest.json ... 390 rows as of 2026-08-12" | 392 rows |
| "sweep `total=286 identical=283 wrong=0 rejection=3`" | `total=288 identical=285 wrong=0` |
| "Presets ... `flash4` = deepseek-v4-flash-0731, `terra` = gpt-5.6-terra@medium" | also `pro4`, `luna`, and `sol` = `gpt-5.6-sol@high` |
| "`.github/CI-KNOWN-RED.md` ... is what CI actually judges against" | that file lists 6 red plunit tests; a clean tree has 15, same set 3 of 3 runs |
| "conformance 392/0" | still true. Keep it, with its command. |
| "TEXT_DOOR `compiled=288 byte_identical=288`" | re-measure |
| "dl_view corpus 397 files" | re-measure |

## Deliverable 1: the rewrite

### The one rule

**Every factual claim carries a citation or dies.** A citation is one of:

| form | example | when |
|---|---|---|
| `path:line` | `v6/prolog/use_resolve.pl:28` | a claim about code |
| `path:line-line` | `v6/prolog/lower.pl:836-839` | a claim about a block |
| a reproducing command in a fenced block | `cd v6/tsv2 && bash scripts/sweep.sh` | a claim about a NUMBER |

A number with no command next to it gets deleted. A "the language cannot X" with
no throw site gets deleted. A date with no receipt gets deleted.

### What survives

Keep, in this order:
1. **Laws.** User-set, non-negotiable. These need no code citation because they
   are decisions, not measurements. Mark each with the date the user set it.
2. **Where state lives.** Pointers to the files that hold the truth, each a real
   path that exists.
3. **Open items the user must decide.** Verbatim user words only.

### What dies

- Every battery number inlined as prose. Replace the whole block with the
  commands that produce them and a line saying the numbers live in the command's
  output, never in this file. A number written down is a number that rots.
- Narrative about past arcs. That is git and `plans/`.
- Anything an agent asserted with no receipt.
- Repetition. Say a law once.

### Size

The file is currently ~200 lines and mostly stale. Aim well under that. The user
asked for "as small and as important as needed". Cut until every remaining line
would change an agent's behavior.

## Deliverable 2: the rail that checks the citations

`examples/gen-skill-ref.dl:26-31` already implements this idea for a different
doc: a checked-claims freshness rail that exits 2 when the prose cites a symbol
that no longer resolves. Read it first. It is the shape to copy.

That file is v5 `.dl`, and the user has ruled: "I DO NOT WANT TO RUN V5 ANYTHING
ANYMORE". So the v5 file is your REFERENCE, never your answer.

The v6 twin to study is `v6/dl/fixtures/reference-docs-rail.dl6`.

### The known obstacle, cited

`v6/dl/fixtures/reference-docs-rail.dl6:22-30` records a measured finding:
`v6/sprefa-extract/src/lang/mod.rs` `sources()` matches Rust, Go, Kotlin, Prolog
and TypeScript, with an ast-grep fallback, and none of it is `.dl6`. The throw
site is `source_for` returning `None` for an unregistered extension.

**Determine whether markdown is in that list.** If `.md` has no extractor, a
`.dl6` rail cannot read `CLAUDE.md` and this deliverable is BLOCKED. In that
case:

- Say so, with the file:line proving it.
- Report the fork: add a markdown grammar to `sprefa-extract`, or write the
  checker as a small script for now. Do NOT pick. The user picks.
- Deliverable 1 still ships.

If `.md` IS extractable, build the rail:

| the rail asserts | fails when |
|---|---|
| every `path:line` in CLAUDE.md names a file that exists | a file was renamed or deleted |
| the file has at least that many lines | a citation points past the end |
| every fenced command in CLAUDE.md is runnable | a `just` leg or script was renamed |

Wire it as a `just` leg. Additive only.

## How to verify a number before you write it

Run each three times, never once, and never from the whole gate. Two
back-to-back whole-gate runs on one tree gave DIFFERENT failing sets under lane
load, measured 2026-08-12.

```
cd v6/prolog/conformance && swipl -g go -t halt go.pl
cd v6/tsv2 && bash scripts/sweep.sh
swipl -g go -t halt v6/prolog/ARCH.pl
bash v6/sprefa-engine-rs/grade.sh
```

`just green-all` is RED by design. `.github/CI-KNOWN-RED.md` is the allowlist,
and it is stale by 9 rows. If you touch that file at all, only to record what
you measured, never to widen it.

## Files you own
`CLAUDE.md`, a new rail file under `v6/dl/fixtures/` or `.dl/` as your research
decides, `v6/justfile` ONLY to add one leg, and plan doc
`plans/2026-08-12-claudemd-cited-and-checked.md`.

## Files you must NOT touch
`v6/prolog/**`, `v6/sprefa-engine-rs/**`, `v6/boop/**`, `v6/labs/exec_shootout/**`,
any `Cargo.toml`. Four other lanes are live and own those.

## COMMIT YOUR WORK
Seven lanes today wrote their whole deliverable and exited rc=0 WITHOUT
COMMITTING, and four of those seven were flash4. Run `git add -A && git commit`
before you exit. An uncommitted tree is an undelivered lane. Check `git log
--oneline -1` shows YOUR commit before you finish.

## Laws
- Doubt yourself before asserting. You are a compression algorithm, not an
  oracle. If you cannot cite it, do not write it.
- A refusal is a hypothesis, never an edict. Never record "the language does not
  support X" without the throw site.
- "refusal" is banned in prose; a compiler error for an unbuilt construct is
  "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No dates, no narrative.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.
- Output form is lists and tables. Prose is a caption, never the medium.

## Report
The old and new line counts, the table of every claim you deleted and why, the
citation count in the new file, and whether the rail is buildable in v6 with the
file:line that decides it.
