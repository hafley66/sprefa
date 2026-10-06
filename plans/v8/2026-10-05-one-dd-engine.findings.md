# Findings: one dd engine across phases (stage 1)

Brief: `plans/v8/2026-10-05-one-dd-engine.brief.md`. sprefa `feat/one-dd-engine` from `56a594575`
(main `105532294` plus the brief); sqlite_ivm clone `sqlite_ivm/`, branch `feat/one-dd-engine` from
main `9d928c2`. Counts only, no clock.

## 1. Design

### Today, hello world (`dl8 compile plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7`, dd)

| step | engine | nodes | dd operators | arrangements | settle rows in |
|---|---|---|---|---|---|
| macro library comptime (`compile_units` of `macrotime/*.dl7`, no prelude) | #1 | 1,057 | 3,897 | 334 | 1,197 |
| macrotime cone (`MacroSession::prepare`), base | #2 | 1,069 | 3,936 | 334 | 180 |
| unit enters | #2 | | | | 214 |
| comptime (prelude 115 rules, user 0 rules) | #3 | 2,137 | 6,960 | 366 | 3,101 |

Two observations the design rests on (measured on this tree):

- The macro program `slice_macro_program` cuts from the comptime runtime has the same rule set and
  the same seeds as the one cut from the macro library's own check before comptime (66 rules, 6
  seeds; same set, different order). So the macrotime cone is known before engine #1 runs.
- The prelude checked alone yields 115 rules; the checked rules of hello world (115) and of the
  registry program (146) contain all 115 as the same `TermId`s. So "the stdlib rules" of any program
  are found by term identity.

### Target and stage 1

```text
process:   one engine = [P1 macro library comptime] + [P2 macrotime cone] + [P3 stdlib comptime]
                         installed once, P2 and P3 bootstrapped in one settle, P1 settled by its round
program k: unit rows  -> P2 as signed rows (previous program's last unit retracted in the same settle)
           comptime   -> stdlib rows of program k -> P3 as signed rows (program k-1's retracted)
                         program rules            -> rule interpreter (DL8_RULES=interpret) or a
                                                     compiled engine of the program rules alone
           lower, check: Rust between the two (batch, unchanged in stage 1)
```

Stage 1 is behind `DL8_ONE_ENGINE=1` (unset or `0` = today's path; any other value exits 2).

### Type signatures

```rust
// sqlite_ivm/crates/ivm-ir/src/4_compose.rs  (new, engine-neutral IR capability)
pub struct Composed {
    pub program: Program,
    /// Per part: the part's relation id -> the composed program's relation id.
    pub rels: Vec<BTreeMap<RelId, RelId>>,
}
/// Programs that share nothing but the dictionary, as one program. A constructor relation is one
/// relation per name across parts (same columns, else Err); every other relation is renamed
/// `<prefix><name>` with a fresh id; nodes, texts and strata are concatenated in part order; output 0
/// reads every source, as each part's own output 0 does.
pub fn compose(parts: &[(&str, &Program)]) -> Result<Composed, String>;

// sqlite_ivm/crates/ivm-engine/src/1_rel.rs  (new trait member, engine-neutral)
/// Constructor relations the installed program does not name, by name and argument columns. The
/// engine returns one relation id per entry (an existing constructor of that name keeps its id);
/// `intern_terms` accepts the ids from then on. No operator reads them.
fn declare_constructors(&mut self, ctors: &[(String, Vec<Ty>)]) -> Result<Vec<RelId>, EngineError>;
//   ivm-dd: a worker command; adds name -> id to the dictionary's constructor map.
//   ivm-sqlite: the `ivm_functor`/`ivm_ctor_*` DDL of `terms::install` for the new names; the
//               relation joins the installed program's relation list.

// src/_6_eval/_9d_one_engine.rs  (new)
pub fn init_one_engine_mode() -> Result<bool, String>;   // DL8_ONE_ENGINE, read once like DL8_RULES
pub fn one_engine() -> bool;
/// Lowered programs waiting to be installed beside the next program an attached `IrSession`
/// installs, and the parts that install became.
pub struct Pool { pending: Vec<(String, Lowered)>, parts: Vec<Part> }
pub type SharedPool = Rc<RefCell<Pool>>;
pub struct Part { name: String, session: Slot, lowered: Lowered, applied: Applied }
/// The prelude's comptime rules on their part; `evaluate` runs one comptime round of a program.
pub struct Stdlib {
    rules: Vec<Rule>,                      // as installed, P3's order
    reads: HashSet<(u32, usize)>,          // relation keys a stdlib goal reads
    keys: HashSet<(u32, usize)>,           // reads plus heads: P3's declared sources
    demanded: HashSet<(u32, usize)>,       // stdlib heads the demand rewrite drives
    session: IrSession,                    // adopted P3
    resident: BTreeSet<Row>,               // seed rows P3 holds now
}
impl Stdlib {
    fn evaluate(&mut self, u, program: &Program, answerer, fx,
                rest: &mut dyn FnMut(&mut Universe, &Program, Option<&mut Answerer>) -> Closure)
        -> Closure;
}

// src/_6_eval/_9b_ir_eval.rs (changed)
type Slot = Rc<RefCell<Box<dyn Session>>>;              // a session several IrSessions may hold
struct Core<E: Engine> { engine: E, mapped, decoded, dynamic: BTreeMap<shape, RelId> }
struct EngineSession<E> { core: Rc<RefCell<Core<E>>>, relocation: Option<BTreeMap<RelId,RelId>>, .. }
fn open_composed(u, backend, parts: &[(&str, &Lowered)]) -> Result<Vec<Slot>, Diagnostic>;
impl IrSession {
    pub fn with_pool(self, pool: SharedPool) -> Self;   // install composes; prepare/evaluate adopt
    pub fn with_stdlib(self, stdlib: Rc<RefCell<Stdlib>>) -> Self;  // evaluate splits
    pub fn snapshot(&mut self, u) -> Closure;           // the prepared program's whole closure
}

// src/_8_driver/_7_one.rs  (new): the process context the driver threads through
pub struct OneEngine { pool: SharedPool, macro_program: Option<MacroProgram>,
                       macro_session: Option<MacroSession>, stdlib: Option<Rc<RefCell<Stdlib>>> }
pub fn standard_macro_program_one(u, one: &mut OneEngine, fx) -> Result<(Option<MacroProgram>, Vec<TermId>), Stop>;
// src/lib.rs
pub fn compile_in(u, one: &mut OneEngine, path, fx) -> Result<Compile, Stop>;     // `compile` = fresh OneEngine
pub fn compile_project_in(u, one: &mut OneEngine, root, paths, streams, fx) -> ..;
```

### Instance timelines

| instance | created | read | dropped |
|---|---|---|---|
| `OneEngine` | `compile` (one per call; the binary compiles once per process) or the caller of `compile_in` | macrotime and comptime of every program compiled with it | its owner; holds TermIds, so one per `Universe` |
| composed engine (P1+P2+P3) | first install of the macro library's comptime round, with P2 and P3 pending in the pool | P1 by the macro library rounds, P2 by the macro session, P3 by `Stdlib` | the last `Slot` holder |
| P2, P3 bootstrap rows | one settle right after the composed install | macro session `base`; `Stdlib::resident` | never (the parts keep them) |
| macro session | `standard_macro_program_one`, adopting P2 (no install) | every unit of every program | `OneEngine` drop; an apply failure drops it and the next unit installs a separate engine (logged) |
| `Stdlib` | first comptime of the first program: prepare of P3's program adopts P3 | every comptime round of every program | `OneEngine` drop; an apply failure: later rounds take the whole-program path (logged) |
| program-rules session (`IrSession`, compiled part or interpreter) | per `RoundState`, as today | the program rules of that round loop | with the `RoundState` |
| dynamic constructor | first seed term whose shape the part did not declare | `intern_terms` | the engine |

### Storage and the sequence per program

Storage is the one engine: P1, P2, P3 relations renamed `p1.`/`p2.`/`p3.`; one dictionary
(constructors by name) for all parts; Rust keeps per part `last_seeds`, `answered`, and the shared
`mapped`/`decoded` term caches in `Core`.

```text
init (first program only)
  R1 Rust: macrotime unit -> lower -> check -> slice  => macro cone C, declared sources/shapes
  R2 Rust: prelude unit -> lower -> check -> round_program; seeds = prelude round-1 seeds
           keys = relation keys of stdlib goals and heads
  R3 Rust: lower_declared(C), lower_declared(stdlib, keys)  -> pool.pending
  E1 macro library comptime round 1: lower P1 -> compose(P1, P2, P3) -> ONE install
  E2 one settle: P2 and P3 bootstrap rows
  E3 P1 settle (its round), closure, finish -> runtime -> slice; its rule set equal to C's, else
     the macro session installs C' separately (logged `one engine macro mismatch`)
per program k
  W1 macrotime: unit rows +1 on P2 (program k-1's last unit -1 in the same settle); waves as today
  L  lower + check: Rust, unchanged
  per comptime round r:
  S1 split rules: stdlib (term-equal to P3's) | program rules; program seeds by `keys`
  S2 check: no program rule calls a demand-driven stdlib relation; no program rule interns a
     constructor the stdlib reads back in-round; no served relation in `keys`; a program rule may
     head a relation a stdlib goal reads (loop input) only when no rule on the loop negates or
     aggregates a relation the input reaches; else the whole program on a separate engine
     (logged `one engine fallback`, reason)
  S3 P3 apply: resident -> this round's stdlib seeds plus loop rows as signed rows (new shapes
     declared as dynamic constructors first); P3 snapshot = stdlib closure
  S4 program rules: none -> closure = stdlib closure + other seeds; else the program-rules session
     evaluates {program rules, other seeds + stdlib closure rows of the keys they read or head +
     answers of earlier passes}
  S5 loop: program rows of loop-input relations the stdlib closure lacks join the loop rows; back
     to S3 until none is new (monotone, so the least fixed point; 64 passes, then S2's fallback)
  S6 merge, sorted as `snapshot_closure`
```

Uniqueness: one constructor relation per name in the engine (compose and `declare_constructors`
both key on the name); one cell per `TermId` (`Core.mapped`, shared by every part); a seed row is
in P3 at most once (`resident` is a set; every apply is the exact difference); relation names are
unique per part by prefix.

### Which parts can be rules, which stay Rust over deltas

| piece | stage 1 | could be rules | Rust state it must keep |
|---|---|---|---|
| macro library comptime | rules (P1) | yes | none beyond the session |
| macro claims, rewrite | Rust rewrite (`DL8_MACROTIME=rust`) or the waves LetRec (`engine`) on P2 | waves already are | `Graph` of the unit, `resident` rows |
| lower (`_2_lower`) | Rust, whole unit | partly: graph shape rules; the reader and lexical scopes stay Rust | the unit's lowered basement per unit, to retract an edited unit's rows |
| check (`_3_check`) | Rust, whole program | stratification and binding checks are relation rules; diagnostics as rows | relation declarations per unit |
| comptime stdlib | rules (P3) | yes | `resident` seed rows |
| comptime program rules | interpreter rows or compiled fallback | interpreter where `rule_reason` allows | the interned rule rows |
| frozen edges, requests, answers (round loop) | Rust between rounds | the round loop is a LetRec with a limit over the frozen relations | frozen lists |

### Engine-neutral capabilities

| capability | where | stage 1 |
|---|---|---|
| compose independent programs into one install (`ivm_ir::compose`) | ivm-ir | built |
| constructors declared after install (`Engine::declare_constructors`) | ivm-engine, ivm-dd, ivm-sqlite | built |
| a settle that touches one part steps only that part | ivm-dd (one dataflow, every input advanced per settle); ivm-sqlite (programs walked in order) | not built: counted as dd `steps` |
| extend/retire a fragment (`plans/v8/2026-10-05-engine-extend.design.md`) | ivm-engine | not built; the compiled fallback for program rules uses a second engine until it exists |
| a part-scoped `mark`/`rewind` | ivm-engine | not built; one-engine mode retracts (no rewind) on every engine |

## 2. Stage 1 as built

| piece | where |
|---|---|
| switch `DL8_ONE_ENGINE=0|1` (unset 0, other exits 2), read in `main` beside `DL8_RULES` | `src/_6_eval/_9d_one_engine.rs`, `src/bin/dl8.rs` |
| `ivm_ir::compose` | `sqlite_ivm/crates/ivm-ir/src/4_compose.rs` |
| `Engine::declare_constructors`, dd and sqlite | `sqlite_ivm/crates/ivm-engine/src/1_rel.rs`, `ivm-dd/src/2_dd.rs`, `ivm-sqlite/src/4_engine.rs` |
| `Core` (engine + shared term caches), parts with relocation, per-part `known` term set, `Pool`, adopt-by-IR | `src/_6_eval/_9b_ir_eval.rs` |
| `Stdlib`: split, loop check, signed seed diff, loop passes | `src/_6_eval/_9d_one_engine.rs` |
| process context, macro cone speculation, stdlib program | `src/_8_driver/_7_one.rs`; `compile_in`/`compile_project_in` in `src/lib.rs` |
| round loops get their session from `Sources::ir_session` (`Live.one`) | `src/_4_comptime/_1_api.rs`, `_5_finish.rs`, `_7_sources.rs` |

Differences from the design above, measured:

- A program rule that heads a relation a stdlib goal reads (61 of the 69 census fallbacks before
  this: `:` edges from derived binds and generated rules) became the loop of S5; only a
  non-monotone loop falls back.
- A part never rewinds (sqlite `rewinds()` is false for a part): the macro session retracts on both
  engines in one-engine mode.
- P1's settle stays its own (its round's `run_session` bootstraps it); P2 and P3 bootstrap in one
  settle. Hello world: 4 settles either way.
- The P1 settle with term-map rows of every mapped term sent 2,115 rows; term-map rows are now per
  part (`known`), 1,197 rows, as switch-off.

## 3. Work counts (no clock)

Produced by `cargo test -j 4 --test it _9m_one_engine -- --include-ignored --test-threads=1 --nocapture`
(`tests/it/_9m_one_engine.rs`; `RUST_LOG=ivm_sqlite::work=info,ivm_dd=debug,dl8::ir=info,dl8::one=info`).
"installs" counts `ir install` events; "process engine" is the composed one (`parts=3`); "settles"
counts `ir settle` and `ir apply`; "rows entered" sums their `sent`.

### hello world (`plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7`), stdout identical off/on
| count | ir:dd off | ir:dd on | ir:sqlite off | ir:sqlite on |
|---|---|---|---|---|
| installs | 3 | 1 | 3 | 1 |
| install nodes | 4263 | 4676 | 4263 | 4676 |
| dd operators | 14793 | 15302 | - | - |
| dd arrangements | 1034 | 1034 | - | - |
| dd arranged records | 9600 | 9600 | - | - |
| settles | 4 | 4 | 4 | 4 |
| rows entered | 4692 | 4684 | 4692 | 4684 |
| sqlite creates | - | - | 3308 | 3241 |
| sqlite prepared | - | - | 5349 | 5008 |
| sqlite vm_steps | - | - | 4794111 | 4730833 |
| sqlite written_d | - | - | 18792 | 18112 |
| sqlite written_i | - | - | 11412 | 10954 |
| sqlite written_source | - | - | 2529 | 2524 |

The composed program holds the same three parts, so a single program's CREATEs move 3,308 ->
3,241 and dd operators 14,793 -> 15,302 (the composed sentinel and one dataflow scope). The drop is
in the next program on the same engine (below).

### registry (`dl8 build std/tsi_registry_emit.dl7`), `registry.rs` identical in all four runs
| count | ir:dd off | ir:dd on | ir:sqlite off | ir:sqlite on |
|---|---|---|---|---|
| installs | 3 | 2 | 3 | 2 |
| installs, process engine | 0 | 1 | 0 | 1 |
| installs, other (program rules, compiled fallback: 1,609 nodes) | 3 | 1 | 3 | 1 |
| install nodes | 5766 | 6285 | 5766 | 6285 |
| dd operators | 21603 | 22403 | - | - |
| dd arrangements | 1474 | 1479 | - | - |
| dd arranged records | 115730 | 115045 | - | - |
| settles | 9 | 15 | 9 | 15 |
| rows entered | 37225 | 40647 | 35527 | 40647 |
| sqlite creates | - | - | 4963 | 4775 |
| sqlite prepared | - | - | 11334 | 11105 |
| sqlite vm_steps | - | - | 42207672 | 53687586 |
| sqlite written_d | - | - | 221502 | 283192 |
| sqlite written_i | - | - | 108610 | 169709 |
| sqlite written_source | - | - | 23381 | 22550 |

Registry rounds: 2, each 2 loop passes (31 program rules head `:`; pass 1 adds 6 rows). Each round
starts the loop from no loop rows, so pass 0 of round 2 retracts round 1's 6 loop rows and pass 1
adds them back: the 6 extra settles and the sqlite `written_*` growth.

### two programs on one process engine (`second_program_reuses_the_process_engine`)
First hello world, then `fixtures/literals/3_str_cons.dl7` on the same universe and `OneEngine`;
the second program's compiler rows and runtime program equal its own compile in a fresh universe
(asserted in the child).

| count | ir:dd first | ir:dd second | ir:dd second alone, off | ir:sqlite first | ir:sqlite second | ir:sqlite second alone, off |
|---|---|---|---|---|---|---|
| installs, process engine | 1 | 0 | 0 | 1 | 0 | 0 |
| installs, other (3 program rules) | 0 | 1 | 3 | 0 | 1 | 3 |
| install nodes | 4676 | 134 | 4313 | 4676 | 134 | 4313 |
| dd operators | 15302 | 384 | 14917 | - | - | - |
| dd arrangements | 1034 | 2 | 1036 | - | - | - |
| dd arranged records | 9600 | 6030 | 12010 | - | - | - |
| settles | 4 | 3 | 4 | 4 | 3 | 4 |
| rows entered | 4684 | 1806 | 5827 | 4684 | 1806 | 5827 |
| sqlite creates | - | - | - | 3241 | 122 | 3333 |
| sqlite prepared | - | - | - | 5008 | 759 | 5448 |
| sqlite vm_steps | - | - | - | 4697832 | 2110759 | 5691026 |

Second program: no part bootstraps, no stdlib or macro-library settle of their own seeds; its 3
settles are the unit entering P2 (first unit of hello world retracted in the same settle), the
stdlib seed difference on P3, and the program rules' compiled fallback.

## 4. Parity

| run | result |
|---|---|
| `DL8_ENGINE=ir:dd DL8_ONE_ENGINE=0 cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1` | 182 passed (2 + 180), 0 failed, 5 ignored |
| same, `DL8_ONE_ENGINE=1` | 182 passed, 0 failed |
| `DL8_ENGINE=ir:sqlite`, `DL8_ONE_ENGINE=0` | 182 passed, 0 failed |
| `DL8_ENGINE=ir:sqlite`, `DL8_ONE_ENGINE=1` | 182 passed, 0 failed |
| `_7_book` (not skipped, dd, both switch values) | 12 failed both off and on: trace and output blocks written for the v7 evaluator; not touched |
| registry `cmp` with `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` | identical, dd and sqlite, off and on |
| census `dl8 compile`, 132 programs (`fixtures examples book/src/probes std`), stdout + exit off vs on | dd 132 equal; sqlite 132 equal |
| macrotime oracle `every_macrotime_case_matches_v7` (24 cases) | passes in all four runs; `dl8 expand` builds its own macro program and does not read the switch |
| sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1` (extension dylib rebuilt from the clone) | 265 passed, 0 failed, 14 ignored |

`_9h_build::tsi_registry_sqlite_work_counts` pins switch-off work and now sets `DL8_ONE_ENGINE=0`.

Census fallbacks with the switch on (programs, each 1-2 rounds): 7 `non-monotone loop between
stdlib and program rules` (all at prelude `reader_node 555`), 3 `program rule interns a constructor
the stdlib reads back` (`fixtures/sum/*`). These rounds take the whole-program path on a separate
engine, logged `one engine fallback` with the reason.

## 5. Not built

| item | effect today |
|---|---|
| program rules on the process engine (engine extend, or the interpreter part composed into the install) | each program with rules installs its compiled rules (1,609 nodes for the registry, 134 for `3_str_cons`) |
| loop rows kept across rounds of one program | every round re-derives them from none (registry: 6 extra settles) |
| a settle that steps only the touched part | dd advances every input of the one dataflow per settle |
| lower and check as delta steps | Rust over the whole program per compile |
| the macro library's comptime part (P1) dropped after its one round | it stays installed and idle |
