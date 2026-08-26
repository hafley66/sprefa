# extract move for TypeScript: corpus walk, path resolution, batch list

Auditor copy. Every claim carries `path:line`. Design decided 2026-08-25, not
re-litigated. No Rust written in this lane; signatures live here only.

<!-- todo(feature): the move's TS arm reads ~/.agent/dl6.db? it does not; it self-builds move_candidate from its own ast-grep scan, same as the prolog arm (0_move.rs candidate_store). The oxc Specifier rows (ts.rs:1271) are background, not the source of the move's facts. -->

<!-- todo(feature): alias / tsconfig paths / package.json exports resolution is out of scope for v1; oxc_resolver documented as the buy candidate for a v2 alias arm (risk table row). -->

## Context

`extract move` is prolog-only by construction. `0_move.rs` walks `.pl/.plt`
with `prolog_files` (`0_move.rs:461-487`), resolves prolog specifiers against
the loading file's directory taking `.pl` when bare (`0_move.rs:490-501`), and
stages one soopy `Replace` per importer plus one `Move` per moved file
(`0_move.rs:71-86`, `250`). The rule is data, `rules/move_specifier.yml`,
with `language: prolog` prepended by the caller (`0_move.rs:316-325`).

`lang/ts.rs` already emits `Specifier` rows for ES static imports and export-FROM
re-exports via oxc (`ts.rs:1143-1273`, `module_specifiers`), but nothing
consumes them and they exclude `require(...)` and `import x = require(...)`
(`ts.rs:1149-1152`). The oxc rows are not the move's fact source: arc C decided
the move self-builds its `move_candidate` rel from its own ast-grep scan so a
move is correct on any root (`plans/2026-08-25-extract-astgrep-soopy.PLAN.md`,
deviation 1 in PR #475).

Missing for TS: a `.ts/.tsx/.mts/.cts` corpus walk, TS path resolution
(extensionless, `index.ts`, `.js` written for `.ts`, package.json `exports`/
`main`, tsconfig `paths`/`baseUrl`), a second language arm in the specifier
rule, and a batch form `--list <tsv>` that stages every move and every importer
rewrite in one soopy transaction. First real corpus: hafley-rxjs grapht layout
(38 files into 4 folders), done by hand in another session. This plan builds on
the merged arcs A (#472), B (#473), C (#475).

The ast-grep surface the move uses is generic over `ExtractLang`
(`lang/extract_lang.rs`), so a TS arm is one more grammar selected by
`ExtractLang::Sg(SupportLang::TypeScript|Tsx)`; `SupportLang::TypeScript` covers
`.ts/.cts/.mts` and `Tsx` covers `.tsx` (ast-grep-language `lib.rs:485-486`).

## The four decisions

The issue lists four decision areas. Each is an arc below: corpus walk (arc 1),
the TS specifier rule (arc 2), path resolution (arc 3), batch `--list` (arc 4).
Arcs 1-3 are the single-move TS arm; arc 4 generalizes the plan to many moves.

## Arc 1: the TS corpus walk

### Decision

`prolog_files` (`0_move.rs:461-487`) is the walker. It BFS-walks the root,
skips `[.git, target, node_modules, .boop-worktrees]` (`0_move.rs:463`), and
collects `.pl/.plt`. The TS arm generalizes the same walker to the four TS
extensions plus the JS ones the ESM-style corpus writes.

Chosen: one corpus walker parameterized by extension set, collecting
`.ts .tsx .mts .cts .js .jsx .mjs .cjs`. Rejected: two separate walkers (one
prolog, one ts); the two never overlap and one walker with a predicate is less
code than two.

The JS extensions are in scope because the grapht corpus writes ESM-style
`.js` specifiers and a `.js` file can itself carry `import './x'` or
`import('./x')` that names a moved file. The corpus walk names a file a
"specifier carrier" if its grammar is TS-family.

Monorepo roots: hafley-rxjs is a workspace with a root `tsconfig.json` whose
`references` list eight `packages/*` (`~/projects/hafley-rxjs/tsconfig.json:1-9`).
The move's `--root` is the git root containing `old` (default, `0_move.rs:113-125`),
which for a monorepo is the repo root; the walker skips `node_modules` and
`.boop-worktrees` so the corpus stays the source tree.

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs (generalize prolog_files)
/// A corpus file plus the grammar that parses it. The rule body is shared
/// between TypeScript and Tsx; only the language name differs (extract_lang.rs name()).
enum CorpusLang { Prolog, Ts, Tsx }

/// Files that can carry a module specifier, in path order, each with its grammar.
fn specifier_corpus(root: &Path) -> Vec<(PathBuf, CorpusLang)>;

/// The per-grammar extension membership test.
fn is_ts_family(path: &str) -> bool; // .ts .tsx .mts .cts .js .jsx .mjs .cjs
```

Body (pseudo-code):

```rust
fn specifier_corpus(root: &Path) -> Vec<(PathBuf, CorpusLang)> {
    // BFS from root, skip [.git target node_modules .boop-worktrees] (mirror
    // 0_move.rs:463); collect files by extension:
    //   .pl/.plt -> Prolog
    //   .ts/.cts/.mts/.js/.mjs/.cjs -> Ts     (SupportLang::TypeScript, lib.rs:485)
    //   .tsx/.jsx -> Tsx                      (SupportLang::Tsx, lib.rs:486)
    // sort by path (preserve the merge-order guarantee, 0_move.rs:139)
}
```

### Instance lifetimes

`CorpusLang` is a `Copy` token; the per-file value lives for the parse call and
is moved into the `ExtractLang`/`StrDoc` (`extract_lang.rs` `build_pattern`).
The `Vec<(PathBuf, CorpusLang)>` lives for `Plan::build`, parallel-scanned by
the rayon pool (`0_move.rs:141-155`).

### Storage layout

None new. Read sequence: root -> walk -> per-file `(path, lang)` -> bytes ->
`ExtractLang::Sg(SupportLang::...)` -> ast-grep parse -> `Vec<SpecRows>`.
Uniqueness: one `(path, lang)` per file, first extension match wins.

### Files touched (Arc 1)

- `v6/sprefa-extract/src/0_move.rs`: replace `prolog_files` with
  `specifier_corpus`; thread the per-file `CorpusLang` into the rule load and
  the drain.
- `v6/sprefa-extract/src/lang/mod.rs`: no change (TS already routes via
  `SupportLang`, `mod.rs:59-71`).

### Files forbidden (Arc 1)

- `src/lang/ts.rs` projection logic (the oxc `Specifier` emission stays; the
  move does not read it).
- `v6/prolog/**`, the soopy crate.

### Tests to add (Arc 1)

- `corpus_walks_ts_and_tsx_but_skips_vendor` (tests/1_move.rs): a temp repo
  with `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, a `node_modules/` and a
  `.boop-worktrees/` dir; assert the walker returns the six TS-family files and
  not the two skip dirs. Breaks if the skip list or extension set is wrong.

### Gate command (Arc 1)

```sh
cargo test --release --features cli --test 1_move
```

### Risk table (Arc 1)

| risk | citation that raises it | mitigation |
|---|---|---|
| `.js`/`.jsx` carriers double-count when a `.js` re-exports a `.ts` | ts.rs:1149-1152 (extractor omits some) vs the move's own scan | the move scans every carrier independently; a `.js` that names a moved `.ts` is a real importer to rewrite |
| monorepo walk cost on a large tree | 0_move.rs:461-487 BFS | skip list already drops `node_modules`/`.boop-worktrees`; corpus scale is the move's existing bound |

## Arc 2: the TS specifier rule

### Decision: one rule file per language arm, matched node is the whole `string`

The prolog rule matches the `atom` node including its quotes and the replacer
re-quotes (`0_move.rs:447-470`, `unquote`/`requote`). The TS analogue matches
the whole `string` node (`string` = `'"'` + fragment + `'"'`, tree-sitter-
javascript `grammar.js:932-951`), and rewrites its full text preserving the
quote char. Matching the whole `string` keeps one replacer shape for both
languages and never has to splice a `string_fragment` (the inner content,
`grammar.js:955-958`).

One rule file per language, not one file with two arms: `ast_grep_config`
`SerializableRuleConfig` carries a single `language: L` field
(`rule_config.rs:74`), and the caller prepends it (`0_move.rs:316-325`), so a
single YAML file cannot hold two different rule bodies for two grammars.
Existing `rules/move_specifier.yml` stays the prolog arm; add
`rules/move_specifier.typescript.yml` for the TS family. The `.tsx` file routes
through the same rule body with `language: tsx` prepended.

### The rule body

The `string` node must be a module source. tree-sitter-typescript fields the
source on `import_statement` (`common/define-grammar.js:308-316`,
`field('source', $.string)`), on `export_statement` via the hidden `_from_clause`
(`javascript/grammar.js:217-219`, `define-grammar.js:320-328`), and on dynamic
`import()` as a `call_expression` argument (`javascript/grammar.js:785-792`).
ast-grep's `inside` accepts a `field` key and matches a node that is the field
child of an ancestor of the given kind (`relational_rule.rs:68-91`).

```yaml
# rules/move_specifier.typescript.yml
# The string literal that names a module: the `source` field of a static
# import/export, or a string argument of any call (dynamic import(), require()).
# language: is prepended by the caller, prolog-arm precedent 0_move.rs:316-325.
id: move-specifier-ts
rule:
  any:
    - kind: string
      inside:
        kind: import_statement
        field: source
    - kind: string
      inside:
        kind: export_statement
        field: source
    - kind: string
      inside:
        kind: call_expression
        field: arguments
```

The third arm deliberately matches any call with a string argument, not only
`import(...)`: the `FactMatcher` gate (`fact.rs:238-250`) fires only on strings
whose value names a moved file, so `require('./moved')` and `import('./moved')`
both re-aim, and an unrelated `readFile('./moved')` also re-aims, which is
correct. Narrowing to `import(...)` alone would need a pattern pin on the
function being the `import` keyword, which YAML relations cannot express; the
value gate is the precise filter. This is a documented widening over the issue's
`call_expression (dynamic import())` list, with the citation that forces it.

The `specifiers()` reader (`0_move.rs:327-340`) is unchanged: `matched.text()`
for a `string` node includes the quotes, so `raw` is the full quoted spec and
`by_raw` keys on it, exactly like the prolog atom.

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs
/// Load the rule body for a corpus language, prepending its grammar name
/// (mirror specifier_rule, 0_move.rs:316-325).
fn specifier_rule_for(lang: CorpusLang) -> Result<RuleConfig<ExtractLang>, String>;
```

Body (pseudo-code):

```rust
fn specifier_rule_for(lang: CorpusLang) -> ... {
    let (yaml, name) = match lang {
        Prolog => (MOVE_SPECIFIER_RULE, "prolog"),
        Ts | Tsx => (MOVE_SPECIFIER_TS_RULE, if Ts {"typescript"} else {"tsx"}),
    };
    from_yaml_string(&format!("language: {name}\n{yaml}"), &GlobalRules::default())?
        .into_iter().next()
}
```

### Instance lifetimes

The `RuleConfig<ExtractLang>` is built once per corpus language per run and
shared across the parallel scan (`0_move.rs:141-155`); `ExtractLang` is `Copy`.

### Storage layout

None new. Read sequence: rule file bytes (compile-time `include_str!`) ->
`RuleConfig` -> ast-grep `find_all` per file. Uniqueness: one rule body per
grammar.

### Files touched (Arc 2)

- `v6/sprefa-extract/rules/move_specifier.typescript.yml` (new).
- `v6/sprefa-extract/src/0_move.rs`: `specifier_rule_for`, thread the rule into
  `Plan::build` and `drain_file`.

### Files forbidden (Arc 2)

- `v6/sprefa-extract/rules/move_specifier.yml` (the prolog arm, unchanged).
- `src/lang/1_ast_rule.rs` (its `RuleCore` construction stays private;
  `from_yaml_string` is the move's door, arc C precedent).

### Tests to add (Arc 2)

- `ts_rule_finds_import_export_and_dynamic_import_sources` (tests/37_fact_matcher.rs
  style): run the TS rule body over a fixture with `import './a'`,
  `import {x} from './b'`, `export {y} from './c'`, `export * from './d'`,
  `import('./e')`, and one non-source string (`const s = "hello"`); assert the
  rule matches the five sources and not the plain string. Breaks if the `inside
  field: source` wiring is wrong.
- `ts_rule_preserves_quote_style` (tests/1_move.rs): a single-quoted and a
  double-quoted source both re-aim and keep their quote. Breaks if the replacer
  hardcodes a quote.

### Gate command (Arc 2)

```sh
cargo test --release --features cli --test 37_fact_matcher --test 1_move
```

### Risk table (Arc 2)

| risk | citation that raises it | mitigation |
|---|---|---|
| the `source` field is defined inside a hidden `_from_clause`, so `child_by_field_id(source)` on the statement must still return the string | javascript/grammar.js:217-219, 932-951 (hidden rules inline in tree-sitter) | gate a static and a re-export fixture; if the field walk misses, fall back to `inside { kind: string }` under the statement kinds and filter by "has a sibling `from`" |
| the `call_expression` arm over-rewrites non-path string args | relational_rule.rs:68-91 (inside matches any call) | value gate (FactMatcher) fires only on moved-target strings; test that a `"hello"` literal is untouched |
| TSX uses the same grammar but a different `language:` name | ast-grep-language lib.rs:485-486 | prepend `language: tsx` for `.tsx`; one rule body shared |

## Arc 3: TS path resolution

### Build-vs-buy: the resolution engine

The move resolves a specifier only to answer one question: does it name a file
being moved? That is a file-existence probe, not a bundler load. Candidates,
measured against the crate's existing dependency set (`v6/sprefa-extract/
Cargo.toml`):

| candidate | what it resolves | deps | verdict |
|---|---|---|---|
| `oxc_resolver` 11.24 | ESM/CJS, extensionless, `index.ts`, `.js`->`.ts` (extensionAlias), tsconfig `paths`/`baseUrl` incl. extends/references, package.json `exports`/`main`, node_modules, in-memory `FileSystem` trait | oxc project family (serde/serde_json already present), rust 1.95 | DEFERRED to v2 alias arm (below) |
| `swc` resolver | module resolution inside its transform pipeline, no standalone ergonomic crate | `swc_ecma_*` | REJECTED: no clean standalone crate |
| `tsconfig` / `tsconfig-resolver` crates | parse tsconfig only, no resolution | serde_json | REJECTED: partial, resolution absent |
| hand-rolled relative probe | relative extensionless / `index.ts` / `.js`->`.ts` | none | TAKEN for v1 |

Why the hand-rolled relative probe is taken and `oxc_resolver` deferred, with a
written excuse (v6 rule 1, `v6/README.md`):

1. A bare specifier (`zod`, `rxjs`) resolves into `node_modules` and can never
   name a file the move relocates inside the root, so package.json
   `exports`/`main` resolution is moot for the move's question. The only
   specifiers that can name a moved file are relative (`./`, `../`) or a
   tsconfig-path alias into the source tree.
2. The acceptance corpus (grapht) writes only relative ESM-style specifiers
   (`from "./0_benchProtocol.js"`, `~/projects/hafley-rxjs/packages/grapht/src/
   index.ts:12-40`) and its `tsconfig.json` has no `paths`/`baseUrl`
   (`packages/grapht/tsconfig.json:1-19`). Relative resolution is the whole job.
3. The relative probe mirrors the existing prolog `resolve` (`0_move.rs:490-501`)
   and the repo's own TS `source_type_for` extension table (`ts.rs:89-99`), so
   it is a copy of in-repo precedent, not bespoke logic.
4. tsconfig-path alias resolution is the one genuinely nontrivial case (extends,
   references, `${configDir}`, `*` wildcards) and is exactly what `oxc_resolver`
   implements; that is a v2 arm behind a `cli` feature, not a v1 need.

### The resolution algorithm

Relative specifier `./x` in loading dir `D`:

1. If the spec starts with `./` or `../` (or is an absolute path): resolve
   against `D`, else skip (bare/alias = not a move target).
2. Strip quotes. Normalize `D + bare`.
3. Probe in order, first existing file wins:
   - exact `D/bare` (already a file, or the moved file's old path),
   - `bare.ts`, `bare.tsx`, `bare.mts`, `bare.cts`, `bare.d.ts`,
     `bare.js`, `bare.jsx`, `bare.mjs`, `bare.cjs` (ESM-style `.js` for `.ts`),
   - `bare/index.ts`, `bare/index.tsx`, `bare/index.d.ts`,
     `bare/index.js`, `bare/index.mjs`, `bare/index.cjs`.

The probe extension order reuses `source_type_for`'s table (`ts.rs:89-99`).

### The replacement (re-aim)

For an importer at path `P` writing spec `S` (quote `q`, bare `b`) that resolves
to moved file `M` now at `M'`:

```
relative = relative_from(dir(P), M')        // 0_move.rs:544-558, existing helper
replacement = requote(q, with_ext_style(relative, b))
```

`with_ext_style` maps the relative path's extension to match the original
spec's extension style, so an ESM-style `.js` import stays `.js`
(`./0_benchProtocol.js` -> `./0_bench/0_protocol.js`), a `.ts` import stays
`.ts`, and an extensionless import stays extensionless. The quote char `q` is
preserved. For a moved file's own internal relative specs, the re-aim runs from
the moved file's new directory (the `is_old` branch, `0_move.rs:180-215`).

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs
/// Resolve a TS specifier written in dir to a file path, or None (not a move
/// target: bare package, alias, unresolvable).
fn resolve_ts(dir: &Path, raw: &str) -> Option<PathBuf>;

/// The replacement spec preserving quote and extension style.
fn spec_text_ts(from_dir: &Path, target: &Path, original: &str) -> String;
```

Body (pseudo-code):

```rust
fn resolve_ts(dir, raw) -> Option<PathBuf> {
    let (_, bare) = unquote_ts(raw);                       // strip ' or "
    if bare.is_empty() || !(bare.starts_with("./") || bare.starts_with("../")
        || Path::new(bare).is_absolute()) { return None; } // bare/alias skip
    let joined = normalize(&dir.join(bare));               // 0_move.rs:560
    for probe in ts_probe_paths(&joined) {                 // extless, .ts.., index
        if probe.is_file() { return Some(probe); }
    }
    None
}

fn spec_text_ts(from_dir, target, original) -> String {
    let (q, bare) = unquote_ts(original);
    let rel = relative_from(from_dir, target);             // 0_move.rs:544-558
    let rel = with_ext_style(&rel, bare);                  // .js stays .js, etc.
    requote_ts(q, &rel)                                    // keep ' or "
}
```

### Instance lifetimes

`resolve_ts`/`spec_text_ts` are pure, no state. The resolver runs inside the
parallel prescan (`0_move.rs:141-155`) and the sequential merge
(`0_move.rs:139-220`).

### Storage layout

Input: the corpus file paths (read for parse) and the moved `old`/`new` paths.
Read sequence: spec -> resolve probe (filesystem stat) -> target; replacement =
pure path math. Uniqueness: one rewrite per raw spec text per file
(`rewrites: BTreeMap<rel, BTreeMap<raw, replacement>>`, `0_move.rs:164`).

### Files touched (Arc 3)

- `v6/sprefa-extract/src/0_move.rs`: `resolve_ts`, `spec_text_ts`, `unquote_ts`,
  `requote_ts`, `ts_probe_paths`, `with_ext_style`.

### Files forbidden (Arc 3)

- No new `Cargo.toml` dependency in v1 (the hand-rolled probe uses only `std`).
  `oxc_resolver` is documented for v2, not added here.

### Tests to add (Arc 3)

- `ts_fixture_relative_index_extensionless_and_js_for_ts` (tests/1_move.rs, the
  issue's receipt): a fixture repo under `tests/fixtures/ts_move/` with four
  cases: relative with extension (`from './b.ts'`), extensionless
  (`from './b'` -> `./b.ts`), `index.ts` (`from './dir'` -> `./dir/index.ts`),
  and ESM-style (`from './b.js'` -> `./b.ts`). Move `b.ts` into a subdir and
  assert every importer re-aims and every probe resolves. Breaks if any probe
  order or the re-aim extension mapping is wrong.
- `ts_moved_file_reaims_its_own_relative_imports` (tests/1_move.rs): the moved
  file imports a sibling that also moves; both land correctly from the new dir.
  Breaks if the `is_old` new-dir re-aim is wrong.
- `ts_bare_and_alias_specifiers_are_untouched` (tests/1_move.rs): `import "zod"`
  and a `@/x` alias stay byte-identical. Breaks if resolution wrongly treats a
  bare/alias spec as a move target.

### Gate command (Arc 3)

```sh
cargo test --release --features cli --test 1_move
# plus byte-identical `tsc --noEmit` / `node --check` on the fixture before and
# after (the issue's receipt) where the fixture has a tsconfig / is runnable.
```

### Risk table (Arc 3)

| risk | citation that raises it | mitigation |
|---|---|---|
| extensionless `./b` resolves differently than the bundler would | ts.rs:89-99 extension table | probe order matches `source_type_for`; gate the index/extensionless fixtures |
| ESM `.js` written for `.ts` is the dominant grapht style | index.ts:12-40 (`from "./0_benchProtocol.js"`) | `ts_probe_paths` tries `bare.js`; `with_ext_style` keeps `.js` on the way out |
| tsconfig `paths` aliases can name moved files | oxc_resolver README (the tsconfig-paths feature) | out of scope v1, documented; v2 buys `oxc_resolver` behind `cli` |
| package.json `exports`/`main` never name an internal moved file | resolution semantics | bare specifiers resolve to `node_modules`, skipped by `resolve_ts`'s relative-only gate |

## Arc 4: batch `--list <tsv>`

### Decision

The current CLI moves one file: `extract move <old> <new>` (`0_move.rs:43-62`).
The batch form adds `--list <tsv>`, where each line is `old<TAB>new` and every
row is one move. The plan generalizes `Plan` from one `(old, new)` to a set of
moves `old -> new`, then stages the whole batch.

soopy accepts ONE operation per source file (`0_move.rs:93-94`, cited
`_7d_mutation_plan.rs` `insert_non_replace`), so the batch cannot put a file's
`Replace` and its `Move` in one `StageRequest`. The batch therefore collapses
into the same stage shape as the single move:

- stage 1: one soopy `Replace` per distinct importer, across all moves, in ONE
  `StageRequest` (`drain::replace_action`, `drain.rs:84-97`);
- stage 2: one soopy `Move` per moved file, in ONE `StageRequest`
  (`0_move.rs:250`).

This satisfies "every move and every importer rewrite in ONE soopy StageRequest"
at the stage granularity the one-op-per-file rule forces.

### Expected hashes when two moves touch the same importer

A file `X` importing both moved `A` and moved `B` contributes both edits to one
`Replace` on `X`; `by_raw` is a `BTreeMap<raw, replacement>` (`0_move.rs:164`),
so two raws in one file are two edits in one `Replace`, and the `expected` hash
is computed once against `X`'s pre-move bytes. `bind_action` re-reads each
`expected` at the root that actually stages it (`drain.rs:141-171`), so the hash
matches the on-disk bytes at stage time regardless of ordering (arc B deviation
2, PR #473). Within stage 1, edits never re-read each other (they are one
Replace); between stages, stage 1 commits before stage 2's moves read their
`expected` (the moved file's post-edit bytes), which is exactly the single-move
contract.

### Ordering and validation

- `--list` rows are processed in file order; the corpus merge stays sequential
  over path order (`0_move.rs:139`).
- Validation before staging: every `old` must exist; every `new` must not
  exist; no `new` may equal any `old` (a move cannot overwrite a still-present
  source of another move); no two `new` equal. A violation is a hard error
  before any stage is built.
- A file that is both an importer and a moved file lands in stage 1 (its
  internal re-aims) and stage 2 (its Move), per the one-op-per-file split.
- Dry run prints one plan: the existing `Mirror` (`0_move.rs:723-773`) copies
  every file any stage touches, and every stage commits against it with
  `Durability::DryRun` (`0_move.rs:82-86`), so a batch dry run shows the full
  preview set and leaves the tree untouched.

### Type signatures

```rust
// v6/sprefa-extract/src/0_move.rs
/// One move in a batch: source rel -> destination rel (root-relative).
struct MoveSpec { old_rel: String, new_rel: String }

/// Read a `--list` tsv (old<TAB>new per line) into validated MoveSpecs.
fn read_move_list(path: &Path) -> Result<Vec<MoveSpec>, String>;

// Plan::build gains a move set instead of one (old, new):
//   moves: Vec<(old_rel, new_rel)>
// the `is_old` test becomes `moves.contains_key(rel)`, and the "target == old"
// test becomes `moves.contains_key(target_rel)`.
```

Body (pseudo-code):

```rust
fn read_move_list(path) -> Result<Vec<MoveSpec>> {
    // read lines; split_once('\t'); non-empty old/new; no duplicate old;
    // every old under root; no new exists; no new == any old.
}

// in Plan::build:
//   moved: BTreeMap<String,String>  // old_rel -> new_rel (storage: sorted)
//   for each file, each spec:
//     if file is being moved: re-aim all relative specs from new_dir(file)
//     else if resolve(spec) is a moved old: re-aim to that move's new
//   stage 1 = all Replace actions; stage 2 = all Move actions
```

### Instance lifetimes

`MoveSpec`/`moved` map live for `Plan::build`; `Plan.stages` owns the built
`Vec<Vec<soopy::SourceAction>>` (`0_move.rs:93-101`). The `Mirror` owns the temp
root for a dry run, dropped at end (`0_move.rs:769-773`).

### Storage layout

Read: the tsv (once), the corpus, the moved files. Write: stage 1 `Replace`s
then stage 2 `Move`s through soopy (`drain::stage_edits` / `stage_and_commit`,
`0_move.rs:649-688`). Uniqueness: one `Replace` per file (soopy rule); one
`Move` per moved file; edits deduped by `(start, end)` (`drain.rs:84-97`).

### Files touched (Arc 4)

- `v6/sprefa-extract/src/0_move.rs`: `MoveSpec`, `read_move_list`, `--list` flag
  in `MoveCli` (`0_move.rs:43-62`), generalize `Plan::build`.
- `v6/sprefa-extract/src/bin/extract.rs`: the `move` dispatch (`extract.rs:289-294`)
  already forwards args; `--list` needs no new dispatch branch.

### Files forbidden (Arc 4)

- The soopy crate (one-op-per-file and `StageRequest` are already sufficient).
- `src/lang/**` beyond `0_move.rs` and the new rule file.

### Tests to add (Arc 4)

- `batch_moves_two_files_and_rewrites_a_shared_importer_once` (tests/1_move.rs):
  `--list` with `a.ts<TAB>x/a.ts` and `b.ts<TAB>x/b.ts`; an importer `index.ts`
  naming both; assert one `replace` row for `index.ts` with two edits and two
  `move` rows. Breaks if the one-op-per-file grouping or the shared importer
  `expected` hash is wrong.
- `batch_rejects_colliding_destinations` (tests/1_move.rs): a `new` equal to
  another row's `old`, and a duplicate destination; assert a hard error before
  staging. Breaks if validation is missing.
- `batch_dry_run_prints_one_plan_and_writes_nothing` (tests/1_move.rs): the
  full preview set on a batch, `git status` clean. Breaks if a dry run touches
  the tree.
- `batch_move_of_a_moved_importer` (tests/1_move.rs): a moved file that itself
  imports another moved file; its `Replace` and `Move` split across the two
  stages with correct `expected` on each.

### Gate command (Arc 4)

```sh
cargo test --release --features cli --test 1_move
```

### Risk table (Arc 4)

| risk | citation that raises it | mitigation |
|---|---|---|
| two moves edit the same importer; their edits must land in one Replace | soopy one-op-per-file (`0_move.rs:93-94`) | `by_raw` BTreeMap per file holds both; `expected` read once at stage time (`drain.rs:141-171`) |
| a moved file is also an importer | `_7d_mutation_plan.rs` one-op-per-file | Replace (stage 1) and Move (stage 2) are separate stages, the single-move contract |
| a destination collides with a source or another destination | validation need | `read_move_list` rejects before any stage builds |
| dry run must not touch the tree | `Mirror` (`0_move.rs:723-773`) | every stage commits into the temp mirror with `Durability::DryRun` |

## Sequencing

Arc 1 -> Arc 2 -> Arc 3 -> Arc 4. Arc 1 is the walker every later arc consumes;
Arc 2 is the rule that names the nodes; Arc 3 resolves and re-aims; Arc 4 lifts
the single-move plan to a batch. Each arc ends green on its gate.

## Verification

- Single-move TS: `cargo test --release --features cli --test 1_move`, plus
  `tests/fixtures/ts_move/` byte-identical `tsc --noEmit` / `node --check`
  before and after where the fixture is runnable.
- Batch: `cargo test --release --features cli --test 1_move` with the batch
  cases above.
- Acceptance corpus, dry run only from this plan lane: run the batch `--list`
  of the 38 grapht moves (`.boop-worktrees/feature/generic-graph-rxjs-renderers/
  issues/grapht-source-layout/item.md`, the target map) on
  `~/projects/hafley-rxjs` and confirm the printed plan re-aims
  `./0_benchProtocol.js` -> `./0_bench/0_protocol.js` style imports and the
  barrels, without committing. The config updates in that item (package.json
  `browser` export, adapters) are out of scope for the move verb; they are
  manual.

## Staffing

- Implements in `v6/sprefa-extract` on a worktree off this branch. Base SHA:
  `4e478c60725d3d4cf8d86f2674af4c44b5723a81` (merged ff-only into this branch;
  PRs #472 #473 #475 already merged).
- No soopy changes. No new `Cargo.toml` dependency in v1 (`oxc_resolver` is a
  documented v2 arm behind `cli`).
- Gate per arc as above; CI = build + `cargo test --features cli`. No Rust
  written in this plan lane; signatures live in this doc.

## Build-vs-buy paragraph

The one new library question is path resolution. The move needs a resolution
only to answer "does this specifier name a file being moved", which is a
file-existence probe, and only relative specifiers can name a moved file (bare
specifiers resolve into `node_modules`, out of the root). So the hand-rolled
relative probe is taken for v1: it mirrors the in-repo prolog `resolve`
(`0_move.rs:490-501`) and the in-repo TS extension table (`ts.rs:89-99`), and
the acceptance corpus writes only relative ESM-style specifiers
(`index.ts:12-40`, `tsconfig.json:1-19`). `oxc_resolver` is the buy candidate
for the one case the probe cannot handle, tsconfig `paths`/`baseUrl` alias
resolution (extends, references, wildcards, `${configDir}`); it is documented,
not added, so v1 keeps a zero-dependency diff. `swc`'s resolver is rejected
because it is not a standalone crate, and the `tsconfig` crates are rejected
because they parse config without resolving.
