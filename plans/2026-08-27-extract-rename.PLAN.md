# extract rename: a symbol rename over the resolved edge plane

Status: plan only, 2026-08-27. Rank 5 of
`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md:39`. Issue
`issues/move-symbol-rename-plan/item.md`. No code lands with this doc.

Decision (Chris, 2026-08-26, carried from
`plans/2026-08-26-extract-move-rehome-trait.PLAN.md:3`): every language is its
own impl, no `match` on language in the core.

- [Verdict](#verdict)
- [Prior art](#prior-art)
- [The edge plane a rename needs](#the-edge-plane-a-rename-needs)
  - [Per-language span table](#per-language-span-table)
  - [Three structural gaps](#three-structural-gaps)
  - [SCIP as the second source](#scip-as-the-second-source)
- [Build-vs-buy: where exact identifier spans come from](#build-vs-buy-where-exact-identifier-spans-come-from)
- [Contract](#contract)
  - [Signatures](#signatures)
  - [Sibling trait or methods on Rehome](#sibling-trait-or-methods-on-rehome)
  - [Lifetimes](#lifetimes)
  - [Storage, reads and writes, uniqueness](#storage-reads-and-writes-uniqueness)
- [Scope fence](#scope-fence)
- [Arcs](#arcs)
- [Receipts](#receipts)
- [Out of scope](#out-of-scope)

## Verdict

The rank-5 row says a rename "needs the resolved edge plane (`Resolve<F>`)".
Measured against the code, the resolved edge plane **cannot spell a rename**,
for three reasons proved in [Three structural gaps](#three-structural-gaps).
The plane answers "which declaration does this reference bind to". A rename
asks "which bytes spell that reference", and no row in `Resolve<F>`'s output
carries those bytes.

The rename verb is therefore built on a **new per-language seam over an exact
span source**, sibling to `Rehome`. For TypeScript that source is
`oxc_semantic` (+1 crate in this lock, [measured below](#build-vs-buy-where-exact-identifier-spans-come-from)).
`Resolve<F>` and SCIP stay in the design as a **verify** leg, never as the
plan's source.

## Prior art

### v1: `DeclChange::Rename` and `plan_decl_rename`

v1 had a rename and it was watcher-driven: the user renamed a declaration in
the editor, the watcher re-extracted, diffed old refs against new refs, and
propagated.

| stage | site | shape |
|---|---|---|
| detect | `crates/watch/src/diff.rs:32 diff_refs` | same kind + span within `SPAN_PROXIMITY_THRESHOLD` = 64 bytes, different value -> `Rename` (`diff.rs:20`) |
| carry | `crates/watch/src/change.rs:38 DeclChange::Rename` | `{file_id, kind, old_name, new_name, new_span_start, new_span_end}` |
| kinds watched | `diff.rs:11 DECL_KINDS` | `export_name`, `rs_declare`, `rs_mod`, `import_name` (`crates/extract/src/lib.rs:9,10,16,17`) |
| plan | `crates/watch/src/plan.rs:106` -> `:271 plan_decl_rename` | one arm per kind |

The four arms:

| kind | arm | what it rewrites |
|---|---|---|
| `export_name` | `plan.rs:289 rename_through_reexports` | BFS over importers and re-export relays (`plan.rs:442`), rewriting each `import { OLD }` clause to `NEW` |
| `rs_declare` | `plan.rs:291-375` | every `use crate::mod::OLD` (all prefix styles resolved to absolute first), plus glob expansion `use crate::m::*` -> `use crate::m::{A, B}` with the rename applied (`plan.rs:322-348`), plus cross-crate `use crate_name::mod::OLD` (`plan.rs:351-374`) |
| `import_name` | `plan.rs:377-415` | walks UP to the declaring file, renames the export there, then propagates back down skipping the file the user already edited |
| `rs_mod` | none | listed in `DECL_KINDS`, falls through `plan.rs:413-415` `_ => {}` |

**The v1 hole worth naming.** Every arm rewrites `use` paths and
import/export clauses. **None rewrites a usage in a function body.** For Rust
that is mostly fine (a `use` brings the name in and the body's bare `OLD`
still breaks). For TypeScript it is broken by construction: rewriting a
consumer's `import { Foo }` to `import { Bar }` while its body still calls
`Foo(...)` leaves the consumer not compiling. v1 got away with it because the
editor's own rename had usually already fixed the originating file, never the
downstream ones.

The v6 rename does not inherit that hole. [Arc 1](#arcs) makes the body-usage
set the primary output and the import clause one role inside it.

### v5: no symbol rename

```
$ grep -rnc "DeclChange\|plan_decl_rename\|decl_rename\|rename_through_reexport" ~/projects/sprefa/src/
(no file with a non-zero count)
```

Every `rename` hit in v5 `src/**` is either `std::fs::rename` (`eventlog.rs:69`,
`scip_setup.rs:347,435`, `lib.rs:1661`) or datalog alpha-renaming
(`frontend.rs:473,500`). v5's `--move` moves files and re-homes `mod`
declarations (`lib.rs:1207`, `:1670`); it never renames a symbol. This
restates `plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md:20`, "symbol
rename | v1 yes | v5 no | v6 no", with the grep behind it.

## The edge plane a rename needs

A rename plan is a list of `(file, byte span, replacement)` triples. For that
list to be correct, every span must cover **exactly the identifier token** and
nothing else. Measure each candidate source against that bar.

### Per-language span table

`Resolve<F>` impls exist for 7 languages (`grep -rn "impl Resolve<" src/`):

```
lang/dl6/_0_source.rs:440,464   lang/go.rs:1799,1936    lang/kotlin.rs:1546,1619
lang/markdown/_0_source.rs:262  lang/prolog/_0_source.rs:925
lang/rust.rs:829,962            lang/ts.rs:3147,3274
```

`Rehome` impls exist for 3 (`lang/mod.rs:91`): rust, prolog, ts.

| language | def-site row and span | ref-site row and span | identifier-exact? |
|---|---|---|---|
| **ts** | `CallF` node, `ts.rs:1443` `func.span` (whole fn incl. body); `TypeF` node, `ts.rs:445` `class.span`, `:416` `interface.span`, `:423` `alias.span` (whole declaration) | `CallSite`, `ts.rs:1666` `call.callee.span()` (whole callee expr: `a.b.c` for `a.b.c()`); `ts.rs:1678` `new_expr.span` (the WHOLE `new Foo(x)` incl. args); `ts.rs:1696` `element.opening_element.span` (whole `<Card ...>`); `Specifier`, `ts.rs:1206` `named.span` (whole `Foo as Bar` clause) | **no**, at every seat |
| **rust** | `TypeF` node, `rust.rs:266,274,283,292` `ident.span()` (**the identifier**); `CallF` node, `rust.rs:1066` `def_span(sig.ident, block)` (ident start through body end) | `CallSite`, `rust.rs:1521` `call.func.span()` (whole path `a::b::f`); `rust.rs:1533` `call.method.span()` (**the method ident**) | **partly**: TypeF defs and method-call sites yes, path-call sites and CallF defs no |
| **go** | `TypeF` node, `go.rs:125` `node_span(spec)`, `:142` `node_span(child)` (whole declaration) | `CallSite`, `go.rs:815` `node_span(func)` (identifier for a bare call, whole `pkg.Fn` selector otherwise) | **partly** |
| **kotlin** | `TypeF` node, `kotlin.rs:144,159` `node_span(child)` (whole declaration) | `CallSite`, `kotlin.rs:782` `node_span(lead)`, where `lead` is the whole navigation chain for `a.b.c()` (`kotlin.rs:802-812`) | **partly** |
| **prolog** | `CallF` node, `prolog/_0_source.rs:182` `span(clause)` (whole clause); `TypeF` `:160` same span | `CallSite`, `prolog/_0_source.rs:665` `span(node)` (whole goal term `foo(X, Y)`) | **no** |
| **dl6** | `TypeF` `dl6/_0_source.rs:114` `rel_span`; `CallF` `:202` `rule_span` (whole declaration / rule) | `CallSite`, `dl6/_0_source.rs:340` `span(node)` (whole goal) | **no** |
| **markdown** | none: `MarkdownSource` mints `DocNode` headings, no def rows | `Resolve<TypeF>` `DocRef` off a heading, `markdown/_0_source.rs:262-286` | **no** |

Only two seats in the whole extractor are identifier-exact today:
`rust.rs:266-292` (TypeF item names) and `rust.rs:1533` (method-call idents).
Nothing in TypeScript is.

### Three structural gaps

**Gap 1: the reference-carrying rows carry no reference span.**

| row | fields | the missing seat |
|---|---|---|
| `TypeEdgeCandidate` (`types.rs:325`) | `owner: Span`, `to: NameId`, `kind` | `to` is the referenced name as TEXT. There is no span for where `to` is written. |
| `TypeSig` (`types.rs:264`) | `owner: Span`, `slot`, `pos`, `ty: NameId` | same: `ty` is text, the annotation's own span is not kept |
| `Specifier` (`types.rs:521`) | `span`, `name`, `kind`, `module`, `imported` | `span` is the whole clause; `imported` is text with no span of its own |

A `Resolve<TypeF>` edge (`ts.rs:3147`) is minted from a
`TypeEdgeCandidate`, so `ProjectEdge::src` points at the OWNING declaration's
node, never at the reference. `ProjectEdge` (`types.rs:1292`) has
`src: NodeRef`, `dst_blob`, `dst_span`, `kind`, `call_site`. Its only
reference-side coordinate is `call_site: Option<Span>`, and that seat is only
filled by the `Resolve<CallF>` arms (`ts.rs:3324`, `prolog/_0_source.rs:946`).
For TypeF the field is documented as always empty (`types.rs:1300-1302`).

**Gap 2: `ProjectCx` has no file set.**

```rust
// types.rs:1415-1417
pub struct FileSet;
pub struct ManifestMap;
```

```rust
// project.rs:160-161
let files = FileSet;
let manifests = ManifestMap;
```

Both are unit structs with zero fields, and `resolve_project`
(`project.rs:152`) constructs them empty. `ProjectCx.files` and
`ProjectCx.manifests` (`types.rs:1391`) carry nothing at runtime. The only
corpus handles a `Resolve<F>` arm actually gets are
`cx.indexes.def_index` (`types.rs:1466`, a name -> def-site map),
`cx.indexes.scip_index`, and `cx.reader`, which is `Some` only when a SCIP
index loaded (`project.rs:188`).

Consequence: an arm cannot ask "which files reference X" from `ProjectCx`.
`MoveCx` (`move_cx.rs:33`) is the type that CAN: it holds `root`, `files`,
`present`, and `read` (`move_cx.rs:34-36,121`), from one `ignore::WalkBuilder`
pass (`move_cx.rs:47`). The rename verb needs a `MoveCx`-shaped context, not
`ProjectCx`.

**Gap 3: TypeScript's local export list has no row at all.**

`scan_module_specifiers` (`ts.rs:1183`) handles `ExportNamedDeclaration` only
when `export.source` is `Some` (`ts.rs:1239-1241`). A bare `export { foo }`
with no `from` clause is skipped with the comment "a local export marker, not
a module specifier". v1's `export_name` kind (`crates/extract/src/lib.rs:10`)
covered exactly that form and it was the primary anchor for
`rename_through_reexports`. In v6 it is inexpressible from phase-1 rows.

### SCIP as the second source

SCIP is the one plane in this crate that already carries identifier-exact
occurrence ranges with roles.

| piece | site | what it gives |
|---|---|---|
| `ScipOccurrence` | `types.rs:1736` | `symbol`, `range: [i32; 4]`, `roles: OccurrenceRole` |
| roles | `types.rs:1685-1691` | `DEFINITION` 0x1, `IMPORT` 0x2, `WRITE_ACCESS` 0x4, `READ_ACCESS` 0x8, `GENERATED` 0x10, `TEST` 0x20, `FORWARD_DEF` 0x40; `contains` at `:1692` |
| line/col -> byte | `scip.rs:491 byte_range`, `:534 byte_range_at` | the bridge, honouring `PositionEncoding` (`types.rs:1701`), UTF-16 by default per the SCIP spec |
| document | `ScipDocument` (`types.rs:1814`) | `relative_path` + `position_encoding` + `occurrences` |
| indexers | `ScipSource` impls, `scip.rs:106,120,143,167,181,195` | ts, rust, go, python, java/kotlin, clang |

An occurrence range names the identifier token, which is exactly the bar a
Replace has to clear.

**Why SCIP is the verify leg and not the plan source.** Three costs, each
already recorded in this repo:

1. **Cost.** Running an indexer is minutes, and it is the ONE named exception
   to the 10-second law (`CLAUDE.md`, "Named exception: SCIP indexing"). The
   budget machinery exists precisely because indexers wedge
   (`scip_ensure.rs:105-108`, `IndexBudget`).
2. **Availability.** `scip_ensure.rs:343-353` skips any indexer whose binary
   is absent from PATH, as a NAMED skip row. A rename that only works on a
   machine with `scip-typescript` installed is a rename that mostly does not
   work.
3. **Staleness.** A SCIP index describes the tree at index time. A rename
   plan built from a stale index proposes spans that no longer hold the old
   name. The existing `tests/scip_freshness.rs` is the shape of that hazard.

So: the plan is built from the language's own parse of the current bytes, and
SCIP verifies it when an index is in hand (arc 6). That inverts nothing about
`Rehome`, which already reads current bytes through `MoveCx::read`.

## Build-vs-buy: where exact identifier spans come from

The problem shape: "given a declaration, enumerate every occurrence of the
name it binds, as exact byte spans, within one TypeScript program". This is
scope analysis, a common-shaped problem, so no bespoke answer is proposed
before the library survey.

| candidate | new crates in this lock | span exactness | cross-file | verdict |
|---|---|---|---|---|
| **`oxc_semantic` 0.135** | **+1** (measured below) | exact: `Reference.node_id` -> `AstKind::span()` on an `IdentifierReference`; `Scoping::symbol_span` (`scoping.rs:335`) for the binding | file-local; composes with `TsRehome::import_refs` for the module graph | **chosen** |
| `scip-typescript` via `ScipTypescript` (`scip.rs:106`) | 0 | exact, through `scip::byte_range` (`scip.rs:491`) | yes, whole corpus, with roles | verify leg only; see the three costs above |
| `ts-morph` / `tsserver` subprocess | 0 rust crates, +1 node toolchain hard dependency | exact, and it is the reference implementation of TS rename | yes | a second runtime and a second parse of the same files, when `oxc` is already the front-end here (`ts.rs:38`); rejected |
| `tree-sitter-typescript` + hand-rolled scope tree | 0 (grammar not in this lock; scope analysis would be bespoke) | exact spans, but the scope tree is ours to write | ours to write | writing a JS/TS scope analyser is the thing the build-vs-buy law exists to stop; rejected |
| widen the phase-1 rows to carry identifier spans | 0 | exact once written | no, still needs the module graph | this is `oxc_semantic`'s job re-implemented inside `ts.rs`, plus it edits the extractor's row shapes and its goldens for a verb that does not need them; rejected |

### The `oxc_semantic` cost, measured

```
$ cargo add --dry-run oxc_semantic@0.135
      Adding oxc_semantic v0.135 to dependencies
             Features as of v0.135.0: cfg, jsdoc, linter, serialize
```

Default features are empty (`oxc_semantic-0.135.0/Cargo.toml.orig:48`), so
`oxc_cfg` and `oxc_jsdoc` stay out. Its required dependency set
(`Cargo.toml.orig:22-39`) against this lock:

| dependency | in `Cargo.lock` today |
|---|---|
| `oxc_allocator`, `oxc_ast`, `oxc_ast_visit`, `oxc_span`, `oxc_syntax` | yes, all pinned 0.135 in `Cargo.toml:47-52` |
| `oxc_diagnostics`, `oxc_ecmascript`, `oxc_index`, `oxc_str` | yes, as transitives |
| `itertools`, `memchr`, `rustc-hash`, `self_cell`, `smallvec` | yes, all five |

Every dependency is already present. **Adding `oxc_semantic` adds exactly one
crate**, from the oxc 0.135 family this crate already pins. It does turn on
`oxc_allocator/bitset` (`Cargo.toml.orig:23`) through Cargo feature
unification, which is a feature flip, not a crate.

The API the arc uses, read from the vendored 0.135 source:

| call | site | returns |
|---|---|---|
| `SemanticBuilder::build(&program)` | `lib.rs:48` | `SemanticBuilderReturn` |
| `Semantic::scoping()` | `lib.rs:131` | `&Scoping` |
| `Semantic::nodes()` | `lib.rs:126` | `&AstNodes`, node id -> `AstKind` -> `span()` |
| `Scoping::symbol_span(SymbolId)` | `scoping.rs:335` | the binding identifier's `Span` |
| `Scoping::get_binding(ScopeId, Ident)` | `scoping.rs:817` | `Option<SymbolId>` |
| `Scoping::get_resolved_references(SymbolId)` | `scoping.rs:551` | every `Reference` bound to that symbol |
| `Reference::node_id()` | `oxc_syntax-0.135.0/src/reference.rs:254` | the `IdentifierReference` node |
| `ReferenceFlags` | `reference.rs:81` | `Read`, `Write`, `Value`, `MemberWriteTarget` |

`ReferenceFlags` maps onto `OccurrenceRole` one for one, which is what lets
the SCIP verify leg (arc 6) compare the two without a translation table.

## Contract

Planning protocol order: signatures, pseudo-code under each, lifetimes,
storage plus read/write order plus uniqueness.

### Signatures

New file `src/rename_cx.rs`, `MoveCx`'s twin. It does not edit `move_cx.rs`.

```rust
// src/rename_cx.rs

/// One symbol this run renames. The anchor names the DECLARING file; the
/// declaration inside it is found by name, or by byte offset when a file
/// declares the name twice.
pub struct RenameRequest {
    pub anchor: String,          // project-relative path of the declaring file
    pub old: String,             // the identifier as written today
    pub new: String,             // what it becomes
    pub at: Option<u32>,         // byte offset INSIDE the declaration, when `old` is ambiguous
                                 // user decision 2026-08-28: a root-scope binding wins without --at;
                                 // a nested same-name binding shadows its own block only. Ambiguous
                                 // = 0 or 2+ root bindings. --at still selects among every binding.
}

/// One `extract rename` run's corpus view. Same walk, same skip set, same
/// root-relative spelling law as `MoveCx` (`move_cx.rs:26,45,158`).
pub struct RenameCx {
    root: PathBuf,
    files: Vec<String>,
    batch: Vec<RenameRequest>,
}

impl RenameCx {
    pub fn open(root: &Path) -> Result<Self, String>;
    pub fn with_batch(self, batch: Vec<RenameRequest>) -> Self;
    pub fn root(&self) -> &Path;
    pub fn files(&self) -> &[String];
    pub fn files_of(&self, arm: &dyn Rename) -> Vec<&str>;   // roster first-match, as move_cx.rs:108
    pub fn read(&self, rel: &str) -> Option<Vec<u8>>;
    pub fn text(&self, rel: &str) -> Option<String>;
    pub fn batch(&self) -> &[RenameRequest];
}
```

```rust
// src/types.rs, beside Rehome (:1976)

/// Where one occurrence of a symbol sits. `span` covers EXACTLY the identifier
/// token: no quotes, no path prefix, no surrounding expression.
pub struct SymbolRef {
    pub file: String,
    pub span: Span,
    pub role: RefRole,
    /// The bytes at `span` as the arm read them. The core re-reads the tree and
    /// asserts equality before staging; a mismatch is a plan error, never a
    /// silent skip.
    pub text: String,
}

/// What one occurrence does with the symbol. One-for-one with SCIP's
/// `OccurrenceRole` (`types.rs:1685-1691`), so the verify leg compares directly.
pub enum RefRole {
    Definition,   // OccurrenceRole::DEFINITION
    Import,       // OccurrenceRole::IMPORT      -- the imported name in `import {OLD}`
    Export,       // the exported name in `export {OLD}` / `export {x as OLD}`
    Read,         // OccurrenceRole::READ_ACCESS
    Write,        // OccurrenceRole::WRITE_ACCESS
    TypeRef,      // a type-position mention; SCIP folds this into READ_ACCESS
}

/// Why an arm will not plan. A partial rename compiles less often than no
/// rename at all, so an arm stops instead of emitting a subset.
pub enum RenameStop {
    /// `old` names more than one declaration in `anchor`; `at` disambiguates.
    Ambiguous { anchor: String, old: String, sites: Vec<Span> },
    /// `old` names no declaration in `anchor`.
    NotFound { anchor: String, old: String },
    /// A reference the arm found but cannot span exactly.
    Inexact { file: String, span: Span, why: &'static str },
    /// A reference reachable only through a runtime form (computed member,
    /// dynamic import, string key). Reported, never rewritten.
    Dynamic { file: String, span: Span, form: &'static str },
}

/// What one language answers when a symbol it owns is renamed. Sibling to
/// `Rehome` (:1976), held `&'static` in the `renames()` roster.
pub trait Rename: Source + Sync + Send {
    /// Every occurrence of `request`'s symbol this language owns, across
    /// `cx.files()`. One parse per file that can reach the anchor.
    fn symbol_refs(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
    ) -> Result<Vec<SymbolRef>, RenameStop>;
    //  ts: SemanticBuilder over the anchor -> the anchor's own SymbolId, its
    //      symbol_span + every get_resolved_references node span. Then
    //      TsRehome::import_refs (ts_rehome.rs:191) narrows the corpus to files
    //      whose specifier resolves to the anchor; each importer is parsed once,
    //      its ImportSpecifier for `old` located, and that clause's LOCAL binding
    //      re-run through the same scope walk. A re-export relay enqueues its
    //      own importers (v1's BFS, plan.rs:442, with body usages added).
    //  rust: syn walk. Item idents already exact (rust.rs:266-292); a `use`
    //      path's trailing segment, an ExprPath's trailing segment, and
    //      ExprMethodCall::method (rust.rs:1533) are the ref seats. A glob
    //      `use m::*` importer is a Dynamic stop, not a silent skip.
    //  prolog: functor/arity. Head and goal spans are whole terms
    //      (prolog/_0_source.rs:182,664), so the arm re-locates the functor
    //      token inside the term before emitting a SymbolRef.

    /// The replacement bytes for one occurrence. `None` = unchanged (an
    /// aliased import `{OLD as local}` leaves `local` alone).
    fn respell_symbol(
        &self,
        cx: &RenameCx,
        request: &RenameRequest,
        reference: &SymbolRef,
    ) -> Option<Respell>;
    //  every arm: Some(Respell { file, span, text: request.new.clone(), receipt })
    //  for Definition | Read | Write | TypeRef.
    //  ts: an Import whose clause is `{OLD as local}` respells only the OLD
    //      seat; an Import whose clause is bare `{OLD}` respells the seat AND
    //      the local binding is the same token, so one Respell covers both.
    //  rust: a `use m::OLD as Local` respells the OLD seat only.

    /// Spellings of the old name this language's corpus wears outside the
    /// scope plane, for the `--text-refs` report. NEVER rewritten.
    fn text_spellings(
        &self,
        _cx: &RenameCx,
        _request: &RenameRequest,
    ) -> Vec<(String, String)> {
        Vec::new()
    }
    //  ts: ("OLD", "NEW") only when the anchor is exported, plus the
    //      default-export file-stem spelling when the anchor is a default export.
}
```

```rust
// src/lang/mod.rs, beside rehomes() (:90)

/// The `Rename` roster, in `sources()` order. A language absent from this
/// roster is a named stop, never a `match` arm in the rename core.
pub fn renames() -> &'static [&'static dyn Rename];
pub fn rename_for(path: &str) -> Option<&'static dyn Rename>;
//  same first-match law as rehome_for (mod.rs:97): source_for wins ownership,
//  then the roster is searched by name.
```

```rust
// src/0_rename.rs, language-free. Mirrors 0_move.rs:108-206.
//
// plan(cli):
//   cx      = RenameCx::open(root)                    // one walk
//   batch   = requested_renames(cli)                  // <FILE>#<OLD> pairs, or --list tsv
//   refs    = for request in batch:
//               arm = rename_for(request.anchor)?     // else "no rename arm for {anchor}"
//               arm.symbol_refs(&cx, request)?        // RenameStop propagates, nothing stages
//   verify  = for r in refs: cx.text(r.file)[r.span] == r.text  // else plan error
//   edits   = refs.filter_map(|r| rename_for(&r.file)?.respell_symbol(&cx, request, r))
//   stages  = [Replace(edits) grouped by file]        // ONE stage: a rename moves no file
//   commit  -> soopy StageRequest, then --text-refs report
//
// No CorpusLang, no is_ts, no match on language. The receipt is the same grep
// PR #489 runs against the move core.
```

CLI, mirroring `extract move` (`0_move.rs:20-53`):

| flag | meaning |
|---|---|
| `<FILE>#<OLD> <NEW>` | positional form; `<FILE>@<OFFSET> <NEW>` when `<OLD>` is declared twice |
| `--list <tsv>` | `anchor<TAB>old<TAB>new` rows, one rename per line |
| `--root <dir>` | corpus root, defaults to the git root holding the first anchor |
| `--state <dir>` | soopy state root, outside the corpus |
| `--commit` | apply, instead of the dry-run default |
| `--text-refs` | report old-name spellings in files no arm owns |
| `--verify-scip` | arc 6: cross-check the plan against an index, report disagreements |

### Sibling trait or methods on Rehome

Both shapes, then the call.

**Shape A, methods on `Rehome`.** Add `symbol_refs` and `respell_symbol` to
the existing trait (`types.rs:1976`) with default empty bodies, and grow
`MoveCx` (`move_cx.rs:33`) with a `renames: Vec<RenameRequest>` field beside
`moved` (`move_cx.rs:37`).

**Shape B, sibling trait `Rename` with its own `RenameCx` and its own
roster.** The shape written above.

**Recommendation: B.** Two lines of why:

1. The two verbs take different context. `MoveCx`'s batch is `path -> path`
   (`move_cx.rs:37`) and every `Rehome` method reads it through `destination`
   and `after` (`move_cx.rs:134,140`); a rename batch is
   `(anchor, old) -> new` with no path pair, so shape A makes every existing
   `Rehome` method carry a field it ignores, and makes every `Rename` method
   carry `moved` it ignores.
2. The rosters differ. `rehomes()` is 3 of 9 languages (`lang/mod.rs:91`)
   chosen by "does this language have import paths"; the rename roster is
   chosen by "does this language have a scope plane with exact spans", which
   from the [span table](#per-language-span-table) is a different set. One
   roster per verb keeps the first-match ownership law (`lang/mod.rs:97`)
   readable.

Sequencing agrees. `src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`,
`types.rs`'s `Rehome`, `lang/rust_rehome.rs`, `tests/1_move.rs` and
`tests/3_move_rust.rs` are owned by six concurrent lanes. Shape B needs one
new `pub trait` block in `types.rs` and one roster function in `lang/mod.rs`;
shape A rewrites `MoveCx` and every `Rehome` signature under six lanes'
feet.

### Lifetimes

| type | lives |
|---|---|
| `RenameCx` | one per `extract rename` invocation; built once, borrowed by every arm |
| `RenameRequest` | owned by `RenameCx.batch`, borrowed everywhere else |
| `&'static dyn Rename` | process-static roster, zero state, exactly `sources()` and `rehomes()` |
| `Semantic<'a>` (ts arm) | one per parsed file, inside `symbol_refs`; never crosses the seam, same law as `Source::extract` "no borrowed parse crosses the seam" (`types.rs:1944`) |
| `Vec<SymbolRef>` | one plan; dropped once `Respell`s are built |
| soopy `StageRequest` | one per run |

### Storage, reads and writes, uniqueness

| step | reads | writes |
|---|---|---|
| open | one `ignore::WalkBuilder` pass over `root`, skipping `SKIP_DIRS` (`move_cx.rs:26`) | none |
| anchor bind | `cx.read(request.anchor)`, one parse | none |
| symbol_refs | `cx.read` per candidate file, one parse each; candidate set narrowed by the importer graph, never the whole corpus | none |
| span verify | `cx.text(file)` per touched file, byte compare against `SymbolRef.text` | none |
| respell | in memory | none |
| commit | none | one soopy `StageRequest` of `Replace` actions, built exactly as `0_move.rs:150-176` |

**Read order.** The anchor is parsed first. Its `SymbolId` and export status
decide whether any other file is opened at all: a symbol with no `Export` role
occurrence is file-local and the run touches one file.

**Uniqueness, three conditions.**

1. `(file, span.start)` names ONE replacement across all arms. Two arms or two
   texts on one start is a plan error naming both arms. This is the invariant
   `0_move.rs:209-236 respells` already enforces for the move verb, reused
   verbatim.
2. `cx.text(file)[span] == reference.text == request.old` for every emitted
   `SymbolRef`. A span holding different bytes means the arm's parse and the
   tree disagree; the run stops with the file and offset. The move verb has no
   equivalent because a path literal's identity is checked by soopy's
   `expected` content id; a rename's spans are interior to a file soopy is
   already replacing, so the check has to be explicit.
3. No two `RenameRequest`s in one batch may target the same
   `(anchor, old)`, and `new` must not collide with a name already bound in
   the anchor's scope. A collision is a plan error, not a shadowed binding.

## Scope fence

A rename rewrites identifier tokens the language's scope plane binds. It
rewrites nothing else. The fence, and the report that covers each hole:

| never rewritten | example | why | covered by |
|---|---|---|---|
| string literals | `container.get("UserService")`, `require("./OldName")` | a string is data; the scope plane does not bind it, and a rewrite guesses | `--text-refs` |
| computed member access | `obj["Foo"]`, `obj[key]` | the member name is a value at runtime | `RenameStop::Dynamic`, then `--text-refs` |
| reflection and eval | `eval("Foo()")`, decorator metadata keyed by name | not in any AST as an identifier | `--text-refs` |
| doc comments and prose | `/** see {@link Foo} */`, `README.md` | text, not code; and a doc rewrite is a judgment call | `--text-refs` |
| test snapshots and goldens | `__snapshots__/*.snap`, `tests/fixtures/**` expected output | rewriting a golden hides the regression the golden exists to catch | `--text-refs` |
| build output | `target/`, `node_modules/`, `dist/` | not corpus; `SKIP_DIRS` (`move_cx.rs:26`) never walks them | nothing, by design |
| files outside `--root` | a sibling repo consuming a published package | a rename cannot see a consumer it does not walk | `--text-refs` cannot see them either; the report says the anchor is exported |
| the git history | commit messages, changelogs | history is a record of what was, not a reference | nothing, by design |

The report reuses `2_move_text.rs`'s shape exactly. `report` (`2_move_text.rs:13`)
prints one line per hit:

```
text-ref <file>:<line> <matched> -> <proposed>
```

For rename the candidate pairs come from `Rename::text_spellings` instead of
`segment_pairs` (`2_move_text.rs:41`), because a symbol has no path segments
to peel. Everything downstream (the carrier exclusion, the longest-match-wins
sort, the per-line scan) is unchanged.

**One extra fence a move does not need.** A rename may not proceed when the
anchor is exported from a package whose manifest lists it as a public entry
(`package.json` `exports`/`main`, `Cargo.toml` `[lib]`). The plan prints the
manifest path and the entry, then continues only under `--commit`. Renaming a
published name is a breaking change and the tool says so once.

## Arcs

Smallest first. Each row is one lane, one PR. None adds a language switch to
the move core or the rename core.

| arc | scope | files | receipt |
|---|---|---|---|
| **1** | `Rename` trait + `RenameStop` + `SymbolRef`/`RefRole` in `types.rs`; `src/rename_cx.rs`; `renames()`/`rename_for` in `lang/mod.rs`; `src/0_rename.rs` bin arm; `src/lang/ts_rename.rs` restricted to **the anchor file only**; `oxc_semantic` added | new: `rename_cx.rs`, `0_rename.rs`, `lang/ts_rename.rs`, `tests/4_rename_ts.rs`, `tests/fixtures/ts_rename/local/`; edited: `types.rs` (append only), `lang/mod.rs` (roster), `Cargo.toml`, `src/bin/extract.rs` (verb) | **byte-exact against a hand rename.** `tests/fixtures/ts_rename/local/` ships `before/` and `after/`, `after/` written by hand. `extract rename src/app.ts#oldName newName --commit` over a copy of `before/` must produce a tree `diff -rq` identical to `after/`, zero entries. Fail-first: with `TsSource` dropped from `renames()`, the test fails on "no rename arm for src/app.ts" |
| **2** | the stops. `Ambiguous` (two `const Foo` in one file, disambiguated by `--at`), `NotFound`, `Inexact`, `Dynamic` (`obj["Foo"]`, `import("./m").then(m => m.Foo)`) | `lang/ts_rename.rs`, `tests/4_rename_ts.rs`, four fixtures under `tests/fixtures/ts_rename/stops/` | each stop exits non-zero with its named message and the tree is byte-identical after (`diff -rq` vs the untouched copy, zero entries). No stop panics |
| **3** | TS cross-file. Importers found through `TsRehome::import_refs` (`ts_rehome.rs:191`); per importer, its `ImportSpecifier` for `old` located, its local binding re-walked; re-export relays enqueued (v1's BFS, `plan.rs:442`) with body usages added | `lang/ts_rename.rs`, `tests/fixtures/ts_rename/exports/` | fixture with 5 importers: one bare `import {Foo}`, one `import {Foo as Bar}` (only the `Foo` seat moves, `Bar` and its 3 body uses stay), one barrel re-export, one `export * from`, one that only mentions `"Foo"` in a string. Committed tree byte-identical to a hand-written `after/`. Second receipt: `npx tsc --noEmit` green on the committed tree |
| **4** | `--text-refs` for rename: `Rename::text_spellings` feeding `2_move_text.rs`'s scan through a shared entry point | `src/2_move_text.rs` (add a rename entry beside `report`), `lang/ts_rename.rs` | the arc-3 fixture's string mention and its `README.md` mention both appear as `text-ref` rows; both files byte-identical after `--commit` |
| **5** | rust impl. `use` trailing segments, `ExprPath` trailing segments, `ExprMethodCall::method` (`rust.rs:1533`), item idents (`rust.rs:266-292`); a glob `use m::*` importer is a `Dynamic` stop | new `src/lang/rust_rename.rs`, `tests/5_rename_rust.rs`; reuses `rust.rs`'s `build_line_starts` (`:56`) and `syn_span` (`:80`), already `pub(crate)` since PR #489 | **self-rename oracle, judged by rustc.** This crate copied to a temp dir with path deps re-aimed, `extract rename src/rename_cx.rs#RenameCx SymbolCx --commit`, then `cargo check --features cli` green. Carries `#[ignore]` with its measured time if it exceeds the 10s cap, the shape `tests/3_move_rust.rs` already uses |
| **6** | the SCIP verify leg. `--verify-scip <index.scip>` loads through `ScipSource::load` (`types.rs:1889`), maps occurrence ranges to bytes with `scip::byte_range` (`scip.rs:491`), and reports every plan span with no matching `DEFINITION`/`IMPORT`/`READ_ACCESS`/`WRITE_ACCESS` occurrence, and every such occurrence the plan missed | `src/0_rename.rs`, `tests/4_rename_ts.rs` | on the arc-3 fixture with a fresh `scip-typescript` index: zero disagreements in both directions. The flag reports; it never changes the plan |

Sequence: 1 -> 2 -> 3 -> 4; 5 and 6 are independent of each other once 3 lands.

**Ownership.** Arcs 1 through 4 and 6 touch no file the six move lanes own.
Arc 5 reads `lang/rust.rs`'s two `pub(crate)` helpers and writes only
`lang/rust_rename.rs`; it must not edit `lang/rust_rehome.rs` or
`tests/3_move_rust.rs`.

## Receipts

Every claim above, as a command.

```bash
cd v6/sprefa-extract

# the Resolve roster, 7 languages, 12 impls
git grep -n 'impl Resolve<' -- src/

# the Rehome roster, 3 languages
sed -n '90,92p' src/lang/mod.rs

# ProjectCx's file set and manifest map are hollow
sed -n '1415,1417p' src/types.rs
sed -n '160,161p' src/project.rs

# the reference-carrying rows carry no reference span
sed -n '325,332p' src/types.rs      # TypeEdgeCandidate: owner, to, kind
sed -n '264,271p' src/types.rs      # TypeSig: owner, slot, pos, ty
sed -n '521,536p' src/types.rs      # Specifier: span is the whole clause

# ts spans are declaration-covering / expression-covering
sed -n '1443,1446p' src/lang/ts.rs  # fn_call_def -> func.span
sed -n '1665,1668p' src/lang/ts.rs  # CallSite -> call.callee.span()
sed -n '1677,1681p' src/lang/ts.rs  # new-expression -> the whole new expr

# rust is the only identifier-exact seat set
sed -n '263,268p' src/lang/rust.rs  # TypeF struct -> s.ident.span()
sed -n '1531,1538p' src/lang/rust.rs # method call -> call.method.span()

# ts local `export {foo}` has no specifier row
sed -n '1239,1242p' src/lang/ts.rs

# the scip bridge and its roles
sed -n '491,495p' src/scip.rs
sed -n '1685,1694p' src/types.rs

# oxc_semantic costs one crate
cargo add --dry-run oxc_semantic@0.135
grep -A 20 '^\[dependencies\]' ~/.cargo/registry/src/*/oxc_semantic-0.135.0/Cargo.toml.orig
for c in itertools memchr rustc-hash self_cell smallvec oxc_index oxc_str \
         oxc_ecmascript oxc_diagnostics; do
  printf '%-16s %s\n' "$c" "$(grep -c "^name = \"$c\"\$" Cargo.lock)"
done   # every row prints 1

# v5 has no symbol rename
grep -rnc 'DeclChange\|plan_decl_rename\|decl_rename\|rename_through_reexport' \
  ~/projects/sprefa/src/ | grep -v ':0'   # no output
```

v1 sites, all in `~/projects/sprefa-archive-20260428`:
`crates/watch/src/plan.rs:106,271,289,442`, `crates/watch/src/diff.rs:11,20,32`,
`crates/watch/src/change.rs:38`, `crates/extract/src/lib.rs:9,10,16,17`.

v6 sites, all in `v6/sprefa-extract`: `src/types.rs:264,325,453,521,1292,1391,
1415,1417,1466,1665,1685,1736,1814,1889,1940,1952,1965,1976`,
`src/project.rs:152,160,185`, `src/move_cx.rs:26,33,37,45,108,121,134,140`,
`src/lang/mod.rs:68,90,97`, `src/0_move.rs:20,108,209`,
`src/2_move_text.rs:13,41`, `src/scip.rs:106,120,143,167,181,195,491,534`,
`src/scip_ensure.rs:105,343`, `src/lang/ts.rs:1183,1206,1239,1443,1665,1677,
1695,3147,3274,3324`, `src/lang/rust.rs:56,80,266,1066,1521,1533`,
`src/lang/go.rs:126,141,815`, `src/lang/kotlin.rs:144,159,781,802`,
`src/lang/prolog/_0_source.rs:160,182,664,925`,
`src/lang/dl6/_0_source.rs:114,202,339`,
`src/lang/markdown/_0_source.rs:262`, `src/lang/ts_rehome.rs:191`.

## Out of scope

- **Watcher-driven rename.** v1 fired on an fs event with no dry run
  (`crates/watch/src/plan.rs:106`). v6 keeps the explicit verb and the
  `--commit` gate, the same call
  `plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md:44` makes for move.
- **Renaming a file and a symbol in one run.** `extract move` and
  `extract rename` stage separately. A batch doing both is two invocations,
  move first, and nothing in this design makes a combined verb harder later.
- **Kotlin, go, dl6, markdown, python impls.** The trait admits them. The
  [span table](#per-language-span-table) says go and kotlin need a
  token-relocation step inside their whole-node spans, the same step the
  prolog arm needs; none is requested.
- **Rewriting text carriers.** `--text-refs` stays report-only, matching
  `2_move_text.rs:3` and
  `plans/2026-08-25-extract-move-typescript.PLAN.md:642-644`.
- **Renaming a field, a method on an interface, or an enum variant.** The
  first arcs rename a module-scope binding. A member rename needs receiver
  typing, which the `Resolve<CallF>` arms explicitly put out of scope
  (`types.rs:427-430`, "Receiver typing is OUT OF SCOPE"). It is a later arc
  with its own plan.
- **`Resolve<F>` growing reference spans.** Adding an `at: Span` seat to
  `TypeEdgeCandidate` and `TypeSig` would make the resolved edge plane answer
  rename questions directly. That is a change to the extractor's row shapes
  and every golden that pins them, for a verb that reaches the same answer
  from its own parse. Named, not proposed.
