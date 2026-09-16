# MAPPING.md — alloy/tsp concepts -> prolog constructs in this lab

Corpus: `alloy-core/SKILL.md` (426 lines) and `alloy-languages/SKILL.md`
(331 lines) from `~/projects/claude-research/skills_archive/`. The lab files
live in `v6/prolog/labs/alloy_semantics/`.

Status key: **mapped** = a prolog construct here implements it; **unmapped** =
no implementation here (either the skill names a concept this lab does not
carry, or the skills never introduce the concept).

| alloy / tsp concept | prolog construct here | skill cite | lab cite | status |
|---|---|---|---|---|
| Component tree (JSX) | term tree: `ts_file` / `rust_mod` / `ts_interface` / `rust_struct`, folded to text only at the end | alloy-core 21-42 (architecture), 174-191 (render pipeline) | 3_render.pl:38 (build per-file tree), 51-52, 62-63 (tree terms), 96-104 (fold) | mapped |
| refkey identity (`refkey(schemaObj)` stable per object) | symbol id atom `s_<table>`, derived from the table name | alloy-core 104-138 | 0_facts.pl:28-52 (table/column facts), 1_collect.pl:71-79 (base_decl), 109 (decl_table) | mapped |
| Binder: refkey -> declaration resolution, lazy and order-independent | collect derives `decl` and `ref` from the fact base; check guarantees every ref has exactly one decl | alloy-core 140-151 | 1_collect.pl:58-92 (collect of decl/ref), 2_check.pl:35-40 (check_unresolved) | mapped |
| Manifest: component declares a symbol in scope | `decl(Id, Kind, File)` entry | alloy-core 160-165 (Declaration component); alloy-languages 153-173 | 1_collect.pl:71-79 | mapped |
| Reference site: auto-generated import when ref crosses files | `import_needed` derived from `ref/2` only where FromFile \= ToFile; the import/use line is rendered from it, never hand-written | alloy-core 129-133, 199-227 (walkthrough); alloy-languages 51-66 (Reference.tsx) | 1_collect.pl:66-69, 92; 3_render.pl:44-48 (import_term), 83-94 | mapped |
| Name policy (TS/rust casing; element-keyed) | `rendered_name` + `words_pascal` (ts adds `Row` suffix, rust plain pascal) | alloy-languages 71-100 (name-policy.ts) | 1_collect.pl:99-107, 112-122 | mapped |
| Rust `use` paths are module-tree relative (`crate::`/`super::`) vs ts file-relative | `import_term`: rust `super::core::<Name>`, ts `./core` | alloy-languages 298 | 3_render.pl:83-94 | mapped |
| Binder diagnostics: unresolved refkey warning / `emitDiagnostic` | named refusal `codegen_refused/1` thrown by the check pass before any render | alloy-core 146-151, 416-427 | 2_check.pl:3-13, 35-75; run.pl:12-20 | mapped |
| Rust name-conflict rule: same-scope conflicts forbidden (vs ts auto-renames) | invariant 2: one rendered name per target file -> `codegen_refused(duplicate_name)` | alloy-languages 308 | 2_check.pl:46-57 | mapped |
| AppendZone / deferred teleport collection (gather from children, render at top) | collect's `forall`-then-`assertz` dynamic accumulation, consumed by render | alloy-core 262-279 | 1_collect.pl:58-69 | mapped |
| Reactive propagation: re-render a subtree when its tracked deps change (Vue effect graph) | not carried; swipl runs are one-shot, no reactive re-run | alloy-core 174-191, 69 (reactive contexts) | — | unmapped |
| Context: parent-chain walking via globalContext.owner, Provider/useContext | not carried; file identity and target config are explicit facts, no parent-chain lookup | alloy-core 79-102 | — | unmapped |
| Mono-morphization | not present in either skill file this lab read; no construct to map | none in alloy-core / alloy-languages | — | unmapped |

Summary: **10 mapped, 3 unmapped** (reactive re-render, context parent-chain,
mono-morphization). The three unmapped entries share a cause: they rely on
runtime reactivity/external state that a pure backward-chaining loader does
not model.
