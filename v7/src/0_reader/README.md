# SWI-hosted DL7 frontend foundation

This folder is the V7 text boundary. It reads the bounded DL7 prefix surface,
records canonical reader nodes and spans, exposes an explicit syntax-expansion
fixpoint, and constructs one immutable unit through either a standalone file or
an SWI quasi quotation. It performs no binding, evaluation, type work, runtime
rule execution, or target emission.

## Filesystem and dependency order

```text
v7/src/0_reader/
  0_README.md
  0_parser.pl
  1_expander.pl
  2_embedder.pl
  3_file_loader.pl
  4_cli_mainer.pl
  test/
    0_reader.test.pl
    1_entrypoints.test.pl
    fixtures/
      0_minimal.dl7
      1_embedded.pl
```

`0_parser.pl` owns text scanning and has no V7 module dependency.
`1_expander.pl` owns the static rewrite registry and expansion fixpoint.
`2_embedder.pl` imports both and owns the shared text-to-unit pipeline plus the
`dl7/4` quasi quoter. `3_file_loader.pl` imports that pipeline for files.
`4_cli_mainer.pl` imports only the loader. No dependency requires a change from the
brief's target shape.

Production modules remain below 300 nonblank, noncomment lines. A module that
would cross 500 lines is stopped and split in dependency order before further
work.

## Reader and unit terms

The one reader entry point is:

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics).
```

It accepts atoms matching `[A-Za-z_][A-Za-z0-9_-]*`, the symbolic atoms `:`,
`*`, `+`, `->`, and `<-`, `?Name` logic variables, decimal integers, strings,
`'Name` symbol literals, parenthesized forms, whitespace, and `;` line
comments. Strings decode `\n`,
`\t`, `\r`, `\\`, and `\"`; an unknown escape preserves its backslash and
following character. A symbol literal reads one identifier after `'` and
yields `literal(symbol(Name))`; it is data immediately and never enters name
resolution. Comments are layout and produce no node.

Canonical rows follow the current V7 contract:

```prolog
node(reader_node(Path, PreorderIndex), atom(Name)).
node(reader_node(Path, PreorderIndex), literal(Value)).
node(reader_node(Path, PreorderIndex),
     variable(VariableIdentity, Name)).
node(reader_node(Path, PreorderIndex), form(Children)).

source(NodeId, Path, StartOffset, EndOffset,
       StartLine, StartColumn, EndLine, EndColumn).
```

Preorder indices and offsets are zero-based Unicode code-point counts. Lines
and columns are one-based, and span ends are exclusive. A named variable uses
`variable(TopNodeId, Name)` throughout one top-level form. Each `?_` uses
`variable(NodeId, '_')`. No Prolog variable identity enters reader output.
Expected source failures return one ordered row shaped as
`diagnostic(reader, Path, NodeId, Code, position(Offset, Line, Column))`; a
source failure returns no partial forms or source rows.

Both entry points construct:

```prolog
dl7_unit(Origin, content_sha256(Digest),
         ExpandedForms, ExpandedSourceRows, ExpansionRows).
```

For files, `Origin = file(CanonicalPath)`. For quotations,
`Origin = embedded(CanonicalSourceFile,
                   position(StartOffset, StartLine, StartColumn))`.
The reader path is the canonical file path or
`embedded(CanonicalSourceFile, StartOffset)`, respectively. File uniqueness is
canonical path plus digest. Embedded uniqueness is source file plus quotation
start plus digest.

## Expansion seam

```prolog
expand_dl7(+Forms, +SourceRows,
           -ExpandedForms, -ExpandedSourceRows,
           -ExpansionRows, -Diagnostics).
```

The module-local, static multifile registry is
`dl7_syntax_rewrite(+InputTree, -MacroIdentity, -ReplacementTree)`. Trees use
the reader payload vocabulary without node identities. A rewrite mints
`expansion_node(InputNodeId, MacroIdentity, Wave, PreorderIndex)` identities,
copies the input span to generated source rows, and emits
`expansion(InputNodeId, MacroIdentity, Wave, GeneratedNodeId)` provenance.
Recursive rewriting stops with the named diagnostic
`expansion_cycle(MacroIdentities)` when a tree shape repeats. The empty
registry is an identity transformation with no expansion rows.

## Supported entry spellings

Standalone loading is explicit:

```prolog
load_dl7('path/to/program.dl7', Unit).
load_dl7('path/to/program.dl7', Unit, Diagnostics).
```

The command-line spelling is:

```text
swipl -q -s v7/src/0_reader/4_cli_mainer.pl -- path/to/program.dl7
```

The driver writes one canonical `dl7_unit/5` term to stdout. It writes source
diagnostics to stderr and exits 0 on success, 2 on source diagnostics, and 1
on usage or implementation failure.

SWI 10.0.2 accepts a quasi quotation as a bare source term, so the embedded
spelling has no DL7-specific Prolog wrapper:

```prolog
:- use_module('path/to/3_quasi', [dl7/4]).

{|dl7||
  (: User (* (: id int) (: name text)))
|}.
```

The quoter returns the ground `dl7_unit/5` term. In bare position SWI loads
that term as a fact; no runtime DL7 rule is called. It can also occupy an
ordinary Prolog argument position when a caller wants the unit as a value.

No custom `load_files/2` hook is installed. SWI's supported
[`prolog_load_file/2` hook](https://www.swi-prolog.org/pldoc/man?section=loadfilehook)
must be defined in module `user`; installing it would add process-global
loader state and make `.dl7` participate in Prolog's source-loading path.
`load_dl7/3` and the driver keep the data-language boundary explicit. The
quasi syntax follows SWI's documented
[`library(quasi_quotations)` protocol](https://www.swi-prolog.org/pldoc/man?section=quasiquotations).

## V6 donor measurements

Measurements are from base commit `ad06713975236be0c964dbad0f4702b074792d5a`.
Direct dependencies count `:- use_module` directives in each named file.

| donor | lines | bytes | direct module dependencies |
|---|---:|---:|---:|
| `v6/prolog/compile/parse_dl_dcg.pl` | 1,776 | 60,704 | 3 |
| `v6/prolog/compile.pl` | 1,027 | 46,720 | 23 |
| `v6/prolog/dl6c.pl` | 150 | 5,048 | 7 |
| `v6/prolog/print_dl.pl` | 905 | 42,119 | 6 |
| `v6/prolog/tools/prolog_lint.pl` | 251 | 9,813 | 4 |
| `v6/prolog/compile/test/` | 20,791 | 1,029,012 | 43 imports and 25 `ensure_loaded` directives in `plunit_tests.pl` |

The V6 test directory contains 97 files: 26 `*.test.pl`, one `*.plt`, eight
other `*.pl`, one shell runner, and 61 golden or data files. Its custom
`run_plunit.pl` adds six imports and one aggregator load.

## V6 donor audit

| V6 donor | useful law/mechanism | rough edge measured | V7 treatment |
|---|---|---|---|
| `parse_dl_dcg.pl:382-500` | DCG terminals, comment skipping, integer/string escape handling, and exclusive source positions | The reader is 1,776 lines and imports 3 modules; `mark/1` measures the remaining suffix and location data lives in non-backtrackable globals | Keep the terminal and span laws in one call-local recursive reader; replace `#` with the brief's `;` comment spelling and retain only integer/string literals |
| `parse_dl_dcg.pl:503-513` | Equal authored variable names share identity and `_` is fresh | `dl_vars` is one `b_setval` dictionary scoped to the complete parse and stores actual Prolog variables | Scope a ground name-to-identity table to each top-level DL7 form; emit explicit `variable/2` terms |
| `parse_dl_dcg.pl:125-217` | A parse failure has a stable name and exact line/column | Failure performs a throwaway prepass plus one marked replay and uses 5 `nb_setval` keys | Return the first reader diagnostic as data from a single pass; throw only for bad API arguments or unavailable files |
| `parse_dl_dcg.pl:30-35` | Parallel parses must not erase each other's temporary state | 6 thread-local predicates, 5 non-backtrackable keys, and 1 backtrackable variable table are hidden behind the entry point | Thread offsets, node counters, variable tables, rows, and diagnostics through ordinary predicate arguments |
| `compile.pl` and `1_expansion.pl` | Module order and named expansion phase order are semantic data | `compile.pl` is 1,027 lines with 23 imports; parsing, host preparation, expansion, checks, planning, lowering, boot, emission, and writing share one driver | Reader, expansion, unit construction, file loading, and CLI occupy separate numbered modules; the initial registry is identity-only |
| `print_dl.pl` | Canonical text round trips preserve authored order and variable sharing | 905 lines, 42,119 bytes, and 6 imports couple printing to analysis, relation records, registry policy, and CST serialization | No printer enters this foundation; driver output is the canonical ground unit term used by golden/comparison adapters |
| `compile/test/` | PLUnit units are grouped by module, and a runner gives deterministic exit status and counts | 97 files and 1,029,012 bytes; the central aggregator has 43 imports and 25 direct loads | Two focused files hold four structural scenarios and invoke only the five V7 frontend modules and two fixtures |
| `dl6c.pl` | Compute one exit code outside catches; 0 is clean, 2 is a named source refusal, 1 is an implementation failure | 150 lines and 7 imports include two emitters, output-directory creation, option tables, and a dynamic build stamp | Keep one positional `.dl7` argument, canonical stdout, diagnostic stderr, and the 0/2/1 exit contract |
| `tools/prolog_lint.pl` | Loaded-code and xref checks need explicit cluster roots | 251 lines, 4 imports, and 4 dynamic capture/index predicates require three separately loaded clusters | Excluded from the focused proof; frontend tests load their modules directly and add no process-global lint state |
| parser, expansion, and compiler drivers | Preserve authored order while crossing one named phase boundary at a time | The parse result is immediately reshaped by host preparation and an ordered expansion fold before a 23-dependency compiler driver consumes it | Both file and quasi entry points join at `dl7_text_unit/5`; expansion is an explicit pure call and no compiler phase is imported |

V6 files remain unchanged. Reused mechanisms are the lexical boundary rules,
five string escapes, comment erasure, source-position convention, authored
order, named failures, variable-name sharing law, and driver exit-code split.
DL6 statement dispatch, declaration prepass, puns, braces, infix grammar,
global parser tables, semantic expansion phases, printer mining, emitters, and
compiler planning stay outside this folder.

## Focused command

After all files exist, the only focused test command is:

```text
swipl -q -g "load_files(['v7/test/0_reader.test.pl','v7/test/1_entrypoints.test.pl'],[silent(true)]),run_tests,halt"
```

The command is run at most twice. No V6, Rust, TypeScript, generated-corpus,
or Common Lisp suite participates.
