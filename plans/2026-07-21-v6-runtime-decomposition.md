# v6 decomposition — runtime, calculus, language core, dense identity

Converged design after the cascade lab (E1/E2 + RAM audit + Big-O). Supersedes the
flat "maximal decomposition." feldera is SLIM (calculus only); the language core is
its own richly-nested thing; the runtime is the reactive/async/push-pull engine.

## Framing: sprefa is ReactDOM for code facts
- `.dl` rules = React components: declarative "these facts SHOULD hold given the code."
- Derived relations = the element tree: what should be true.
- `sprefa-runtime` = the reconciler: diff desired vs committed, commit the minimal
  delta. The commit phase IS the retraction cascade (O(delta), proven today).
- store-sqlite = the real DOM: committed facts on disk.
- A file edit / git checkout = props/state change -> source re-extraction (EDB delta).
- The daemon = React's scheduler: an OPTIONAL outer reactor that watches and calls
  `runtime.tick()`. ONE daemon, toggleable. Engine off = one-shot `dl`; on = reactor
  drives ticks. Same runtime.

## "Always running is wrong" = push-pull, decided PER REL at lowering
Today every rel is eager-materialized-and-maintained -> work for rels nobody reads
(the 39x db/corpus bloat). The reactive/lazy/rx story = four strategies:

  strategy              storage           runs when                rel kind
  materialized+push     table, maintained on every input delta     hot derived rels
  view / pull (lazy)    zero (SQL VIEW)   only when read            cold / staging-of-staging
  demand-materialized   table, evictable  first read, then push     memoize hot queries
  clock / push-on-timer table             a tick (@async clock)     ghcacher poll loop

That is WHY clocks/intervals exist: ghcacher's "poll GitHub every 5s" is a
clock-driven push rel. The runtime is the push-pull scheduler (theory:push-pull-dam
+ rx) deciding, per rel, hot-subject (push) vs cold-observable (pull) vs timer.
A rel runs only when something DEMANDS it or a clock/delta WAKES it.

### The reactive model is json-rx (~/projects/hafley-rxjs/packages/json-rx)
json-rx = a JSON-Schema/TypeSpec-defined, SERIALIZABLE RxJS circuit
(sources -> pipe(map{jsonata}/shareReplay/...) -> outputs) that `compileAutomation`
turns into a live graph. sprefa-runtime IS compileAutomation for `.dl`: compile the
declared reactive rules into a running stream graph. Operators map 1:1:
  json-rx node                 RxJS            sprefa
  source (http.event)          Observable      Source/Effect rule (World input)
  map {jsonata}                map()           Extract rule (json/jsonp body)
  shareReplay{buf:1,refCount}  shareReplay     Demand-materialized (memoize while
                                               subscribed, drop when cold)
  reducer                      scan()          @next carry / aggregation
  clock / interval             interval()      Clock(ClockSpec) push rel (ghcacher)
`refCount:true` is precisely "not always running": a rel runs only while subscribed.

### Docker-layer effect cache — allow shell, skip if inputs unchanged
Shell/http effects are ALLOWED (cmd/http rules) but CONTENT-ADDRESSED: hash
everything leading up to the effect (the command text + the bound input facts); if
the digest matches the last run, SKIP execution and reuse the cached output. Same
idea in three costumes you already use: Docker layer cache, RxJS
distinctUntilChanged(inputDigest)+cached-last, v5 recompute-guard digest-skip
(load_rel_digest). First-class on EffectRule (see type sketch).

## Dot access <-> normalization (the repo/rev/file/line "dance" is a special case)
The file/rev/repo chain is NOT a bespoke coordinate system — it is one instance of
NESTED RECORDS + DOT ACCESS in the type system, and datalog is the NORMALIZATION
functor that flattens all such nesting into flat keyed relations. Two dual views:

  type-system view (nested, dotted)         datalog view (normalized, flat)
    call.loc.file.rev.repo        <-lower->   loc(id)->file_id, file(id)->rev_id, ...
    x.field                       <-lower->   a JOIN through the containment relation

- `.` (dot access) COMPILES TO A JOIN through the normalization relation. `x.file.rev`
  is not stored nesting — it is two joins. The dense-id tables ARE the normalized
  form; `.` is the un-normalized surface sugar. Same collapse as (tag,id)->i64, and
  it kills v5's repeated-string leak (nothing carried per-tuple).
- "file/rev/repo always appear together" = total containment / referential integrity:
  every loc resolves through file->rev->repo as a unit; the normalization guarantees
  no orphan, so the triple co-occurs by REFERENCE, never by being copied.
- "nesting must make sense" = WELL-TYPED PROJECTION: the checker proves every `.field`
  is valid on its receiver and the chain resolves. That check is what makes the
  normalization total (the lowered join never dangles).

    fact:          call_site(loc_id, callee_sym)   -- dense ids only, no nesting stored
    loc(loc_id)  -> (file_id, line, col)           -- the normalization / containment tables
    file(file_id)-> (rev_id, path_sym)
    rev(rev_id)  -> (repo_id, sha_sym)
    surface `call.loc.file.rev` == JOIN loc,file,rev by id ; datalog normalizes+flattens.

### Prior art on dot access (build-vs-buy; verified 2026-07-21)
  system    nested data            field access                       storage
  Souffle   records [a,b], ADTs    DESTRUCTURE only: r=[x,y]. NO dot.  records INTERNED to one int
  DDlog     Rust structs           x.field dot + full expr sublang     Rust types
  Datomic   datoms (E,A,V)         navigate attribute = a JOIN         fully normalized
  Flix      records + row poly     r.field dot                         -
  NF2 theory nested relations      projection; nest/unnest             - (proves nesting<->flat lossless)
Two camps: destructure/unify (Souffle: tiny core, no projection typing, but
loc.file.rev.repo = 3 nested destructures) vs dot-as-join (Datomic/DDlog/Flix:
x.field = follow a functional dependency = one join).

DECISION (keep simple, steal from both):
- STORAGE = Souffle record INTERNING (a record -> one dense int, fields in a record
  table). That IS our loc/file/rev dense-id tables. Settled; Souffle is the citation.
- SURFACE = ONE `Proj` (dot) operator, general, NOT functional-only. Its lowering is
  dispatched by the field's KIND so it covers every scenario without excluding any:
    field kind             x.field lowers to              prior art
    functional column      one join (loc.file)            Datomic cardinality-one
    record field           record-table lookup            Souffle interning
    ADT branch accessor    a match/guard                  Souffle $Branch
    relation-valued (many) a join that FANS OUT (set)     Datomic cardinality-many
  New dot scenario = ONE new field-kind lowering rule (generic, never per-field code).
  "Nesting must make sense" stays decidable: the checker resolves the field-kind; no
  kind -> type error. DDlog's full expression sublanguage is still REJECTED (bloat).

## Simplicity proof — new extract/effect = one trait + one registry line, no new built-in
The v5 disease: adding an op ran a 6-step spine (parse->lex->lower->typecheck->engine
->test) editing engine guts, and duplicated coordinate columns. v6 forbids both.

TRICK: extract/effect ops are GENERIC CALLS, not keyword built-ins. The parser has
ONE production `name(inputs...) -> (outputs...)`; `name` binds to a REGISTERED
handler. No `json`/`regex` keyword, no per-op grammar, one engine code path.

    // kernel: ONE extract kind, parameterized by a registry name.
    pub struct ExtractRule { extractor: SymId, args: Args, outputs: Vec<ColId> }
    pub trait Extractor { fn extract(&self, input: &str, args: &Args) -> RowSet; }
    // adding yaml = the ENTIRE diff:
    struct Yaml; impl Extractor for Yaml { fn extract(..) -> RowSet {..} }
    registry.insert("yaml", Box::new(Yaml));            // one line
    // .dl works immediately, no grammar change:
    //   dep(n,v) <- manifest(blob), yaml(blob, "$.dependencies.*", n, v).

    // effects identical; cache/cancel/clock SHARED by the runtime (free for new ones):
    pub struct EffectRule { effect: SymId, inputs: Vec<ColId>, cache: CachePolicy }
    pub trait Effect {
        async fn run(&self, inputs: &FactRow) -> RowSet;
        fn cache_key(&self, inputs: &FactRow) -> Digest;   // keyFn; runtime does the rest
    }
    // adding graphql/grpc/s3 = one impl + one registry line; gets skip-if-same +
    // cancel-stale + clock scheduling unwritten.

THE KERNEL (fixed, nothing re-implements it):
    4 rule kinds · Proj (dot) · Extractor registry · Effect registry ·
    semi-naive fixpoint + retraction cascade · dense-id normalization
Everything else = "implement one trait, register it." A sonnet-level task is
"write this Extractor: &str -> RowSet" — bounded, pure, CANNOT break the fixpoint or
engine (touches only its own leaf). No coordinate columns are ever added to a rel
(facts carry ids; a new coordinate dimension = one table, not per-rel columns), so
"duplicate the piss out of cols" is structurally impossible.

## Crate tree (lean; nesting groups it)
    sprefa-key/          identity: dense loc/file/rev/repo/sym ids, interning, memcap   [lasso, libc]
    sprefa-feldera/      SLIM calculus: ZSet/Weight/Relation/Retract/Fixpoint. denotational only.
    sprefa-lang/         LANGUAGE CORE — one crate, nested modules (they share the AST):
      ::syntax           lexer + parser -> lossless CST            [rowan | chumsky]
      ::ast              typed AST: Rel, Rule{Source|Derived|Extract|Effect}, Body, Term
      ::types            the TYPE SYSTEM / value space: NESTED RECORD types + dot-access
                         (projection) typing, rel schemas, term types, effect types.
                         "nesting must make sense" = well-typed projection (no dangling .field).
      ::resolve          name resolution + variable binding
      ::lower            AST -> plan; NORMALIZES dot access into datalog joins (the
                         flattening functor), + relational algebra + fixpoint + per-rel
                         eval strategy + clock schedule
    sprefa-store/        Store trait: relational tables, VIEW vs MATERIALIZED, bounded load/commit
      store-sqlite/      THE LEASH — impl Store on the cascade (only crate with a sqlite dep)
    sprefa-extract/      source rules: World -> Facts (scan/regex/ast/sg/json/cmd)  [ast-grep, tree-sitter, ignore]
    sprefa-runtime/      reconciler + reactive engine (async/stream/rx); push-pull scheduler
      ::effects          http / cmd / clock execution (async)      [tokio, reqwest]
    sprefa-watch/        change source: fs + GIT (a checkout changes the rev!)   [notify + gix]
    sprefa-cli/          `dl` binary; hosts the optional outer reactor = the ONE daemon

Dep dead-ends (containment is the point): sqlite stops at store-sqlite; ast-grep at
extract; parser at lang; git+notify at watch; tokio at runtime. Pure crates (key,
feldera, lang) hold no runtime state, so they cannot leak.

## Type sketch (types first; bodies are comments; descriptive names)

### sprefa-key — identity (pure value types)
    pub struct LocId(pub u32);   pub struct FileId(pub u32);
    pub struct RevId(pub u32);   pub struct RepoId(pub u32);
    pub struct SymId(pub u32);   // interned string (lasso Spur), = a dl symbol/name
    // resolution tables live in the store; these keys are the only thing on facts.

### sprefa-feldera — the calculus (pure traits, NO algorithm)
    /// Multiplicity in a Z-set: how many derivations support a fact. Add on union,
    /// sub on retraction, live iff > ZERO. i64 now; a source-tracking semiring later.
    pub trait Weight: Copy + Ord {
        const ZERO: Self;
        fn add(self, other: Self) -> Self;
        fn sub(self, other: Self) -> Self;
        fn is_live(self) -> bool;            // self > ZERO
    }

    /// A relation = a map key -> weight (a Z-set). Shape only; no storage choice.
    pub trait Relation { type Key: Ord + Copy; type Weight: Weight; }

    /// The unit of ALL incremental work: signed weight changes.
    pub struct Delta<K, W> { pub changes: Vec<(K, W)> }   // +w add, -w retract

    /// Retraction cascade CONTRACT (the laws we measured): apply a delta, settle to
    /// fixpoint; work O(delta), memory bounded, a fact dies only when its LAST
    /// support hits ZERO. Implemented by the runtime over a Store, not here.
    pub trait Retract: Relation {
        // fn apply(&mut self, delta: &Delta<Self::Key, Self::Weight>);  // contract only
    }

    /// Denotational marker: this rel IS the least fixpoint of its rules. The runtime
    /// computes it semi-naively; feldera only says what it means.
    pub trait Fixpoint: Relation {}

### sprefa-lang::ast — the rule split + eval strategy (pure types)
    pub struct Rel {
        pub name:   SymId,
        pub schema: Schema,          // column types (::types)
        pub eval:   EvalStrategy,    // push-pull decision  <- "not always running"
        pub rules:  Vec<Rule>,       // INVARIANT: all one kind (the DELETE FROM rel bug)
    }

    /// The heart: rules split by WHERE facts come from (EDB vs IDB vs effect).
    pub enum Rule {
        Source(SourceRule),   // EDB: World -> Facts. effectful, reconciled (per-file diff).
        Derived(DerivedRule), // IDB: Rels -> Rels. pure, recursive, retraction-cascaded.
        Extract(ExtractRule), // term-extract: json/jsonp over a bound string (pr_number split).
        Effect(EffectRule),   // async: clock/http/cmd. the ghcacher interval loop.
    }

    /// Effects allowed but content-addressed + demand-cached. Rust mirror of
    /// hafley-rxjs `makeSwitchMapCached` (packages/rxjs-ext), which is the
    /// executable spec — it does three things the runtime needs at once:
    ///   1. keyFn(value) = input digest; a cache HIT skips project() = Docker-layer skip.
    ///   2. share(ReplaySubject(1), resetOnRefCountZero: ttl?timer:true) = demand-
    ///      materialized: memoize last, drop when no subscriber, optional ttl.
    ///   3. switchMap unsubscribes the prior inner = CANCEL the stale in-flight
    ///      effect when inputs change mid-flight (= v5 reqid-midtick cancellation).
    pub struct EffectRule {
        pub kind:   EffectKind,         // Cmd(String) | Http(Req) | Clock(ClockSpec)
        pub key_fn: KeyFn,              // digest(command + bound input facts) = keyFn
        pub cache:  CachePolicy,        // the resetOnRefCountZero / ttl choice
    }
    /// Skip-if-same-digest is inherent (the cache lookup by key_fn). The policy is
    /// the EVICTION, mirroring makeSwitchMapCached's ttl:
    pub enum CachePolicy {
        Demand,          // ttl=0: cached while subscribed, dropped on refCount -> 0
        Ttl(Duration),   // keep ttl after last unsubscribe (timer reset)
        Pin,             // never evict (a materialized effect result)
    }

    pub enum Term {
        Const(Value),          // interned literal
        Col(ColId),            // a head/body column
        Proj(Box<Term>, SymId),// DOT ACCESS: term.field. ONE operator; lowering dispatched
                               // by the field's KIND (functional -> one join; record ->
                               // interned lookup; ADT branch -> match; many -> fan-out join).
                               // General, excludes no scenario; new kind = one lowering rule.
        Var(VarId),            // logic variable — bound by unification (reserved for Prolog)
        // future: Compound(SymId, Vec<Term>) for Prolog functor terms
    }

    /// The push-pull strategy, chosen by ::lower, rendered by store-sqlite.
    pub enum EvalStrategy {
        Materialized,        // table, push-maintained on every input delta
        View,                // SQL VIEW, zero storage, pull/lazy on read
        Demand,              // materialize on first read, maintain, evict under pressure
        Clock(ClockSpec),    // push on a timer: @async clock(5, _)  -> ghcacher
    }

### sprefa-store — the seam sqlite hides behind (trait + one impl)
    pub trait Store {
        type Key: Ord + Copy;  type Weight: Weight;
        /// pull ONLY the wavefront (bounded — the proven property).
        fn load_frontier(&self, seeds: &[Self::Key])
            -> impl Iterator<Item = (Self::Key, Self::Weight)>;
        /// commit a cascade delta in one bounded transaction.
        fn commit(&mut self, rel: SymId, delta: &Delta<Self::Key, Self::Weight>) -> Result<()>;
        /// declare a rel's physical form; EvalStrategy lowers to this.
        fn declare(&mut self, rel: SymId, form: PhysForm) -> Result<()>;   // Table | View
    }

### sprefa-runtime — the reconciler (async, push-pull)
    pub enum Change { FileChanged(FileId), RevChanged(RevId), ClockFired(SymId) }

    /// One reconcile tick (ReactDOM commit phase): given input changes, run the
    /// affected source rules (reconcile) + derived rules (semi-naive to fixpoint),
    /// commit deltas for MATERIALIZED rels, skip VIEW rels, wake fired CLOCK rels.
    pub trait Runtime {
        async fn tick(&mut self, changes: Vec<Change>) -> Result<TickReport>;
    }

## Why four rule types — adversarial defense (rationale for the typed split)
For each kind the strongest adversary tries to COLLAPSE it into another. The defense
is the invariant only that kind holds; three of the four collapses are documented v5
data-loss / non-termination bugs, not hypotheticals. This is the case for encoding
Rule{Source|Derived|Extract|Effect} at the type level.

Discriminators are pairwise-disjoint (no two kinds share a row):
  rule      facts from        pure?          in fixpoint  async/clock  maintenance
  Source    World (files)     effectful read no           no           reconcile (per-file diff)
  Derived   DB rels           pure           YES          no           retraction cascade
  Extract   DB (bound string) pure generator no           no           extract-phase fill
  Effect    DB (bound facts)  IMPURE         no           YES          cached + cancelled + clocked

- SOURCE. Attack: "the FS is a relation file_bytes(path,content); a source rule is a
  Derived rule over it." Defense: source facts are present-iff-file-contains, weight
  in {0,1} (mirror-of-file, NOT support-count); maintenance is reconcile (per-file
  diff), trigger is file/rev change, and it is effectful + time-varying (impure).
  Failure if collapsed: rebuild_derived's `DELETE FROM rel` wipes the reconciled
  source rows (the v5 bug); lose the reconcile diff -> full rescan or stale facts.

- DERIVED. Attack: "the only real rule; infer it from 'reads a rel'." Defense: the
  ONLY kind that is recursive (needs semi-naive + stratification), pure (hence
  view-able / demand-materializable), and home to Z-set multiplicity (a fact with
  two derivations survives losing one; weight 2->1 = the retraction math). Failure
  if collapsed: putting effects "in" the fixpoint = non-terminating/non-det loop
  (a fixpoint needs monotone pure functions). Purity is the whole engine's precond.

- EXTRACT (most collapsible). Attack: "json is a body unnest; it's a Derived rule
  with a json body-item, no third kind." Defense: the separator is EVALUATION PHASE,
  not purity. Extract explodes a bound DB string (1 row -> N, data-dependent
  cardinality = a generator, not a join); eval_extract_rules fills the rows, then
  rebuild_derived runs after and would DELETE them if the same rel were headed by
  both; it also cannot feed a @next carry directly (rebuild ordering wipes it) ->
  the pr_number->change_log split in gh-cache.dl. Failure if collapsed: into Derived
  -> DELETE wipes extract rows (guarded twin of the Source bug); into Source -> no
  file to reconcile against. Mark it so the engine never rebuild_derived's its rows.

- EFFECT. Attack: "not a rule; put shell/http in imperative glue, or it's a Source."
  Defense: produces facts like Source BUT input is the DB (bound facts fill the
  url/command) so reconcile-by-file does not apply; impure, expensive, async,
  time-driven -> needs content-addressed skip (makeSwitchMapCached keyFn) +
  cancel-stale (switchMap = reqid-midtick) + clock scheduling that no other kind has.
  The TYPE IS THE IMPURITY FIREWALL: effects are quarantined from the fixpoint, run
  in a separate async phase, outputs re-enter as a delta on the NEXT tick (stratified
  strictly after the pure fixpoint). Failure if collapsed: into Derived ->
  impurity-in-fixpoint = the "always running the shell" non-termination; into Source
  -> sync/async mismatch, no reconcile key, loses the cache/cancel/clock triplet.

Each kind is the sole holder of one invariant: Source = reconcile-vs-file,
Derived = recursion + multiplicity, Extract = extract-phase head-ownership,
Effect = async/impurity quarantine. Collapse any pair -> lose that invariant.

## Instance lifetimes / who holds state
- STATELESS (pure values, no lifetime concerns): everything in feldera, lang, key.
  Rel/Rule/Term/Delta are owned values passed around; the calculus holds nothing.
- STATEFUL (the only things that own resources): the `Store` impl (store-sqlite owns
  the connection + the cascade tables) and the `Runtime` impl (owns the plan + the
  push-pull schedule + async effect handles). Both live for the daemon's lifetime;
  a one-shot `dl` builds them, ticks once, drops them.
- The bounded-memory invariant is a Store trait law, tested by store-sqlite (proven:
  54MB C-heap, wavefront-bounded). The calculus can't leak: it holds no state.

## Invariant -> enforcement
- keep sqlite in its place  -> only store-sqlite depends on it (name = leash; `cargo tree -i` proves)
- no leaks this time        -> bounded working set is a Store law; feldera/lang/key are stateless
- one library per invariant -> each heavy lib is one leaf: swap parser=lang, backend=store-* sibling
- Prolog one day            -> Term::Var reserved; future sprefa-unify + a runtime goal-directed mode
- git checkouts are not fs  -> sprefa-watch = notify + gix, emits RevChanged, not just mtimes

## Open calls
- type-intensity: which invariants earn compile-time encoding. Leading: the
  Rule{Source|Derived|...} split (EDB/IDB = the DELETE FROM rel data-loss bug).
- who drives the build-out (me types-first / codex worktree / kimi3).
- deferred experiment: graspan CFL-reachability (multi-way join + union + recursion)
  to stress the cascade under sprefa's real shape.
