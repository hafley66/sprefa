# Lane brief: the python front-end arm, type + call planes (issue extract-python-arm)

First action: `git merge --ff-only 725d06804c63f056973a3853a066eeb84db6d82e`.
Failure or missing tree = STOP AND REPORT. Do not work around it.

Do not ask a question before starting. Everything you need is below with a
`path:line`. If a receipt does not match what you read, say which one and stop.

## The gap

`v6/sprefa-extract/src/lang/mod.rs:40-51` has no `PythonSource`, so a `.py` file
falls to the cst-only ast-grep fallback (`lang/mod.rs:7-8`, `lang/astgrep.rs:219`)
and yields zero type, call and df facts. v5 carries a full python front-end at
`src/graph/typegraph/python.rs` (1845 lines, at the REPO ROOT, readable).

## Scope, fixed. Do not widen it.

You land `PythonSource` as a `Source` with the **cst, type and call** planes.

| in scope | v5 function you port |
|---|---|
| skeleton + cst via ast-grep's python grammar | copy the block at `src/lang/astgrep.rs:229-249` |
| TypeF entities | `src/graph/typegraph/python.rs:302` `walk_py_entities` |
| TypeF arrow-type sigs | `src/graph/typegraph/python.rs:248` `py_fn_type`, with `:84` `py_param_name_and_type`, `:150` `py_type_refs`, `:205` `is_noise_python` |
| CallF call defs | `src/graph/typegraph/python.rs:515` `py_walk_call_defs` |
| CallF call sites | `src/graph/typegraph/python.rs:599` `py_walk_call_sites` + `:620` `py_callee` |

**OUT of scope, and each one is a named follow-up in your PR body, never code:**
DfF (`py_dataflow_from`), the docs facet (`py_docs_from`), type-edge candidates
(`py_edges_from`), both `Resolve` arms, the module plane
(`src/graph/modgraph/python.rs`), and the roster wiring.

**Why the roster wiring is out of scope, stated so you do not try it:** adding
`&PythonSource` to `lang::sources()` (`lang/mod.rs:40-51`) makes
`tests/1_resolve_cli.rs:167-192` RED, because that test asserts every roster
`Source` has a `RESOLVE_ARMS` row in `src/project.rs:456-497`, and `project.rs`
plus every existing test file are FORBIDDEN to you (another lane owns them).
`tests/4_capability_parity.rs:76` reads the same roster. So: export
`PythonSource` from `lang/mod.rs`, leave `sources()` UNCHANGED, and name the
three-line wiring (roster entry + `RESOLVE_ARMS` row + `ROSTER_FIXTURES` entry)
as the follow-up. Your new test drives `PythonSource` directly.

## The twin to mirror: `src/lang/go.rs`

Go is the closest shape (tree-sitter front-end, raw byte offsets, no line/col
bridge). Read it whole before writing. The anchors:

| thing | `src/lang/go.rs` |
|---|---|
| the one parse feeding every family | `:43-56` `go_parse` |
| node text helper | `:57` `go_text` |
| `node_span` off raw byte offsets | `:170` |
| the type projection entry | `:83` `project_types`, `:97` `walk_go_entities`, `:158` `push_entity` |
| the sig projection | `:281` `fn_sigs`, `:344` `push_sig` |
| the call projection | `:501` `project_call`, `:529` `go_walk_call_defs`, `:586` `go_walk_call_sites` |
| the `Source` impl and its mask handling | `:1412-1500` |
| the header comment shape (commit split + deferrals) | `:1-24` |

Span law, same as go: `Span { start: node.start_byte(), len: end - start }`.
No line/col table. Every string goes through the `Strings` interner as a
`NameId`; no `String` on a row in `types.rs`.

## Dependency

Add to `v6/sprefa-extract/Cargo.toml` `[dependencies]`:

```toml
tree-sitter-python = "0.23"
```

It is ALREADY in this lock as an `ast-grep-language` transitive at 0.23.6
(`cargo tree -p sprefa-extract | rg tree-sitter-python`), exports
`LANGUAGE: LanguageFn` like `tree-sitter-go 0.23`, and pairs with the
`tree-sitter 0.25` runtime already in the manifest. Cargo UNIFIES it: one copy,
no dupe. Write that reason as the comment above the line, in the manifest's
existing voice (see the `tree-sitter-kotlin-sg` comment for the exact register).
Run `cargo tree -d` after and paste the result; a NEW duplicate group is a
report-and-stop.

## Layout

```
v6/sprefa-extract/src/lang/python/mod.rs         # `mod _0_source; pub use _0_source::PythonSource;`
v6/sprefa-extract/src/lang/python/_0_source.rs   # everything
```

Matching `src/lang/dl6/`, `src/lang/prolog/`, `src/lang/markdown/`.
`src/lang/mod.rs` gets exactly two lines: `pub mod python;` beside the other
`pub mod`s, and `pub use python::PythonSource;` beside the other `pub use`s.
`sources()` is UNCHANGED. Re-export `PythonSource` from `src/lib.rs` beside the
other sources so your test can name it.

`matches()` accepts `.py` and `.pyi`. Check what `SupportLang::from_path`
returns for `.pyi` before you claim the cst plane covers it; if it returns None,
say so in the header and accept `.py` only.

## Fixture and test, both NEW files

- `v6/sprefa-extract/tests/fixtures/python/sample.py` ; new. Cover, in one small
  file: a module-level function with annotated params and an annotated return, a
  class with a base, a method, a nested function, a call to a module function, a
  method call, and a call through a dotted path. Keep it under 60 lines.
- `v6/sprefa-extract/tests/16_python.rs` ; new. Drive `PythonSource` directly
  through `dispatch`-style extraction (see how `tests/0_dl6.rs` and
  `tests/0_prolog.rs` build their assertions off `flatten`), and assert:
  1. exact TypeF node set: `(kind, name, byte start)` for every entity;
  2. exact sig set: `(owner, slot, pos, ty)`;
  3. exact CallF def set and call-site set with the callee AS WRITTEN;
  4. the cst plane is non-empty and its root node kind is `module`;
  5. `FamilyMask` honored: extracting with a mask of cst only leaves
     `types`/`call` as `None` (the familymask law: a masked-off family is None,
     never an empty bundle).

Every expected value in that test is hand-derived from `sample.py` and stated as
a literal. Do not write a test that asserts whatever the code produced.

**Fail-first receipt, required.** Write the test first against the empty
skeleton, run it, paste the red output into the commit body. Then land the
walkers and paste the green.

## Gate, run each twice, echo rc explicitly, never pipe through tail

```bash
cd <your worktree>/v6/sprefa-extract
cargo build --all-targets --features cli; echo "BUILD rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli; echo "TEST rc=$?"
cargo test --features cli --test 16_python; echo "PY rc=$?"
cargo test --features cli --test 4_capability_parity; echo "CAP rc=$?"
cargo test --features cli --test golden_parity; echo "PARITY rc=$?"
```

Baseline at the base sha is rc=0 with every leg green. Paste the `test result:`
summary line of EVERY test binary from both full runs; the two runs must agree
binary-for-binary. A leg that differs between runs is a report-and-stop.
`4_capability_parity` and `golden_parity` must be UNCHANGED, since you do not
touch the roster.

## File ownership

OWNS, and nothing else:
- `v6/sprefa-extract/src/lang/python/**` (new)
- `v6/sprefa-extract/src/lang/mod.rs` (the two export lines only; `sources()` untouched)
- `v6/sprefa-extract/src/lib.rs` (re-export line only)
- `v6/sprefa-extract/Cargo.toml` + `Cargo.lock` (the one dep)
- `v6/sprefa-extract/tests/16_python.rs` (new)
- `v6/sprefa-extract/tests/fixtures/python/**` (new)

FORBIDDEN, do not open to edit:
- `v6/sprefa-extract/src/project.rs`
- `v6/sprefa-extract/src/types.rs`, `src/wire.rs`, `src/schema.rs` ; four
  concurrent lanes own these. If python needs a new row type there, STOP AND
  REPORT; do not open them.
- `v6/sprefa-extract/src/lang/ts.rs`, `lang/go.rs`, `lang/kotlin.rs`,
  `lang/rust.rs`, `lang/astgrep.rs`, `lang/dl6/**`, `lang/prolog/**`,
  `lang/markdown/**` (READ them freely; edit none)
- every EXISTING file under `v6/sprefa-extract/tests/` (new test files only)
- every `.v5.jsonl` baseline (they are the oracle; never regenerate one)
- `v6/sprefa-engine-rs/**`, `v6/tsv2/**`, `v6/prolog/**`
- everything outside `v6/sprefa-extract/`

## Laws that bind you

- Never spawn a subagent.
- Comment budget: comments state only constraints the code cannot show. The
  module header may carry the commit split and the deferral list, matching
  `lang/go.rs:1-24`. No dates, no change-log narrative, no restating the next line.
- Identifiers are descriptive, never single-letter.
- No em dashes. Banned in prose AND identifiers: provenance, substrate,
  load-bearing, regime. "refusal" is banned in prose; say TODO or not built yet.
- No `eprintln!` under `src/**`; `tracing` only.
- Commit in slices (skeleton+cst, types+sigs, calls, test). Use
  `COMMENT_RAIL_IDLE_MS=3000 git commit ...`. Never pipe a commit.
- Check `git log` and `git status` before reporting done. An uncommitted
  deliverable is a failed lane.

## Landing

1. You are already on branch `feature/extract-python-arm` in your worktree.
2. Commit in slices.
3. `git push -u origin feature/extract-python-arm`
4. `gh pr create` with a body carrying: the gap + `path:line`, the v5 functions
   ported with their line numbers, the fixture's expected-row table, the
   fail-first red output and the green, both gate runs with per-binary counts,
   `cargo tree -d` output, and a `Follow-up` heading listing every out-of-scope
   item above with the exact wiring lines the roster needs.
5. The PR body ends with the trailer line: `Refs-Issue: extract-python-arm`
6. NEVER merge the PR. Report the URL and stop.
