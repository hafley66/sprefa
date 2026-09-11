# Parser inference profile

## Scope

This is a read-only profile of `dl7_parser:read_dl7/5` on the current main
(`70b250b3d`). No parser, evaluator, or test source was changed. The working
tree's unrelated existing changes were preserved.

The reported nearest-shadow parser cost is `378,841` inclusive inferences. The
measurements below use fresh SWI processes, with modules loaded before the
measurement interval where applicable.

## Junctions and measured cardinalities

The parser entry point is:

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics)
```

It validates `Path` and `Text`, converts text to a code list, runs
`once(read_top_forms/5)`, then returns either `ok(Forms, SourceRows)` or one
diagnostic. The parser is called by `dl7_text_unit/5`; the compiler obtains
three text units on the nearest-shadow compile: the six-file type prelude,
the program file, and the one-file macrotime source.

| input | text bytes | top forms | source rows | parser inferences | parser wall |
| --- | ---: | ---: | ---: | ---: | ---: |
| type prelude | 30,829 | 231 | 3,929 | 342,579 | 30.08 ms |
| macrotime | 3,535 | 22 | 382 | 34,986 | 3.24 ms |
| nearest-shadow program | 116 | 3 | 28 | 3,439 | 1.18 ms |

The three direct parser measurements sum to `381,004` inferences. The
`378,841` figure is the inclusive nearest-shadow profile target supplied for
this investigation; the difference is measurement-boundary variation between
the isolated calls and the inclusive profile.

The prelude accounts for approximately 90.43% of the supplied parser total,
macrotime 9.24%, and the program 0.91%. The program itself is therefore not a
useful parser optimization target for the nearest-shadow gate.

## Concrete parser call counts

Coverage over the full nearest-shadow compiler path showed three
`read_dl7/5` calls and these parser-local counts:

| predicate or branch | calls or covered branch count |
| --- | ---: |
| `read_top_forms/5` | 259 |
| `continue_top_forms/3` | 256 |
| `read_term/7` | 4,339 |
| form `read_term_kind/8` branch | 1,137 |
| string `read_term_kind/8` branch | 23 |
| variable `read_term_kind/8` branch | 1,290 |
| generic token `read_term_kind/8` branch | 1,889 |
| `read_form_items/8` | 5,220 |
| closed-form branch | 1,137 |
| item-recursion branch | 4,083 |
| `continue_form_items/5` | 4,083 |
| `finish_form/5` | 1,137 |
| `read_string_codes/3` | 120 total string-open/character calls |
| `finish_string/7` | 23 |
| `finish_variable/9` | 1,290 |
| `variable_identity/6` | 1,290 |
| `finish_token/9` | 1,889 |
| integer-code attempt/success | 1,889 / 27 |
| `valid_atom_codes/1` | 1,889 |

Variable identity lookup used `memberchk/2` 1,290 times: 635 lookups found an
existing name and 655 installed a new name. Token validation attempted the
known-atom list for each token, tried the colon split for ordinary atoms, and
then reached the identifier path for 1,181 calls. Numeric validation made 27
successful integer classifications and 1,862 failures before atom handling.

The parser uses explicit recursive clauses, cuts, and one outer `once/1`.
There is no DCG and therefore no DCG choicepoint cost. Dispatch cuts make the
form, string, variable, and token paths deterministic after their initial
character tests. The nearest-shadow input exercised no query or symbol path;
those paths remain part of the required parity surface.

## Repeated scans and source-row construction

The parser scans the code list once through `skip_layout/4`,
`skip_comment/4`, `take_token/5`, and `advance/3`. The following repeated list
operations are the concrete candidates:

1. `continue_form_items/5` computes `append(ItemRows, RestRows, SourceRows)`
   while unwinding every form item.
2. `continue_top_forms/3` computes `append(FormRows, RestRows, SourceRows)`
   while unwinding top-level forms.
3. `prepend_string_codes/3` and `prepend_query_codes/3` use `append/3` for
   every decoded character. The nearest-shadow path had 23 strings and no
   query forms, so this is a small local cost for this gate.
4. `valid_atom_codes/1` uses `append(NameCodes, [0':], Codes)` after the
   known-atom check for each non-special token.
5. `variable_identity/6` linearly scans the current form's variable bindings.

`source_row/5` itself constructs one fixed-arity term from already computed
positions. The larger list-assembly cost is the repeated `append/3` around
those rows. Source-row ordering is authored order: a difference-list rewrite
must emit child rows before the enclosing form row is added by `finish_form/5`.

## Upstream file and text I/O

Compiler junctions are:

```prolog
compile_dl7/4
  -> read_program_texts/4
  -> dl7_text_unit/5
  -> read_dl7/5
```

The prelude consists of six files and 30,824 source bytes joined with five
newline separators. Fresh-process measurement after loading the compiler gave
`read_prelude_texts/2 + join_prelude_texts/2`: 1,937 inferences and 3.61 ms.
The macrotime source is one file, 3,535 bytes: 1,331 inferences and 1.22 ms.
A warmed direct 116-byte program `read_file_to_string/3` measured 119
inferences and 0.15 ms. These are below the parser's measured prelude and
macrotime costs.

## Smallest proposed semantics-preserving patch

The first candidate is an internal difference-list threading change limited to
parser row assembly. Keep the public signature unchanged:

```prolog
read_dl7(+Path, +Text, -Forms, -SourceRows, -Diagnostics)
```

Replace the row-list result plumbing in these internal predicates:

```prolog
read_top_forms(+Path, +Codes, +Position, +Index, -Result)
continue_top_forms(+Path, +TermResult, -Result)
read_form_items(+Path, +TopNodeId, +FormNodeId, +Codes,
                +Position, +Index, +Variables0, -Result)
continue_form_items(+Path, +TopNodeId, +FormNodeId, +ItemResult, -Result)
```

with private accumulator variants of the form:

```prolog
read_top_forms_dl(+Path, +Codes, +Position, +Index,
                  -Result, +Rows0, -Rows)
read_form_items_dl(+Path, +TopNodeId, +FormNodeId, +Codes,
                   +Position, +Index, +Variables0,
                   -Result, +Rows0, -Rows)
```

The wrapper closes `Rows0-Rows` into the existing `SourceRows` list. The
recursive paths retain the current result order, node IDs, variable state,
diagnostic exits, and all position values. This removes the repeated
`append(ItemRows, RestRows, ...)` and `append(FormRows, RestRows, ...)` copies
without changing token recognition or the demand cone. The string/query
prepend helpers and validation scans should remain unchanged in that first
patch because their nearest-shadow cardinalities are small and their changes
would widen the parity surface.

Supported modes remain the existing mode of `read_dl7/5` (`+Path, +Text,
-Forms, -SourceRows, -Diagnostics`). The private helpers would only be called
with ground path/text, code lists, positions, indices, variable lists, and a
fresh difference-list tail. Direct tests that currently call internal parser
predicates through module qualification would require preserving the existing
five-argument wrappers.

The expected bound after row threading is linear list assembly in the number of
parsed nodes and source rows, while code scanning remains linear in text bytes.
An exact inference reduction cannot be asserted without implementing and
measuring the candidate. Acceptance should require a non-increase in the
`378,841` nearest-shadow inclusive profile, plus exact output hashes and all
focused parser tests below.

## Exact parity anchors and tests

Existing canonical compiler output anchors on current main are:

| fixture | rows | canonical SHA-256 |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

The exact test set for a future implementation is:

- `v7/test/0_reader.test.pl`
- `v7/test/1a_syntax_expander.test.pl`
- focused `read_dl7/5` and source-row tests in `v7/test/1_entrypoints.test.pl`
- canonical nearest-shadow and `2_partial` compiler probes using the hashes
  above
- malformed strings, queries, symbols, variables, comments, duplicate
  bindings, generated `Option(text)`, and nested `dl7_text_unit/5` evaluation

No patch was implemented in this profiling task, so no new CI coverage was
added or changed.
