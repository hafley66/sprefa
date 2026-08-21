# v5 retirement

Measured 2026-08-21 at `6967750a7`. Companion: `docs/v5-extraction-parity.md`,
`docs/v5-rail-census.md`. Plain-words twin with the diagrams:
`plans/2026-08-21-v5-retirement.PLAN.visual.human.unga.md`.

## Contents

- [The finding](#the-finding)
- [What already stopped working](#what-already-stopped-working)
- [What must exist before `src/` moves](#what-must-exist-before-src-moves)
- [The order](#the-order)
- [The deletion commit's shape](#the-deletion-commits-shape)
- [The day v5 stops building](#the-day-v5-stops-building)
- [What is deliberately NOT ported](#what-is-deliberately-not-ported)

## The finding

**Exactly one live green gate still runs the v5 binary.**

```
v6/tsv2/goldens/multirepo_crawl/2_gate.sh:134
    local release="$REPO/target/release/dl"
    if [ ! -x "$release" ]; then
      fail "no v5 dl binary at $release. A gate does not build; run:
        cd $REPO && cargo build --release --bin dl"
```

That is `just multirepo-golden`, a leg of `green-all`
(`v6/tools/green-parallel.sh:34`) and NOT on the known-red allowlist
(`.github/CI-KNOWN-RED.md:142-159`), so it is expected green and it hard-fails
without the v5 binary.

Everything else that reaches v5 is already dead, already allowed-red, or a
developer convenience with no gate behind it. The retirement is one gate away.

## What already stopped working

Four things believed to be live v5 dependencies are not.

| thing | believed | measured | receipt |
|---|---|---|---|
| CI v5 gates | removed 2026-08-11 | correct, and nothing crept back | `grep -rn 'v5\|bin dl' .github/workflows/*.yml` returns one comment line, `ci.yml:3` |
| diagnostics through v5 (`v6/tools/lsp-v5-bridge.sh`) | "the only v6 editor feature that reaches an editor through v5" (CLAUDE.md) | **broken**: the script's v6 half is `v6/dl/src/5_diag.ts` and `v6/dl/src/main.ts`; `v6/dl/src/` does not exist | `ls v6/dl/src` -> No such file or directory |
| `just flagship` (the v5-vs-v6 callgraph receipt) | the headline parity receipt | **red and allowed**, because its v5 golden went stale and regenerating it runs the v5 binary, which the user forbade | `.github/CI-KNOWN-RED.md:115`, `allow: flagship` at `:146` |
| `lsp-diags`, `flagship-flow` | gate legs | already deleted for the same reason | `.github/CI-KNOWN-RED.md:39-41` |

So the retirement's cost is not "lose diagnostics". Diagnostics through v5
stopped working when the old TypeScript v6 server was deleted and nobody
noticed. That is the whole editor cost, and it is already paid.

## What must exist before `src/` moves

Three lists. Only the first is a blocker.

### Blocking: the one live gate

| # | thing | why it blocks | how it clears |
|---|---|---|---|
| B1 | `just multirepo-golden` runs `target/release/dl` | it is in `green-all` and expected green | the v5 side is a CHECKED-IN golden under `v6/tsv2/goldens/multirepo_crawl/v5_golden/` (`2_gate.sh:5-6`). Pin the golden by content digest, delete `resolve_v5_bin`, and the gate compares v6 against a committed file with no binary in the loop |

That is the entire blocking list.

### Not blocking, but owed: the door gaps

The three gaps in `docs/v5-extraction-parity.md`. None of them keeps v5 alive;
they decide whether the 144 blocked `.dl` rails are ever REWRITTEN or simply
archived as reference text.

| # | gap | rails it unblocks | issue |
|---|---|---|---|
| D1 | no text plane and no ast-grep pattern door from dl6 | 68 | `@dl6-no-text-extraction-door` |
| D2 | the resolve arm (`--resolve`) unreachable from dl6 | 66 | `@dl6-scip-facts-door` |
| D3 | `--deps` / `--package-deps` / `--family cfg` / `--scip-facts` unreachable from dl6 | 27 | `@dl6-deps-package-door`, `@dl6-cfg-family-unlinked` |

D1 and D3 are wiring: the records exist, are tested, and reach the CLI. D1's
`--ast-pattern` arm is three lines in `SprefaExtractExecutor::run`
(`v6/sprefa-engine-rs/src/hosts.rs:890-908`). D3's `cfg` arm is one match arm
(`hosts.rs:936-951`). D2 needs a host name and an input contract.

Only `ast_yaml` needs a language-design call (`sg_pattern/3` is
`refuse(slot_sg_metavariable_semantics)`, `registry.pl:199`), and that is
Chris's, not a lane's.

### Not blocking, not owed: the delete list

Seven planes v5 had that v6 will not have, with zero live consumers between
them. Named so nobody re-opens them as gaps.

| plane | v5 rels | last `.dl` touch | verdict |
|---|---|---|---|
| embeddings (`similar`, `node2vec`) | `rels/embed.rs:29` | 2026-07-02 | delete |
| refactor proposals (`propose_extract`, `propose_clone`) | `rels/propose.rs:28,108` | 2026-07-09 | delete |
| type anti-unification (`type_shape`, `type_lgg`) | `rels/analysis.rs:364,426` | 2026-07-09 | delete |
| drawable graph sinks (`graph_node`, `graph_edge`) | `decls.rs:314,320` | 2026-07-20 | delete; there is no v6 flow panel |
| harness ingest (`agent_edit`, `agent_touch`, `skill_loaded`, `hook_event`) | `rels/analysis.rs:26-34`, `decls.rs:454` | 2026-07-09 | delete; boop owns the harness trail |
| v5's self-catalog (`rel_catalog`, `rel_col`, `fn_catalog`, `op_catalog`, `verb_catalog`) | `rels/catalog.rs` | 2026-07-20 | delete; v6's equivalent is compile-time |
| first-author attribution (`created`) | `rels/git.rs:494` | 2026-07-01 | delete; its only rail is dead |

## The order

Nine steps. Each names its gate and its owner. Steps 1 to 4 can land in one PR.

| # | step | gate | owner |
|---|---|---|---|
| 1 | pin `multirepo-golden` to its checked-in v5 golden by content digest; delete `resolve_v5_bin` from `2_gate.sh` | `just multirepo-golden` green with `target/release/dl` absent | a lane |
| 2 | delete `just flagship` and `v6/tsv2/scripts/flagship-callgraph.sh`; drop `allow: flagship` from `.github/CI-KNOWN-RED.md` and the leg from `green-parallel.sh:36` | `just green-all` has one fewer red leg and the allowlist shrinks | a lane |
| 3 | delete `v6/tools/lsp-v5-bridge.sh` (its v6 half no longer exists) | `grep -rn lsp-v5-bridge v6/` returns nothing | a lane |
| 4 | delete the three tsv2-door v5 scripts: `v5-parity.sh`, `comment-parity.sh`, `crawl-bench.sh`; delete `just v5-parity` and the second half of `just comment-rails`; drop the `check_binary target/release/dl` line from `v6/tools/staleness-gate.sh:130` | `bash v6/tools/staleness-gate.sh` prints `STALENESS_GATE_OK`; `just comment-rails` green on its first half | a lane |
| 5 | **HOLD FOR CHRIS**: confirm the 12 root-`justfile` recipes are conveniences nobody runs, then delete the root `justfile` | none: it is the last consumer | Chris |
| 6 | move `src/`, `tests/`, `examples/`, `.dl/`, `std/`, `bench/`, `deck/`, `assets/`, `editors/`, `vendor/`, `tree-sitter-dl/`, `build.rs`, `install*.sh`, `dist-workspace.toml`, `.github/workflows/release.yml` to `~/projects/sprefa-archive-20260821` | `cargo metadata` at the repo root resolves with no `sprefa-dl` package; `just green-all` failing set unchanged from the pre-move measurement | a lane |
| 7 | rewrite the root `Cargo.toml` so the repository root is no longer a cargo package | `cargo metadata --no-deps` at the root exits 0 | same lane as 6 |
| 8 | delete the four v5 agent/skill definitions: `.agents/agents/{builtin-rel-implementer,extraction-op-implementer,magic-rel-auditor}.md`, `.claude/skills/{sprefa-dl,sprefa-v5-new-builtin-rel,sprefa-v5-new-extraction-op,sprefa-v5-no-magic-rels,sprefa-v5-working-conventions,sprefa-flow-panel-layers}` | no skill or agent names a path under `src/` | a lane |
| 9 | rewrite the CLAUDE.md rows that name v5 as live: the "one editor feature" line, the "sprefa-extract has no markdown extractor" line (stale), and the archive paths | the file's own rule holds: every claim is a decision, a `path:line`, or a command | Chris or a lane |

Steps 1 to 4 remove every automated consumer. Step 5 is the only decision left.

## The deletion commit's shape

Step 6 and 7, in one commit. Measured sizes.

| target | what | size |
|---|---|---|
| `src/` | the v5 engine | 200 files, 106,269 lines |
| `tests/` | the v5 test estate | 58,502 lines |
| `examples/` | 132 `.dl` rails + 6 `.rs` examples | 940 KB |
| `.dl/` | 28 `.dl` rails | 272 KB |
| `bench/` | 16 `.dl` + corpora | 676 KB |
| `std/` | 9 `.dl` library rails | 72 KB |
| `deck/`, `assets/` | 8 + 1 `.dl` snippets | 132 KB |
| `editors/` | the vscode extension + claude plugin | 2.3 MB |
| `vendor/`, `tree-sitter-dl/`, `build.rs` | grammars the v5 engine cc-compiles | 896 KB |
| `install.sh`, `install-dl.sh`, `dist-workspace.toml`, `.github/workflows/release.yml` | the `sprefa-dl` release path | — |

The root `Cargo.toml` edits, by line:

| line | today | after |
|---|---|---|
| `Cargo.toml:9` | `members = ["tree-sitter-dl"]` | the member is gone; the file stops being a workspace root or names v6 members |
| `Cargo.toml:12-35` | `[package] name = "sprefa-dl"` | deleted |
| `Cargo.toml:53-58` | `[lib] name = "sprefa_v5"` | deleted |
| `Cargo.toml:64-66` | `[[bin]] name = "dl"` | deleted |
| `Cargo.toml:68-70` | `[[test]] name = "it"` | deleted |
| `Cargo.toml:72-80` | `[features]` embed backends | deleted |
| `Cargo.toml:46-51` | `[lints.clippy]` baselines | deleted |

`v6/justfile`, 11 lines name v5 or `target/release/dl`
(`grep -n 'v5\|release/dl' v6/justfile`): lines 188, 191, 204, 312, 346, 359,
380, 383, 387, 388, 391. Steps 2 and 4 remove all of them.

Kept at the root, unchanged: `v6/`, `issues/`, `plans/`, `chat_log/`, `docs/`,
`.github/workflows/{ci,release-dl6}.yml`, `scripts/`, `tools/`, `labs/`,
`research/`, `proofs/`, `book/`, `books/`, `anim/`, `sprefa-lanes/`,
`archive/`, `TASKS/`, `README.md`, `CLAUDE.md`, `AGENTS.md`.

## The day v5 stops building

**The day step 1 lands.**

After `multirepo-golden` stops resolving `target/release/dl`, nothing automated
builds or runs v5. Steps 2 to 4 are cleanup of already-dead paths; step 5 is a
human decision about a convenience file; step 6 is disk.

There is no port that has to finish first. The rail census's 144 blocked files
are blocked on v6 door gaps, and every one of those gaps is a v6 change that
does not touch `src/`. Holding v5's build hostage to them buys nothing: a rail
nobody can run in v6 is equally unrunnable whether or not a v5 binary that
nobody invokes still compiles.

Recommended: land steps 1 to 4 as one PR this week, take step 5 to Chris in the
same PR body, and let steps 6 to 9 follow on his word.

## What is deliberately NOT ported

Stated so a future census does not re-open them.

| v5 thing | why not |
|---|---|
| `cmd` op (shell out per file) | user decision 2026-08-21, "Zero shell in the engine". `ShellExecutor` is deleted; `LINKED_EXECUTORS` at `hosts.rs:40` is the whole roster |
| the flow panel and its `graph_node`/`graph_edge` sinks | no v6 flow panel exists and none is planned |
| `similar` / `node2vec` | no embedding plane in v6; no asker since 2026-07-02 |
| `propose_extract` / `propose_clone` | soopy owns mutation now, and it stages real edits rather than proposing spans |
| `type_shape` / `type_lgg` | v6 has a real type plane (`v6/prolog/compile/0_type_plane.pl`); anti-unification over shape hashes was v5's substitute for it |
| v5's self-describing catalogs | v6 answers the same question at compile time from `registry.pl` and `compile/out/manifest.json` |
| the 12 dead `.dl` files | nothing has named them in seven weeks; see `docs/v5-rail-census.md` section 4 |
| `plans/2026-07-30-v5-parity-table.tsv` | derived through the paused TypeScript door and not regenerable; superseded by `docs/v5-extraction-parity.md` |
