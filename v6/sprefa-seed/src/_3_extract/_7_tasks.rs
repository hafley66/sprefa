//! THE PARITY SURFACE + CURRENT MIND: extraction contract as a living ledger.
//!
//! Mirrors `v6/sprefa-store/src/tasks.rs`: traits are the contract, the `Tasks`
//! impl bodies are `todo!()`, the DOCS are the task notes, `ExtractPlan` is the
//! proof-token epic ledger. Read the plan off this file.
//!
//! ════════════════════════════════════════════════════════════════════════
//! CURRENT MIND: session 2026-07-23. What this crate IS, after the rulings.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Job (one leaf below the store): a corpus at a version -> graph facts. Pure CPU
//! + rayon, arena-mastered. NO database, NO reactivity, NO async facade.
//! Reactivity, the async-eval flip, and the sprefa-language are other crates /
//! another session.
//!
//! Scope (this layer owns all four):
//!   blob source  file bytes in, content-hashed. SOURCE-AGNOSTIC contract
//!                (`BlobSource`); git is ONE impl, not the only shape. A corpus
//!                may be a plain directory, a tarball, etc. with no revs. git-fu
//!                (the shellout path: `cat-file --batch` bulk, `cat-file -e`
//!                existence, `rev-parse`) is the `GitShellout` impl; the v5 lab
//!                found shellout usually beats libgit2 on big histories (linux-
//!                kernel-class revs back in time). v5: `engine/repo.rs:1169`
//!                `read(rev,path)`, `:1152` cat-file --batch, `:108` rev_parse,
//!                `:123` cat-file -e; `engine/revid.rs` Rev identity + the
//!                worktree `+` suffix + `GitOid`. Non-git corpora use a plain
//!                `Filesystem` impl (path -> bytes; "version" = now/mtime).
//!   extraction   syntax/semantics families, per-language, rayon, arena-per-file.
//!   scip         Tier-1 resolution source, BIDIRECTIONAL wire + ratchet (D-scip-wire).
//!   tree-iter    tree-sitter integration points (the floor + the CstF family).
//!
//! Identity (content-addressed; holds for git AND non-git corpora):
//!   project = a corpus root (a git worktree OR a plain directory OR ...).
//!   file    = path + content hash (BlobHash).  version is source-specific (a git
//!             rev, or filesystem "now"): it is how bytes are FOUND, never the
//!             cache key. The cache keys on content:
//!   phase-1 key  (BlobHash, lang, Mask):            same bytes anywhere -> one extract.
//!   phase-2 key  (BlobHash, ProjectDigest, Mask):   corpus state changes -> re-resolve.
//!
//! Decisions landed this session (type math is the spec; code stubs catch up):
//!   D-families   families are TYPE-LEVEL: `Family` trait + marker structs, not a
//!                `NodeKind`/`EdgeKind` sum. `Node<F>`/`Edge<F>`; the sums DELETE.
//!                (Orthogonal axes are not variants of one type; the store splits by
//!                family anyway, so flatten-then-resplit is wasted motion. v5 + the
//!                bundles already had per-family types; the sum was the false unification.)
//!   D-planes     3 planes now: RESOLUTION (Type|Call|Module, SCIP-wire, ratchet-
//!                able) + VALUE-FLOW (Df|Flow, native, AST-only, the differentiator)
//!                + STRUCTURE (Cst, the lossless tree-sitter named-node tree).
//!   D-module     Module collapses: resolution half -> SCIP namespace edges (a file
//!                IS a namespace; SCIP's symbol scheme already nests modules);
//!                binding half -> aux side metadata. Not a standalone resolution family.
//!   D-scip-wire  SCIP is a BIDIRECTIONAL wire: `ScipOccurrence <-> Node<F>` both
//!                ways. Our AST facts project OUT (joinable, ratchet-eligible);
//!                foreign indexers project IN. Round-trippable for the 3 resolution
//!                families ONLY (df/flow/cst/binding have no SCIP shape).
//!   D-ratchet    `merge` = per-fact best-producer-wins over N producers (`Ast`,
//!                `Scip(&indexer)`, `Ghcacher`). The `Producer` tag rides the bundle.
//!   D-sync-only  NO async facade. `_6_facade` (`ReactiveExtract`/`ProjectView`) is
//!                CUT. Pure CPU + rayon; nothing awaits. The engine wraps our sync
//!                `dispatch` in ITS spawn_blocking if async. SCIP build =
//!                `std::process::Command`. (tokio available/safe but unreached here.)
//!   D-port-clean port + clean v5's roster (syn/oxc/tree-sitter) as-is; no buy-vs-buy gate.
//!   D-concrete   concrete structs until a second impl (crate-map practicality ruling).
//!   D-arrow-type functions/methods STAY in `TypeF`: a function IS a type
//!                (`[A] => B`); the TypeF entity carries the arrow signature and
//!                owns the `param`/`returns`/`uses` type-edges. v5 test
//!                `ts_entities_kinds_lines_and_arrow_types` (v5 `typegraph/ts/
//!                mod.rs:1275`, line 1304 "a function IS a type") locks this.
//!                TypeF = the TYPE facet; CallF (commit 3) = the CALL facet
//!                (def/sites/resolution). Two orthogonal projections of one
//!                declaration, NOT false unification. (An earlier "trim
//!                TypeEntityKind to pure types" suggestion was RETRACTED on this
//!                evidence.) scip-typescript's divergences (arrow-const `sub`,
//!                nested method `magnitude`, value-const `origin`) are modeling
//!                differences: v5 models callable arrow-types in the type graph
//!                and excludes value-consts from TypeF (they are df value nodes).
//!                REPRESENTATION (locked 2026-07-23 commit 2c): the arrow sig is a
//!                SIGS SIDE TABLE (`TypeSig{owner:Span, slot:SigSlot, pos, ty:NameId}`
//!                in Family::Aux), NOT span-pair edges. Rationale: a type REFERENCE
//!                is not a declared entity (would pollute TypeEntityKind, which is
//!                declaration kinds); its target is a bare name unresolved in phase 1
//!                (a span edge would lie about a binding we have not made); and pos
//!                must survive for the node-level type join. The Param/Returns EDGES
//!                (span-to-span, resolved) still land at Resolve<TypeF> (commit 4).
//!
//! BUILD STATUS (2026-07-24, commits 1-3c + Tier-2 PARITY GOLD (TS, incl. const facet) + the
//! RUST + GO languages + a self-describing CLI + the commit-4a Resolve DESIGN FREEZE) LANDED in
//! v6/sprefa-extract/. The TS + Rust + Go phase-1 families - Cst / Type(+sigs) / Call / Df
//! (+const) - all project + stream + snapshot through ONE uniform surface, AND each ported set
//! matches a captured v5 oracle (see the (parity) / (rust) / (go) entries). NEXT: HUMAN REVIEW
//! of the 4a Resolve<F> surface, then 4b Resolve<TypeF> for TsSource; broaden parity fixtures
//! (lambda/docs):
//!   6a29d920  commit 1   CstF via ast-grep (one dep = rust/ts/tsx/js/go grammars);
//!              clap bin streaming flat JSONL with --bench; snapshot. Piping proof.
//!   f3ceb4fa  commit 2a  Parser/Project seam -> arena-passing GAT: oxc's
//!              Program<'a> borrows its Allocator, so parse takes a caller-owned
//!              Arena + content:'a; the dispatch holds the arena across parse+project.
//!   4fbf9a68  commit 2b  OxcParser + Project<TypeF>: ports v5 ts_entities_from
//!              (class/interface/alias/enum/function/method entities; ctor +
//!              non-fn const skipped). oxc race in --bench. cargo tree clean of
//!              tokio/sqlx/sea-orm/rusqlite/axum; public API names no store-id type.
//!   (2c code)  commit 2c  D-arrow-type PAYLOAD: the callable arrow signature is
//!              now emitted as `TypeSig` rows in the TypeF aux (one per named
//!              type ref in a param slot / the return slot; port of v5
//!              ts_fn_signature_edges' param/returns half). Representation
//!              decision: a SIGS SIDE TABLE, NOT span-pair edges (the target is
//!              a bare NameId, unresolved in phase 1; Resolve<TypeF> binds it at
//!              commit 4). Keyword types (number) emit no sig; a union slot
//!              emits one per arm. param POSITION preserved (for the node-level
//!              type join). Added Family::Aux + FlatFact::Sig; CstF::Aux=().
//!   (3a code)  commit 3a  CallF: callable DEF nodes (CallKind{Free,Method}; ports
//!              v5 ts_call_defs_from incl nested named-fn defs + the class ctor
//!              whose call-name is the class name so `new Foo()` resolves) + call
//!              SITES in the CallF aux (CallSite{span, callee, callee_path}; ports
//!              v5 TsCallSites: CallExpression + NewExpression + JSXElement
//!              component). Sites unresolved in phase 1 (callee as written); caller
//!              is span-containment at the seam. Lambda defs (df lift) -> DfF. Added
//!              CallF/CallKind/CallEdgeKind/CallSite/CallFAux + FlatFact::Site +
//!              dispatch_call. Def span MATCHES the TypeF entity span (two facets
//!              join on one coordinate).
//!   (3b code)  commit 3b  DfF: the intra-procedural value-flow graph. Ports v5
//!              ts_dataflow_from (ts/flow.rs): every value-bearing position -> a
//!              NODE (param/let_bind/var_read/lit/call_res/new/member/ret/binop/
//!              concat/template/cond/logic/closure/expr), local value flow -> a
//!              Direct EDGE. Full DfNodeKind(23) + DfEdgeKind{Direct}. DROPPED vs
//!              v5 (deliberate deferrals): fn_sym/mint_sym/lambda_sym (the
//!              enclosing callable is DERIVED at the seam by span-containment
//!              over CallF defs, like the CallF site caller; NOT stored on the
//!              node — keeps the uniform Node<F> shape); line_at/line_index; the
//!              enrichment aux (args positional slots / fields names / lits texts
//!              / param_pos / loops / nests) — the EDGES already carry every
//!              value flow; JSX element/fragment flow (catch-all covers it). The
//!              transient scope HashMap (var name -> NodeRef) for intra-procedural
//!              resolution is kept. Added DfF/DfNodeKind/DfEdgeKind + flatten_df +
//!              dispatch_df. df_reaches walks this on the same fixpoint.
//!   (3c code)  commit 3c  Epic U LANDED: the uniform surface v5 had. New
//!              `source.rs` (`Source` trait + `FamilyMask` + `ExtractOutput`),
//!              `TsSource` (cst via ast-grep + type/call/df via ONE oxc parse, one
//!              shared `Strings`) + `AstgrepSource` (cst-only fallback) behind a
//!              first-match `sources()`/`source_for()` roster (`lang/mod.rs`).
//!              Collapsed: 4 `dispatch_*` -> one `dispatch(path,content,mask) ->
//!              Option<ExtractOutput>`; 8 `flatten_*` -> one `flatten` +
//!              `flatten_jsonl` (per-family flatteners demoted to private helpers);
//!              the hand-coded bin -> `dispatch`+`flatten`+`--family`; 4 hand tests
//!              -> ONE loop-driven `ts_uniform_surface` (+ a roster test).
//!              `lang/oxc.rs` renamed `lang/ts.rs`. Done condition held: 4 TS
//!              snapshots byte-identical (no `UPDATE_SNAP`), `pub fn dispatch`=1,
//!              bin names no ast-grep/oxc type outside `Source` impls, `cargo tree`
//!              still clean. The two-parser reality + `Resolve<F>` extending
//!              `Source` are Epic U's frontier (see the plan).
//!   (parity)  Tier-2 PARITY GOLD LANDED (TS, ported facets). Captured v5 oracle:
//!              `examples/v5_normalize.rs` (v5 root crate) runs
//!              `TsTypes.extract/extract_calls/extract_dataflow` IN-PROCESS (no DB,
//!              no repo - TypeLang takes just (file,content)) + emits canonical
//!              sorted lines -> committed `tests/fixtures/ts/sample.v5.jsonl` ->
//!              `tests/golden_parity.rs` twin-normalizes v6's flatten + diffs. v6's
//!              workspace is deliberately isolated (no v5 in its build graph), so
//!              the oracle is CAPTURED, never linked. RESULT: PORTED facets
//!              (type_node/type_sig/call_def/call_site/df_node/df_edge) match v5 -
//!              byte-exact for df (v5 line+byte-col round-trips to v6 Span.start),
//!              line-exact for type/call (v5 drops the byte offset there). ONE
//!              divergence found + fixed: CallKind::Free tag "free"->"function"
//!              (v5 call_def.kind; v6 had coined "free" with no rationale).
//!              DEFERRED v5-only (measured, destinations logged): type_edge
//!              (Resolve<TypeF> commit 4), df aux (args/fields/lits/param_pos;
//!              labels-not-graph), docs (follow-up). const is now PORTED (see the
//!              (const) entry). v6-ONLY: cst (v5 has NO TS tree-sitter grammar ->
//!              incomparable). Fixtures: sample.ts (graph shapes) + consts.ts
//!              (the full const matrix: string/template/object-dotted-path/
//!              string-enum/as-const/mutable+numeric-skip/nested-scope).
//!   (const)   Const facet PORTED (v5 model restored). A string-bearing const/
//!              `as const` binding -> a Const TypeEntity (Node<TypeF> kind=Const)
//!              + one ConstValue per resolved string (lit cooked / template raw
//!              slice; object literals -> dotted field paths; string-enum members
//!              key off the enum entity). Non-string consts emit nothing (both v5
//!              and v6). Port of v5 ts_const_facts_from (`lang/ts.rs` ConstWalker)
//!              driven from TsSource::extract (needs source bytes for template
//!              slices); `family.rs` ConstValue/ConstKind + TypeFAux.consts;
//!              `wire.rs` FlatFact::Const. The D-arrow-type "consts stay df"
//!              reading had dropped the string-const entity + values v5 kept - a
//!              FALSE DICHOTOMY (a const is a declaration AND a value AND carries
//!              its text; the df let_bind node is separate + unaffected). v6 drops
//!              v5's scope/sym machinery (spans disambiguate). PARITY GREEN on
//!              consts.ts (5 Const entities + 11 const_value rows, byte-match).
//!   (rust)   SECOND LANGUAGE LANDED: `RustSource` (lang/rust.rs, ~1170 lines),
//!             PREPENDED in the roster so .rs routes to it (not the cst-only
//!             AstgrepSource). Mirror of TsSource: cst via ast-grep (ast-grep's
//!             rust grammar) + type/call/df/const via ONE `syn::parse_file`.
//!             Ports v5 `src/graph/typegraph/rust/mod.rs`: TypeF entities
//!             (struct/enum/trait/fn/impl/const) + arrow sigs + const facet,
//!             CallF (defs incl. nested-fn/closure visitor + sites), DfF (nodes
//!             + Direct edges). syn yields line/col, not byte offsets: a
//!             rust-local `line_col_to_byte` over precomputed `line_starts`
//!             bridges to v6's byte `Span` (NO shape.rs change; the freeze held).
//!             TIER-2 PARITY GREEN vs the v5 oracle (rust arm in v5_normalize.rs):
//!             type/call/const LINE-exact, df BYTE-exact (25 nodes, 19 edges).
//!             ONE documented + SELF-VERIFYING waiver - the closure df-node NAME
//!             (v5 stored the `lam_sym` join key; v6 drops it, span-containment
//!             joins instead - same call the TS DfF port makes). The waiver's test
//!             asserts every line it touches is a `df_node closure` row, so a real
//!             regression cannot hide behind it. DEFERRED (same set as TS):
//!             type_edge (Resolve<TypeF> commit 4), docs, df aux. Commits
//!             de94cceb (skeleton) / 9529b358 (TypeF+const) / 380ea1d5 (CallF) /
//!             7a37922d (DfF) / d28f43f8 (parity gold). Proves the seams generalize
//!             to a second front-end with zero structural change.
//!   (cli)     The `extract` bin is now SELF-DESCIBING: `--help` long_about
//!             covers the output shape + the first-match language matrix + exit
//!             codes; `--schema` prints the full JSONL contract (5 record shapes,
//!             every field, per-family kind vocabularies, phase-1 limits);
//!             `--version` works; PATH is conditionally required so `extract
//!             --schema` runs standalone. Commit a08ce4b5. `cargo install --path
//!             v6/sprefa-extract --features cli --locked` puts `extract` on PATH.
//!   (go)     THIRD LANGUAGE LANDED: `GoSource` (lang/go.rs, ~1120 lines), PREPENDED in the
//!             roster so .go routes to it. Mirror of TsSource/RustSource: cst via ast-grep
//!             (ast-grep's go grammar) + type/call/df via tree-sitter-go (`go_parse` ->
//!             `tree_sitter::Tree`, the "floor as the only tier" - no oxc/syn analog for go).
//!             Ports v5 `src/graph/typegraph/go.rs` (GoTypes): TypeF entities + arrow sigs,
//!             CallF (defs + sites), DfF (nodes + Direct edges). tree-sitter yields BYTE
//!             offsets directly (node.start_byte/end_byte) -> v6 Span with NO line/col bridge
//!             (simpler than the syn port). TIER-2 PARITY GREEN vs the v5 oracle (go arm in
//!             v5_normalize.rs): ZERO divergence (type/call line-exact, df byte-exact). v5 go
//!             emits NO const facet (walk_go_entities skips const_declaration); v6 matches
//!             (const_value=0 both sides). DEFERRED (same set as TS/Rust): type_edge
//!             (Resolve<TypeF> commit 4), docs, df aux. Commits 8abdc38e (skeleton) /
//!             aa3c782e (TypeF) / aab204d1 (CallF) / d6427eab (DfF) / 16bc0855 (parity gold).
//!             tree-sitter + tree-sitter-go unify with ast-grep's transitives (one copy each).
//!   (4a)     COMMIT 4a: hollow Resolve<F> surface LANDED - a DESIGN FREEZE (human review
//!             gates 4b). types.rs gains, per the _2_traits.rs spec: `ProjectCx` (files /
//!             manifests / reader / digest / indexes - every field a hollow declaration with
//!             a per-field spec citation, `_2_traits.rs`:35-51, plus the `_1_mask.rs`:78-82
//!             ProjectDigest atom), `ProjectEdge<F>` (the seed `_0_shape.rs`:227-232 row made
//!             generic - the EdgeKind sum is deleted per D-families, so kind is F::EdgeKind),
//!             and `Resolve<F>: Source` with
//!             `resolve(&ExtractOutput, &ProjectCx) -> Vec<ProjectEdge<F>>` (default body
//!             todo!(); NO impls, NO call sites, ZERO behavioral change - gate green, the 4
//!             snaps byte-identical, no UPDATE_SNAP). The whole ExtractOutput is the input
//!             (not a bare FamilyBundle) because resolution joins on names and the interner
//!             lives on ExtractOutput.strings; no FamilyMask param (F is a type param here,
//!             so the family is already selected per impl). CACHE KEY doc: phase 2 =
//!             (BlobHash, ProjectDigest, FamilyMask) vs phase 1 = (BlobHash, lang,
//!             FamilyMask). WHICH FAMILIES: TypeF + CallF only; ModuleF is still commented
//!             out (types.rs S2, "PENDING - collapsed"), so NO ModuleF resolve surface is
//!             declared - the Resolve trait doc carries the module placeholder; DfF/CstF
//!             NEVER resolve (_2_traits.rs:80-84). DESIGN ANSWERS (these gate 4b):
//!             (a) WIRE SHAPE = extend FlatFact, ONE new project-edge arm - NOT per-family
//!             TypeEdge/CallEdge arms, NOT a side channel. The existing Edge arm
//!             (types.rs:755-760) cannot carry a cross-file dst: both endpoints are bare
//!             SpanOuts and the flatteners resolve NodeRef through the PRODUCING file's own
//!             bundle (wire.rs:68-79, 160-169), while a ProjectEdge dst is (dst_blob,
//!             dst_span) in ANOTHER blob. A side channel contradicts the one-flatten-three-
//!             consumers ruling (stdout JSONL / store seam / parity golden; wire.rs:1-4) -
//!             all three would have to re-join it. `kind` stays a String (as in every arm),
//!             so TypeEdgeKind + CallEdgeKind ride ONE arm: {record, family, kind, from,
//!             to_blob, to} (arm + field names TBD in 4b; wire.rs IS in 4b's allowlist
//!             "only if 4a's wire answer requires it" - it does).
//!             (b) SNAPSHOT GROWTH = DECLARED NONE for 4b; UPDATE_SNAP stays FORBIDDEN. The
//!             4 committed snaps come from flatten_jsonl(dispatch(path, bytes, mask))
//!             (snapshot.rs:44-67) and flatten reads only the phase-1 bundles
//!             (wire.rs:27-42); dispatch.rs is NOT in 4b's allowlist, so dispatch stays
//!             phase-1-only and no project_edge rows reach flatten_jsonl. The type_edge
//!             facet flips ASSERTED-IN-TEST instead: golden_parity calls
//!             Resolve<TypeF>::resolve directly (ProjectCx built over the fixture) and moves
//!             "type_edge" from DEFERRED to PORTED (golden_parity.rs:15-17, 64-66); the
//!             captured .v5.jsonl oracles do NOT change (v5 already emits type_edge lines -
//!             ts/sample.v5.jsonl:98-107). If human review instead wants resolve rows on the
//!             CLI stream, that is a dispatch-seam change = its own increment, and only then
//!             does sample.typef.snap grow (a declared UPDATE_SNAP at that point).
//!   PENDING:   the name-resolved type EDGES (field/impl/variant/uses + the
//!              resolved param/returns binding) still land at Resolve<TypeF>
//!              (commit 4) by design. ts_const_facts_from is PORTED (see (const)).
//!   ORACLE:     scip-typescript 0.4.0 was run on the fixture (throwaway /tmp). The
//!              real correctness gate is occurrence/resolution parity (the commit 4
//!              ratchet), NOT a raw symbol diff (scip is a flat exhaustive symbol
//!              table; v5/v6 model callable arrow-types in the type graph + exclude
//!              value-consts, so the models differ by construction).
//!
//! Partitioning recon vs v5 sprawl (verified 2026-07-23, grep on v5 src/):
//!   v5 TypeLang methods -> v6 (uniform across langs, no special-case lang):
//!     extract            (types)   -> `Project<TypeF>`
//!     extract_calls                -> `Project<CallF>`
//!     extract_dataflow             -> `Project<DfF>`
//!     ModuleResolver.edges         -> `Project<ModuleF>` + `Resolve<{Module,Call,Type}>`
//!   PORT SCOPE (this iteration): 3 langs only: rust (syn), ts+js (oxc), go.
//!   python + kotlin deferred. Go has NO native Rust parser (no syn/oxc analog): its
//!   `Parser` is tree-sitter, so Go's `Project<F>`/`Resolve<F>` walk the tree-sitter
//!   CST directly (the floor as the only tier) + scip-go for resolution. The trait
//!   model already handles this: `Project<F>` consumes whatever `Parsed` is (syn tree
//!   / oxc AST / tree-sitter CST).
//!   COVERAGE GAP found + closed: CstF. v5 `src/cst.rs` `walk_cst` enumerates the
//!   lossless tree-sitter named-node tree (backs `node`/`child` query relations +
//!   codemod anchors + the spine). It is a 5th family: `Node<CstF>` = {kind: NameId
//!   (grammar kind, OPEN vocabulary, interned), span, parent_ix}, edges = child. The
//!   STRUCTURE plane. Every tree-sitter lang gets it for free (the floor, first-class).
//!   const_value: v5 brand `const_value_kind = [lit, template]` (decls.rs:130); seed
//!   has 3 (+concat, from the TS collector). Reconcile variant count on port. Stays a
//!   Type-family aux (consts), not its own family. flow_edge stays the value-flow
//!   union (std/flow.dl:89 -> typed `Flow<F>`).
//!
//! CPU trait factoring: one seam per orthogonal dimension, no fat trait:
//!   tool    `Parser`        syn / oxc / tree-sitter: one impl per backing engine
//!   family  `Project<F>`    phase 1: `Parsed -> FamilyBundle<F>`
//!           `Resolve<F>`    phase 2: `FamilyBundle<F> + ProjectCx -> Vec<ProjectEdge>`
//!   binding `Source`        one row per lang: parser + per-family projectors + scip
//!   orch    `Dispatch`      ONE generic impl; rayon + arena-per-worker live here
//!   blobs   `BlobSource`    file locator -> Blob (`GitShellout` / `Filesystem` / ...; lab picks)
//!   scip    `ScipSource`    build (subprocess) + load (protobuf parse)
//!   (enum, not trait, for the closed vocabularies: per-family kind enums, `Producer`,
//!    `FamilyTag`. trait for the open extension points. CstF's kind is the exception:
//!    an open interned grammar name, so `NameId`, not a closed enum.)
//!
//! Family dimension (type-level; the marker structs + trait; the sums DELETE):
//!   trait Family { type NodeKind; type EdgeKind; const TAG: FamilyTag; }
//!      DfF     -> DfNodeKind(23) · DfEdgeKind{ Direct, Flow(FlowEdgeKind) }  [value-flow]
//!      CallF   -> CallNodeKind{Free,Method,Lambda} · CallEdgeKind{...}       [resolution]
//!      TypeF   -> TypeEntity(9) · TypeEdge(7)                               [resolution]
//!      ModuleF -> ModuleNode{File,PkgRoot} · ModuleEdge{...}                [resolution]
//!      CstF    -> kind: NameId (OPEN grammar vocab) · CstEdge{Child}        [structure]
//!   FlowEdgeKind{ DfDirect, ArgToParam, RetToCallRes, LambdaElem, LambdaRet }
//!
//! Turnkey test plan (a new lang is ONE file + ONE fixture; the codegen loop):
//!   Tier 1 SNAPSHOT (dev feedback, per-lang):
//!     `tests/snapshot.rs` iterates registered langs; for each, parses
//!     `tests/fixtures/<lang>/sample.<ext>` and diffs each family's flattened
//!     `(family, span, kind, name)` node+edge set against `sample.<family>.snap`.
//!     Add a lang = write `lang/<name>.rs` (impl the trait interface), register in
//!     `lang/mod.rs`, drop a fixture, run. The `.snap` files generate on first run,
//!     get reviewed + committed. This is what an AI codegenning a new lang runs.
//!   Tier 2 PARITY GOLDEN (the unified v5 intent, the correctness gate):
//!     `tests/golden_parity.rs` runs v5 AND v6 on the same fixtures, normalizes both
//!     to `(family, span, kind)`, asserts an empty diff. v5's disparate inline tests
//!     (`rust/tests.rs`, `typegraph/{kotlin,go,python,ts/*}`, `engine/extract/*_tests`,
//!     `cst.rs`, `scip_import.rs`) get ported HERE: each one's INTENT becomes one
//!     parity case, cleaned up + unified behind the trait interface.
//!   The trait interface IS the turnkey contract: codegen fills `lang/<name>.rs`
//!   against the signatures, runs the snapshot harness, reads the diff. No per-lang
//!   test scaffolding. Single file per lang (for now).
//!
//! Future filesystem (sprefa-extract; the real crate. commits 1-2 LANDED 2026-07-23
//! (see BUILD STATUS above); the rest lands with commits 3-6):
//!   src/
//!     lib.rs        re-export the trait interface + Family + shape (the "lib")
//!     shape.rs      S1 atoms: Span, BlobHash, NameId, NodeRef, FamilyTag, Project, File
//!     family.rs     S2: Family trait + DfF/CallF/TypeF/ModuleF/CstF + kind enums
//!     rows.rs       S3: Node<F>, Edge<F>, ProjectEdge, FamilyBundle<F>
//!     scip.rs       S4: ScipIndex wire + the in/out projections
//!     seams.rs      S5: Parser, Project<F>, Resolve<F>, BlobSource, ScipSource, Source
//!     budget.rs     ParseArena, ExtractBudget, ExtractJob, FileCacheKey
//!     project.rs    ProjectCx + FileSet/ManifestMap/IndexBag/ProjectDigest
//!     output.rs     ExtractOutput, AuxFacts, Producer, ratchet
//!     wire.rs       the flat tagged envelope (FamilyTag, span, kind, name) + serde
//!     dispatch.rs   Dispatch (the ONE rayon orchestrator) + streaming sink variant
//!     lang/         ONE FILE PER LANGUAGE (the turnkey unit)
//!       mod.rs        the Source roster: [rust, ts, go]   (python/kotlin deferred)
//!       rust.rs       SynParser + Project<{Type,Call,Df,Module,Cst}> + Resolve<{...}>
//!       ts.rs         OxcParser + ...   (js + ts)
//!       go.rs         TsParser + Project/Resolve (tree-sitter walk) + scip-go
//!   src/bin/extract.rs  the CLI: clap args + streaming JSONL stdout (rxjs-driven; no tokio)
//!   tests/
//!     snapshot.rs       Tier 1: per-lang family snapshots over fixtures/
//!     golden_parity.rs  Tier 2: v5-vs-v6 normalized diff (the unified v5 intent)
//!     fixtures/<lang>/sample.<ext>  +  sample.<family>.snap
//!
//! CLI + streaming wire (the RxJS-prototype path; reactivity lives in TS, not here):
//!   A thin `[[bin]]` target wraps the sync lib: clap for args, serde for the wire,
//!   NO tokio (sync stdout drain). The reactivity this iteration is an RxJS prototype
//!   that spawns this bin and reads the stdout fact stream: confirms D-sync-only and
//!   "reactivity elsewhere." Three bakes:
//!   1. wire = the FLAT tagged form `(FamilyTag, span, kind, name)`: the SAME shape as
//!      the store seam (S7) and the parity-golden normalization. One flatten, three
//!      consumers (stdout JSONL / store adapter / parity diff). serde on the flat
//!      envelope, NOT on the generic `Node<F>` (which stays in-memory).
//!   2. STREAMING emission: rayon workers feed a channel; the bin drains + writes JSONL
//!      to stdout as each blob extracts, so RSS does not buffer the whole corpus. The
//!      lib offers `dispatch` -> Vec AND a streaming sink variant.
//!   3. dep placement: serde on the lib's flat wire types (cheap, always on); clap on
//!      the bin only. The lib stays pure-graph + rayon.
//!   This bin is ALSO the "CLI oracle" frontier item (purity proof vs biome/oxc),
//!   promoted from deferred to near-term because it IS the prototype path.
//!
//! Frontier (deferred, evidence-gated):
//!   CLI oracle   ship an ast-grep/biome-shaped CLI from this crate as a purity-proof
//!                oracle against biome / oxc (esp. when doing oxc-class work). Lineage:
//!                the v3 parser-rayon perf labs. Parked until the port lands.
//!   git-fu lab   re-establish the efficient rev->blob story (shellout vs libgit2 vs
//!                pack-index direct) on linux-kernel-history class input. v5's results
//!                were confusing; shellout usually won. RELEASES GitFuLabbed.
//!                (non-git blob sourcing is plain; not labbed.)
//!   k-CFA / node-level types / CFG-dominators: extract or engine? (Evidence-gated.)
//!
//! Companion epic plan: `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`.

use crate::_3_extract::_0_shape::{ProjectEdge, RawEdge, RawNode};
use crate::_3_extract::_1_mask::{FamilyMask, FileBundle, ProjectBundle};
use crate::_3_extract::_2_traits::{ExtractBudget, ExtractJob, ProjectCx, Source};
use crate::_3_extract::_4_scip::{ScipError, ScipIndex};

// Proof tokens RELEASED by an unlanded task (mirror store tasks.rs convention):
//   TypedFamilies  epic 0 : per-family Family trait + Node<F>/Edge<F>; sums deleted; CstF the 5th
//   GitFuLabbed    epic G : rev->blob shellout-vs-libgit2 lab (linux-kernel-history class)
//   Ported         epic P : v5 five families (incl CstF) + SCIP ported behind Project<F>/Resolve<F>
//   TurnkeyTest    epic T : Tier-1 snapshot harness + single-file-per-lang codegen loop
//   Arened         epic 2 : arena-per-file RSS flat under N-worker parse
//   Merged         epic 3 : ratchet: per-fact best-producer-wins over N producers
//   Dispatched     epic 4 : rayon dispatch, no lock contention / livelock
//   FlowUnified    epic 5 : flow_edge promoted to typed Flow<F> edges
//   Evidence       frontier: a measurement that closes a question
pub struct TypedFamilies;
pub struct GitFuLabbed;
pub struct Ported;
pub struct TurnkeyTest;
pub struct Arened;
pub struct Merged;
pub struct Dispatched;
pub struct FlowUnified;
pub struct Evidence;

// =============================================================================
// blob source: file bytes in (the layer this crate owns per the scope ruling)
// =============================================================================
/// File bytes in, content-hashed out. SOURCE-AGNOSTIC: a corpus may be a git
/// worktree (`GitShellout` impl: rev baked in at construction, bytes via cat-file)
/// or a plain directory (`Filesystem` impl: path -> bytes, "version" = now). The
/// engine MAY pre-stage bytes and bypass this. The content hash is the cache key;
/// how bytes were found (rev vs now) never is. v5 git path: `engine/repo.rs:1169
/// read()`, `:1152 cat-file --batch`, `revid.rs` Rev/`GitOid`. The git
/// efficient-vs-not story is a lab (frontier: GitFuLabbed); non-git is plain.
pub trait BlobSource: Sync {
    /// Read one file's bytes for the corpus this source was built over. Returns
    /// the bytes (the caller hashes them into the `BlobHash` cache key) or None.
    /// Whatever "version" means (a git rev, fs now) is construction state of the
    /// concrete impl, not a parameter here.
    fn blob(&self, path: &str) -> Option<Vec<u8>>;
}

// =============================================================================
// Trait · Extract: the contract surface (each method doc = the note)
// =============================================================================
/// The extraction contract. SYNC throughout. The engine calls `dispatch` with the
/// changed blobs + the active cone mask; extract returns normalized nodes + edges +
/// aux for exactly the masked families.
///
/// REVISION (this session): signatures below still use the seed's pre-refactor types
/// (`FileBundle`/`ProjectBundle`/`ProjectCx`). They become, per the decisions above:
///   extract_file    -> per-family `Project<F>::project` (one per family, masked)
///   resolve_project -> per-family `Resolve<F>::resolve` (call/type/module; df/cst none)
///   ProjectCx       -> kept (project = a corpus root; git is one BlobSource, not assumed)
///   merge           -> `ratchet(&[(Producer, ExtractOutput)]) -> ExtractOutput`
///   dispatch        -> unchanged shape (the ONE generic rayon orchestrator)
pub trait Extract {
    /// OPEN · rayon fan-out over `jobs`, one arena per worker · oracle: v5
    /// extractors (syn/oxc/tree-sitter) byte-identical on the same corpus ·
    /// parity: byte-identical node/edge set vs v5 · THE RAM GUN (RSS flat).
    fn dispatch(
        &self,
        jobs: Vec<ExtractJob>,
        cx: &ProjectCx,
        sources: &[Source],
        budget: &ExtractBudget,
    ) -> Vec<ExtractOutput>;

    /// OPEN · phase 1: one parse, masked projections · cache key (blob, lang,
    /// mask) · oracle v5 `TypeLang::extract_bundle` · identical bytes = one hit.
    /// REVISION -> `Project<F>::project`.
    fn extract_file(&self, job: &ExtractJob, sources: &[Source]) -> FileBundle;

    /// OPEN · phase 2: cross-file resolution · cache key (blob, project_digest,
    /// mask) · oracle v5 `ModuleResolver::edges` + type/call resolvers.
    /// REVISION -> `Resolve<F>::resolve` (ProjectCx).
    fn resolve_project(
        &self,
        blob: &ExtractJob,
        file: &FileBundle,
        cx: &ProjectCx,
        sources: &[Source],
    ) -> ProjectBundle;

    /// OPEN · Tier 1: shell out the foreign indexer over `root` · oracle v5
    /// `scip_setup` INDEXERS · subprocess (`std::process`), never bespoke FFI.
    fn scip_build(&self, root: &std::path::Path, indexer: &'static str) -> Result<(), ScipError>;

    /// OPEN · Tier 1: parse index.scip -> diet ScipIndex · oracle v5
    /// `scip_import::load` · reload-gated by mtime.
    fn scip_load(&self, index_path: &std::path::Path) -> Result<ScipIndex, ScipError>;

    /// OPEN · the ratchet: per-fact best-producer-wins over N producers
    /// (`Ast` / `Scip` / `Ghcacher`). SCIP ground-truth for call/type/module
    /// resolution is ONE rule, not the whole policy. · oracle: producer agreement.
    /// REVISION -> `ratchet(&[(Producer, ExtractOutput)])`. releases `Merged`.
    fn merge(&self, scip: &ScipIndex, ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle;
}

/// What one blob's extraction yields. REVISION -> per-family `FamilyBundle<F>` vecs
/// (df/call/type/module/cst) + aux; the flat `nodes: Vec<RawNode>` (which carried
/// the now-deleted `NodeKind` sum) goes away.
#[derive(Clone, Debug, Default)]
pub struct ExtractOutput {
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    pub project_edges: Vec<ProjectEdge>,
    pub aux: AuxFacts,
}

/// The family side tables (bindings, import forms, param_pos, args, fields, lits,
/// loops, docs, consts). Per-occurrence/per-node attributes, NOT a plane.
#[derive(Clone, Debug, Default)]
pub struct AuxFacts;

/// SCIP resolution layered over AST facts. REVISION -> the ratchet output: a chosen
/// `ExtractOutput` per fact + the producer that won. `scip_resolution` generalizes
/// to "winning producer's edges."
#[derive(Clone, Debug, Default)]
pub struct MergedBundle {
    pub ast: Vec<ExtractOutput>,
    pub scip_resolution: Vec<ProjectEdge>,
}

// =============================================================================
// The stub impl: every body is `todo!()`; the doc on each method IS the note.
// =============================================================================
pub struct Tasks;

impl Extract for Tasks {
    fn dispatch(
        &self,
        _jobs: Vec<ExtractJob>,
        _cx: &ProjectCx,
        _sources: &[Source],
        _budget: &ExtractBudget,
    ) -> Vec<ExtractOutput> {
        todo!("epic 4: rayon par_iter over jobs; each worker owns ParseArena; budget-cap RSS")
    }
    fn extract_file(&self, _job: &ExtractJob, _sources: &[Source]) -> FileBundle {
        todo!("epic 1: tiered parse -> Project<F>::project per masked family")
    }
    fn resolve_project(
        &self,
        _blob: &ExtractJob,
        _file: &FileBundle,
        _cx: &ProjectCx,
        _sources: &[Source],
    ) -> ProjectBundle {
        todo!("epic 1: Resolve<F>::resolve for call/type/module; df/cst skipped")
    }
    fn scip_build(&self, _root: &std::path::Path, _indexer: &'static str) -> Result<(), ScipError> {
        todo!("epic 3: shell out rust-analyzer/scip-typescript/...; write index.scip")
    }
    fn scip_load(&self, _index_path: &std::path::Path) -> Result<ScipIndex, ScipError> {
        todo!("epic 3: parse index.scip -> diet ScipIndex (symbol/range/role/relations only)")
    }
    fn merge(&self, _scip: &ScipIndex, _ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle {
        todo!("epic 3: ratchet(producers) per-fact best-wins; releases Merged")
    }
}

// =============================================================================
// The remaining plan, as a trait: proof-token methods for the open epics.
// =============================================================================
/// A method's ARGS are body predicates (facts released earlier); its RETURN is
/// the head predicate. Linear narrative ordering, not hard build deps. Epic 0 types
/// families (incl CstF); G labs git-fu; P ports; T stands up the turnkey test loop;
/// 2 masters RAM; 3 ratchets; 4 proves parallelism; 5 promotes flow; frontier measures.
pub trait ExtractPlan {
    /// 0  families are type-level: `Family` trait + `Node<F>`/`Edge<F>`; sums
    ///    deleted; CstF is the 5th family (STRUCTURE plane, open grammar kind);
    ///    family discriminant is a flat `FamilyTag` at the seam + ratchet key only.
    fn families_typed(&self) -> TypedFamilies;
    /// G  rev->blob git-fu lab: shellout vs libgit2 vs pack-index direct, on
    ///    linux-kernel-history class input. v5: shellout usually won. (non-git
    ///    blob sourcing is a plain `Filesystem` impl, not labbed.)
    fn git_fu_labbed(&self, proof: &TypedFamilies) -> GitFuLabbed;
    /// P  port v5's five families (incl CstF) + SCIP behind `Project<F>`/`Resolve<F>`,
    ///    normalized (sym->span, kind-String->typed enum, one Span). Parity: byte-
    ///    identical vs v5. Each lang is ONE file under lang/.
    fn v5_ported(&self, proof: &GitFuLabbed) -> Ported;
    /// T  the turnkey test loop: Tier-1 snapshot harness (per-lang, one fixture +
    ///    `.snap` per family) + the codegen contract (fill lang/<name>.rs, run, read
    ///    the diff). A new lang is turnkey because it targets a trait interface.
    fn turnkey_test(&self, proof: &Ported) -> TurnkeyTest;
    /// 2  arena-per-file parse keeps RSS flat under N-worker rayon dispatch.
    fn arena_ram_mastered(&self, proof: &TurnkeyTest) -> Arened;
    /// 3  the ratchet: per-fact best-producer-wins over N producers (Ast/Scip/Ghcacher).
    fn ratchet_proven(&self, proof: &Arened) -> Merged;
    /// 4  rayon dispatch over a real corpus hits no lock contention / livelock.
    fn parallel_dispatch_proven(&self, proof: &Merged) -> Dispatched;
    /// 5  flow_edge (v5 stdlib 5th family, std/flow.dl:89) promoted to typed
    ///    `Flow<F>` edges: the interprocedural value-flow union in the type system.
    fn flow_edge_promoted(&self, proof: &Dispatched) -> FlowUnified;
    /// frontier: k-CFA / node-level types / CFG-dominators: extract or engine?
    ///    CLI oracle (ast-grep/biome-shaped, v3 perf-lab lineage) parks here too.
    ///    Returns Evidence, not a shipped change.
    fn frontier(&self) -> Evidence;
}
