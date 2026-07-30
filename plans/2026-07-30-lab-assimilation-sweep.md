# Lab assimilation sweep, 2026-07-30

Lane `lane/stalelabs`, base `22c0c9f71ca6b16e848c53f8980f4b0c6e3d6ecd`.

The lab protocol says labs die on landing: durable output distils to a permanent
home, the lab directory is deleted, and the plan doc records the commit that
holds the last copy. It was being violated in two places at once. Four labs sat
at repo-root `labs/` untouched since 2026-07-20, ten days and roughly thirty
landed arcs before this sweep, and four more sat in `v6/prolog/labs/` with no
owner or with their fate already named and unexecuted.

Every lab below was read in full and every receipt it ships was RUN at this
base, hermetically (`SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1`, cargo
target dirs redirected to scratch, no daemon touched, nothing under
`~/.local/state` read or written). Nothing here is graded from its README.

**Recovery for every deleted path is `git show 22c0c9f7:<path>`.** All eight
lab directories exist in full at that commit.

---

## 0. The table

| lab | files | receipts at this base | fate | why |
|---|---:|---|---|---|
| `labs/bootstrap-typegen-lab` | 21 | 10/10 cargo tests pass, `check` runs | **FOLD** | every claim answered or superseded; its one honest negative result distilled below |
| `labs/facet-datalog-lab` | 6 | runs, prints all four fact families | **FOLD** | superseded by `compound_storage = struct_as_rows` and by the live `openapi_codegen` lane |
| `labs/swi-typespec-lab` | 12 | 11/11 plunit, LSP smoke green twice | **FOLD** | five of six claims superseded; claim 6's packaging measurement distilled below and is the only survivor |
| `labs/type-system-rust` | 4 | runs, 4 stress cells re-measured | **FOLD** | its type system is refused by two locks; its interning numbers survive and apply to a shipped dependency |
| `v6/prolog/labs/ghcacher_tick_golden` | 7 | `GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1`, exit 0 | **PROMOTE** | not debt at all; a working hermetic oracle-vs-emitter gate that nothing ran |
| `v6/prolog/labs/rel_value_unification` | 12 | 11 green, 4 red, all four reds explained below | **FOLD** | D5 landed, `11_ref_necessity.pl` is 7/7, regression covered by two real tests |
| `v6/prolog/labs/rel_definition_hash` | 1 | 11/11 PASS | **KEEP** | the only executable statement of the three-axis coordinate; Card 5 (rule-generics surface) is still unruled |
| `v6/prolog/labs/generic_scan_instantiation` | 1 | 10 PASS / 1 pre-existing RED | **KEEP** | the only monomorphization prototype against the real compiler; same Card 5 dependency |

Out of scope by instruction and untouched: `json_syntax`, `json_interop`,
`openapi_codegen`, `rel_as_stream`, `labs/teardown-flatten`.

---

## 1. THE USEFUL OUTPUT: claims that are now WRONG or SUPERSEDED

This is the section that matters. Each row is something a reader of these labs
would come away believing, that the project no longer believes, with the thing
that overturned it.

| # | claim as the lab states it | status | overturned by |
|---|---|---|---|
| W1 | there is a type plane distinct from the relation plane (`type User { ... }` declares a type, `rel` declares data) | **WRONG** | `locked(single_rel_type_system)`: one rel model, one checker, no relation-like intermediate type. Held by three of the four root labs simultaneously |
| W2 | types are parametric: `Array<T>`, `Map<K,V>`, `Optional<T>`, `Page<User>`, and a unification variable `T` that later binds to `string` | **WRONG** | `plans/2026-07-28-types-as-rels-verdict.md:76` makes `list(T)` two monomorphic rels (`cons`/`nil`), and its closing line is "Nothing in the right-hand column is new". `type-system-rust/src/main.rs:597` literally runs `unify_var_value` on a type variable; nothing in the shipped design has type variables |
| W3 | a compound value is stored as a nested structure (JSON, or an arena node with child ids) and destructured by a decode step | **SUPERSEDED** | `ruling(compound_storage, struct_as_rows)` 2026-07-29: a struct value is a rel row plus a content-id ref, `decode/2` dissolves into joins, the inline blob is dead |
| W4 | dotted-path enumeration (`orders[*].id`, `metadata{key}`) needs its own recursive path algebra and its own `Path` fact family | **SUPERSEDED** | same ruling. A dot path is a join chain over ref columns, which the types-as-rels verdict already listed as "ordinary body atoms". Both the facet lab and the swi lab built a bespoke path enumerator; neither is reachable from the current design |
| W5 | reflection over Rust types (`Facet`) is a viable front end for the fact spine | **SUPERSEDED, and measured wrong on its own terms** | the lab's own run classifies `Vec<String>`, `Vec<Order>` and `HashMap<String,String>` as kind **`opaque`**, so the collection structure it needs is recovered from a *second*, separate `Def::` walk and the `Type` fact family is uninformative exactly where it matters. Separately, `facet` appears in zero non-lab `Cargo.toml` in the repo |
| W6 | the emitter tier needs a bespoke typed-template/pattern language (`pattern UserPath = \`/users/{id: UserId}\``) with brace and colon slot spellings, composition, and a matcher | **SUPERSEDED** | `v6/prolog/labs/openapi_codegen` is the live successor and does the same job with **zero new syntax**: routes are prolog facts, the spec is emitted from them, Redocly lints it valid, and a dropped route fact turns the parity gate 0/3. `ARCH.pl:224` `capability(codegen_typegen, ..., far_emitters_over_facts)` is that row |
| W7 | one relation can serve both match and render (`pattern_value/3` bidirectional) so the surface needs one implementation | **REFUSED AT THE SCALE THAT MATTERS** | true at toy scale, and the swi lab proves it: `pattern_source_roundtrip` and `..ender_same_relation` both pass. The registry landing then evaluated the bidirectional single-DCG stretch for the real grammar and **declined it** (variable-binding recovery and printer fidelity are not mechanical); `parse_dl.pl` and `print_dl.pl` are two files consulting one table. Anyone citing the lab as evidence for a bidirectional surface is citing a toy |
| W8 | the typed-template compiler self-hosts (plan phase 6: stage-zero and stage-one produce byte-identical artifacts) | **NOT PROVEN, and the lab says so** | `bootstrap-typegen-lab/src/14_bootstrap.rs` writes a boundary report instead: "stage-one self-regeneration stops at the parser/emitter boundary because the parser and emitters are still trusted Rust modules". No `stage1.rs` is emitted. `plans/2026-07-20-typed-template-bootstrap-lab.md` phases 6 and 7 were never executed. The plan doc reads as if self-hosting is in hand; it is not |
| W9 | SWI-Prolog hosting the language server is a route to shipping | **SUPERSEDED** | the LSP milestone landed with ZERO new LSP code: `diag-rail.dl6` declares `diag_v5` in v5's exact 9-column shape and v5's own rust `dl --lsp --diag-db` reads the table (`just lsp-diags`). The eleven-method SWI language server in `6_lsp.pl` is a road not taken |
| W10 | `rel_value_unification` lab 10's two red checks show a broken same-tick settlement | **WRONG, the reds are representational** | probed directly at this base: the oracle now returns `post(obj([id-account,name-bob]))` where the lab expects `post(user(account,bob))`. The *deltas* match the lab's expectation exactly, element for element. The behaviour the checks assert is CORRECT; only the encoding moved, under the D5 canonicalization |
| W11 | interning symbols is a memory win | **TRUE ONLY WHEN NAMES REPEAT, and the lab has the counter-case** | re-measured at this base, 100k declarations x 3 fields: repeated names interned = 9,734,938 peak bytes; the same shape with **unique** names interned = 24,078,380; unintered strings = 23,786,004. With zero repeats interning cost **1.2% more** than plain strings. This one is live, not superseded, and it applies to a shipped dependency (`v6/sprefa-store/Cargo.toml:24` uses `lasso`) |

Two further items are not claims but silent staleness worth the same treatment:

- **W12.** `v6/prolog/labs/ghcacher_tick_golden` has half of itself already in
  the production tree (`v6/tsv2/scripts/4_ghcacher-tick-golden.ts`) while its
  gate ran from nowhere. A green gate that no battery runs is indistinguishable
  from a red one until someone types the command. Fixed by this sweep.
- **W13.** `plans/2026-07-20-typed-template-bootstrap-lab.md` remains checked in
  and reads as a live arc header with seven phases and four open `todo(...)`
  markers. Phases 0 through 5 landed, 6 and 7 never did, and the whole subject
  moved. It is now a historical document and this sweep is its terminator.

---

## 2. `labs/bootstrap-typegen-lab` — FOLD

Standalone Rust binary, zero path dependencies, zero registry dependencies
(`Cargo.lock` is 7 lines). Twenty-one files.

**Claimed.** One binary reads a declaration file with aliases, literal unions,
records, `Array`/`Map`/`Optional`, brace and colon pattern slots, and HTTP and
channel consumer declarations; emits a fixed fact vocabulary; saturates an
embedded rule evaluator; generates compilable Rust and syntactically valid
JavaScript; and reaches a stated bootstrap boundary.

**Ran.** `cargo test` 10/10 pass, `check` prints the normalized declaration
table. Both hermetic, `CARGO_TARGET_DIR` in scratch.

```
test result: ok. 10 passed; 0 failed; 0 ignored
```

**What survives.**

1. The honest negative result W8. The lab was asked to self-host and reported
   that it could not without faking it. That is the most useful thing in the
   directory and it belongs to the v6 primed-queue item "bootstrap story: how
   the language owns its own utilities (swipl-to-C analogy)".
2. The fact vocabulary it converged on, five families, `TypeKind`, `Field`,
   `SlotType`, `Consumer`, `Path` (`src/10_facts.rs`). The swi lab and the facet
   lab independently converged on four of the same five. Three implementations
   agreeing on a vocabulary is weak evidence that the vocabulary is the natural
   one, and it is the only cross-implementation agreement in the set.

**What does not.** The type enum (`src/5_types.rs`: `Array(TypeId)`,
`Map{key,value}`, `Optional(TypeId)`, `Union(Vec<TypeId>)`, `Alias`) is a second
type system, W1 and W2. The `pattern` construct is W6. The embedded evaluator
is a one-pass `Vec` scan with a `contains` dedupe (`src/10_facts.rs:18`), which
is quadratic and is not a fixpoint; nothing about it informs the shipped SQL
fixpoint.

---

## 3. `labs/facet-datalog-lab` — FOLD

Six files, one dependency (`facet` 0.46.5, cached).

**Claimed.** `#[derive(Facet)]` on ordinary Rust types lowers via `SHAPE` into
four relation-shaped fact families, including generic applications like
`Page<User>`, and the resulting graph answers dotted-path queries.

**Ran.** Builds offline and prints exactly what the README promises, including

```
Path { owner: "User", path: "orders[*].id", ty: "String" }
Path { owner: "User", path: "metadata{key}", ty: "String" }
```

**What survives.** The measured limit, which the README does not state: the
`Type` fact for every collection comes back `opaque`.

```
Type { name: "Vec<String>", kind: "opaque" }
Type { name: "HashMap<String, String>", kind: "opaque" }
Type { name: "Option<String>", kind: "enum" }
```

So the collection structure is not in the `Type` family at all; it is recovered
by a second walk over `Def::List` / `Def::Map` / `Def::Option`, and the type
*name* is a rendered string (`"HashMap<String, String>"`) doing duty as a key.
That is W5, and it is the reason this direction would have cost more than it
looked like it cost.

**What does not.** Everything else. `Page<User>` as a first-class generic
application is W2. The `Path` family is W4. The `Fact` enum as an interchange
layer is W3 and W6. The successor is `openapi_codegen`, which generates from
prolog facts rather than from Rust reflection and needs no new dependency.

---

## 4. `labs/swi-typespec-lab` — FOLD

Twelve files. Tests six numbered claims; it is the swi-flavoured twin of the
bootstrap lab and parses a near-identical schema (`schema.soup` vs `schema.dl`).

**Ran.** `swipl -q -s 4_demo.pl` gives 11/11 plunit green in 0.004s total, and
prints the round trip:

```
matched: [id-"alice",kind-"created"]
rendered: users/bob/events/deleted
paths: ["id","metadata{key}","profile.name","tags[*]"]
```

`node 7_lsp_smoke.mjs` green through the interpreter. `swipl -q -s 8_build.pl -g
build -t halt` then produces a saved executable and the smoke test is green
through that too.

**Claim by claim.**

| claim | verdict |
|---|---|
| 1. DCG parses the source into semantic terms | superseded in territory: the shipped DCG is `parse_dl.pl` over `.dl6` and the schema language is W1/W6 |
| 2. a second DCG parses typed delimiter patterns | W6 |
| 3. one `pattern_value/3` matches and renders | W7, true at toy scale, declined at real scale |
| 4. relations validate nested JSON and enumerate typed paths | W3, W4 |
| 5. emits compilable Rust and valid JS | superseded by `openapi_codegen`, which additionally lints valid OpenAPI 3.1 and carries a sabotage receipt |
| 6. SWI can host a stdio LSP and be packaged as a saved executable | LSP half is W9. **Packaging half survives** |

**What survives, and it is the single most durable item in the four root labs.**
The packaging measurement, re-taken at this base:

```
$ swipl -q -s 8_build.pl -g build -t halt
$ ls -l generated/soup-lsp
-rwxr-xr-x  296495  generated/soup-lsp        # 290 KiB, Mach-O 64-bit arm64
$ otool -L generated/soup-lsp
        @rpath/libswipl.10.dylib (compatibility version 10.0.0, current version 10.0.2)
        /usr/lib/libSystem.B.dylib
```

A swipl saved state is small, and it runs: the LSP smoke test passes against the
built image, not only the interpreter. It is **not** self-contained. A
distributable artifact needs a static SWI runtime build, which this lab did not
attempt.

That is live and unanswered, because `ARCH.pl:250` says
`tech(prolog, compiler_tier, ..., 'never runs fixpoints at scale; bundled with
the eventual rust binary')`. "Bundled with the eventual rust binary" is exactly
the question this measurement prices, and nothing since has priced it. Filed
here rather than kept as twelve live files; the reproduction is three commands
against `git show 22c0c9f7:labs/swi-typespec-lab/8_build.pl`.

---

## 5. `labs/type-system-rust` — FOLD

Four files, 630 lines of Rust, five dependencies (`ena`, `la-arena`, `lasso`,
`miette`, `serde_json`), all cached.

**Claimed.** Reconnaissance over the type-system and tooling direction: ena
unification, la-arena storage, lasso interning, nested records/arrays/maps/
optionals/unions/generic application, recursive dotted-path enumeration, and a
measuring global allocator so peak bytes are reported.

**Ran.** Demo prints the type graph, the path lists, and

```
UNIFICATION
T unified with string => ...
```

Four stress cells re-measured in release at this base:

| variant | workload | declarations | fields | interned | peak allocated bytes |
|---|---|---:|---:|---:|---:|
| `arena-lasso` | deep | 100,000 | — | 100,003 | 39,506,993 |
| `flat-lasso` | repeated | 100,000 | 300,000 | 3 | **9,734,938** |
| `flat-lasso` | unique | 100,000 | 300,000 | 300,000 | **24,078,380** |
| `flat-strings` | repeated | 100,000 | 300,000 | 0 (300,000 direct) | **23,786,004** |

**What survives: W11.** Interning is a 2.44x memory win when names repeat and a
1.2% memory *loss* when they do not. The `arena-lasso deep` cell is the other
shape worth keeping: 100k nested declarations reach 200,002 nodes and 39.5MB,
which is 395 bytes per declaration, and the deepest type id is 200,001, so the
representation is linear in depth with no sharing. Both numbers apply directly:
`v6/sprefa-store/Cargo.toml:24` already depends on `lasso`, and the dense-int
mate in the types-round-2 ruling is the same decision.

The measuring global allocator itself (`src/main.rs:12-51`) is a clean, tiny
pattern for peak-bytes measurement when the host blocks RSS inspection. Worth
knowing it exists; it is 40 lines and recoverable.

**What does not.** `TypeNode::Param(Spur)` and `TypeNode::Apply{constructor,
args}` with `ena` unification are W2, and the whole `Decl`/`TypeNode` graph is
W1. There is no type-variable inference anywhere in the shipped design: colon
types are declared, `col_type/3` is authority, and C2a infers only by literal
witness.

---

## 6. `v6/prolog/labs/ghcacher_tick_golden` — PROMOTE, not fold

The brief called this "no owner, pure debt". It is not debt. It is a working
gate and it passes at this base:

```
$ bash v6/prolog/labs/ghcacher_tick_golden/6_gate.sh
GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1
```

What it does: compiles `0_ghcacher_clock_golden.dl6` through the current
prolog-to-TypeScript compiler, replays one hermetic five-tick JSON schedule
through both the prolog oracle and the emitted SQLite runtime, and byte-diffs
tick logs and final relations three ways (expected vs oracle, expected vs
emitted, oracle vs emitted). No `gh`, no shell host, no wall clock, no network.
Tick 5 is the interesting one: a late replacement for a departed witness
replaces the raw `__host_response_fetch` row and produces **no** public delta,
because witness 1 no longer has `poll` membership.

That is a graded receipt for stopping-point program #1 (ghcacher), on the
keyed-clock/etag feedback shape, and nothing ran it. Its driver
`4_ghcacher-tick-golden.ts` already lives in `v6/tsv2/scripts/`, so half of it
had already left the lab and the other half had not.

**Executed here:** the directory moves to `v6/tsv2/goldens/ghcacher_tick_golden/`
and `just ghcacher-golden` joins `green-all`. The move is same-depth, so
`6_gate.sh` needed only the `LAB_DIR` -> `GOLDEN_DIR` rename; `4_oracle.pl` did
need its two `ensure_loaded` paths fixed (`../../compile` ->
`../../../prolog/compile`). Its README, which is a good one, moves with it.

Receipts for the promotion, all taken at this base:

| leg | result |
|---|---|
| direct run from the new home | `GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1`, exit 0 |
| `just ghcacher-golden` | same, exit 0 |
| **sabotage**: one byte of `2_expected.tick.jsonl`, tick 1 -> tick 9 | **exit 1** |
| sabotage reverted | green again, exit 0 |

---

## 7. `v6/prolog/labs/rel_value_unification` — FOLD

Twelve files. `plans/2026-07-30-rel-as-value-lab.md` Card 6 already said fold it:
row identity is finished, rel identity is not. The precondition was that its red
checks be the fail-first receipts for a defect that has since landed. Verified.

**The two checks the card meant are green.** `11_ref_necessity.pl` is 7/7 PASS at
this base, including `typed_variable_forwards_opaque_identity_without_target_
rejoin` and `incremental_target_frontier_rejoins_dense_identity_without_json`.
The D5 fix landed in merge `8d71f543` ("D5 depth-1 join regression. Receipt
standard raised as instructed: the test now asserts every row source BY NAME and
in FROM order per delta arm, not just access method, with a sabotage receipt in
the header").

**The regression is covered by two real tests, not by the lab.**

- `v6/prolog/conformance/fixtures/6_relation_depth.pl`, 11 fixtures, graded by
  the oracle and replayed byte-for-byte by the sweep in both emitter modes. Its
  header carries the verbatim pre-fix red receipts at `68d1ca3f` (176 pass / 10
  fail oracle-side; `total=133 identical=124 wrong=7` emitter-side).
- `v6/tsv2/tests/relationDepth.test.ts`, a plan-level test that asserts every row
  source by name and in FROM order per delta arm, one hop per level, zero SCAN,
  one integer primary-key lookup per level, and no `json_extract` over an
  aliased column, with two sabotage receipts in its header.

Both are stronger than the lab check they replace, which is the whole point of
folding.

**Full receipt run at this base**, so nothing is folded silently:

| file | result |
|---|---|
| `2_receipt.sh` | 2 PASS, exit 0 |
| `3_real_normalize.pl` | exit 0 |
| `5_reference_relation_holes.pl` | exit 0 |
| `7_kernel_host_ref_holes.pl` | exit 0 |
| `8_key_edge_case_census.pl` | 12/12 PASS |
| `9_reference_construction_contexts.pl` | 6 PASS, **2 red** |
| `10_reference_fixpoint_clock.pl` | 4 PASS, **2 red** |
| `11_ref_necessity.pl` | 7/7 PASS |

**All four reds accounted for, none of them a live defect.**

Lab 9's two reds are the compiler refusing on purpose. They throw
`relation_value_in_edge_rule(post/1,...)` and
`relation_pattern_not_a_relation_value(post/1,...)`. Both refusals are
deliberate, both are Cards 1 and 4 of the rel-as-value lab, and both now have
their own fixtures from the same defect wave:
`relation_value_in_edge_rule_rejected.dl6` and
`relation_value_under_negation_rejected.dl6`. The lab expectations are older
than the refusals.

Lab 10's two reds are W10, purely representational. Probed directly:

```
user rows:   [user(account,bob)]
post rows:   [post(obj([id-account,name-bob]))]
post deltas: [[+post(obj([id-account,name-alice]))],
              [-post(obj([id-account,name-alice])),
               +post(obj([id-account,name-bob]))],
              []]
```

The lab asserts exactly that delta sequence, spelled `post(user(account,alice))`.
Since D5 the oracle canonicalizes a rule-constructed relation value to `obj(...)`
instead of leaving it a plain prolog compound, so the text moved and the
semantics did not. Same-tick settlement holds. Keyed replacement retracting the
old parent in the same tick holds.

**One gap this fold hands forward.** `6_relation_depth.pl` contains no `keyed`
or `replace` fixture; grepping it for either returns one comment line. So the
behaviour lab 10 proves, *a keyed target replacement retracting the dependent
parent row in the same tick*, is correct at HEAD and covered by no fixture. The
program and its verified deltas are printed above and are promotable as-is by
whoever next touches the conformance corpus. Not promoted here: a triage lane
adding a fixture moves the sweep counts, and that is somebody's arc, not this
one.

---

## 8. `rel_definition_hash` and `generic_scan_instantiation` — KEEP, reasoning confirmed

Card 6b/6c said keep both until rule generics are decided. That reasoning holds
at HEAD and both are green enough to be worth their disk.

`rel_definition_hash/0_receipts.pl`, **11/11 PASS**:

```
PASS variable alpha-equivalence
PASS systematic relation rename preserves content hashes
PASS declared column rename changes shape hash
PASS source rule order remains in exact-code hash
PASS conjunction order remains in exact-code hash
PASS match hashes after expansion equal handwritten rules
PASS generated host names normalize; template bytes invalidate
PASS recursive SCC hash survives rename and sees edge/body change
PASS layout, program semantics, and stable storage identity separate
PASS 6 calls -> 3 code templates, 6 state/storage instances
PASS renamed programs share abstract lowered SQL, raw SQL stays bound
11 PASS
```

Live claim, one line: it is the only executable statement of the three-axis
coordinate (shape hash, semantic hash, Name/Arity as storage identity), and the
`6 calls -> 3 code templates, 6 state/storage instances` receipt is the
specialization cache key that `locked(higher_order_lowering)` implies but does
not construct.

`generic_scan_instantiation/0_receipts.pl`, **10 PASS / 1 RED**. `go/0` is a
straight conjunction so it halts at the first failure and prints only 2; run
module-qualified per receipt, the real state is:

| receipt | result |
|---|---|
| `receipt_relational_plan` | PASS |
| `receipt_type_and_clock_substitution` | PASS |
| `receipt_arithmetic_registry` | **RED**, pre-existing, recorded as such by the rel-as-value lane |
| `receipt_real_oracle` | PASS |
| `receipt_real_compiler` | PASS |
| `receipt_reuse_and_helper_name` | PASS |
| `receipt_separate_and_shared_state` | PASS |
| `receipt_nested_scan` | PASS |
| `receipt_missing_init` | PASS |
| `receipt_refusals` | PASS |
| `receipt_first_order_composition` | PASS |

Live claim, one line: it is the only prototype that monomorphizes a named rule
argument against the real checker and SQL lowerer (`3 named rels, 1 TEMP pre, 0
helper tables`), which is what `locked(higher_order_lowering)` asks for and no
shipped path implements.

Both stay until Card 5 (whether rule-level generics get a surface at all) is
ruled. `rulings.pl` has no row on it at this base. Small correction to the
rel-as-value lab's receipts index while I am here: it records
`generic_scan_instantiation, 8/9 receipts`; the file has eleven and the real
score is 10/1.

---

## 9. Post-sweep battery

Run on this lane after every deletion and the promotion, hermetic:

| leg | result |
|---|---|
| `just conformance` | 193 PASS / 0 fail, exit 0 |
| `just plunit` | 222/222, exit 0 (one pre-existing choicepoint warning) |
| `just prolog-lint` | `PROLOG_LINT findings=1 baseline=1 OK`, exit 0 |
| `just ghcacher-golden` | `GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1`, exit 0 |

Nothing outside `labs/`, `v6/prolog/labs/`, `v6/tsv2/goldens/`, `v6/justfile`
and this document was touched.

---

## 10. What this sweep did not do

- Did not touch `json_syntax`, `json_interop`, `openapi_codegen`,
  `rel_as_stream`, or `labs/teardown-flatten`, per instruction.
- Did not touch `parse_dl.pl`, `print_dl.pl`, `lower.pl`, `registry.pl`,
  `analyze.pl`, or `v6/sprefa-extract`, all owned by other live lanes.
- Did not promote the lab-10 keyed-replacement program into the conformance
  corpus; see section 7.
- Did not delete `plans/2026-07-20-typed-template-bootstrap-lab.md`. It is the
  header for a folded lab and history is the archive, but a plan doc that reads
  as live when its subject is dead is W13, and it now points here.
- Left `v6/tsv2/labs/` alone. It holds two golden drivers
  (`0_extraction-clock-golden.ts`, `1_rtkq-extraction-golden.ts`), not labs in
  the protocol sense, and their entry scripts are in `v6/tsv2/scripts/`. Worth
  noting that neither is in `green-all` either, which is the same shape of
  problem as W12 and is unowned.
