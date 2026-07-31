# Prolog packaging research: how to partition v6/prolog

Base: `61f75b0e` (merged clean, ff-only). Read-only research + census. No `.pl`
files touched. Scope per brief: `v6/prolog/**` for the census; ecosystem
research is general SWI-Prolog practice.

User ask, verbatim spirit: "figure out a good partition scheme of the prolog
code, ... how can we package-ify/crate-ify this stuff." Mental model: Cargo
workspace / npm workspace, applied to a ~37k-line SWI-Prolog tree the user has
little prior exposure to.

---

## Part 1: the ecosystem

### 1.1 SWI-Prolog packs

A pack is a directory (or its zip/tgz archive) with a `pack.pl` manifest a
the root and a `prolog/` subdirectory holding the `.pl` files that get added
to the library search path on install. [Creating and submitting extension
packages](https://www.swi-prolog.org/howto/Pack.html), [Developing a
pack](https://www.swi-prolog.org/pldoc/man?section=pack-devel), [pack
metadata](https://www.swi-prolog.org/pldoc/man?section=pack-metadata).

`pack.pl` fields (from the metadata doc + a real example below):

| field | meaning |
|---|---|
| `name` | must match the directory name, `[a-zA-Z0-9_]+` |
| `title` | one-line summary, ~40 chars |
| `version` | dot-separated integers |
| `author` / `maintainer` / `packager` | contact, repeatable |
| `home` | project page URL |
| `download` | git URL or archive wildcard URL |
| `requires(Dep)` | `Dep` is another pack token, or `prolog >= 'X.Y'`, or `prolog:Feature` (e.g. `prolog:threads`, `prolog:library(socket)`) |
| `provides(Token)` | announces a capability other packs can `requires/1` |
| `conflicts` / `replaces` | incompatibility / supersession |
| `pack_version(2)` | opts into the newer foreign-extension convention |

Real `pack.pl`, [ridgeworks/clpBNR](https://github.com/ridgeworks/clpBNR):

```prolog
name(clpBNR).
title('CLP over Reals using Interval Arithmetic...').
version('0.13.1').
author('Rick Workman', 'ridgeworks@mac.com').
home('https://github.com/ridgeworks/clpBNR').
download('https://github.com/ridgeworks/clpBNR.git').
requires(prolog >= '9.1.22').
requires(prolog:rationals).
```

Foreign (C) code has one hard rule: the build must drop native modules under
`lib/<arch>/`; naming/layout of the C sources themselves is unconstrained.
[Packs with foreign
code](https://swish.swi-prolog.org/pldoc/man?section=pack-foreign). No
relevant here  -  nothing in `v6/prolog` links foreign code.

Local development without publishing: `swipl pack install .` symlinks the
current directory into the personal pack directory; `swipl -p pack=..` adds a
path without installing anything.

### 1.2 Three real packs, read for structure

| pack | size | layout | why it's a useful reference |
|---|---|---|---|
| [tokenize](https://github.com/shonfeder/tokenize) | 2 files in `prolog/` (`tokenize.pl`, `tokenize_opts.pl`), 1 test file `test/test.pl` | smallest real shape | floor case: a pack can be two files and still be a pack |
| [clpBNR](https://github.com/ridgeworks/clpBNR) | 4 files in `prolog/` (`clpBNR.pl`, `clpBNR_search.pl`, `clpBNR_toolkit.pl`, `clpBNR/` subdir), no top-level `test/` | mid-size, single flat module list | tests live elsewhere (not every real pack uses `test/`) |
| [s(CASP)](https://github.com/SWI-Prolog/sCASP) | `prolog/scasp.pl` (entry) + `prolog/scasp/` with **~30 internal modules** (`compile.pl`, `solve.pl`, `model.pl`, `html.pl`, `swish.pl`, ...); `test/` has `test_*.pl` plunit files **plus fixture-program directories** (`all_programs/`, `le_programs/`, `min_programs/`) | closest analog to sprefa in scale and shape | **one pack, many internal modules, fixture-corpus-as-directories under `test/`** is a proven pattern at a size comparable to sprefa's `compile/` alone. `pack.pl` is 10 lines: `requires(prolog >= '9')` and nothing else  -  no internal `requires/1` because everything ships as one unit. |

s(CASP) is the strongest single receipt against over-fragmenting: a real,
maintained SWI project with sprefa-comparable internal complexity chose ONE
pack with a subject-organized internal module tree, not N packs.

### 1.3 Module system practice for multi-file trees

- `:- module(Name, Exports)` + `use_module/1,2` is the whole mechanism;
  there is no package/namespace layer above modules in stock SWI.
- **`file_search_path/2`** is the documented answer for "sub-projects using
  search paths" inside one repo: define one alias anchored to the projec
  root via `prolog_load_context(directory, Dir)`, then per-sub-projec
  aliases pointing into it:

  ```prolog
  :- prolog_load_context(directory, Dir),
     asserta(user:file_search_path(myapp, Dir)).
  user:file_search_path(graph, myapp(graph)).
  user:file_search_path(ui,    myapp(ui)).
  ```

  Sub-projects then `use_module(graph(foo))` instead of `use_module('../../graph/foo')`.
  This is SWI's own recommended fix for exactly the relative-path fragility
  problem `v6/prolog` has today (Part 2). [Sub-projects using search
  paths](https://www.swi-prolog.org/pldoc/man?section=projectpaths).
- **`attach_packs/2`** plus a local `packs/` directory is the documented
  pattern for an application that owns several of its own packs withou
  publishing them: `:- attach_packs(packs, [replace(true)]).` restricts pack
  search to that directory. For real version pinning inside one repo, the
  docs recommend `git submodule add <url> packs/<name>` per pack, so each
  sub-pack keeps its own git history and pinned SHA  -  the closest SWI
  equivalent to a Cargo path-dependency-with-a-lockfile. [Using packs for a
  specific application](https://www.swi-prolog.org/howto/ApplicationPack.md).
  This mechanism is generally used for THIRD-PARTY packs an app vendors in,
  not for a project's own first-party source split  -  for first-party code in
  one repo, `file_search_path` aliasing (previous bullet) is the lighter,
  more common tool; `attach_packs` + submodules is the heavier tool for when
  sub-components need independent git history.
- **Logtalk**, priced honestly: it is a compiler that targets a backend
  Prolog (SWI among others), adding objects/protocols/categories/parametric
  objects as a real object layer on top of plain modules. It is a genuine,
  actively maintained packaging answer, distributed itself as an [SWI pack
  named `logtalk`](https://us.swi-prolog.org/pack/list?p=logtalk). Cost: (1)
  every consulted file now compiles through Logtalk first, a second
  toolchain in the loop; (2) its vocabulary (protocols, categories,
  parametric objects) is OO terminology that has no rxjs/prolog/SQL
  equivalent, which collides head-on with this repo's standing "construc
  names use only rxjs/prolog/SQL words" law; (3) it solves a problem sprefa
  does not have  -  inheritance/interface separation across an object graph  -
  and does not touch the actual pain point found in Part 2 (relative-path
  fragility, one small cross-directory dependency cycle). Not a good fit;
  named and priced rather than dismissed in one line, per the standing
  build-vs-buy law.
- SWI's own `library/` tree and s(CASP) both confirm the working default a
  this scale is: **flat or lightly-nested modules, explici
  `use_module/2` export lists, no namespace ceremony beyond the module
  system**. No large SWI project examined uses a home-grown package manager
  layered under packs.

### 1.4 Testing conventions

- The **idiomatic** SWI convention is a `.plt` sidecar file: `foo.pl` +
  `foo.plt`, auto-discovered by `load_test_files/1` because it matches the
  loaded file's basename with the extension swapped. It does **not**
  recurse into a `test/` subdirectory  -  that's a hole in the built-in
  discovery mechanism, not a convention choice. [Using separate tes
  files](https://www.swi-prolog.org/pldoc/man?section=testfiles).
- In practice, **real packs don't follow the sidecar convention**  -  tokenize
  and s(CASP) both use a `test/` directory with hand-named files loaded by a
  runner script, not `.plt` sidecars. There is an [open, unresolved GitHub
  issue](https://github.com/SWI-Prolog/roadmap/issues/60) asking SWI-Prolog
  itself to standardize a pack test layout (`pack_run_tests/2`,
  `pack_verify/2`); no consensus, no owner, still open.
- Conclusion: sprefa's own `compile/test/*.pl` + `conformance/go.pl`-driven
  fixture corpus (a hand-rolled runner over `.pl` fixture files, not `.plt`
  sidecars) is not a deviation from real practice  -  it's the same shape
  real packs use, because the "correct" sidecar convention doesn't scale to
  a fixture corpus anyway (a fixture is a program-under-test, not a test of
  one specific `.pl` file).

---

## Part 2: the census (v6/prolog/**, read-only)

97 `.pl` files, 37,041 lines total (`wc -l` receipts, taken at `61f75b0e`).

### 2.1 Size by directory

| directory | files | lines | contents |
|---|---:|---:|---|
| `v6/prolog/*.pl` (root) | 14 | 4,952 | shared expansion/analysis primitives + `ARCH.pl` |
| `v6/prolog/compile/*.pl` | 14 | 11,604 | the tsv2 compiler front (parse/analyze/strat/lower/emit) |
| `v6/prolog/compile/scripts/*.pl` | 6 | 894 | CLI/grading scripts (roundtrip, sweep, bop_check, oracle door) |
| `v6/prolog/compile/test/*.pl` | 5 | 4,799 | compiler's own plunit suite (`plunit_tests.pl` alone is 3,374) |
| `v6/prolog/conformance/*.pl` | 6 | 2,126 | oracle reference engine + grading harness + rulings ledger |
| `v6/prolog/conformance/fixtures/*.pl` | 34 | 7,067 | the fixture corpus (data: programs-under-test + expected schedules) |
| `v6/prolog/tools/*.pl` | 4 | 1,011 | whole-tree analysis (lint, xref, self-map, ARCH-map) |
| `v6/prolog/src/*.pl` | 4 | 388 | small shared utility cluster + one dead module (below) |
| `v6/prolog/examples/*.pl` | 1 | 113 | `ghcacher.pl`, a standalone demo program |
| `v6/prolog/labs/**/*.pl` | 9 dirs, N files | 4,087 | design labs, mixed status (`canonical_plan`/`labbed`/live), see 2.5 |

`compile/dl_view/` (279 tracked files, generated `.dl6` renderings) and
`compile/out/` (850 tracked files, generated grading artifacts  -  `.ts`,
`.jsonl`, `.json`) are **build output, not source**, and are tracked in gi
(`git ls-files` confirms both, no `.gitignore` excludes them). They add
1,129 tracked non-`.pl` paths under `compile/` that any physical-move
scheme has to account for or explicitly leave behind.

### 2.2 Module-per-file

52 of 97 files carry `:- module(Name, Exports)`; 45 are unmoduled  -  mostly
the `conformance/fixtures/*.pl` corpus (loaded as plain files via
`ensure_loaded`, not consulted as modules  -  correct, they're fixture data,
not libraries), `ARCH.pl`, and the `compile/scripts/*.pl`/`compile/test/*.pl`
runner scripts. (Full module list obtained via
`grep -rln -m1 '^:- module(' v6/prolog --include='*.pl'`.)

**Module-name collision, already resolved on this base**: two files each
declare a module people would naturally call `emit_ts`:

- `v6/prolog/compile/emit_ts.pl:34` → `:- module(emit_ts, ...)`  -  the live
  tsv2 emitter, 2,157 lines, used throughout `compile/`.
- `v6/prolog/src/emit_ts.pl:27` → `:- module(emit_ts_engine_v1, ...)`  -  a
  258-line "engine-v1 seam experiment" that `ARCH.pl:171` itself labels
  **"superseded by the tsv2 rows below"**. Nothing in the tree
  `use_module`s it or references `emit_ts_engine_v1:` anywhere
  (`grep -rn "emit_ts_engine_v1"` finds only its own module declaration and
  two `ARCH.pl` prose mentions). It is dead code kept for history, not a
  live second implementation.

Any partition scheme should treat `src/emit_ts.pl` as **excluded /
archived**, not carried into a new pack as live source  -  carrying i
forward silently would ship a self-declared-dead module as if it were
current.

### 2.3 The dependency graph (file-level, `use_module`/`ensure_loaded`)

232 `use_module`/`ensure_loaded` directives total outside `labs/`; 100 of
those reference another project file by a bare or `../`-relative path
string (26 bare-relative like `'compile/registry'`, 74 `../`-relative).
Zero use absolute paths or `file_search_path` aliases  -  every internal
cross-reference in the tree today is a literal relative path string.

At file granularity the graph is a DAG (no file imports a file tha
(transitively) imports it back). At **directory granularity**  -  the natural
first cut for "one directory = one pack"  -  it is **not** a DAG. Two
independent two-way couplings:

**Coupling 1  -  `compile/` ↔ `conformance/`.**

- `conformance/engine.pl:85` → `use_module('../compile/3_clock_check', [clock_violation/2])`
- `compile/3_clock_check.pl:26` → `use_module('../conformance/body', [rel_ref/2])`

`rel_ref/2` is one clause:

```prolog
% v6/prolog/conformance/body.pl:24
rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).
```

That single `functor/3` wrapper is the entire reason `compile/` depends on
`conformance/` at directory granularity. The reverse edge
(`conformance/engine.pl` → `compile/3_clock_check.pl` for
`clock_violation/2`) is a real, deliberate reuse  -  the standing law tha
mirrored cross-plane checks share one implementation (ledger:
"`0_program_check.pl` = 6 mirrored cross-plane checks one impl")  -  not a
tangle to undo, just a one-way dependency that only becomes a problem
because the other direction also exists.

**Coupling 2  -  root (`0_*`/`1_*`) ↔ `compile/` and `conformance/`.**

- `v6/prolog/0_type_plane.pl:61` → `use_module('conformance/body', [json_canon/2])`  -  a core/shared file reaching UP into the oracle's directory for a 6-clause, 16-line JSON canonicalizer (`conformance/body.pl:133-148`).
- `v6/prolog/0_refusal_messages.pl:19` → `use_module('compile/3_clock_check', [clock_refusal_reason/1])`  -  a core/shared file reaching UP into the compiler for what turns out to be **two facts**: `clock_refusal_reason(clock_path_conflict(_,_,_,_)).` and `clock_refusal_reason(unconstructive_clock_cycle(_,_)).` (`compile/3_clock_check.pl:393-394`).

So the entire directory-level cycle problem reduces to **three small,
misplaced predicates**: `rel_ref/2` (1 clause), `clock_refusal_reason/1` (2
facts), `json_canon/2` (6 clauses, 16 lines)  -  about 20 lines total. Each
currently lives one directory "too high" relative to where its consumers
sit. Relocating them downward (toward `compile/registry.pl` or the roo
`0_*` layer, which is where their siblings already live  -
`conformance/body.pl:19` already imports `compile/registry` and
`conformance/ticklog.pl:33` already imports `0_type_plane` for
`js_float_text/2`, so the traffic direction root/registry → conformance is
already established as the norm) turns the directory graph into a clean
layered DAG with **zero code behavior change**, just three predicates
moving to a lower layer and three `use_module` lines flipping direction.

**Everything else is a clean layered DAG** once those three move:

```
compile/registry.pl  (leaf: 0 project deps)
src/{kernel.pl, grader.pl, checks.pl}  (leaf: 0 project deps, independent axis)
        │
        ▼
root 0_*/1_* files (13 files, ~4,041 lines after json_canon/2 arrives)
   depends on: registry.pl
        │
        ├──────────────┐
        ▼               ▼
compile/*.pl        conformance/*.pl + fixtures/
(depends on: root,   (depends on: root, registry,
 registry)             compile/3_clock_check for
        ▲               clock_violation/2 reuse  -  ONE-WAY
        │               once rel_ref/2 moves)
compile/test/*.pl ──── depends on BOTH compile/ and
                        conformance/ (test-only, expected  -
                        a compiler test suite grading agains
                        the oracle is supposed to depend on it)

examples/ghcacher.pl → src/{kernel,checks,grader} only
   (an ISLAND  -  does not touch compile/, conformance/, or registry.pl at all)

tools/*.pl → depends on everything (directory-walk + explicit ARCH.pl/
   rulings.pl loads); nothing depends on tools/. A workspace-level dev
   tool, not a pack dependency.

ARCH.pl → src/kernel.pl, conformance/rulings.pl, src/grader.pl
   (the one file that ties src/, conformance/, and the workspace together;
   naturally a workspace-root file, not inside any pack)
```

`conformance/rulings.pl` (546 lines) is itself a leaf  -  zero
`use_module`s, pure `ruling/4` facts.

### 2.4 Hardcoded/fragile paths  -  every one found

The brief specifically asks for every hardcoded relative path the compile
scripts use. Full list:

**Prolog-internal (100 call sites, `v6/prolog/**/*.pl`, non-labs):**
26 bare-relative (`'compile/registry'`, `'0_type_plane'`, ...) + 74
`../`-relative (`'../0_program_check'`, `'../../conformance/engine'`, ...).
None are portability bugs today (SWI resolves `use_module` relative to the
*consulting file's* directory, not cwd, so these work regardless of where
`swipl` is invoked from)  -  but every one of them is a path string tha
encodes today's directory layout, so every one breaks or needs rewriting
under any scheme that moves files relative to each other. `tools/self_map_facts.pl:110,129` additionally hardcodes
`'../compile/registry.pl'` and `'../1_expansion.pl'` as **string literals
constructed for a different purpose** (feeding sprefa's own self-diagnosis
tooling)  -  a second edit site beyond the plain `use_module` list if those
files move.

**Shell-script side (`v6/prolog/**/*.sh`):** all six scripts under
`compile/scripts/` and `tools/prolog-lint.sh` compute their own directory
via `HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"` and derive
sibling paths from that  -  genuinely portable, not a fragility source. The
top-level `justfile` (`v6/justfile`) instead hardcodes the tree shape
directly in six recipe lines, e.g. `cd {{v6}}/prolog/conformance && swipl -q
-l go.pl -g go -g halt` (line 24), `cd {{v6}}/prolog/compile && bash
scripts/roundtrip.sh` (line 28), `swipl -q -l {{v6}}/prolog/ARCH.pl -g go -g
halt` (line 40). These six lines are the edit sites for any directory
reshuffle at the workspace-runner level.

**TypeScript-side bridge (the real "what breaks" list, since these are
the critical runtime call sites, not test-only):**

| file:line | reference |
|---|---|
| `v6/tsv2/serve/0_compile.ts:36` | `fileURLToPath(new URL("../../prolog/compile/compile.pl", import.meta.url))`  -  every served `.dl6` compile shells out to this exact path |
| `v6/tsv2/cli/bop.ts:54` | `fileURLToPath(new URL("../../prolog/compile/scripts/bop_check.pl", import.meta.url))`  -  the CLI's own env-check |
| `v6/tsv2/scripts/manifest-reason-diff.ts:177` | `"../prolog/compile/out/manifest.json"` |

**Outside `v6/prolog` entirely, found incidentally (not in the census
scope, flagged because it's a real hardcoded-path defect on the same
subject):** `v6/sprefa-store/bench/engines/swi_ts.sh:8` hardcodes the
absolute binary path `/opt/homebrew/bin/swipl` (breaks on any machine
without Homebrew at that prefix) and references `books/v6/dl_to_ts.pl`, a
prolog file living in a **third** location (`books/v6/`, repo root, outside
both `v6/prolog` and `v6/sprefa-store`) not touched by this census. Noted
for completeness, not priced into the candidates below  -  pre-existing,
unrelated to `v6/prolog`'s own partition.

**Related-but-different concern found in `ARCH.pl:803`**, worth
distinguishing so it isn't conflated with this doc's question: "swipl
packaging unanswered (otool shows `@rpath/libswipl.10.dylib`, a
distributable needs a static SWI build)" is about **shipping a compiled
binary that embeds the SWI runtime**  -  a deployment/distribution problem.
This doc is about **source-tree organization for development**  -  a
different axis entirely. A partition scheme here does not answer that one.

### 2.5 `labs/`

9 subdirectories, 4,087 lines: `csp_idioms`, `extract_t2`, `generic_scan_
instantiation`, `json_interop`, `json_syntax`, `openapi_codegen`,
`rel_as_stream`, `rel_definition_hash`, `type_matrix`. `ARCH.pl` (task
`stale_labs_sweep`, `done`, landed same base commit as this merge) records
these as **already triaged**: "5 folded, 2 kept" as of the most recen
sweep, with individual labs separately tracked at statuses
`canonical_plan` (`rel_definition_hash_lab`) and `labbed`
(`json_interop_lab`) rather than "stray leftover." Not all of `labs/` is
equally alive; `ARCH.pl` is the source of truth for which. None of the
candidate schemes below assign `labs/` to a pack  -  it stays a plain
directory outside pack membership in every candidate, consistent with how
it's already treated (scratch/design space, not shipped source).

---

## Part 3: candidate partition schemes

Three candidates, ordered by cost. All three keep `labs/` untouched and
`ARCH.pl` + `tools/` at the workspace root (nothing in Part 2 makes them
fit inside a single pack  -  `tools/` depends on literally everything by
design, `ARCH.pl` is the cross-cutting architecture ledger).

### Candidate 1  -  wrap-only: one `pack.pl`, zero file moves

Drop `v6/pack.pl` next to the existing `v6/prolog/` directory. A pack only
requires a `prolog/` subdirectory under its root; `v6/prolog/` already *is*
one, nested subdirectories and all  -  SWI resolves `use_module` relative to
the consulting file regardless of pack-ness, so every one of the 100
existing relative-path call sites keeps working unmodified.

| | |
|---|---|
| file moves | 0 |
| path-string edits | 0 (internal) |
| new files | 1 (`v6/pack.pl`, ~10 lines, one `name`/`version`/`requires(prolog >= '...')`) |
| external edits | 0  -  nothing outside `v6/prolog` needs to know this happened |
| fixes the compile↔conformance coupling? | no  -  makes it irrelevant only if nobody ever splits further, since everything is still one unit |
| fixes path fragility? | no  -  the relative paths are still there, just now inside something called a pack |
| gets you: independent versioning, per-component test targets, publishable sub-units | no |

This satisfies the literal ask ("package-ify") at near-zero cost and is a
legitimate real-world shape  -  it's exactly what s(CASP) does (Part 1.2).
It does not address either of the two things Part 2 found actually wrong
(the directory-level coupling, the 100 unaliased relative paths). It is a
valid **first step under any of the other two candidates**, not a
competing end state.

### Candidate 2  -  in-repo alias workspace: fix the graph, move nothing

Add one `v6/prolog/load.pl` (or per-cluster small bootstrap files)
declaring `file_search_path/2` aliases per Part 1.3's documented pattern,
one alias per seam, anchored to today's real directories via
`prolog_load_context(directory, Dir)`:

```prolog
:- prolog_load_context(directory, Dir),
   asserta(user:file_search_path(dl_root, Dir)).
user:file_search_path(dl_registry, dl_root('compile')).   % registry.pl lives here
user:file_search_path(dl_core,     dl_root('.')).          % the 0_*/1_* files
user:file_search_path(dl_compile,  dl_root('compile')).
user:file_search_path(dl_oracle,   dl_root('conformance')).
user:file_search_path(dl_kernel,   dl_root('src')).
```

Then: relocate the three hinge predicates named in Part 2.3 (`rel_ref/2`,
`clock_refusal_reason/1`, `json_canon/2`  -  ~20 lines total, moving toward
`compile/registry.pl` or the root `0_*` layer where their siblings already
sit) so the directory graph becomes a clean one-way DAG, and rewrite the
100 relative-path call sites to alias form (`use_module(dl_core(type_plane))`
instead of `use_module('../0_type_plane')`). No file physically moves.

| | |
|---|---|
| file moves | 0 |
| path-string edits | ~100 (`use_module`/`ensure_loaded` call sites) + `tools/self_map_facts.pl`'s 2 hardcoded strings |
| predicate relocations | 3 (~20 lines: `rel_ref/2`, `clock_refusal_reason/1`, `json_canon/2`) + their import-list updates in ~5 consuming files |
| new files | 1 (`load.pl`) |
| external edits | `v6/justfile`'s 6 recipe lines need `-l load.pl` added before the entry file; the 3 TS-side hardcoded paths (`0_compile.ts:36`, `bop.ts:54`, `manifest-reason-diff.ts:177`) are UNCHANGED (nothing physically moved) |
| fixes the compile↔conformance coupling? | **yes**  -  this is the actual fix; after it, `compile/` no longer imports anything from `conformance/`, only the reverse (deliberate, one-way) |
| fixes path fragility? | **yes**  -  aliases are stable under any later physical reorganization; only `load.pl`'s alias targets need updating, not 100 call sites, if directories move later |
| gets you: independent versioning, publishable sub-units | no  -  still one pack (or no pack at all), just an internally clean graph |

This is the smallest change that fixes something real rather than adding
ceremony around an unfixed problem.

### Candidate 3  -  full multi-pack workspace

Physically split into 6 packs matching the DAG from Part 2.3, each with its
own `pack.pl` under a `prolog/` subdirectory (the real, publishable pack
convention  -  necessary if any component should ever be independently
`pack_install`-able, e.g. if the oracle engine or the registry construc
table is ever wanted by a sibling project), wired together via a shared
`v6/packs/` directory and `:- attach_packs(packs, [replace(true)]).`
(Part 1.3).

| pack | files | lines | deps |
|---|---:|---:|---|
| `dl_registry` | 1 (+3 relocated predicates) | ~471 | none (leaf) |
| `dl_kernel` | 3 (`kernel.pl`, `grader.pl`, `checks.pl`; `emit_ts.pl` excluded, dead per 2.2) | 130 | none (leaf, independent axis) |
| `dl_core` | 13 | ~4,041 | `dl_registry` |
| `dl_compile` | 24: 13 top (after `registry.pl` moves to `dl_registry`) + 6 `scripts/` + 5 `test/` (dev-only target) | 11,156 top+scripts (11,604 − 448 registry) + 4,799 test | `dl_core`, `dl_registry` runtime; `dl_oracle` for `test/` only |
| `dl_oracle` | 40 (6 top + 34 fixtures) | 9,193 | `dl_core`, `dl_registry`; `dl_compile` one-way for `clock_violation/2` reuse (fine once `rel_ref/2` moves out) |
| `dl_examples` | 1 | 113 | `dl_kernel` |

Source-file physical moves: **82 `.pl` files** across the 6 packs
(`dl_registry` 1 + `dl_kernel` 3 + `dl_core` 13 + `dl_compile` 24
(13 top after `registry.pl` moves out, + 6 `scripts/` + 5 `test/`) +
`dl_oracle` 40 + `dl_examples` 1 = 82).
Generated output (`compile/dl_view/` 279 files, `compile/out/` 850 files) is
build output, not source  -  **recommend it stays outside any pack's
`prolog/` tree** (regenerated by CI/scripts, not shipped), which keeps the
82-file number honest; if it moves with the pack instead, add 1,129 more
tracked paths.

| | |
|---|---:|
| file moves | 82 `.pl` (+ optionally 1,129 generated, not recommended) |
| path-string edits | ~100, alias form (same rewrite as Candidate 2, now pointing across real pack boundaries) |
| predicate relocations | 3, same as Candidate 2 (mandatory here  -  a real multi-pack split cannot ship a cycle across pack boundaries at all, packs don't tolerate it the way one module tree tolerates a directory-level cycle) |
| new files | 6 `pack.pl` manifests + 1 workspace `load.pl`/`attach_packs` bootstrap |
| external edits | same `justfile` + TS-side 3 paths as Candidate 2, plus every path now crosses a pack boundary so the alias targets point at `packs/dl_compile/prolog/...` instead of today's `compile/` |
| fixes the compile↔conformance coupling? | yes, and enforced structurally (a real pack boundary, not just a convention) |
| fixes path fragility? | yes |
| gets you: independent versioning, per-pack test targets, `pack_install`-able sub-units | **yes**  -  this is the only candidate that delivers the actual crate/workspace-member experience (`swipl pack install packs/dl_registry`, a `dl_oracle` pack a future project could `requires(dl_registry)` without pulling in the whole compiler) |

---

## Trade table

| | Candidate 1 (wrap) | Candidate 2 (alias, no moves) | Candidate 3 (multi-pack) |
|---|---|---|---|
| file moves | 0 | 0 | 82 `.pl` (+1,129 optional) |
| path edits | 0 | ~100 + 2 | ~100 |
| predicate relocations | 0 | 3 (~20 lines) | 3 (~20 lines) |
| new files | 1 | 1 | 7 |
| fixes the real coupling (2.3) | no | yes | yes |
| fixes path fragility (2.4) | no | yes | yes |
| independent pack versioning / installability | no | no | yes |
| matches a real precedent at this scale | yes (s(CASP)) | yes (SWI's own "sub-projects using search paths" doc, written for exactly one-repo-many-components) | yes (SWI's own multi-pack app pattern, heavier tool for a heavier need) |

**Smallest-correct read, under the standing "smallest correct" directive:**
Candidate 2. It is the only one of the three that actually repairs
something broken (the directory-level cycle, the unaliased relative
paths) rather than adding structure around an unrepaired problem (1) or
paying for independent-installability nobody has asked to use yet (3).
Candidate 1 is compatible with it (can be layered on top for free  -  a
`pack.pl` next to an already-clean alias graph costs nothing extra).
Candidate 3 becomes cheap later specifically because Candidate 2 already
did the two expensive parts (untangling the hinge predicates, converting
100 path strings to aliases)  -  at that point the multi-pack split is
mostly `git mv` plus splitting one `load.pl` into N `pack.pl`s.

This is a recommendation stated for the user to weigh, not a decision  -
per the brief, the user rules on which candidate to execute.

---

## Open items not resolved by this research (flagging, not deciding)

- Which exact target file each of the 3 hinge predicates moves to
  (`registry.pl` vs. staying in the root `0_*` layer) is an implementation
  call, not answered here.
- Whether `dl_compile`'s `test/` and `scripts/` subtrees are worth their own
  packs under Candidate 3 (this doc folds them into `dl_compile` as
  dev-only targets; a case exists for `dl_compile_test` as its own pack
  given `plunit_tests.pl` alone is 3,374 lines  -  not costed separately
  here, flagged for the user's call).
- `ARCH.pl:803`'s "swipl packaging unanswered" (static build for
  distribution) is a separate, real, and still-open problem; this doc does
  not address it.
