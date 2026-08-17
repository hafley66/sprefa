# boop-review

## Goal
Review boop (`~/projects/hafley-rs/crates/boop`, plus `crates/boop-mux` and
`crates/soopy` where boop calls into them) for: CLI ergonomics, redundancy,
breaks in trait math (traits that leak, impls that bypass their trait, dyn
where generics or the reverse), bad wiring (data reaching a call site by a
side door: globals, re-parsed strings, env vars, re-read files), and cleanup
opportunities in the types and the command flow. AUDIT ONLY: no code changes.
Findings become a doc + cards.

## Facts at spawn (verify)
- origin/main 88dd100. `main.rs` 5791 lines, `ident.rs` 3355, `runtime.rs`
  1609, `concatmap.rs` 1390; 26183 lines total in `crates/boop/src`.
- Three traits: `proc.rs:26 ProcReader`, `channel.rs:101 LaneChannel`,
  `harness.rs:15 Harness`. Four harness impls under `src/harness/`
  (opencode, codex, claude, kimi), three channels under `src/channel/`.
- clap derive CLI in `main.rs`; the store is SQLite `~/.agent/boop.db`
  (journal_mode=delete, 380MB, card boop-db-wal-lock).
- Standing laws in `~/projects/sprefa/CLAUDE.md`: boop never reinvents SQLite
  or SQL (no query-flag DSLs; canned reports are named SQL); infra is bought;
  self-diagnosis from the on-disk trail; nothing seizes the machine; 10-second
  law; eprintln banned in src (tracing only, `@eprintln-ok` waiver).
- Recent PRs #12-#15 (lane trail, respawn brief, install rail, native mail
  hooks): read their diffs (`gh pr view N --patch` or git log) for the newest
  seams.
- Open boop cards in `~/projects/sprefa/issues/boop-*/item.md`: db-wal-lock,
  doa-lane-carcass, spawn-flake-cluster, parent-death-cascade,
  parent-broadcast-easy-tell. Read them; do not duplicate.

## Method
1. CLI surface: `boop --help` recursively (`cargo run -q -- <sub> --help`,
   or the installed `~/.cargo/bin/boop`). Table every subcommand: verb /
   args / what it reads / what it writes / who calls it (human, hook,
   coordinator, lane). Mark synonyms, dead verbs, verbs whose flags encode
   SQL that `boop db "<sql>"` already answers, and verbs that need 3+ flags
   for the common case.
2. Trait math: for each trait, table impls, every call site that takes the
   concrete type instead of the trait, every method never called through the
   trait, and every `match harness_kind` that should be a trait method.
3. Wiring: grep for `std::env::var`, `read_to_string`, `parse::<`, `split(`
   on things that were typed upstream; `unwrap()`/`expect(` on IO; `Command::new`
   sites and whether they go through one spawn seam; every place the same
   SQL statement text appears twice; every place a lane row is reconstructed
   from tmux/pane text rather than the store.
4. Type cleanup: structs with 8+ fields, `String` fields that are ids/paths/
   kinds (should be newtypes or enums), `Option<Option<>>`, bool parameters,
   duplicated shape between `rows.rs`, `event.rs`, `lane.rs`, `registry.rs`.
5. Flow: one mermaid sequenceDiagram for `lane create` -> spawn -> supervise
   -> trail -> mail -> coordinator drain, with the file:fn at each hop; a
   second for `inbox drain --hook`. Mark hops where state crosses a side door.
6. main.rs size: propose a split by subcommand family, table of
   proposed module / verbs / lines moved.

## Deliverables (branch audit/boop-review in hafley-rs, open PR, do not merge)
1. `crates/boop/docs/audit-2026-08-17.md` (create dir if absent): TOC; CLI
   table; trait table; wiring findings; type findings; two sequence diagrams;
   main.rs split proposal; ranked findings (finding / path:line / cost S/M/L /
   needs Chris y/n); cards to file (slug + one line).
2. `crates/boop/docs/audit-2026-08-17.visual.human.unga.md`: plain words,
   mermaid, zero citations, one page.
Style: no em dashes; banned words provenance substrate load-bearing regime
refusal honest*; tables over prose; excerpts under 15 lines.
`cargo test -p boop -j4` once, report the count; nothing else heavy.
