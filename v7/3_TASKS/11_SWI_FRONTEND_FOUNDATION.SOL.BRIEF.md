# Sol brief: SWI-hosted DL7 frontend foundation

## Objective

Build the smallest tidy SWI-Prolog project foundation that accepts one literal
Lisp-style DL7 surface through both entry points:

1. a standalone `.dl7` file loader;
2. an SWI quasi quotation embedded in a `.pl` file.

Both entry points must call the same DL7 reader and return the same canonical
reader terms and source rows. The implementation belongs under one new numbered
folder in `v7/`. V6 remains read-only donor material.

## Working scope

- Work only in the agent's Sprefa fork.
- Read this brief first.
- Read `v7/2_DESIGN/0_KERNEL_RECONCILIATION.md`,
  `v7/2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md`,
  `v7/1_AUDIT/results/1_READER.md`, and
  `v7/1_AUDIT/results/2_EXPANSION.md` before choosing term shapes.
- Inspect the actual V6 SWI project, especially:
  - `v6/prolog/compile/parse_dl_dcg.pl`
  - `v6/prolog/compile.pl`
  - `v6/prolog/dl6c.pl`
  - `v6/prolog/print_dl.pl`
  - `v6/prolog/compile/test/`
  - `v6/prolog/tools/prolog_lint.pl`
- Re-find all symbols from current source. File and line references in audit
  prose may be stale.
- Add or edit files under `v7/` only.

## Required filesystem shape

Create one implementation folder named `v7/0_SWIPL/`. Keep its direct files in
dependency and reading order using author-driven numeric prefixes. A target
shape is:

```text
v7/0_SWIPL/
  0_README.md
  1_reader.pl
  2_expand.pl
  3_quasi.pl
  4_loader.pl
  5_driver.pl
  test/
    0_reader.test.pl
    1_entrypoints.test.pl
    fixtures/
      0_minimal.dl7
      1_embedded.pl
```

Change this shape only when an actual SWI module dependency requires a clearer
order. Record the reason in `0_README.md`. Keep production modules below 300
nonblank, noncomment lines. Stop and document the split if any module would
cross 500 lines.

## Reader contract

Use one reader signature, adapted only where SWI quasi-quotation APIs force an
extra stream-oriented wrapper:

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics).

% Read the bounded Lisp-style kernel surface into explicit node terms.
% Preserve authored order and source spans.
% Return diagnostics as data for expected source errors.
% Throw only for programmer errors or unavailable files.
```

The bounded surface for this foundation is:

```text
atom
?logic-variable
integer
string
parenthesized form
semicolon line comment
```

Use the existing V7 reader contract for canonical node and source-row shapes.
Every top-level form receives deterministic preorder node identities. Repeated
`?Name` within one top-level form shares one variable identity. Each `?_` is
fresh. Keep reader output independent of Prolog variable identity.

## Expansion seam

Provide one small explicit expansion entry point even if the first fixture has
no user macro:

```prolog
expand_dl7(+Forms, +SourceRows,
           -ExpandedForms, -ExpandedSourceRows,
           -ExpansionRows, -Diagnostics).

% Recursively apply registered DL7 syntax rewrites.
% Preserve generated-from provenance and stop on a named expansion cycle.
% The empty registry is an identity transformation.
```

This is the seam for later userland DL7 macro rules. Avoid `eval/1`, dynamic
assertion of user forms, and dependence on SWI's global `user:term_expansion/2`
for DL7 semantics.

SWI `term_expansion/2`, `goal_expansion/2`, or `library(macros)` may be used
inside the compiler implementation when they reduce repeated Prolog source and
remain module-local.

## Quasi-quotation contract

Register one `dl7` quasi-quotation syntax using
`library(quasi_quotations)`. The quotation body is literal DL7 text and must be
parsed by the same reader and expansion pipeline used for `.dl7` files.

Prove the smallest valid embedded spelling supported by SWI. Prefer a bare
top-level quotation if SWI can read and term-expand it safely:

```prolog
{|dl7||
  (: User (* (: id int) (: name text)))
|}.
```

If SWI requires an enclosing Prolog term, use exactly one documented wrapper
and pin the reason with a failing reader receipt for the bare form. Do not
invent a second DL7 grammar.

The quasi quoter must return a structured DL7 unit term. It must not execute
runtime DL7 rules during SWI source loading.

## Loader contract

Provide:

```prolog
load_dl7(+Path, -Unit).
load_dl7(+Path, -Unit, -Diagnostics).

% Read and expand one standalone .dl7 file.
% Unit owns path, forms, source rows, expansion rows, and content identity.
```

Also provide a documented SWI loading route for `.dl7`. Use SWI's supported
custom-loading hook when it can be installed module-locally without claiming
that `.dl7` is ordinary Prolog source. Otherwise expose one explicit loader
command through `5_driver.pl` and record the hook limitation with an official
SWI reference. In both cases this command must work:

```text
swipl -q -s v7/0_SWIPL/5_driver.pl -- path/to/program.dl7
```

The driver prints canonical reader/expansion output suitable for a golden or
comparison adapter. Diagnostics go to stderr. Exit 0 means success, 2 means a
source diagnostic, and other nonzero codes mean an implementation failure.

## Instance timeline

```text
process start
  -> load SWI modules
  -> read file or quasi-quotation bytes
  -> mint reader node identities
  -> collect source rows
  -> run syntax-expansion fixpoint
  -> return one immutable DL7 unit
  -> release stream and temporary reader state
```

The file loader and quasi quoter must join at the byte/text-to-reader boundary.
No parse logic may be copied between them.

## Storage and uniqueness

- A unit is unique by canonical path plus content digest for files and by
  source file plus quotation start position plus content digest for embedded
  quotations.
- Reader node identity is deterministic within one unit.
- Variable identity is scoped to one top-level form.
- Expansion rows identify input node, macro identity, wave, and generated node.
- The foundation holds units as returned terms. Avoid process-global mutable
  databases.

## V6 donor audit

Write a compact table in `v7/0_SWIPL/0_README.md` with these columns:

```text
V6 donor | useful law/mechanism | rough edge measured | V7 treatment
```

Measure actual file sizes and direct module dependencies. Include at least:

- DCG terminal and source-position handling;
- variable dictionary and identity behavior;
- parser diagnostics;
- module load order;
- hidden dynamic/thread-local state;
- test organization and invocation;
- CLI/driver behavior;
- parser, semantic expansion, and compiler-driver coupling.

Copy mechanisms only after naming the law they preserve. Keep V6 files
unchanged.

## Focused proofs

Add the smallest deterministic tests that establish:

1. one standalone `.dl7` fixture produces the expected canonical reader terms;
2. the identical literal body through SWI quasi quotation produces equal forms
   and equal variable-sharing behavior;
3. comments, strings, and source locations survive both entry points;
4. malformed input returns one named diagnostic with a source position;
5. the driver emits deterministic canonical output on two consecutive runs.

Prefer one structural assertion or snapshot-like canonical term comparison per
scenario. Avoid granular one-token tests and `toBeDefined`-style existence
checks.

Run focused tests at most twice after implementation. Do not run the V6 suite,
Rust suite, generated corpus, TypeScript suite, or Common Lisp labs.

## Exclusions

- runtime evaluator
- type inference and generic specialization
- compile-time effects
- runtime ticks and retention
- OpenAPI, JSON Schema, or GraphQL import
- LSP and syntax-highlighting implementation
- Common Lisp runtime integration
- changes to `sprefa-engine-rs`
- changes outside `v7/`

## Commit protocol

Make two bounded commits in the agent fork:

1. V6 donor audit and final filesystem contract;
2. runnable reader, expansion seam, quasi quoter, loader, driver, and tests.

Do not push. Use non-interactive commits. Preserve unrelated work.

## Required final report

Return:

- commit hashes and subjects;
- exact files added or changed;
- exact focused test command and counts;
- exact supported standalone and embedded spellings;
- V6 mechanisms reused and rough edges excluded;
- any scope item stopped under the escape hatch, with the concrete blocker.

If the reader term contract, SWI quasi-quotation behavior, or custom loader hook
cannot satisfy this brief without a second DL7 grammar or process-global mutable
state, stop that step and report the smallest reproducer instead of improvising.
