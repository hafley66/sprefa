---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:large
blocked_by: ['@extract-modulef-collapse']
---

# module plane beyond TypeScript

## Description

## Description

The module plane exists for TypeScript only. v5 resolves modules for five
languages and emits ten module relations; v6 emits `Specifier` rows from the ts
front-end alone and resolves them with a ts-only diet resolver.

## Receipts

| fact | receipt |
|---|---|
| v5 rels | `src/engine/family/mod.rs:397-408` `MODULE_RELS` (module_import, module_edge(+_rev), module_unresolved(+_rev), crate_edge, module_binding x4) |
| v5 resolvers | `src/graph/modgraph/{rust,ts,go,kotlin,python}.rs`, contract at `src/graph/modgraph/mod.rs:1-15` |
| v6 diet resolver is ts-only, and says so | `v6/sprefa-extract/src/deps.rs:1-2`, extension table `:57-70` |
| only ts emits specifiers | `grep -rn Specifier v6/sprefa-extract/src/lang/` hits `lang/ts.rs` only (`:892-1010`) |
| v6 has no ModuleF | `v6/sprefa-extract/src/types.rs:629-645` (collapsed, commented out) |
| `crate_edge` has no v6 analog | no hit for `crate_edge` anywhere under `v6/sprefa-extract/src` |

## Fork, for the record

Whether the module plane becomes a family or stays specifier-rows-plus-a-
resolver is @extract-modulef-collapse, which is Chris's call. THIS issue is the
per-language SPECIFIER EMISSION and diet resolution, which is the same shape
either way: rust `use`/`mod`/`#[path]`, go imports, kotlin package/import,
python import/from-import become `CallFAux.specifiers` rows
(`src/types.rs:497-503`), and `deps.rs` grows a per-language resolution policy
beside the ts one.

Size is large; split per language when dispatching, rust first (v5's
`src/graph/modgraph/rust.rs` is the one with an oracle test,
`tests/oracle_rust.rs`).

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 7_diet_deps_cli
```

## Comments

### 2026-08-17T03:08:12Z · @extract-driver

RUST SLICE DISPATCHED by extract-driver 2026-08-16: lane feature-extract-module-plane-rust (codex/luna@medium), base origin/main 4531b4297, brief TASKS/extract-module-plane-rust.BRIEF.md. Card stays OPEN; this is one of the per-language splits the card body asks for, and go/kotlin/python remain undispatched. SCOPE BOUNDARY, set by sprefa-coordinator: src/deps.rs is the soopy driver's file (it falls under the resolve/crawl assignment and that driver is concurrently wiring extract-flow-cli-dispatch through it), so this slice emits specifier ROWS ONLY and lands no resolution policy. Rust specifiers therefore stay unresolved by deps.rs after this slice, by design. FILE SET: src/lang/rust.rs plus new tests/fixtures/rust_modules/sample.rs and tests/24_rust_specifiers.rs. types.rs, schema.rs and wire.rs are forbidden to the lane because Specifier (types.rs:489-495), the closed SpecifierKind vocabulary (types.rs:528-546) and the record=specifier schema line (schema.rs:36) already exist and are language-neutral. ONE OPEN VOCABULARY QUESTION, surfaced to Chris via sprefa-coordinator: rust 'mod foo;' has no SpecifierKind spelling (v5 uses kind="mod" on a different record shape, src/graph/modgraph/rust.rs:65,92). Interim mapping is Named with name=foo and module=foo; the lane is banned from adding a variant. NOTE ON THE PORT: v5's RustResolver is REGEX over stripped text (src/graph/modgraph/rust.rs:15-18); v6's rust arm already parses with syn, so the lane ports v5's SEMANTICS (pub-use = reexport at :20-24 and :134, glob and self leaves bind no single name at :120-126) onto syn::ItemUse / syn::ItemMod rather than porting the regexes.

### 2026-08-17T03:30:01Z · @extract-driver

RUST SLICE LANDED: PR #328 merged to origin/main as 55adcb6a9 (commits e0d839de2 emitter, 47baa3b5e fixture+test). Graded green by extract-driver independently, not taken on the lane's word. Verification performed: (1) file scope is EXACTLY the three owned files, 302 insertions / 2 deletions, worktree clean, zero fmt churn; (2) src/types.rs diff is 0 lines and src/deps.rs is untouched, so both brief section-5 prohibitions held and no SpecifierKind variant was added; (3) all 14 span.start offsets were RECOMPUTED independently with a python byte-offset probe over the fixture and every one matched (167/184/210/217/245/399/416/433/463/479/501/514/527/585); (4) gates run by me in the lane worktree: cargo build --all-targets --features cli rc=0, cargo test --features cli = 33 binaries / 133 passed / 0 failed, tests/24_rust_specifiers.rs run THREE times 2-passed each, golden_parity 9 passed, 4_capability_parity 2 passed, 7_diet_deps_cli 7 passed; (5) style rails on added lines: 0 em dashes, 0 eprintln in src, 0 banned words, 0 single-letter bindings. The lane also PROVED rather than assumed the two traps flagged at resume: syn accepts 'use alpha::eta::self;' (throwaway probe, deleted after) so the fixture genuinely parses, and the Glob empty-prefix early return is live code because syn parses 'use *;' even though rustc rejects it, yielding 0 rows. One rotted citation the lane fixed in e0d839de2: its own comment cited src/graph/modgraph/rust.rs:66,93 for the v5 'mod' kind; the real lines are :65,92. CARD STAYS OPEN: go, kotlin and python specifier emission are still undispatched, and the deps.rs per-language resolution policy remains out of scope under the soopy driver.

### 2026-08-17T03:49:25Z · @extract-driver

GO SLICE DISPATCHED 2026-08-17: lane feature-extract-module-plane-go, flash4 (openrouter deepseek-v4-flash-0731, opencode harness) now that boop is rebuilt (hafley-rs #10, brief goes in as a paste; failure-modes 52 records the one-keystroke-at-a-time pane feed that killed the earlier codex spawns). Base origin/main 7f11724b4. Brief TASKS/extract-module-plane-go.BRIEF.md, written flash4-grade: every grammar node kind and every expected byte offset measured by extract-driver up front rather than left to the lane. GRAMMAR MEASURED, not guessed: I ran the real extractor cst plane over a probe go file. An import_declaration holds either one import_spec directly (single-line form) or an import_spec_list of them (block form). An import_spec carries an OPTIONAL leading name node of kind package_identifier (alias), blank_identifier (the _ form) or dot (the . form), plus a required interpreted_string_literal whose interpreted_string_literal_content child gives the path without quotes. Row span is the import_spec node's own span. MAPPING, following the decision ALREADY RECORDED at src/types.rs:485-492 ('name is the specifier text as written, the bound name; the module path for path-only forms like go's imports' and 'None is for the languages that emit specifiers with the module already in name, go's path-only imports'): plain import and block import to Named/name=path/module=None; aliased to Named/name=alias/module=Some(path), the only form that sets module because it is the only one where the path would otherwise be lost; blank _ import to SideEffect/name=path/module=None; dot import to Namespace/name=path/module=None. Default and Reexport unreachable from go. No SpecifierKind variant added. v5 parses _ and . in the same slot as an alias, src/graph/modgraph/go.rs:37-43 and :53-59, capture group (_|\.|\w+). EXPECTED ROWS given to the lane with offsets I computed from the fixture bytes independently of the extractor, and the brief forbids editing them to match output: (named,fmt,None,142) (named,os,None,159) (named,alias,Some(path/filepath),165) (side_effect,embed,None,188) (namespace,strings,None,199). FILE SET: src/lang/go.rs plus new tests/fixtures/go_modules/sample.go and tests/25_go_specifiers.rs. types.rs, schema.rs, wire.rs, deps.rs all forbidden because the slice needs none of them. Brief carries an explicit never-run-bare-cargo-fmt rule after the churn incident on PR #329.


