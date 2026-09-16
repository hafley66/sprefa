# CST query lowering: breakdown plan

Lane `plan/cst-lowering-breakdown`, worktree `/Users/chrishafley/projects/sprefa-plan-cstlower`.

Base check (FIRST action, per brief and the worktree dispatch law):

```
git rev-parse HEAD   -> 2eceb8361029c140d2698ac25ce6e89336a215c8
git merge --ff-only 2eceb836 -> "Already up to date."  (exit 0)
```

Base confirmed. Planning only: no production code was written, nothing outside
this worktree was touched, no subagents were spawned.

Sources read in full: `plans/2026-08-02-cst-query-rulings.md`,
`plans/duels-2026-08-02/duel-a-flash.md`, `duel-a-kimi.md`,
`lane-a-effectcache.md`.

Read outside this worktree, read-only, because the arc it reports on is
untracked at this base and section 5 D3 needs it:
`~/projects/sprefa/chat_log/20260803.0.duel-instant-panel-bridge-bus-typeir-cstlower.md`
(the type-IR lane's SCIP-identity ruling). Also consulted, read-only, for the
build-vs-buy check in B0 and the grammar-handle question in A1: the vendored
crate sources under `~/.cargo/registry/src/index.crates.io-*/ast-grep-core-0.38.7/`
and `ast-grep-language-0.38.7/`.

---

## 1. Verified receipts

Every location the brief cites, checked against the tree at 2eceb836. Four
citations are wrong or incomplete; two premises do not survive contact with the
code. Those are the rows to read first.

### 1.1 Confirmed as cited

| claim | as found today |
|---|---|
| SYNTAX.md:330 names the phase-2 gap | `v6/prolog/compile/SYNTAX.md:330`, exact line. Table row for `ts_query(Patterns)`, right-hand cell reads "value compiles to query text; phase-2 host execution is named `unsupported_host_execution_phase_2(tree_sitter_query)`" |
| `ts_pattern_text/2` emitter | `v6/prolog/1_host_expand.pl:414-422` = `compile_ts_query/2`; `:424-474` = the `ts_pattern_text/2` clause block, ending in the catch-all throw at `:473-474`. `ts_quoted/2` follows at `:480-489` |
| registry row for `ts_query/1` | `v6/prolog/compile/registry.pl:193` `surface(ts_query/1, world, no_refs, value(tree_sitter_query), live).` `:194` is the `sg_pattern/3` refusal row |
| conformance fixture | `v6/prolog/conformance/fixtures/2_hosts_wiring.pl:200-243`, `fixture(native_ts_query_term, ...)`. The `tree_sitter` host is an `sh_decl` with template `"tree-sitter {file_digest} $query"` |
| `parse_dl.pl:1464-1483` DCG precedent | exact. `add_expr/5`, `add_expr_rest/6`, `mul_expr/5` at those lines, explicit `S0/S` difference lists |
| astgrep.rs emits named nodes only | `v6/sprefa-extract/src/lang/astgrep.rs:167-204`, `impl Project<CstF> for CstProjector`; the comment at `:171-175` states named-nodes-only with unnamed nodes reparenting their named descendants |
| ast-grep-core 0.38 linked | `v6/sprefa-extract/Cargo.toml:18` `ast-grep-core = "0.38"`, `:19` `ast-grep-language = "0.38"` |
| CRAWL-BENCH 40.7 vs 3540.9 | two places. `v6/prolog/ARCH.pl:710` (`task(crawl_bench, done, [])`) carries the numbers verbatim: "v5 org-fan 42,739 files / 389 repos / 12.07s = 3,540.9 files/s; v6 served extraction 779 files / 8 repos / 19.15s = 40.7 files/s. That is ~87x on the same machine". The document is `v6/tsv2/CRAWL-BENCH.md` (178 lines, run date 2026-07-29), whose own headline at `:19` reads 40.68 files/s. Script: `v6/tsv2/scripts/crawl-bench.sh` |
| the 87x cause is spawn-per-witness | second, independent receipt: `v6/prolog/ARCH.pl:814` (`ts_lowering_review`): "host subprocesses run at concurrency 1.0, the structural cause of 40.68 files/s vs v5's 3,540.93 same-run" |
| 7 expanders, 1608 lines, zero positions | exact. `wc -l` over the seven files sums to 1608 and each per-file count in the finding matches: `0_match_expand` 137, `0_enum_expand` 180, `0_coalesce_expand` 274, `0_seq_expand` 194, `0_relation_edge_expand` 92, `1_expansion` 57, `1_host_expand` 674. Finding at `chat_log/20260802.2.opus-flash-fleet-haskell-prolog-dl6-diag.pl:121-122` |
| effect_cache has no response-side column | `v6/dl/src/2_schema.ts:83`: `CREATE TABLE IF NOT EXISTS effect_cache (full_digest INTEGER PRIMARY KEY, identity_digest INTEGER NOT NULL, host TEXT NOT NULL, state TEXT NOT NULL, requested_tick INTEGER NOT NULL)`. Five columns, none of them a response or disk digest. Lane A's finding holds |
| `dl.langium` exists | `v6/dl/grammar/dl.langium`, 190 lines |

### 1.2 Corrected

**C1. There is no phase-2 refusal in the compiler.** The brief says "today dl6
REFUSES `tree_sitter_query`". Repo-wide grep for
`unsupported_host_execution_phase_2` returns five hits and every one of them is
prose: `brief.md:32`, `v6/prolog/compile/SYNTAX.md:330`,
`v6/dl/fixtures/golden-flex.dl6:459`, `plans/2026-08-02-cst-query-rulings.md`,
`plans/duels-2026-08-02/duel-a-kimi.md`. No `.pl` file throws that term. What
the compiler actually does with `ts_query/1` is compile it:
`compile_value_terms/2` at `1_host_expand.pl:154-158` matches `ts_query(_)` and
replaces the term with query text. The only throws nearby are
`unmapped_feature/2` at `:420`, `:422`, `:474`, and those fire on unsupported
pattern SHAPES, not on execution. `2_hosts_wiring.pl:242` records the state
honestly: the fixture's expected `final(captured/1, [])` is empty, and
`final(query_value/1, [...])` asserts the emitted text.

Consequence for the ladder: step A does not flip a refusal off. There is
nothing to delete. It adds a capability where prose currently stands in for one,
and the SYNTAX.md line plus the golden-flex comment are the two places that must
be rewritten when it lands.

**C2. `run_ts` is at `src/engine/eval.rs`, not `src/eval.rs`.** Line number is
right. `pub(crate) fn run_ts(content: &str, lang: &str, query_str: &str,
tree_cache: &mut AstTreeCache) -> Result<Vec<(i64, i64, Vec<(String, String,
usize, usize)>)>>` at `src/engine/eval.rs:1047-1079`, 33 lines of body. It
returns LINE numbers for the match span and byte offsets only inside each
capture tuple. v6 spans are byte offsets throughout
(`v6/sprefa-extract/src/shape.rs` `Span { start, len }`), so a port converts the
outer pair, it does not copy it.

**C3. `parse_dl.pl:95-120` is not a DCG.** That range is
`lookup_column_order/2`, `record_host_signature/3`, and the `parse_dl_file/4` /
`parse_dl_source/5` entry points. The kimi duel's citation is off. The real
DCG-style precedent for a nested term surface is `parse_dl.pl:1462-1520`:
`expr/5`, `add_expr/5`, `add_expr_rest/6`, `mul_expr/5`, `mul_expr_rest/6`,
`factor/5`. Also: the file is 1688 lines today; `duel-a-flash.md:4` cites 1655,
which was true at base 92756b54.

**C4. `effect_cache` is not the cache the dl6 served runtime uses.** This is the
largest correction in the section. `effect_cache` appears nowhere under
`v6/tsv2`. The runtime that executes dl6-compiled `sh` and extract hosts is
`v6/tsv2/serve/1_hosts.ts`, and it keeps its own table:

```
v6/tsv2/serve/1_hosts.ts:63   const WITNESS_TABLE = "__host_witness";
v6/tsv2/serve/1_hosts.ts:70-73  CREATE TABLE IF NOT EXISTS "__host_witness" (
    "host" TEXT NOT NULL, "witness_digest" TEXT NOT NULL,
    "state" TEXT NOT NULL, "response_rows" INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY ("host", "witness_digest")) WITHOUT ROWID
```

with `clearDeadLocks` at `:77-81`, `answered` at `:83-90`, `claim` at `:92-101`,
`settle` at `:103-119`. `effect_cache` belongs to `v6/dl/src` (the M1 slice with
the langium bridge and `HostRunner`), which is a separate runtime still gated by
`just dl-test`. Ruling 3's phrase "request cols widen to (path, content_hash,
lang, grammar_hash, query_text)" therefore names the v6/dl table. The good news
is that the widening the ruling wants costs nothing on either side, and the
reason is C5.

**C5. Request columns already flow into both digests; no cache schema changes.**
`expand_probe/7` at `1_host_expand.pl:530-560` computes two digests from the
host's declared input columns:

```prolog
digest_expr(identity, Name, Inputs, InputValues, [],    Identity),   % :548
digest_expr(witness,  Name, Inputs, InputValues, Salts, Witness),    % :549
```

and `digest_expr/6` at `:562-566` is a plain SQL `concat` over
`Role|Name` plus one `|name:type=value` part per input plus one part per salt.
`generated_host_decls/7` at `:607-627` then mints
`__host_demand_<name>(identity_digest, witness_digest, Inputs..., Salts...)` and
`__host_response_<name>(witness_digest, ordinal, Inputs..., Outputs...)`.
Adding `query_text` and `grammar_hash` as declared inputs on the host decl
extends both digests automatically. Nothing in SQLite schema, `effect_cache`, or
`__host_witness` needs to change to get the caching identity the ruling asks
for. What DOES need a decision is the input ROLE for each new column, which is
step A5.

**C6. ARCH.pl carries `task/3`, not `task/5`.** `v6/prolog/ARCH.pl:651` states
the shape in its own header comment ("BUILD ORDER", then `task(Name, Status,
Needs)`).
223 `task(` rows, all arity 3. `fork/5` IS arity 5 (`ARCH.pl:591`). The project
CLAUDE.md line "task/5 + fork/5" is stale against the file. Section 4 below
writes `task/3` rows to match the file, since the file is the gate
(`swipl -g go -t halt ARCH.pl`).

**C7. `sprefa-extract` has zero tree-sitter query execution, and the CLI already
has a close sibling.** What exists: `query_patterns/3` at
`v6/sprefa-extract/src/lang/astgrep.rs:57-129` runs ast-grep `Pattern`s
(metavariable syntax, `$X` / `$$$ARGS`), driven from the CLI by
`--ast-pattern` / `--ast-selector` / `--ast-capture`
(`src/bin/extract.rs:124-150`), dispatched at `:414-422` via
`stream_ast_queries/3`, tested at `tests/3_ast_pattern_cli.rs` (67 lines).
What does not exist: any use of `tree_sitter::Query` or `QueryCursor`. The
matcher the compiler emits text for and the matcher the extractor runs are two
different engines today.

### 1.3 Premises that do not hold

**P1. "Native spelling gets spans for free" is not true of this compiler.**
The position machinery is statement-granular by construction, at every layer:

- `parse_dl.pl:83` `:- dynamic(source_statement_fact/3).`
- `parse_dl.pl:262-266` `record_statement_source_lines/3` asserts
  `source_statement_fact(rule|decl, Item, RemainingLength)`. The stored key is
  the WHOLE statement and its suffix length. Nothing finer is recorded anywhere.
- `parse_dl.pl:184-196` `build_line_starts/1` builds an `arg/3`-indexed line
  table; `remaining_line_column/3` at `:161` turns a suffix length into
  line/column.
- `parse_dl.pl:231-240` `statement_location_for_reference/4` finds the statement
  by relation reference, head-first, sub-term scan as fallback.
- `diag.pl:86-105` enumerates the three resolution tiers and the third is
  `diag_position(_, 1, 1)`.
- `chat_log/20260802.2...pl:121-122`: all seven expansion passes carry zero
  positions across 1608 lines.

So a bad node kind inside a native unquoted S-expr resolves to the enclosing
statement's start, exactly as a bad node kind inside a quoted string would. The
options are NOT "spans vs no spans". The genuine asymmetry is narrower, is
about parse-time syntax errors only, and is priced in section 3.

**P2. `2_hosts_wiring.pl` cannot be the phase-2 execution gate.** The brief and
the rulings doc both suggest wiring that fixture to a real runner. The fixture
is an oracle-only conformance fixture: it asserts final relation contents
computed by the prolog oracle, and its own `final(captured/1, [])` records that
zero rows come back. Making it execute would mean running a subprocess inside
`just conformance`, which today is a pure swipl run capped at 300s
(`v6/justfile:39-40`). The executing receipt belongs in `v6/tsv2/tests` and in
`extraction-live`, and the conformance fixture stays an emitted-text assertion.
Recorded as a deviation from the rulings doc's "Unbuilt queue" bullet 1.

### 1.4 Unverifiable at this base

| claim | status |
|---|---|
| type-IR lane's SCIP-identity ruling (symbol strings as fact ids) | **UNVERIFIED.** `v6/prolog/rulings.pl` does not exist; the rulings file is `v6/prolog/conformance/rulings.pl` and it has zero `scip` hits. No plan document dated 2026-08-02 or later mentions symbol strings as fact ids. That lane's output is not in this tree at 2eceb836 |
| whether the 87x gap closes with a serve protocol | **UNMEASURED.** The 87x figure is a receipt for the CURRENT shape. No experiment in the tree measures a long-lived extractor. Step B5 is the measurement, not a restatement |
| the 87x decomposition | **UNMEASURED.** `ARCH.pl:710` lists three confounders in its own text (v5 reads a git tree at HEAD while v6 hashes the working tree; v6 runs cst+type+call+df where v5 does a scan fact; v6 has no org fan-out spelling at all). Nothing in the tree separates process startup from parse+emit from engine overhead, so how much of the 87x `--serve` can even reach is unknown. That split should be measured BEFORE B1, because if parse+emit dominates then B3's parallelism is the win and the protocol is only its vehicle |
| **the stale-witness hazard** (suspected LIVE DEFECT, evidence complete, experiment not run) | see below |

**The stale-witness hazard.** Three facts hold individually; I did not run the
experiment that proves they compose, so this is stated as a hypothesis with its
chain, not as a finding.

1. The witness digest folds the declared INPUT columns and the salts, and
   nothing else. `digest_expr/6`, `1_host_expand.pl:562-566`. The shell
   TEMPLATE is not an argument to it.
2. The durable cache's primary key is `(host, witness_digest)`,
   `v6/tsv2/serve/1_hosts.ts:71-74`. The host NAME, not the host's command.
3. Nothing invalidates that table on a program swap. `clearDeadLocks`
   (`1_hosts.ts:77-81`) deletes only `state = 'pending'`; `grep -rn
   "program_hash\|programHash" v6/tsv2/` returns nothing.

If those compose, then editing a pattern INSIDE a host template while keeping
the host name and the db file leaves every already-answered file a permanent
cache hit, and the new pattern never runs. That is not hypothetical code: it is
the live spelling at `v6/dl/fixtures/1_rtkq-extraction-golden.dl6:30`, four
ast-grep patterns inside one template, gated in `green-all` as `rtkq-golden`.

Why it belongs in THIS plan rather than a defect queue: it is the exact failure
ruling 3 forecloses by making the query a request COLUMN instead of template
text, so it is the strongest available argument for step A5's `query_text =
identity` and it is a fail-first receipt A5 can be graded against. It is also
the second reading of section 3's option-(a) precedent (see 3.7's "shipping
precedent in-tree" row): the RTKQ fixture works, and if this hypothesis holds it
works partly because nobody has edited those patterns.

Owed before any fix: the receipt. Serve a program with a template-embedded
pattern to `done`; edit the pattern; re-serve against the same db; assert the
host refires. If it does not refire, price the general fix carefully, because
putting the template into the witness digest invalidates every existing witness
in every existing db exactly once, which is a one-time full re-extraction and
must not be discovered during a soak.

### 1.5 Found while verifying, not in the brief, and needed by the ladders

| fact | location |
|---|---|
| `tree-sitter = "0.25"` is ALREADY a direct dependency of `sprefa-extract` (added for the Go front-end), with a comment stating cargo unifies it with ast-grep's copy | `v6/sprefa-extract/Cargo.toml:39-45` |
| `ast_grep_core::tree_sitter::LanguageExt::get_ts_language(&self) -> TSLanguage` is a public trait method, and `TSLanguage` is a re-export of `tree_sitter::Language` | vendored source, `~/.cargo/registry/.../ast-grep-core-0.38.7/src/tree_sitter/mod.rs:274` and `:13` |
| `StrDoc<L> { pub src: String, pub lang: L, pub tree: Tree }` has a PUBLIC `tree` field, and `StrDoc::try_new(src, lang)` is public. `Doc::Node<'r>` for `StrDoc` is `tree_sitter::Node` | same file, `:45-67`, `:69-95` |
| the executor-selection seam: `host_execution/3` (clause ORDER is the selection, and the header comment says so), `host_executor_contract/2`, `host_input_contract/3` | `v6/prolog/compile/registry.pl:300-355` |
| the served executor registry and the fold set | `v6/tsv2/serve/1_hosts.ts:250-274`. `runSprefaExtract` at `:252-254` is a one-line delegate to `runShellLine` at `:226-248`, which does one `spawn` per call and `child.kill()` on unsubscribe at `:246` |
| the served host loop is serialized | `v6/tsv2/serve/1_hosts.ts:523-527`, `concatMap((batch) => from(groupInvocations(batch)).pipe(concatMap((invocation) => this.runInvocation(invocation))))`. Two nested `concatMap`s. This is the code behind the measured concurrency 1.0 |
| comments already arrive on the CST plane as ordinary named nodes | `v6/sprefa-extract/tests/fixtures/ts/sample.cstf.snap:214`, `{"record":"node","family":"cst","span":{"start":164,"end":236},"kind":"comment","name":null}` and three more at `:343`, `:380`, `:397` |
| the `sg_pattern/3` metavariable refusal | `v6/prolog/1_host_expand.pl:419-420` throws `unmapped_feature(slot_sg_metavariable_semantics, Term)`; registry row `registry.pl:194`; SYNTAX rows `:162` and `:331` |
| A14 open-item record | `plans/2026-07-29-hosts-extraction-verdict.md:447`, slot name `slot_comment_span_trailing_bind` |
| gate recipes | `v6/justfile:368` `green: conformance roundtrip text-door plunit prolog-lint typecheck tsv2-test import-gate one-subscribe golden-flex dl-test store-test`; `:371` `green-all` adds 20 more legs. `capped` at `:29` is `tools/run-capped.sh` |
| dl6 textmate grammar | `editors/vscode-dl/syntaxes/dl6.tmLanguage.json`, 102 lines. `ts_query` is already in the `keyword.control.dl6` alternation |

---

## 2. Step ladders

### Ladder A: phase-2 tree-sitter query runner

Six steps. A1 and A2 are extractor-local and land alone. A3 is the step that
must not be taken before the metavariable ruling (section 5, D2). A4 through A6
are the wire.

---

**A1. Tree-sitter query execution in the extractor library.**

Owns: `v6/sprefa-extract/src/lang/tsquery.rs` (new), re-export line in
`src/lang/mod.rs:18` region and `src/lib.rs:45` region.
LOC: ~110 new, ~6 edited. Test: ~45.

Signatures first.

```rust
/// One compiled tree-sitter query and the caller's id for it. The query text
/// is exactly what `compile_ts_query/2` emitted; this crate never rewrites it.
pub struct TsQuerySpec { pub id: String, pub query_text: String }

/// Byte-addressed. Field-for-field identical to AstCaptureFact (astgrep.rs:42-52)
/// so both matchers share one JSONL row shape and one decode path downstream.
pub struct TsCaptureFact {
    pub record: &'static str, pub query: String, pub capture: String,
    pub text: String, pub start: u32, pub end: u32,
    pub match_start: u32, pub match_end: u32,
}

pub fn run_ts_queries(
    path: &str, content: &[u8], queries: &[TsQuerySpec],
) -> Result<Vec<TsCaptureFact>, ParseError>;
// body:
//   lang    = SupportLang::from_path(path).ok_or(ParseError::NoGrammar)   // astgrep.rs:63
//   source  = str::from_utf8(content).map_err(ParseError::Utf8)           // astgrep.rs:65
//   ts_lang = lang.get_ts_language()          // LanguageExt, ast-grep-core mod.rs:274
//   doc     = StrDoc::try_new(source, lang)   // ONE parse; doc.tree is pub
//   for each spec:
//     query  = tree_sitter::Query::new(&ts_lang, &spec.query_text)
//                .map_err(|e| ParseError::Parse(format!("ts query '{}': {e}", spec.id)))
//     cursor.matches(&query, doc.tree.root_node(), source.as_bytes())
//     per match: match_start/match_end = min/max capture byte range
//     per capture: name from query.capture_names(), text from utf8_text
//   sort by (query, match_start, match_end, capture, start, end, text); dedup
//   (byte-for-byte the astgrep.rs:107-127 ordering law)
```

Instance lifetimes: `StrDoc` lives for the call and owns both the source
`String` and the `tree_sitter::Tree`. `Query` lives per spec. `QueryCursor`
lives per spec. Nothing is cached across calls at this level; per-invocation
parse reuse is A1's `doc`, and cross-invocation reuse is the engine's job by
ruling 3.

The one-parse-serves-both claim in the rulings mermaid is achievable and this is
why: `StrDoc { pub src, pub lang, pub tree }` exposes the tree, `StrDoc::try_new`
is public, and `AstGrep::doc(strdoc)` consumes a `StrDoc` to build the ast-grep
root. So one `StrDoc` can serve the CST projection, the ast-grep pattern path,
and the tree-sitter query path. Verified against the vendored crate source, not
inferred.

Gate: `cd v6/sprefa-extract && cargo test`. New file
`tests/9_ts_query_cli.rs` with a snapshot over an existing fixture. Under 10s
per the 10-second law; the crate's existing test suite is already the yardstick.

Failure modes.
1. `Query::new` errors carry an offset into the query text. Swallowing them
   turns a compiler-emitted typo into zero rows, silently. The precedent for
   surfacing is `astgrep.rs:74-76`. This is the highest-value error path in the
   whole ladder because the query text was machine-generated and the author
   never typed it.
2. A grammar that ast-grep supports may reject a query that names a node kind
   the grammar does not have. Same error path, different message. Both must name
   the spec id, or a batch of queries fails anonymously.
3. `SupportLang::from_path` is extension-driven. A file with no mapped extension
   returns `NoGrammar`, and under a batch that must not abort sibling queries on
   other files.

---

**A2. `--ts-query` on the extract CLI.**

Owns: `v6/sprefa-extract/src/bin/extract.rs`.
LOC: ~28 in the `Cli` struct (mirroring `:124-150`), ~14 dispatch (mirroring
`stream_ast_queries/3` at `:414-422`), ~10 in the `--schema` output.

```rust
/// Tree-sitter query in ID=QUERY form. Repeat to batch queries over one parse.
#[arg(long = "ts-query", value_name = "ID=QUERY",
      action = clap::ArgAction::Append,
      conflicts_with_all = ["family", "bench", "resolve"])]
ts_query: Vec<String>,
```

`--ts-query` and `--ast-pattern` are NOT mutually exclusive: one parse can serve
both, and forbidding the combination throws away the only structural advantage
A1 bought. Document that in the flag's long help.

Gate: `tests/9_ts_query_cli.rs` asserts exact JSONL over
`tests/fixtures/ast_pattern/0_rtkq.ts` or a new fixture.

Failure mode: the `ID=QUERY` split is on the first `=`, and tree-sitter query
text contains `=` inside `#eq?` predicate strings. `splitn(2, '=')` is required;
a naive `split('=')` corrupts every predicate-bearing query. The existing
`--ast-pattern` parser at `:390-402` is the shape to copy and to check for this
same bug while there.

---

**A3. Name the executor and its contract in the compiler.**

Owns: `v6/prolog/compile/registry.pl`,
`v6/prolog/compile/test/plunit_tests.pl`.
LOC: ~12 in registry, ~35 in tests.

Three facts, and the clause ORDER of the first one is the whole selection
(`registry.pl:301-319` states this in its own header, with a measured
`host_executor_mismatch` as the receipt for getting it wrong):

```prolog
host_execution(Name, Template, sprefa_extract_ts) :-
    ( Name == ts_extract
    ; string(Template),
      sub_string(Template, 0, _, _, "\"$DL_EXTRACT_BIN\" --ts-query ")
    ), !.
% MUST sit ABOVE registry.pl:325 (the generic sprefa_extract row): that row
% claims any template starting `"$DL_EXTRACT_BIN" ` and ending `{path}`.

host_executor_contract(sprefa_extract_ts,
                       [col(path, text), col(digest, text),
                        col(query_text, text), col(grammar_hash, text)]).

host_input_contract(ts_extract,
                    [col(path, text), col(digest, text),
                     col(query_text, text), col(grammar_hash, text)],
                    [identity, freshness, identity, RoleForGrammarHash]).
```

`RoleForGrammarHash` is the open sub-ruling; see A5.

The `.dl6` an author writes, and its pure-rxjs lowering (repo law):

```
sh ts_extract(path: text, digest: text, query_text: text, grammar_hash: text)
   -> (capture: text, text: text, start: int, end: int)
   = `"$DL_EXTRACT_BIN" --ts-query q={query_text} {path}`.

rel deprecated_call(file_path: text, call_start: int, call_end: int, callee_name: text).
deprecated_call(file_path, call_start, call_end, callee_name) <-
  file(file_path, content_digest),
  grammar(file_path, grammar_digest),
  probe(ts_extract,
        [file_path, content_digest,
         "(call_expression function: (identifier) @callee_name)", grammar_digest],
        [_capture_name, callee_name, call_start, call_end], []),
  deprecated(callee_name).
```

rx lowering of that rule:

```
const demand$ = combineLatest([file$, grammar$]).pipe(
  map(([fileRow, grammarRow]) => demandRow(fileRow, grammarRow, queryText)),
  distinct(row => row.witnessDigest),          // the __host_witness claim, RX-H1
);
const capture$ = demand$.pipe(
  mergeMap(row => tsExtractClient.submit(row)),   // B3's client; today concatMap
);
const deprecatedCall$ = capture$.pipe(
  withLatestFrom(deprecated$),
  filter(([capture, deprecatedSet]) => deprecatedSet.has(capture.callee_name)),
  map(([capture]) => toRow(capture)),
);
// The `deprecated` filter runs AFTER the host answers. That is the acked
// pushdown caveat (rulings doc, "Known caveat"), visible here as a downstream
// filter rather than an upstream one.
```

Gate: `just plunit`, `just conformance`, `just text-door`.

Failure modes.
1. Clause order, as above. A row placed after `:325` never fires.
2. `validate_host_executor/3` (`1_host_expand.pl:194-199`) throws
   `host_executor_mismatch(Name, Executor, Inputs)` when the contract and the
   declared inputs disagree. That is the good failure. The bad one is a template
   that falls through to `host_execution(_, _, shell)` at `:332` and silently
   runs as a generic shell host with no fold and no contract check.
   Receipt for why the NEW row is mandatory rather than optional: the existing
   contract is an EXACT list, `registry.pl:334-335`
   `host_executor_contract(sprefa_extract, [col(path, text), col(digest, text)]).`
   A four-input host whose template resolves to `sprefa_extract` is therefore
   refused at load today. Widening the request columns is not a matter of adding
   them to a decl; it is blocked until this row exists. That is also the good
   news: the refusal is decidable at load, so the gap cannot be reached by
   accident.
3. `no_reserved_columns/3` (`1_host_expand.pl:184-185`, reserved list at
   `:280`) will reject `identity_digest` / `witness_digest` / `ordinal` as
   column names. `query_text` and `grammar_hash` are clear, but confirm against
   the full reserved list before naming.

---

**A4. Register the executor in the served runtime.**

Owns: `v6/tsv2/serve/1_hosts.ts`.
LOC: ~8 (two map entries), ~4 in the fold set.

```ts
export const HostExecutors: ReadonlyMap<string, HostExecutor> = new Map([
  ["shell", runShellLine],
  ["sprefa_extract", runSprefaExtract],
  ["sprefa_extract_repo", runSprefaExtract],
  ["sprefa_extract_ts", runSprefaExtract],   // new
]);
const ApplicativeExecutors = new Set([
  "sprefa_extract", "sprefa_extract_repo", "sprefa_extract_ts",  // new
]);
```

The fold membership is a real decision, not boilerplate. `groupInvocations/1`
at `:477-495` folds demands that share an invocation key. Two ts-query demands
over the SAME file with DIFFERENT query text are exactly the case the fold
should cover (one parse, two queries), and that requires the group key to carry
path but not query text, with the queries accumulating into repeated
`--ts-query` flags. If the key includes query text, the fold degenerates to
one subprocess per query and A1's batching is wasted.

Gate: `just tsv2-test`, `just extraction-live`.

Failure mode: `runInvocation` builds one command line. Folding N queries into
one command line hits the shell-length ceiling on a corpus with many patterns,
and the existing template splice already carries an escaping hazard flagged in
`ARCH.pl:814` ("unescaped shell injection in the `{col}` template splice with
`spawn(shell:true)`"). Query text is machine-generated and contains quotes,
parens, and `#match?` regexes. This is the single most likely place for A to
produce a wrong answer rather than an error, and it is one more argument for
ladder B (a serve protocol moves query text out of `argv` entirely).

---

**A5. Cache identity: the input roles.**

Owns: `v6/prolog/compile/registry.pl` (`host_input_contract/3` only),
`v6/prolog/compile/test/plunit_tests.pl`.
LOC: 0 new lines beyond A3; this step is a ruling plus its test.

Per C5, request columns already extend both digests. What must be ruled is the
role of each new column, because the roles are not symmetric:

| column | `identity` | `freshness` |
|---|---|---|
| meaning | part of what the answer is about; returns on the response row; enters BOTH digests | invalidation salt only; stays on the demand row; enters the WITNESS digest only |
| precedent | `path` (and `repo` for the repo twin), `registry.pl:345-355` | `digest`, same rows |

`query_text` is unambiguously `identity`: two queries over one file are two
different answers and both must be able to coexist in the response relation.

`grammar_hash` is genuinely open. Ruling 3 says it "salts the effect digest",
and "salt" in this codebase is the freshness role by name
(`salt_digest_parts/2`, `1_host_expand.pl:592-595`). Freshness gives the
invalidation without widening the response relation, which is almost certainly
what was meant. Identity would put a grammar version string on every capture
row. Recommend `freshness`, flag it as needing one word from the owner, and
note that this is the ONE place where the ruling's word ("salts") and the
codebase's word ("salt" = freshness) happen to agree exactly, which is weak
evidence but is evidence.

Gate: a `plunit` test that compiles the host decl and asserts the emitted
`concat(...)` digest expression contains `|query_text:text=` in both the
identity and witness parts, and `|grammar_hash:text=` in the witness part only.
This is a compile-time string assertion, not a runtime one, so it costs
milliseconds.

Failure mode: getting the role wrong is silent. `identity` for `grammar_hash`
compiles, runs, caches correctly, and adds a column nobody asked for to every
response row forever. `freshness` for `query_text` compiles, runs, and silently
collapses two different queries over one file into one witness, returning the
first query's captures for the second query. Write the fail-first receipt for
the second case.

---

**A6. Receipts: one executing query end to end.**

Owns: `v6/tsv2/tests/` (new test file), `v6/tsv2/scripts/extraction-live.sh`
(one new phase), `v6/prolog/compile/SYNTAX.md:330`,
`v6/dl/fixtures/golden-flex.dl6:458-461`.
LOC: ~120 test, ~30 script, ~4 prose.

Per P2, the conformance fixture at `2_hosts_wiring.pl:200-243` does NOT become
executing. It stays an emitted-text assertion, which is what it is good at.
The executing receipt is a tsv2 test that boots the served engine, submits a
`.dl6` carrying the `ts_extract` host, and asserts exact capture rows against a
committed fixture file.

Prose to rewrite when this lands, both of which currently assert the gap:
`SYNTAX.md:330` and the comment block at `golden-flex.dl6:458-461`.

Gate: `just tsv2-test`, `just extraction-live`, then `just green`.

Failure mode: `v6/dl/tasks.d.ts:346` pins `DEFAULT_EXTRACT_BIN` as a string
literal type pointing at a REMOVED worktree
(`.claude/worktrees/extract-golden-plan/...`), recorded as a live blocker in
`chat_log/20260802.2...pl:150-156`. Any new test that resolves the extract
binary through that constant inherits the defect. Use the
`extraction-live.sh` successor shape, which the same finding names as already
carrying the fix.

---

### Ladder B: `extract --serve`

**B0. Buy-vs-build check on the wire protocol.**

The repo law is unconditional: no bespoke code for a common-shaped problem
without candidate-by-candidate library research first. The owner has already
ruled NDJSON. This section does not re-litigate the ruling; it records what the
alternatives would have supplied so the bespoke loop is written knowing what it
owes.

| candidate | Rust side | Node side | what it supplies | what it costs |
|---|---|---|---|---|
| **JSON-RPC 2.0 over stdio, `lsp-server`** | `lsp-server` (rust-analyzer's own crate, a generic Content-Length-framed JSON-RPC stdio server, no LSP types required). **Already in this repo**: v5 root `Cargo.toml:122` pins `lsp-server = "0.7.9"` beside `lsp-types` and `crossbeam-channel`, so the Rust side is a known quantity, not a new evaluation | `vscode-jsonrpc` (mature, same framing, `$/cancelRequest` built in) | framing, id correlation, cancellation, error envelope, an already-debugged partial-read path | 1 new dep on the Node side and a new one for `sprefa-extract` (which is a standalone workspace, `Cargo.toml:4`, and does not inherit the v5 root's deps); length-prefix framing is strictly harder to eyeball in a log than NDJSON; the extractor's stdout is ALREADY NDJSON so the framing would change an existing wire. Worth reading for its cancellation semantics even though the ruling declines the wire |
| **MCP stdio transport** | `rmcp` | `@modelcontextprotocol/sdk` | a specified protocol with id-tagged requests, notifications, and cancellation; NDJSON of JSON-RPC 2.0, so the existing line shape survives | the schema is agent-shaped (tools, resources, prompts), not job-shaped; two heavy SDKs to get a request/response loop; the spec's evolution is outside this repo's control |
| **`tarpc`** | yes | none | typed RPC, futures-based | the peer is Node, so this is not applicable. Excluded on that ground alone |
| **bespoke NDJSON (the ruling)** | `serde_json` (already a dep, `Cargo.toml:59-60`) | `JSON.parse` per line | nothing beyond what is written | must hand-build: cancellation, backpressure, partial-line reassembly, per-job error isolation |

What the ruling already absorbs: cancellation is named in the brief ("cancel
line"), so item 1 of the bespoke debt is acknowledged. What it does not name:
backpressure. A long-lived extractor that is fed faster than it drains will
buffer jobs in the child's stdin pipe and rows in its stdout pipe, and the
failure looks like a hang, not an error. B2 and B3 below carry that as an
explicit obligation.

One narrow buy that costs nothing and is worth naming: `serde_json`'s
`Deserializer::from_reader(stdin).into_iter::<ServeLine>()` gives streaming
line-by-line deserialization with correct partial-read handling, which is the
one piece of the bespoke loop most likely to be written wrong by hand. That is
already a dep, so it is a buy with zero new dependency.

---

**B1. The wire envelope.**

Owns: `v6/sprefa-extract/src/serve.rs` (new, types half),
`v6/sprefa-extract/src/wire.rs` (existing flat-envelope home; check whether the
new types belong there instead).
LOC: ~55 types, ~40 round-trip test.

```rust
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ServeLine {
    Job(JobLine),
    Cancel { id: String },
    Shutdown,
}

#[derive(Deserialize)]
pub struct JobLine {
    pub id: String,
    pub path: String,
    pub families: Option<Vec<String>>,
    pub ts_query: Option<Vec<TsQuerySpec>>,
    pub ast_pattern: Option<Vec<AstPatternQuery>>,
}

#[derive(Serialize)]
pub struct DoneLine {
    pub id: String,
    pub done: bool,            // always true; the field is the reader's sentinel
    pub rows: u64,
    pub error: Option<String>,
}
```

Every fact line the child emits gains one field: `id`. That is the only change
to the existing JSONL row shapes, and it is additive, so the existing
non-serve output stays byte-identical when the field is skipped
(`#[serde(skip_serializing_if = "Option::is_none")]`).

Gate: `cargo test` round-trip over each variant.

Failure mode: `#[serde(tag = "op")]` on `ServeLine` collides with any future
job field literally named `op`. Reserve it.

---

**B2. The serve loop.**

Owns: `v6/sprefa-extract/src/serve.rs`, `v6/sprefa-extract/src/bin/extract.rs`
(one `--serve` arm).
LOC: ~130 serve, ~15 CLI.

Storage layout, then reads and writes, then uniqueness:

- one `Mutex<BufWriter<Stdout>>` for the process. Every emitted line takes the
  lock for the duration of one `writeln!` plus one `flush`. This is the entire
  interleaving discipline and it is not optional.
- one `DashMap<String, AtomicBool>` (or `Mutex<HashSet<String>>` given the
  expected small size) of cancelled ids. Written by the reader thread on a
  `Cancel` line, read by workers between files and between queries within a
  job.
- per job: one `StrDoc` (owns source + tree), dropped at job end.

Sequence: reader thread owns stdin and never blocks on work. Jobs go to a rayon
scope. Each worker emits its fact lines then its `DoneLine`, both under the
stdout lock. `Shutdown`, or stdin EOF, drains outstanding jobs and exits 0.

No async anywhere: the crate charter is `Cargo.toml:9` "Sync, rayon-parallel,
arena-mastered. No DB, no async."

**The charter's "rayon-parallel" is aspirational and the LOC above assumes it is
already true. It is not.** `grep -rn rayon v6/sprefa-extract/` returns exactly
three hits, all prose: the description string at `Cargo.toml:10`, a comment at
`src/dispatch.rs:3-5`, and one at `src/types.rs:1785`. `rayon` is absent from
`[dependencies]`. The dispatch it would parallelize is single-file and
single-threaded today, `src/dispatch.rs:14-16`:

```rust
pub fn dispatch(path: &str, content: &[u8], mask: FamilyMask) -> Option<ExtractOutput> {
    source_for(path).map(|src| src.extract(path, content, mask))
}
```

and `dispatch.rs:3-5` says the parallel version "land[s] in the parallelism lab
(epic 4)", which has not landed. So B2 carries a dependency addition (`rayon =
"1"`, precedented in the v5 root at `Cargo.toml:100`) plus the arena-per-worker
work `dispatch.rs:3-5` describes, and the `ExtractOutput`/`Strings` interning
path has never been exercised from more than one thread. Add ~60 LOC and treat
the thread-safety review of `Strings`/`FamilyBundle` as a real step, not a given.
This also means B2 is the FIRST place per-file parallelism can exist at all,
which strengthens B5's case: a long-lived process is the prerequisite for a
worker pool, so part of B's payoff is parallelism the current shape cannot have.

Gate: `cargo test` with an integration test that pipes a scripted NDJSON
sequence (two jobs, one cancel, one shutdown) and asserts the output multiset
and the `done` lines. Under 10s.

Failure modes, ranked by how silent they are.
1. **Interleaved partial lines.** Two workers writing without the lock produce
   a spliced JSON line that parses as neither job's row. This corrupts data and
   does not error. The lock is the mitigation and the test above is the receipt.
2. **Backpressure deadlock.** The child writes rows faster than the parent
   reads; the OS pipe buffer fills; the child blocks in `write`; the parent is
   blocked writing more jobs into a full stdin pipe. Classic two-pipe deadlock,
   and it presents as a hang. The parent MUST read stdout continuously and
   independently of when it writes stdin (B3 owns this).
3. **Rayon pool seizes the machine.** `rayon::ThreadPoolBuilder` default is one
   thread per core. The standing law is that nothing seizes the machine and
   budgets are capped. The pool size must come from a flag with a default below
   core count, not from rayon's default.
4. **A panicking worker kills the process.** `catch_unwind` per job, emit
   `{id, done: true, rows: 0, error: "..."}`. Without it, one malformed file
   ends the served engine's extractor and every later demand fails.
5. **Orphaned child.** If the parent is SIGKILLed the child keeps running with a
   dead stdin. Exit on stdin EOF handles the clean case; a
   `--parent-pid` watchdog or `prctl`-equivalent is the unclean case, and macOS
   has no `PDEATHSIG`, so the honest answer on this platform is the EOF check
   plus the parent's `child.kill()` on unsubscribe (`1_hosts.ts:246` precedent).

---

**B3. Runtime side: a long-lived client.**

Owns: `v6/tsv2/serve/1_hosts.ts`, `v6/tsv2/runtime/types.ts` (interface
declaration, per the header-types law).
LOC: ~170 client, ~25 types, ~90 test.

Interface first, in `runtime/types.ts` because the header-types law requires
every new class to declare its interface in the package's header:

```ts
export interface IExtractServeJob {
  readonly id: string;
  readonly path: string;
  readonly tsQuery?: readonly { id: string; queryText: string }[];
  readonly astPattern?: readonly { id: string; pattern: string }[];
}

export interface IExtractServeClient {
  /** Cold. One subscription = one job submitted; unsubscribe sends a cancel
   *  line. Completes on the job's {id, done} line. */
  submit(job: IExtractServeJob): Observable<IRow>;
}
```

The rx spelling, which is the whole point of the step:

```ts
// ONE shared line stream off the child's stdout, multiplexed by id.
const line$ = readLines(child.stdout).pipe(map(parse), share());

submit(job) {
  return defer(() => {
    writeJobLine(job);                       // side effect at subscribe, not before
    return line$.pipe(
      filter(line => line.id === job.id),
      takeWhile(line => !line.done),          // the {id,done} sentinel ends it
      map(toRow),
      finalize(() => writeCancelLine(job.id)),
    );
  });
}
```

No Subscription fields, no Subject bridges, no `.subscribe()` (the ratchet
baseline of 1 manual subscribe stays at `main.ts`). `share()` on `line$` is what
makes the child's stdout a single reader with N logical consumers, and it is
also what satisfies B2's backpressure obligation: the parent drains stdout
continuously regardless of the job submission cadence.

Then `runSprefaExtract` at `1_hosts.ts:252-254` stops being a delegate to
`runShellLine` and becomes a call into the client.

Gate: `just tsv2-test`, `just extraction-live`, `just one-subscribe`
(the ratchet).

Failure modes.
1. **The concurrency does not change.** `1_hosts.ts:523-527` is two nested
   `concatMap`s. Swapping the spawn for a serve client while keeping `concatMap`
   buys process reuse and nothing else. The throughput claim needs the inner
   `concatMap` to become `mergeMap(..., N)`, and N is a budget number that must
   be named, not defaulted. This is the step where the 87x either moves or does
   not, and it is one line.
2. **`await` on an Observable.** The standing trap: `await someObservable`
   never subscribes. The client is Observable-returning throughout, the host
   runner is already rx, so the seam is clean, but the decode helpers at
   `1_hosts.ts:283-330` are sync and must stay sync.
3. **Child lifetime vs program swap.** The served engine swaps programs. A
   client held across a swap must not leak a child per swap; `just leak-soak`
   is the gate that catches it.

---

**B4. Endurance and lifecycle.**

Owns: `v6/tsv2/serve/1_hosts.ts` (teardown), no new files.
LOC: ~40.

The durable side already exists and is correct for spawn-per-witness:
`WitnessCache.claim` before, `settle` after, `clearDeadLocks` at boot
(`1_hosts.ts:77-119`). With a long-lived child, one new hole opens: a SIGKILL of
the ENGINE leaves `pending` rows (handled by `clearDeadLocks`) AND an orphaned
child (handled by nothing). B2 failure mode 5 is the mitigation; this step is
where it gets its receipt.

Gate: `just endurance` (kill -9, reboot, exactly-once), `just leak-soak`,
`just serve-endurance`, `just serve-leak-soak`.

Failure mode: `clearDeadLocks` deletes ALL `pending` rows at boot
(`:79`). With a long-lived child that survives a fast engine restart, a job
genuinely in flight in the child would have its lock cleared and be re-fired.
Either the child dies with the parent (B2 mode 5, which makes this moot) or the
claim protocol needs a generation counter. Prefer the former.

---

**B5. The measurement.**

Owns: nothing in `src`. Re-runs `v6/tsv2/scripts/crawl-bench.sh` at the same
8-repo pin and records the new number beside 40.68 files/s.

This is NOT a green-all leg: `ARCH.pl:710` records that `crawl_bench` is
deliberately out of green-all, and the run is 19.15s at the 8-repo pin, already
over the 10-second law and standing as a named exception in the same class as
SCIP indexing.

What the number must be compared against, honestly:
- 40.68 files/s, v6 served, 779 files / 8 repos (the before)
- 3,540.9 files/s, v5 org-fan, 42,739 files / 389 repos, same machine, same run
- the doc's own stated non-comparabilities (`CRAWL-BENCH.md`): v5 reads a git
  tree at HEAD while v6 hashes the working tree; v6 has no org fan-out spelling;
  v6 runs cst+type+call+df where v5 does a scan fact

A serve protocol addresses process-boundary cost only. It does not close the
family-count gap or the working-tree-hash gap, so a result well short of 3,540
is the expected outcome and is not a failure of B.

Failure mode: reporting the delta without the concurrency number from B3 mode 1
makes the result uninterpretable. Record both.

---

## 3. Decision packet C: surface spelling

**This packet does not pick a winner.** The STRING ruling settles what crosses
the host boundary. What an author TYPES is unruled, and the ruling document says
so at `plans/2026-08-02-cst-query-rulings.md:88-95`.

### 3.1 The three options

| | spelling | status today |
|---|---|---|
| **(a)** | quoted pattern, parsed at compile time. Precedent: v5 `ast`, where the compiler parses, refuses unmapped shapes, and binds `@captures` to dl variables. Never a pass-through blob | not built |
| **(b)** | native unquoted S-expr in the dl6 surface | not built |
| **(c)** | bare `ts_query/1` term form | **live today.** `registry.pl:193`, `golden-flex.dl6:463`, `2_hosts_wiring.pl:207-228`, `dl_view/native_ts_query_term.dl6:4` |

All three lower through the same term to text path
(`compile_ts_query/2`, `1_host_expand.pl:414-422`). The emitter is written and
covers everything except anchors, negation, and sg metavariables. That cost is
identical across the three and is therefore not a differentiator.

Worked comparison of the same query, so the reader is comparing text and not
descriptions:

```
# (a) quoted
deprecated_call(file_path, call_start, call_end, callee_name) <-
  file(file_path, content_digest),
  ts("(call_expression function: (identifier) @callee_name)",
     file_path, content_digest, callee_name, call_start, call_end),
  deprecated(callee_name).

# (b) native
deprecated_call(file_path, call_start, call_end, callee_name) <-
  file(file_path, content_digest),
  ts((call_expression function: (identifier) @callee_name),
     file_path, content_digest, callee_name, call_start, call_end),
  deprecated(callee_name).

# (c) bare term, as it exists today
query_value(ts_query([node(call_expression,
  [field(function, capture(callee_name, node(identifier, [])))])])) <- unit.
deprecated_call(file_path, call_start, call_end, callee_name) <-
  file(file_path, content_digest), query_value(query_text),
  probe(ts_extract, [file_path, content_digest, query_text, grammar_digest],
        [_capture, callee_name, call_start, call_end], []),
  deprecated(callee_name).
```

rx lowering, identical for all three (this is the point: the surface choice does
not reach the lowering):

```
combineLatest([file$, grammar$]).pipe(
  map(toDemandRow), distinct(r => r.witnessDigest),
  mergeMap(row => tsExtractClient.submit(row)),
  withLatestFrom(deprecated$),
  filter(([capture, set]) => set.has(capture.callee_name)),
  map(([capture]) => toRow(capture)),
)
```

### 3.2 Parser cost

The DCG productions needed are the SAME set for (a) and (b): the emitter accepts
15 term forms (`group`, `node`, `field`, `capture`, `capture_ref`, `anonymous`,
`string`, `predicate(eq,_,_)`, `predicate(match,_,_)`, `quant` x3,
`alternative`, `wildcard`, `named_wildcard`), which map to roughly 8 S-expr
productions.

Baseline for calibration: `parse_dl.pl:1462-1520` is 59 lines for 5 productions
with left-associative rest loops. An S-expr grammar is simpler per production
(no precedence, no rest loops) but has more of them and needs a `@name` /
`#pred?` / quantifier-suffix lexer.

| | new lines in `parse_dl.pl` | new lines in `print_dl.pl` | emitter lines |
|---|---|---|---|
| (a) | 120-180 for the inner DCG, plus ~25 refusal wiring. The outer parser already produces the string: `string_lit/4` at `:489` | ~0. The pattern round-trips as the string it already is | 0 |
| (b) | 120-180 for the same productions, plus the collision work below | 40-70. A printer for the term, gated by `roundtrip` (109/109) and `text-door` (196/196) | 0 |
| (c) | 0 | 0 | 0 |

**The extra cost of (b) is a grammar collision, and it is concrete.**
`factor/5` at `parse_dl.pl:1501-1520` tries alternatives in order, and its FIRST
alternative is `lit_dcg(`(`)` for a parenthesized expression (`:1504-1505`). A
bare `(call_expression ...)` in a rule body enters that arm, parses
`call_expression` as a variable by the bare-identifier rule, then fails on the
next token. So (b) requires either a positional restriction (the S-expr is
reachable only in an argument slot where an expression is not) or lookahead
disambiguation inside `factor/5`. Estimate 25-50 additional lines and a
non-trivial risk of regressing the 771-input refusal-position test pinned by
`plunit parse_error_positions` (`ARCH.pl:885`).

### 3.3 Langium and the JS door

`v6/dl/grammar/dl.langium` is 190 lines and, by its own header comment plus
`parse_dl.pl:1457-1459`, has NO expression grammar at all: `ArgTerm := Var |
Literal | Wildcard`.

`ARCH.pl:663` (`task(surface_dcg, done, ...)`) records the standing ruling:
"DCG is the CANONICAL parser (langium demoted)."

So the honest langium cost for BOTH (a) and (b) is plausibly zero, by an
existing ruling. That is the packet's question 1 for the owner: does the
demotion still hold, or must the JS door keep surface parity?

If parity is required: (a) needs zero langium change (a quoted pattern is
already a `Literal`); (b) needs a new production plus a regenerate of the
langium parser (`v6/dl/package.json:11` `"grammar": "langium generate"`) plus
`0_ast_bridge.ts` work. Call it 30-60 langium lines and an unmeasured bridge
delta.

### 3.4 Editor surface

`editors/vscode-dl/syntaxes/dl6.tmLanguage.json`, 102 lines.

(a): zero edits. The existing `string.quoted.double.dl6` rule (`:38-42`) colors
the pattern as one flat string. Node kinds, fields, and `@captures` get no
distinct color. That is a real ergonomic loss and it is the strongest argument
for (b).

(b): a new `begin`/`end` pattern block for the S-expr with inner rules for node
kind, `field:`, `@capture`, and `#predicate?`. Estimate 20-35 lines. One
concrete collision: the `entity.name.function.dl6` rule at `:82-83` matches
`\b[a-z_]\w*(?=\()`, so in `(call_expression function: ...)` the token
`call_expression` is followed by nothing and is safe, but `function:` and any
nested `(identifier)` sit next to parens in ways that need checking
case by case.

(c): already colored. `ts_query` is in the `keyword.control.dl6` alternation at
`:75`.

### 3.5 The diagnostics dimension, corrected

The brief's framing ("quoted patterns are opaque to squiggles while native
spelling gets spans for free") does not survive the code. See P1 in section 1.3
for the six citations. Summary: positions in this compiler are statement-
granular for EVERY construct. A semantic error inside a native S-expr resolves
to the enclosing statement's start, exactly as one inside a quoted string would.

Where (a) and (b) genuinely differ, and only here:

**Parse-time syntax errors.** Under (b), a malformed S-expr fails the DCG, and
`parse_failure/1` (`:139`) throws `dl_parse_error(Reason, position(Line,
Column))` with a real column derived from `mark_furthest/1` (`:147-153`). Under
(a), the outer parser sees a well-formed string literal, succeeds, and the inner
parse happens later against a bare code list with no offset.

**That gap is closable, cheaply, and here is the mechanism.** `string_lit/4`
at `:489` runs at a suffix position; the position machinery's currency is
exactly "remaining length" (`remaining_line_column/3`, `:161`). Handing the
string's start suffix-length into the inner DCG and adding the inner furthest
mark to it yields a true line:column inside the quoted pattern. Estimate 20-40
lines. After that, (a) and (b) have identical syntax-error quality.

**What neither option buys, at any price in this ladder:** semantic-error spans.
`compile_ts_query/2` throws `unmapped_feature(slot_ts_query_term, Term)` at
`:422` and `ts_pattern_text/2` throws `unmapped_feature(slot_ts_pattern_form,
Term)` at `:473-474`. Both carry the offending TERM and no position. Pointing a
squiggle at the offending sub-pattern needs a term-to-offset map, which does not
exist for any construct in the language.

**Cost of inner-position mapping, if (a) wins (the brief's explicit ask):**

Scoped to patterns ONLY, and only because the two throws above already carry the
term as their key:

| piece | lines |
|---|---|
| inner DCG records `(term_path, byte_offset)` pairs into a side table as it builds each sub-term | ~60 |
| `compile_ts_query/2` and `ts_pattern_text/2` look the offending term up before throwing, and throw a positioned variant | ~30 |
| `diag.pl` resolves the positioned variant; `dl6_span/6` (already exported, `diag.pl:32`) is the shape to extend | ~25 |
| plunit coverage over the three throw sites | ~40 |
| **total, pattern-scoped** | **~155** |

That number is only valid because patterns are ONE construct with ONE parser and
ONE consumer. The general fix, positions surviving all seven expansion passes
over 1608 lines, is the `sugar_spans_absent` arc and is **UNVERIFIED** in size:
nobody has scoped it. Do not read 155 as an estimate for that.

And note the symmetry the correction produces: this ~155 line cost is the SAME
under (b). Native spelling does not avoid it. It only avoids the 20-40 line
syntax-error-offset piece, which is a subset.

### 3.6 What is expensive to change later

(a) -> (b) later: the term vocabulary and the emitter are unchanged, so the
migration is a parser change plus a mechanical rewrite of every `.dl6` carrying
a pattern. Cheap while the corpus is small.

(b) -> (a) later: same, plus removing the `factor/5` disambiguation, which is
the risky part to unwind once other productions have been written around it.

(c) -> either: purely additive. (c) keeps working; it is a term form and the
sugar is another way to spell the same term. This is the option with zero
lock-in and the worst ergonomics.

### 3.7 The ruling table

| dimension | (a) quoted, parsed at compile | (b) native S-expr | (c) bare `ts_query/1` term |
|---|---|---|---|
| exists today | no | no | **yes**, live |
| `parse_dl.pl` new lines | 120-180 + ~25 refusals | 120-180 + ~25 refusals + **25-50 collision work** | 0 |
| `print_dl.pl` new lines | ~0 | 40-70 | 0 |
| emitter new lines | 0 | 0 | 0 |
| grammar collision risk | none | **`factor/5:1504-1505` parenthesized-expr arm**; risks the 771-input `parse_error_positions` pin | none |
| `dl.langium` | 0 (a pattern is a `Literal`) | 0 IF the langium demotion (`ARCH.pl:663`) holds; else 30-60 + bridge | 0 |
| textmate | 0 lines, and no inner coloring | 20-35 lines, full inner coloring; check the `:82-83` function-name rule | 0, already colored |
| syntax errors inside the pattern | statement-level unless the offset is threaded; **20-40 lines** to thread it, then equal to (b) | true line:column from `mark_furthest` | statement-level, same as everything |
| semantic errors inside the pattern | statement-level. **~155 lines** for pattern-scoped inner mapping | statement-level. **same ~155 lines** | statement-level |
| general sugar-span fix | UNVERIFIED size; separate arc; benefits all three equally | UNVERIFIED, same | UNVERIFIED, same |
| readability of a long query | one line, no escaping of `(`/`@`, but `"` inside `#eq?` needs escaping | no escaping at all | verbose; the fixture query is 1 line of ~300 chars as a term |
| cost to change later | low | low, minus the collision unwind | zero, additive |
| ships nothing new | no | no | **yes** |

Three questions the packet needs a word on, in the order they block work:

1. Does the langium demotion (`ARCH.pl:663`, "DCG is the CANONICAL parser") still
   hold? If yes, the langium column is zero for both and (b) gets materially
   cheaper.
2. Is inner-pattern coloring in the editor worth 25-50 lines of parser collision
   work? That is the single largest real difference between (a) and (b).
3. Does (c) stay reachable after (a) or (b) lands, or does the sugar become the
   only spelling? (c) is a live registry surface with a conformance fixture and
   a `dl_view` fixture, so removing it is a separate, gated change.

---

## 4. ARCH-style task rows

Shape verified against the file: `task(Name, Status, Needs)`, arity 3
(`v6/prolog/ARCH.pl:651` states the shape; 223 rows, all arity 3). The
project CLAUDE.md's "task/5" is stale, per C6. Gate for these rows:
`cd v6/prolog && swipl -g go -t halt ARCH.pl`, which checks the build-order
graph is acyclic and total, so every `Needs` name below must resolve to an
existing or co-landed row.

```prolog
% ── ladder A: phase-2 tree-sitter query execution ───────────────────────────
task(ts_query_runner,     unbuilt, []).
% sprefa-extract library: run_ts_queries/3 over tree_sitter::Query + QueryCursor.
% NO new deps: tree-sitter 0.25 is already a direct dep (Cargo.toml:45) and
% ast_grep_core::tree_sitter::LanguageExt::get_ts_language/1 hands over the
% grammar (vendored source mod.rs:274). StrDoc.tree is a pub tree_sitter::Tree,
% so ONE parse serves the CST projection, the ast-grep pattern path, and this.
% Port target: v5 run_ts, src/engine/eval.rs:1047-1079 (33 lines), converted
% from line numbers to byte offsets. Row shape reuses AstCaptureFact
% (astgrep.rs:42-52) field-for-field. Gate: cargo test, tests/9_ts_query_cli.rs.

task(ts_query_cli,        unbuilt, [ts_query_runner]).
% --ts-query ID=QUERY on the extract bin, mirroring --ast-pattern
% (src/bin/extract.rs:124-150) and stream_ast_queries/3 (:414-422). NOT
% conflicts_with --ast-pattern: one parse must be able to serve both. Split on
% the FIRST '=' only; #eq? predicate text contains '='.

task(ts_query_executor,   unbuilt, [ts_query_cli, sg_metavariable_ruling]).
% registry.pl host_execution/3 + host_executor_contract/2 + host_input_contract/3
% for a sprefa_extract_ts executor. CLAUSE ORDER IS THE SELECTION: the row must
% sit ABOVE registry.pl:325, which claims every "$DL_EXTRACT_BIN" template
% ending {path} (the file's own header at :301-319 records a measured
% host_executor_mismatch as the receipt for getting this wrong). BLOCKED on the
% metavariable ruling: this row decides whether one executor carries both
% matchers or there are two.

task(ts_query_serve_reg,  unbuilt, [ts_query_executor]).
% v6/tsv2/serve/1_hosts.ts: HostExecutors (:261-265) + ApplicativeExecutors
% (:274). The fold key must carry path and NOT query text, or groupInvocations
% (:477-495) degenerates to one subprocess per query and the batching is wasted.

task(ts_query_cache_role, unbuilt, [ts_query_executor]).
% Input ROLES only; ZERO schema change. Request cols already feed both digests:
% expand_probe/7 (1_host_expand.pl:548-549) -> digest_expr/6 (:562-566), a SQL
% concat over the declared inputs. query_text = identity (two queries over one
% file are two answers). grammar_hash = freshness RECOMMENDED, needs one word
% (ruling 3 says "salts", and salt IS the freshness role, salt_digest_parts/2
% :592-595). Fail-first receipt required for the query_text=freshness miscompile,
% which silently returns query 1's captures for query 2.

task(ts_query_witness_staleness, unbuilt, []).
% SUSPECTED LIVE DEFECT, evidence chain complete, experiment NOT run (PLAN.md
% section 1.4). digest_expr/6 (1_host_expand.pl:562-566) folds declared inputs
% and salts and NOT the shell template; __host_witness's PK is (host,
% witness_digest) (tsv2/serve/1_hosts.ts:71-74); nothing invalidates that table
% on a program swap (clearDeadLocks deletes only 'pending', :77-81; no program
% hash anywhere in v6/tsv2). If those compose, editing a pattern INSIDE a host
% template leaves every answered file a permanent cache hit and the new pattern
% never runs. The live program with four such patterns is
% v6/dl/fixtures/1_rtkq-extraction-golden.dl6:30, gated in green-all as
% rtkq-golden. This is the fail-first receipt ts_query_cache_role should be
% graded against, and it is the concrete argument for query_text = identity.
% The general fix (template into the digest) costs a ONE-TIME full
% re-extraction of every existing db; price that before landing it.

task(ts_query_receipt,    unbuilt, [ts_query_serve_reg, ts_query_cache_role]).
% Executing receipt is a tsv2 test + one extraction-live phase, NOT the
% conformance fixture: 2_hosts_wiring.pl:200-243 is oracle-only, its
% final(captured/1, []) is honestly empty, and just conformance is a pure swipl
% run capped at 300s (v6/justfile:39-40). Rewrites the two prose sites that
% currently assert the gap: SYNTAX.md:330, golden-flex.dl6:458-461. Watch
% v6/dl/tasks.d.ts:346, which pins DEFAULT_EXTRACT_BIN at a REMOVED worktree.

% ── ladder B: extract --serve ───────────────────────────────────────────────
task(extract_spawn_decomposition, unbuilt, []).
% MEASURE BEFORE BUILDING. ARCH.pl:710's 87x is end to end and that same row
% names three confounders (git tree at HEAD vs working-tree hash; four families
% vs one scan fact; no org fan-out spelling). Separate (i) process startup, (ii)
% extract's own parse+emit for one warm file, (iii) served-engine per-tick
% overhead, over the crawl-bench corpus. The split decides the ORDER of the rest
% of ladder B: if (i) dominates, extract_serve_loop is the win; if (ii)
% dominates, extract_parallel_dispatch is, and the protocol is only its vehicle.

task(extract_serve_wire,  unbuilt, [extract_spawn_decomposition]).
% NDJSON envelope: ServeLine{Job|Cancel|Shutdown}, DoneLine{id,done,rows,error};
% every fact line gains an `id`, skip_serializing_if none so non-serve output
% stays byte-identical. Buy-check recorded in PLAN.md section B0: lsp-server +
% vscode-jsonrpc, MCP stdio (rmcp + @modelcontextprotocol/sdk), tarpc (excluded,
% the peer is Node) vs bespoke NDJSON (ruled). serde_json is already a dep, so
% Deserializer::from_reader().into_iter() is a zero-new-dep buy for the one
% piece hand-rolling gets wrong (partial-read reassembly).

task(extract_serve_loop,  unbuilt, [extract_serve_wire, ts_query_cli]).
% Reader thread owns stdin, rayon scope runs jobs, ONE Mutex<BufWriter<Stdout>>
% for every emitted line. Sync only (crate charter Cargo.toml:9 "No DB, no
% async"). Rayon pool size from a flag below core count, never rayon's default
% (nothing seizes the machine). catch_unwind per job. Exit on stdin EOF; macOS
% has no PDEATHSIG so EOF plus the parent's kill is the whole orphan story.
% CARRIES A DEP ADDITION the charter hides: Cargo.toml:10 calls the crate
% "rayon-parallel" and rayon is NOT in [dependencies]; grep finds it only in
% that string and two comments. dispatch.rs:14-16 is one file on one thread and
% dispatch.rs:3-5 defers the parallel version to an unlanded lab. So this row
% adds rayon = "1" (precedented, v5 root Cargo.toml:100), the arena-per-worker
% budget, and a real thread-safety review of Strings/FamilyBundle, which no code
% has ever driven from more than one thread. +~60 LOC beyond the estimate above.

task(extract_serve_client, unbuilt, [extract_serve_loop]).
% v6/tsv2/serve/1_hosts.ts + runtime/types.ts (IExtractServeClient, per the
% header-types law). ONE share()d stdout line stream multiplexed by id;
% submit() is cold, filter by id, takeWhile(!done), finalize sends cancel.
% share() also satisfies the backpressure obligation (the parent drains stdout
% independently of stdin writes). THE ONE LINE THAT MOVES THE NUMBER: the inner
% concatMap at :525-527 becomes mergeMap with a NAMED bounded concurrency.
% Process reuse alone changes nothing.

task(extract_serve_life,  unbuilt, [extract_serve_client]).
% Teardown + endurance. WitnessCache claim/settle/clearDeadLocks
% (1_hosts.ts:77-119) is correct for spawn-per-witness; the new hole is an
% orphaned child on engine SIGKILL, and clearDeadLocks deleting ALL pending rows
% (:79) would re-fire a job genuinely in flight in a surviving child. Preferred
% resolution: the child dies with the parent, which makes the generation counter
% unnecessary. Gates: endurance, leak-soak, serve-endurance, serve-leak-soak.

task(extract_serve_bench, unbuilt, [extract_serve_life]).
% Re-run v6/tsv2/scripts/crawl-bench.sh at the same 8-repo pin. NOT a green-all
% leg (ARCH.pl:710 records crawl_bench as deliberately out; the run is 19.15s,
% a named 10-second-law exception in the SCIP-indexing class). Report the new
% files/s beside 40.68 AND the concurrency from extract_serve_client, or the
% result is uninterpretable. Expect well short of v5's 3,540.9: a serve protocol
% touches the process boundary only, not the family-count or working-tree-hash
% gaps CRAWL-BENCH.md names itself.

% ── the two rulings the ladders wait on ─────────────────────────────────────
task(sg_metavariable_ruling, unbuilt, []).
% BLOCKS ts_query_executor. The compiler REFUSES metavariables
% (1_host_expand.pl:419-420, registry.pl:194) while the extractor's ONLY built
% matcher IS metavariables (astgrep.rs:57-129, CLI :124-150, tests/3_...rs).
% ts_query_runner adds a SECOND matcher beside it. Decide: one executor with a
% matcher-kind column, or two executors. Deciding after ts_query_serve_reg means
% changing a wire contract the cache digest already depends on.

task(trivia_plane_ruling, unbuilt, []).
% BLOCKS NEITHER LADDER. Comments already arrive as ordinary CST named nodes
% (v6/sprefa-extract/tests/fixtures/ts/sample.cstf.snap:214, kind "comment",
% byte span). v6 has no separate trivia family; v5 has comment_node
% (src/cst.rs:66-69). A14 = slot_comment_span_trailing_bind
% (plans/2026-07-29-hosts-extraction-verdict.md:447). Must be ruled before the
% FIRST attachment rule ("the comment belonging to this node") and before any
% splice rewrite that must preserve or move a leading comment.

task(cst_surface_spelling, unbuilt, [ts_query_receipt]).
% The section-3 decision packet. Deliberately AFTER the receipt: (c) is live
% today (registry.pl:193 + golden-flex.dl6:463 + 2_hosts_wiring.pl:207-228 +
% dl_view/native_ts_query_term.dl6:4), so the whole ladder can land and be
% measured on the existing spelling before any surface is committed to.
```

Dependency reading of the rows: the two ladders are independent except that
`extract_serve_loop` wants `ts_query_cli` (so the serve loop can carry both job
kinds from the start). B can land first, entirely, on the existing ast-grep
pattern path, and that is the ordering that gets a measurement soonest.

---

## 5. Interactions and what tonight's other arcs feed in

### D1. Trivia / A14

State of fact, verified: comments already arrive on the CST plane as ordinary
named nodes with byte spans (`sample.cstf.snap:214` and three more). There is no
trivia family in `v6/sprefa-extract`; v5 has `comment_node` (`src/cst.rs:66-69`)
as a separate rel. The open slot is `slot_comment_span_trailing_bind`
(`plans/2026-07-29-hosts-extraction-verdict.md:447`).

Where in the ladder it MUST be ruled: nowhere in A, nowhere in B. Neither ladder
reads or writes a trivia rel, and a tree-sitter query that matches `(comment)`
works today against the CST plane because comments are already named nodes.

What proceeds without it: all of ladder A, all of ladder B, and every query that
treats comments as ordinary nodes.

What it blocks: the first attachment rule (any rule of the form "the comment
belonging to this declaration"), the doc-format plane
(`v6/sprefa-extract/tests/6_document_formats.rs` exists as the separate
treatment kimi's arm proposed), and stage-7 splice rewrites that must preserve
or relocate a leading comment. It should be ruled before the first of those, not
before the runner.

### D2. Metavariable semantics

State of fact, verified: `compile_ts_query/2` throws
`unmapped_feature(slot_sg_metavariable_semantics, Term)`
(`1_host_expand.pl:419-420`); `registry.pl:194` marks `sg_pattern/3` `refused`;
`SYNTAX.md:162` and `:331` say the same. Meanwhile the extractor's only working
pattern engine IS metavariable-based ast-grep (`astgrep.rs:57-129`, CLI
`extract.rs:124-150`, test `tests/3_ast_pattern_cli.rs`). The compiler refuses
exactly what the executor does best, and ladder A adds a second, different
matcher beside it.

Where in the ladder it MUST be ruled: **step A3** (`ts_query_executor`). A3
writes the executor name and its column contract. If both matchers are to be
reachable from dl6, either one executor carries a matcher-kind column or there
are two executors with two contracts. Making that call after A4/A5 means
changing a wire contract that the cache digest already depends on, and the
digest is computed in emitted SQL (`digest_expr/6`), so a late change
invalidates every cached witness.

What proceeds without it: A1, A2 (both matcher-specific by construction and
useful on their own), and all of ladder B.
What does not: A3, A4, A5, A6.

### D3. Tonight's other arcs

Lane A's effect_cache finding is confirmed and enlarges: `effect_cache` at
`v6/dl/src/2_schema.ts:83` has five columns and no response-side or disk digest,
AND the table is not the one the dl6 served runtime uses at all, since
`v6/tsv2/serve/1_hosts.ts:63-119` keeps its own `__host_witness(host,
witness_digest, state, response_rows)` instead, so ruling 3's "widen the
effect_cache request cols" names the v6/dl M1-slice table rather than the served
cache; the good outcome is that neither table needs widening, because request
columns already flow into both the identity and witness digests through
`digest_expr/6` (`1_host_expand.pl:562-566`) and adding `query_text` and
`grammar_hash` as declared host inputs extends the digests with zero schema
change, which reduces ruling 3 from a schema arc to the single role decision in
step A5. The type-IR lane's SCIP-identity ruling is **not in this worktree but is
verified in the main tree** (`~/projects/sprefa/chat_log/20260803.0.duel-instant-panel-bridge-bus-typeir-cstlower.md:26`,
untracked at this base, which is why the earlier pass could not find it): "SCIP
symbol strings = IDENTITY column only, scip.proto verified 962 lines, NO
structural payload (Signature.text display-only :236-249); shapes stay prolog
fact terms; join v5 scip index for use sites", restated at `:51` as "SCIP is an
identity graph, not a type IR". It touches the CST fact planes additively and in
the direction both ladders want: CST rows stay byte-addressed `(path, start,
end)` exactly as `astgrep.rs:43-52` already emits them, and a symbol string
becomes an extra identity COLUMN on the planes that have one, never a
replacement for the span, so ruling 4's flat-int span decision
(`plans/2026-08-02-cst-query-rulings.md:188-189`) stands and neither ladder
changes. The one thing to watch is that a rel carrying both a byte span and a
symbol string has two candidate keys, and which one is declared `key(...)`
decides the delta path; that is the type-IR lane's call, not this one's.
