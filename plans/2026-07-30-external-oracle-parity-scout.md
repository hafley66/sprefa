# External oracle parity scout

## Context

Base verification passed before inspection:

```text
git rev-parse HEAD
d0eb3fea184d00753bb67fcb2a2c0a546f3c52e8
```

The repository contains nine `tests/it/oracle_*.rs` files. They are registered by
`tests/it/main.rs:129-137`. The direct answer is:

| Question | Answer | Receipt |
|---|---|---|
| Did v5 use a Madge oracle? | Yes. `oracle_madge.rs` invokes Madge and compares several dependency-graph relations. | `tests/it/oracle_madge.rs:1-6`, `tests/it/oracle_madge.rs:156-209`, `tests/it/oracle_madge.rs:270-490` |
| Does the listed v6 TSV2 grading path use Madge? | No Madge command occurs in the Prolog, flagship, live extraction, conformance, or sweep legs inspected. | `v6/justfile:21-23`, `v6/justfile:50-53`, `v6/justfile:102-115`, `v6/tsv2/scripts/flagship-callgraph.sh:1-8`, `v6/tsv2/scripts/extraction-live.sh:1-10`, `v6/tsv2/scripts/sweep.sh:1-15` |
| Does any v6 extractor grading path use an external non-sprefa oracle? | Yes. `v6/sprefa-extract/tests/golden_parity.rs` has ordinary tests using SCIP TypeScript, Go, and Rust indexers for call-resolution ratchets. | `v6/sprefa-extract/tests/golden_parity.rs:607-670`, `v6/sprefa-extract/tests/golden_parity.rs:877-952`, `v6/sprefa-extract/tests/golden_parity.rs:1159-1222` |
| How many v5 oracle tests execute by default on this machine? | Zero. The filtered default run reported 14 tests and 14 ignored, with zero passed. `oracle_parity.rs` is a helper module and contributes no test function. | `tests/it/oracle_parity.rs:1-22`, `tests/it/main.rs:129-137`; command receipt below |

## V5 inventory

The default test path excludes ignored tests. `just verify` runs `cargo test
--no-fail-fast` without `--ignored`, and its own comment records that ignored tests
remain unrun. CI likewise runs the integration target without `--ignored`.
`tests/it/oracle_*.rs` therefore provide on-demand oracles rather than default
verification gates. `scripts/verify.sh:54-64`, `justfile:148-158`, `.github/workflows/ci.yml:38-50`

| File | External ground truth | Invocation | DL program and graded relations | Ignore or tool gate | Default path |
|---|---|---|---|---|---|
| `tests/it/oracle_corpus.rs` | Five real OpenTelemetry corpora, with compiler SCIP indexes: Rust uses rust-analyzer, Go uses scip-go, TypeScript uses scip-typescript, Python uses scip-python, and Kotlin uses scip-java. | Each test locates or skips the corpus and indexer, builds a cached SCIP index, then runs two DL arms: syntactic and SCIP-backed. | A generated program scans files and emits `seen(path:file)`. The shared scorer grades `call_site` and `site_pick` against SCIP references. | All five corpus tests are `#[ignore]`; each has an indexer and corpus gate. | No. `tests/it/oracle_corpus.rs:1-22`, `tests/it/oracle_corpus.rs:34-42`, `tests/it/oracle_corpus.rs:138-194`, `tests/it/oracle_corpus.rs:207-262`, `tests/it/oracle_corpus.rs:279-464`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_go.rs` | scip-go SCIP index over `tests/fixtures/go_ws`. | Runs `scip-go index --output <index.scip>` in a copied fixture and parses the index. | Generated DL scans `**/*.go`, emits `seen`, then `SITE_PICK_TAIL`; the scorer grades call-site resolution. | `#[ignore]`; `SPREFA_SCIP_GO` or a PATH `scip-go` gate. | No. `tests/it/oracle_go.rs:1-10`, `tests/it/oracle_go.rs:28-40`, `tests/it/oracle_go.rs:43-96`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_kotlin.rs` | scip-java SCIP index over `tests/fixtures/kt_ws`, with a JDK/compiler requirement. | Runs `scip-java index --output <index.scip>` and reads Kotlin file edges. | Temporary `mg.dl` scans `**/*.kt` and queries `module_edge(src,dst)`. | `#[ignore]`; `SPREFA_SCIP_JAVA` or PATH `scip-java`, plus the JDK/indexer gate. | No. `tests/it/oracle_kotlin.rs:1-10`, `tests/it/oracle_kotlin.rs:21-34`, `tests/it/oracle_kotlin.rs:81-170`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_kotlin_parity.rs` | scip-java SCIP call-resolution index over `tests/fixtures/kt_ws`. | Runs `scip-java index --output <index.scip>` in a copied fixture. | Generated DL scans `**/*.kt`, emits `SITE_PICK_TAIL`, and the shared scorer grades `call_site` and `site_pick`. | `#[ignore]`; `SPREFA_SCIP_JAVA` or PATH `scip-java`, plus the JDK/indexer gate. | No. `tests/it/oracle_kotlin_parity.rs:1-15`, `tests/it/oracle_kotlin_parity.rs:42-99`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_madge.rs` | Madge's TypeScript/JavaScript dependency graph and its cycle, orphan, leaf, summary, reverse-dependency, and warning reports. | Finds `madge` from `SPREFA_MADGE` or PATH. The helper invokes `madge --json .`, `madge --circular --json .`, `madge --leaves --json .`, `madge --summary --json .`, `madge --depends <target> --json .`, and `madge --warning .`. | `examples/madge.dl` grades `dep`, `cycle_member`, `orphan`, `leaf`, `summary`, `depends_on`, `npm_dep`, and `skipped`. | `#[ignore]`; `SPREFA_MADGE` or PATH `madge`. The module documentation also describes an npm-install environment gap. | No. `tests/it/oracle_madge.rs:1-6`, `tests/it/oracle_madge.rs:16-28`, `tests/it/oracle_madge.rs:134-209`, `tests/it/oracle_madge.rs:216-247`, `tests/it/oracle_madge.rs:270-490`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_parity.rs` | The per-language SCIP reference occurrences supplied by the caller tests. | No standalone test or external process. Callers pass a parsed SCIP index and DL stdout to `score_parity`. | Parses `call_site` and `site_pick` rows, resolves them to source positions, and grades confirmed positives by caller with a precision floor. | Helper only. No `#[test]` function. | No standalone execution. `tests/it/oracle_parity.rs:1-22`, `tests/it/oracle_parity.rs:298-346` |
| `tests/it/oracle_python.rs` | scip-python SCIP index over `tests/fixtures/py_ws`. | Runs `scip-python index --project-name py_ws --project-version 0.0.1 --output <index.scip>` in a copied fixture. | Generated DL scans `**/*.py`, emits `SITE_PICK_TAIL`, and the shared scorer grades call-site resolution. | `#[ignore]`; `SPREFA_SCIP_PYTHON` or PATH `scip-python`. | No. `tests/it/oracle_python.rs:1-11`, `tests/it/oracle_python.rs:39-50`, `tests/it/oracle_python.rs:54-132`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_rust.rs` | rust-analyzer SCIP symbol resolution. | Module fixture runs `rust-analyzer scip <root> --output <index.scip>`; call parity uses the same form over the v5 source tree. | Module fixture grades `module_edge`; call parity scans `src/**/*.rs`, emits `SITE_PICK_TAIL`, and grades `call_site` and `site_pick`. The recall snapshot queries `module_edge` over the real v5 crate. | All three tests are `#[ignore]`; `SPREFA_RUST_ANALYZER` or a discovered rust-analyzer gate. | No. `tests/it/oracle_rust.rs:1-10`, `tests/it/oracle_rust.rs:90-113`, `tests/it/oracle_rust.rs:136-195`, `tests/it/oracle_rust.rs:211-296`; `scripts/verify.sh:54-64` |
| `tests/it/oracle_ts.rs` | scip-typescript SCIP symbol resolution over `tests/fixtures/ts_ws`. | Finds `SPREFA_SCIP_TYPESCRIPT`, `scip-typescript`, or an npx fallback, then runs `index --output <index.scip>`. | Generated DL scans `src/**/*.ts`, emits `SITE_PICK_TAIL`, and the shared scorer grades `call_site` and `site_pick`. | `#[ignore]`; explicit binary, PATH binary, or npx fallback. | No. `tests/it/oracle_ts.rs:1-11`, `tests/it/oracle_ts.rs:23-45`, `tests/it/oracle_ts.rs:54-99`; `scripts/verify.sh:54-64` |

The exact default receipt used the requested hermetic configuration:

```text
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 \
  CARGO_TARGET_DIR=/private/tmp/sprefa-oracle-target \
  cargo test --test it oracle_ -- --nocapture

running 14 tests
... 14 ignored ...
test result: ok. 0 passed; 0 failed; 0 measured; 14 ignored; ...
```

The two on-demand fixture commands were:

```sh
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 \
  CARGO_TARGET_DIR=/private/tmp/sprefa-oracle-target \
  cargo test --test it oracle_madge::madge_oracle_dep_graph_and_cycles_agree \
  -- --ignored --nocapture

SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 \
  CARGO_TARGET_DIR=/private/tmp/sprefa-oracle-target \
  cargo test --test it oracle_rust::module_edge_is_subset_of_rust_analyzer \
  -- --ignored --nocapture
```

The nine files contain 14 test functions because the corpus and Rust files each
contain multiple ignored tests. The default execution count is therefore 0 of 14
test functions, and 0 of 9 oracle files contributes an executed oracle assertion.
The receipt is a test-run result; the 14-function inventory is visible in the
registered modules and the files cited in the table. `tests/it/main.rs:129-137`

Two ignored fixture checks were cheap enough to execute with the same environment
and a separate Cargo target directory. Madge passed with 13 matching edges and
matching leaves. The Rust module fixture passed with 3 matching edges, precision
1.00, and recall 1.00.

```text
[oracle:madge] ours=13 madge=13 matched=13
[oracle:fixture] our edges=3 ra edges=3 matched=3 precision=1.00 recall=1.00
```

The remaining ignored tests were not run here. The corpus tests require real
OpenTelemetry checkouts and per-language indexers. The Go and Kotlin SCIP binaries
were absent from PATH on this machine. `command -v` found Madge, rust-analyzer,
scip-typescript, and scip-python; it found no scip-go, scip-java, or Kotlin binary.
The tool gates and corpus setup are specified in `tests/it/oracle_corpus.rs:279-464`
and `tests/it/oracle_go.rs:28-74`, `tests/it/oracle_kotlin.rs:21-143`.

Tool lookup receipt:

```text
/Users/chrishafley/.nvm/versions/node/v24.15.0/bin/madge
/Users/chrishafley/.cargo/bin/rust-analyzer
/Users/chrishafley/.nvm/versions/node/v24.15.0/bin/scip-typescript
/Users/chrishafley/.nvm/versions/node/v24.15.0/bin/scip-python
/usr/local/go/bin/go
```

The omitted `scip-go`, `scip-java`, and Kotlin lookup lines produced no path.

## V6 grading legs

| Leg | What it compares | External ground truth | Receipt |
|---|---|---|---|
| Prolog conformance | The reference Prolog engine's fixture expectations against the same engine's fixture runs. | None. The runner loads `engine.pl` and all fixture programs, then calls `engine:fixture_expectations_hold/1`. | `v6/prolog/conformance/go.pl:1-29`, `v6/prolog/src/grader.pl:1-16`, `v6/prolog/conformance/FIXTURES.md:1-10`, `v6/prolog/conformance/FIXTURES.md:61-84` |
| Prolog tick-log oracle | Generated TypeScript module tick logs against `.oracle.jsonl` logs emitted by the unchanged Prolog fixture engine over the same program, initial state, and schedule. | The Prolog engine and fixture schedule. This is an independent implementation comparison inside sprefa, not an external language analyzer. | `v6/prolog/conformance/ticklog.pl:1-6`, `v6/prolog/conformance/ticklog.pl:47-74`, `v6/prolog/compile/oracle_dump.pl:1-9`, `v6/tsv2/scripts/sweep.ts:1-10`, `v6/tsv2/scripts/sweep.ts:168-225` |
| `just flagship` | V5 and V6 executions of the pinned `sprefa-extract` core corpus. It compares `file`, `def`, `call`, `calls`, and `unused` rows, then classifies differences by source bytes and reruns the V5 rules over each engine's own fact sets. | V5 sprefa output is the baseline. The classifier uses source bytes and rule reproduction. No Madge, SCIP, tsc, ripgrep, or tree-sitter CLI. | `v6/justfile:102-108`, `v6/dl/fixtures/flagship-callgraph.dl6:1-4`, `v6/dl/fixtures/flagship-callgraph.dl6:28-45`, `v6/tsv2/scripts/flagship-callgraph.sh:1-8`, `v6/tsv2/scripts/flagship-callgraph.sh:37-81`, `v6/tsv2/scripts/flagship-callgraph.sh:197-303` |
| `extraction-live.sh` | Expected state transitions around edits, identical bytes, additions, deletion, restart, boot reconciliation, and a killed extraction process. | The shell script's expected strings and SQLite state transitions. | `v6/justfile:110-115`, `v6/tsv2/scripts/extraction-live.sh:1-10`, `v6/tsv2/scripts/extraction-live.sh:12-42`, `v6/tsv2/scripts/extraction-live.sh:158-307` |
| `sweep` | Generated module tick logs and final state against oracle logs produced by Prolog over each fixture's schedule. | Prolog-generated oracle logs over the same fixture set. | `v6/justfile:50-53`, `v6/tsv2/scripts/sweep.sh:1-52`, `v6/tsv2/scripts/sweep.ts:168-203` |
| `sprefa-extract` SCIP ratchets | V6 extracted call occurrences and resolved call edges against real SCIP indexes. TS, Go, and Rust ratchets use their corresponding external indexers. | SCIP from scip-typescript, scip-go, and rust-analyzer. | `v6/sprefa-extract/tests/golden_parity.rs:607-670`, `v6/sprefa-extract/tests/golden_parity.rs:877-952`, `v6/sprefa-extract/tests/golden_parity.rs:1159-1222`, `v6/sprefa-extract/src/scip.rs:1-22`, `v6/sprefa-extract/src/scip.rs:65-207` |

The extractor's normal JSONL stream emits phase-one `node`, `site`, and
`specifier` facts. Specifier rows contain the module text as written. The stream
does not emit resolved project edges, and the `ModuleF` resolve surface remains
commented out. `v6/sprefa-extract/src/wire.rs:122-155`,
`v6/sprefa-extract/src/wire.rs:1314-1327`, `v6/sprefa-extract/src/types.rs:406-423`,
`v6/sprefa-extract/src/types.rs:1041-1046`

The five named TSV2 legs are self-comparisons or V5-to-V6 comparisons. The
standalone extractor test suite adds real SCIP checks for TypeScript, Go, and Rust;
the v6 `green` recipe does not list that crate's tests. `v6/justfile:156-160`
The inspected v6 grading path contains no Madge invocation, no `tsc` invocation,
no ripgrep-based ground truth, and no tree-sitter CLI invocation. Tree-sitter and
ast-grep are linked parsing dependencies in the extractor crate, not command-line
oracle processes. `v6/sprefa-extract/Cargo.toml:14-56`,
`v6/tsv2/scripts/flagship-callgraph.sh:197-303`,
`v6/tsv2/scripts/extraction-live.sh:158-307`, `v6/tsv2/scripts/sweep.sh:19-52`

## Parity table

`Default` means execution through the repository's `just verify` or CI path. The
v6 column refers to an equivalent external fact check in the extractor or TSV2
path, not merely a parser fixture with expected JSONL.

| Language or graph | V5 oracle exists? | V5 runs by default? | V6 equivalent exists? | What v6 would need to grade the same fact |
|---|---|---|---|---|
| TypeScript imports | Yes. Madge grades `dep` plus the graph reports in `examples/madge.dl`. `tests/it/oracle_madge.rs:270-490` | No. `oracle_madge` is ignored and `verify` does not select ignored tests. `tests/it/oracle_madge.rs:270-276`, `scripts/verify.sh:54-64` | No Madge-equivalent relation in the named TSV2 legs. Phase-one `specifier` rows exist, while module resolution is pending. `v6/sprefa-extract/src/wire.rs:122-155`, `v6/sprefa-extract/src/types.rs:1041-1046` | Run Madge over the same TS/JS root, normalize its resolved file pairs, project v6 specifiers to resolved file pairs, emit `dep(src,dst)` from a `.dl6` program, and diff sorted TSV rows. |
| Rust | Yes. rust-analyzer SCIP grades `module_edge`, call resolution, and the recall snapshot. `tests/it/oracle_rust.rs:136-296` | No. All three Rust oracle tests are ignored. `tests/it/oracle_rust.rs:211-250`, `scripts/verify.sh:54-64` | Partial. V6 has a standalone Rust SCIP call-resolution ratchet. The named TSV2 legs have no Rust external module-edge oracle. `v6/sprefa-extract/tests/golden_parity.rs:1159-1222`, `v6/tsv2/scripts/flagship-callgraph.sh:20-26` | For V5 module-edge parity, run rust-analyzer SCIP over the same corpus, extract file-level import edges, emit `module_edge`, and compare sets. For call parity, retain the existing SCIP ratchet as an explicit gate. |
| Python | Yes. scip-python grades fixture and corpus call resolution. `tests/it/oracle_python.rs:1-11`, `tests/it/oracle_corpus.rs:379-420` | No. The fixture and corpus tests are ignored. `tests/it/oracle_python.rs:54-61`, `tests/it/oracle_corpus.rs:379-388` | No equivalent in the extractor's stated language coverage or TSV2 legs. Python is listed as CST-only. `v6/sprefa-extract/src/bin/extract.rs:45-55` | Add Python call extraction and resolution output, build a scip-python index over the same fixture or corpus, and score `call_site` and `site_pick` with the V5 confirmed-positive rules. |
| Go | Yes. scip-go grades fixture and corpus call resolution. `tests/it/oracle_go.rs:1-10`, `tests/it/oracle_corpus.rs:314-337` | No. Both Go tests are ignored. `tests/it/oracle_go.rs:43-49`, `tests/it/oracle_corpus.rs:314-321` | Partial. V6 has a standalone Go SCIP call-resolution ratchet. No TSV2 module-edge oracle exists. `v6/sprefa-extract/tests/golden_parity.rs:877-952` | For the same module graph fact, run scip-go over the corpus, define the file-edge projection, emit the equivalent relation, and diff it. |
| Kotlin | Yes. scip-java grades module edges and call resolution, including corpus coverage. `tests/it/oracle_kotlin.rs:115-170`, `tests/it/oracle_kotlin_parity.rs:42-120`, `tests/it/oracle_corpus.rs:426-464` | No. All Kotlin oracle tests are ignored. `tests/it/oracle_kotlin.rs:115-118`, `tests/it/oracle_kotlin_parity.rs:42-48`, `tests/it/oracle_corpus.rs:426-434` | No external Kotlin oracle ratchet was found in `golden_parity.rs`; the extractor has Kotlin phase-one families. `v6/sprefa-extract/tests/golden_parity.rs:607-670`, `v6/sprefa-extract/src/bin/extract.rs:45-50` | Run scip-java with its JDK/compiler requirements, define the same file-edge or call-site projection, and compare against v6 rows. |
| Corpus | Yes. Five OpenTelemetry corpus tests use language-specific SCIP indexes. `tests/it/oracle_corpus.rs:1-22`, `tests/it/oracle_corpus.rs:279-464` | No. Every corpus test is ignored. `tests/it/oracle_corpus.rs:279-288`, `tests/it/oracle_corpus.rs:314-321`, `tests/it/oracle_corpus.rs:339-348`, `tests/it/oracle_corpus.rs:379-388`, `tests/it/oracle_corpus.rs:426-434` | No equivalent corpus in the TSV2 flagship. The flagship pins the `sprefa-extract` core and frontend files, while frontends are explicitly excluded. `v6/tsv2/scripts/flagship-callgraph.sh:20-26`, `v6/tsv2/scripts/flagship-callgraph.sh:100-128` | Reuse the corpus checkouts, invoke each external indexer, run the v6 extractor and resolver over the same files, and apply the per-language parity scorer. |
| Parity | Yes. `oracle_parity.rs` supplies the shared SCIP occurrence scorer used by the language tests. `tests/it/oracle_parity.rs:1-22`, `tests/it/oracle_parity.rs:298-346` | No standalone parity test. The callers are ignored. `tests/it/oracle_ts.rs:54-59`, `tests/it/oracle_python.rs:54-61`, `tests/it/oracle_go.rs:43-49`, `tests/it/oracle_kotlin_parity.rs:42-48` | Partial. V6 has captured V5 facet equality and SCIP ratchets for TS, Go, and Rust. The captured V5 oracle covers only the ported facets; v6-only specifiers are reported without an assertion. `v6/sprefa-extract/tests/golden_parity.rs:1-25`, `v6/sprefa-extract/tests/golden_parity.rs:235-280`, `v6/sprefa-extract/tests/golden_parity.rs:540-600` | Keep the captured V5 baseline for ported facts, add external ratchets for each language and graph fact, and add a named gate that runs them rather than leaving them as ordinary crate tests outside `just verify`. |

## Cheapest path to a real Madge oracle

The smallest valid scope is the TypeScript/JavaScript file dependency relation.
Madge already has the required graph command and the v6 host decoder already
accepts JSON arrays, JSON lines, or whitespace rows. `tests/it/oracle_madge.rs:134-167`,
`v6/tsv2/serve/1_hosts.ts:192-275`

The external command shape can be reduced to sorted two-column rows as follows:

```sh
madge --extensions ts,js --json "$ROOT" |
  node -e '
    let s = "";
    process.stdin.on("data", b => s += b);
    process.stdin.on("end", () => {
      for (const [src, dsts] of Object.entries(JSON.parse(s)))
        for (const dst of dsts) process.stdout.write(`${src}\t${dst}\n`);
    });
  ' |
  LC_ALL=C sort -u > "$WORK/madge.dep.tsv"
```

The `.dl6` program must consume v6 phase-one specifier rows and resolve each
specifier to a project-relative file before emitting the comparable relation. A
minimal shape, using the existing extractor projection pattern, is:

```text
sh call_node(path: text, digest: text) ->
  (record: text, family: text, kind: text, name: text) =
  `"$DL_EXTRACT_BIN" --family cst,type,call,df {path}`

rel file(path: text, digest: text).
rel specifier(src: text, name: text, kind: text).
rel dep(src: text, dst: text).

specifier(src, name, kind) <-
  file(src, digest), call_node(src, digest, 'specifier', 'call', kind, name).
dep(src, dst) <- specifier(src, name, _), resolve_ts_module(src, name, dst).
? dep(src, dst).
```

The declaration above is the required shape; implementation is absent. The current
extractor emits `specifier` rows with the name as written and has no `ModuleF`
resolution arm. The missing `resolve_ts_module` relation must implement the
project-relative path rules that make a v6 row comparable with Madge's resolved
edge. `v6/dl/fixtures/flagship-callgraph.dl6:79-101`,
`v6/sprefa-extract/src/types.rs:406-423`,
`v6/sprefa-extract/src/types.rs:587-603`, `v6/sprefa-extract/src/bin/extract.rs:45-68`

The diff step is a set comparison after both sides use identical path and
extension normalization:

```sh
curl -fsS "$V6_BASE/idb/dep" |
  node -e '/* decode the two relation columns */' |
  LC_ALL=C sort -u > "$WORK/v6.dep.tsv"
diff -u "$WORK/madge.dep.tsv" "$WORK/v6.dep.tsv"
```

The existing flagship script demonstrates the same relation-dump and `cmp` shape
for V5 and V6 SQLite outputs. `v6/tsv2/scripts/flagship-callgraph.sh:269-299`

Cost facts:

| Item | Cost stated from source |
|---|---|
| Madge process | One `madge --extensions ts,js --json <root>` execution per graph snapshot, followed by JSON flattening. `tests/it/oracle_madge.rs:156-167` |
| `.dl6` work | A new fixture or program, one `sh` extraction declaration, a file enumeration relation, a TypeScript module resolver, and a `dep` query. The resolver is absent from the current extractor source, so exact implementation size is not determinable from source. `v6/sprefa-extract/src/types.rs:587-603` |
| Harness work | A sorted relation dump and `diff -u`, modeled on the flagship dump. `v6/tsv2/scripts/flagship-callgraph.sh:269-299` |
| Environment | Madge must be installed or supplied through an explicit path. The existing V5 test uses `SPREFA_MADGE` or PATH. `tests/it/oracle_madge.rs:16-28` |
| Semantic risk | Path normalization, extension resolution, package imports, JSON imports, unresolved warnings, and dynamic imports require explicit policy. The V5 fixture exercises JSON, JavaScript, unresolved imports, npm dependency exclusion, and warnings. `tests/it/oracle_madge.rs:434-490` |

The cheapest complete oracle therefore covers `dep(src,dst)` first. Cycle,
orphan, leaf, summary, reverse-dependency, and warning parity require additional
`.dl6` projections or separate queries matching the existing V5 relation set.
`tests/it/oracle_madge.rs:312-490`

## Decisions

1. Count default v5 oracle execution from the hermetic test receipt: 0 of 14 test
   functions, with 14 ignored. `scripts/verify.sh:54-64`
2. Treat the five named TSV2 legs as internal comparisons and record extractor
   SCIP ratchets separately, because the ratchets invoke foreign indexer binaries.
   `v6/tsv2/scripts/sweep.ts:168-225`, `v6/sprefa-extract/src/scip.rs:65-207`
3. Mark the v6 TypeScript Madge parity row as a gap until a resolved `dep` relation
   and a Madge-side diff are present.

## Verification

| Check | Result | Receipt |
|---|---|---|
| Base SHA | Passed, exact match | `git rev-parse HEAD` output recorded in `## Context` |
| Oracle file count | 9 files | `tests/it/main.rs:129-137` |
| Default oracle run | 0 passed, 0 failed, 14 ignored | Hermetic command receipt in `## V5 inventory` |
| Madge fixture oracle | Passed, 13/13 edges and matching leaves | On-demand test output in `## V5 inventory`; implementation `tests/it/oracle_madge.rs:270-395` |
| Rust module fixture oracle | Passed, 3/3 edges, precision 1.00, recall 1.00 | On-demand test output in `## V5 inventory`; implementation `tests/it/oracle_rust.rs:211-228` |
| Tree scope | Only this plan file is intended to be written | User-requested analysis-only lane |

## Staffing

No implementation was performed. The next implementation task requires a choice
of TypeScript module-resolution policy for `resolve_ts_module`, then an explicit
gate wiring the Madge snapshot and v6 `dep` dump into a sorted diff.
