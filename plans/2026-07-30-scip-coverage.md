# SCIP passthrough coverage, measured 2026-07-30

Lane `lane/scip-passthrough`, base `fc4dc96f`. The question this answers: what
does `scip.proto` carry, and how much of it reaches a dl program.

Sources read: the vendored schema `v6/sprefa-extract/proto/scip.proto` and its
committed prost bindings `src/scip/scip_proto.rs`.

## The table

Every field a serialized `index.scip` can carry, by message. "before" is the
lane's base; "after" is this lane's tip. A field is EMITTED when a dl program
can read it off a JSONL row.

### Index (3 fields)

| field | before | after | wire |
| --- | --- | --- | --- |
| `metadata` | loaded, collapsed to one `tool` string that NEVER reached the wire | emitted | `scip_metadata` |
| `documents` | emitted | emitted | `scip_document` + children |
| `external_symbols` | emitted | emitted | `scip_symbol` with `path` null |

### Metadata (4) and ToolInfo (3)

| field | before | after | wire |
| --- | --- | --- | --- |
| `Metadata.version` | dropped | emitted | `scip_metadata.version` |
| `Metadata.tool_info` | collapsed to `"name version"`, never emitted | emitted | see the three below |
| `Metadata.project_root` | dropped | emitted | `scip_metadata.project_root` |
| `Metadata.text_document_encoding` | dropped | emitted | `scip_metadata.text_document_encoding` |
| `ToolInfo.name` | collapsed | emitted | `scip_metadata.tool_name` |
| `ToolInfo.version` | collapsed | emitted | `scip_metadata.tool_version` |
| `ToolInfo.arguments` | dropped | emitted | `scip_metadata.tool_arguments` |

### Document (6)

| field | before | after | wire |
| --- | --- | --- | --- |
| `relative_path` | emitted | emitted | `scip_document.path`, and every child row |
| `occurrences` | emitted | emitted | `scip_occurrence` |
| `symbols` | emitted | emitted | `scip_symbol` |
| `language` | dropped | emitted | `scip_document.language` |
| `text` | dropped | emitted | `scip_document.text`, null when the indexer inlined nothing |
| `position_encoding` | consumed for the line/col to byte conversion, never emitted | consumed AND emitted | `scip_document.position_encoding` |

### Occurrence (7)

| field | before | after | wire |
| --- | --- | --- | --- |
| `range` / `single_line_range` / `multi_line_range` | emitted as a byte span | same | `scip_occurrence.start` / `.end` |
| `symbol` | emitted | emitted | `scip_occurrence.symbol` |
| `symbol_roles` | emitted as the raw bitfield plus ONE hoisted bool (`definition`) | emitted as the bitfield plus SEVEN bools | `roles`, `definition`, `import`, `write_access`, `read_access`, `generated`, `test`, `forward_definition` |
| `override_documentation` | dropped | emitted | `scip_occurrence_doc` |
| `syntax_kind` | dropped | emitted | `scip_occurrence.syntax_kind` |
| `diagnostics` | dropped | emitted | `scip_diagnostic` |
| `enclosing_range` / `typed_enclosing_range` | dropped | emitted as a byte span | `scip_occurrence.enclosing_start` / `.enclosing_end` |

### Diagnostic (5)

| field | before | after | wire |
| --- | --- | --- | --- |
| `severity` | dropped | emitted | `scip_diagnostic.severity` |
| `code` | dropped | emitted | `scip_diagnostic.code` |
| `message` | dropped | emitted | `scip_diagnostic.message` |
| `source` | dropped | emitted | `scip_diagnostic.source` |
| `tags` | dropped | emitted | `scip_diagnostic.tags` |

### SymbolInformation (7)

| field | before | after | wire |
| --- | --- | --- | --- |
| `symbol` | emitted | emitted | `scip_symbol.symbol` |
| `documentation` | dropped | emitted | `scip_documentation` |
| `relationships` | emitted | emitted | `scip_relationship` |
| `kind` | emitted | emitted | `scip_symbol.kind` |
| `display_name` | emitted | emitted | `scip_symbol.display_name` |
| `signature_documentation` | dropped | emitted | `scip_signature` + `scip_signature_occurrence` |
| `enclosing_symbol` | dropped | emitted | `scip_symbol.enclosing_symbol` |

### Signature (3)

| field | before | after | wire |
| --- | --- | --- | --- |
| `language` | dropped | emitted | `scip_signature.language` |
| `text` | dropped | emitted | `scip_signature.text` |
| `occurrences` | dropped | emitted | `scip_signature_occurrence` |

### Relationship (5)

| field | before | after | wire |
| --- | --- | --- | --- |
| `symbol` | emitted | emitted | `scip_relationship.related_symbol` |
| `is_reference` | emitted | emitted | `scip_relationship.is_reference` |
| `is_implementation` | emitted | emitted | `scip_relationship.is_implementation` |
| `is_type_definition` | emitted | emitted | `scip_relationship.is_type_definition` |
| `is_definition` | emitted | emitted | `scip_relationship.is_definition` |

### HEADLINE

**17 of 43 serialized fields emitted before; 43 of 43 after.**

## What still does not reach the wire, and why

1. **`Symbol`, `Package`, `Descriptor`.** These three messages are NEVER
   serialized into an index. They document the grammar of the symbol STRING,
   which is emitted verbatim on every row that carries a symbol. Splitting a
   symbol into scheme / package manager / package name / version / descriptors
   is a string parse over a field the consumer already holds, and string work
   belongs in the dl layer with the joins.
2. **Occurrence ranges that do not convert.** SCIP ranges are (line, col) in the
   document's position encoding and the wire is byte offsets, so an occurrence
   is dropped rather than clamped when its range does not convert, and every
   occurrence of a document the reader cannot read is dropped. That is a
   projection loss, not a field drop, and it is the pre-existing `byte_range`
   law.
3. **Nested occurrence fields inside a signature.** `scip_signature_occurrence`
   carries symbol, range and roles. An indexer that set `syntax_kind` or
   `diagnostics` on a signature-internal occurrence would lose them. No indexer
   in the roster does.

## The measured cost of 100 percent

One corpus, `v6/tsv2`, 204 documents in the tsconfig program, indexed once with
scip-typescript 0.4.0 and read from the same `index.scip` both ways.

| | rows | bytes |
| --- | --- | --- |
| before (occurrence + symbol + relationship) | 150,640 | 32,144,048 |
| after (full passthrough) | 177,967 | 59,384,798 |
| delta | +18.1% | +84.8% |

Per record, after:

| record | rows | bytes |
| --- | --- | --- |
| `scip_occurrence` | 123,655 | 48,491,662 |
| `scip_symbol` | 26,965 | 5,757,993 |
| `scip_documentation` | 27,122 | 5,103,138 |
| `scip_document` | 204 | 25,478 |
| `scip_relationship` | 20 | 6,311 |
| `scip_metadata` | 1 | 216 |

WHERE THE GROWTH WENT, and this is the uncomfortable half:

- `scip_occurrence` grew 26,972,974 -> 48,491,662 bytes, +79.8%, on ONE corpus
  where `syntax_kind` is 0 in 123,655 of 123,655 rows and `enclosing_range` is
  set in 3,303 of 123,655 (2.7%). scip-typescript emits no syntax kinds at all.
  The six added role bools are false in nearly every row for the same reason the
  bitfield was mostly 0 or 1 before. So roughly 17MB of the 21.5MB occurrence
  growth is columns that are constant on this corpus, paid per row.
- `scip_documentation` is 5.1MB of genuinely new content: scip-typescript
  renders a hover block per symbol and the diet dropped all of it.

### Verdict

**The tick engine cannot hold full passthrough as a standing feed, and
demand-side filtering is required.** 59.4MB and 178k rows is one corpus of 204
files at one revision. The ledger's own ingest figure for this engine is 74.2
files/s with `commit_ms` at ~10.8ms/file as the dominant remaining cost, and the
memory soak's ceiling is stated in page counts. A relation that arrives at 291k
rows per 1,000 files, reasserted on every index rebuild, is a different order of
input from the extraction feed the engine was measured on.

The lever shipped with the passthrough is `--scip-facts --scip-record KINDS`. It
filters at production, not at the wire, so a narrowed stream also skips reading
the corpus when no occurrence-side kind is asked for. Measured on the same
corpus: `--scip-record scip_symbol,scip_relationship` is 26,985 rows and 5.76MB,
0.46 percent of the row count and 9.7 percent of the bytes.

The recommendation is not to walk the passthrough back. The fields cost nothing
when nobody asks for them, and the alternative (a rust-side guess about which
fields matter) is what put six of seven role bits out of the language's reach in
the first place. The recommendation is that a program which wants occurrences
states which kinds it wants, and that a standing feed never asks for
`scip_occurrence` over a whole corpus without a reason.

## Where the code lives

| file | role |
| --- | --- |
| `v6/sprefa-extract/proto/scip.proto` | the vendored schema |
| `v6/sprefa-extract/src/scip/scip_proto.rs` | generated prost bindings, private to `scip_decode` |
| `v6/sprefa-extract/src/scip_decode.rs` | protobuf -> flat types |
| `v6/sprefa-extract/src/scip.rs` | indexer subprocesses + the byte bridge + the resolution joins |
| `v6/sprefa-extract/src/scip_rows.rs` | flat types -> JSONL rows, and the record filter |
| `v6/sprefa-extract/src/types.rs` | the flat types and the `FlatFact` wire enum |
| `v6/sprefa-extract/tests/5_scip_facts_cli.rs` | the goldens pinning every field name |
