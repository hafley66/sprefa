# Brief: option B, core + per-language extension variant, for the family kind enums

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-leaky-types-review.PLAN.md` rows 2, 3, 4, 5 and the
visual twin. User decision (Chris, 2026-08-27): **option B**. The shared
core of each kind enum stays a closed, exhaustive enum in `types.rs`; a
language that needs a kind the core lacks adds it through ONE extension
variant it owns, never by editing the core list. Nobody matches per
language anywhere; the language files only construct.

## First action
```bash
git merge --ff-only 946460d75   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/types.rs` (the three enums + their tag fns only)
- `v6/sprefa-extract/src/lang/*.rs`, `src/lang/**/_0_source.rs` (constructions only)
- `src/lang/extract_lang.rs`
- `src/wire.rs` only if a tag fn signature forces it
- `tests/**` as needed; new `tests/6_kind_vocab.rs`
- new issue: `issuectl new -t improvement --slug kind-vocab-core-ext --title "extract: family kind enums, core + per-language extension (option B)" -a chris -p normal -l extract -l refactor --description "leaky-types review rows 2-5; user decision option B 2026-08-27"`; tick it as its own commit.
FORBIDDEN: `src/0_move.rs`, `src/move_*.rs`, `src/lang/*_rehome.rs`, `src/lang/prolog/_1_rehome.rs`, `src/scip*.rs`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`.

## The shape (signatures first)
```rust
// types.rs, for each of DfNodeKind (:784), TypeEntityKind (:203), CallKind (:405)
pub enum DfNodeKind {
    // ...every core variant that at least TWO languages construct today, unchanged...
    /// A kind one language owns. The tag is the language's own snake_case
    /// string; it never collides with a core tag (assert in tests).
    Ext(LangKind),
}
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LangKind { pub lang: &'static str, pub tag: &'static str }
impl DfNodeKind { pub fn as_str(self) -> &'static str /* core table + Ext => tag */ }
```
- Measure first, per enum: for every variant, which language files construct it (`git grep -n 'DfNodeKind::<V>' -- src/lang`). A variant constructed by exactly ONE language leaves the core and becomes that language's `Ext` constant (`pub const BORROW: DfNodeKind = DfNodeKind::Ext(LangKind { lang: "rust", tag: "borrow" })` in that language's file). Put the measured table in the PR body; the core list after the change is the receipt.
- Wire tags are byte-identical for every kind that exists today: `as_str` returns the same string before and after. Golden: run `extract` on `tests/fixtures/**` before and after and `diff` the JSONL; zero bytes differ.
- `TypeEntityKind`'s doc already names `Interface` TS-only and `Struct`/`Trait` Rust-only; if the measurement agrees, they move.
- `ExtractLang` (row 2): `from_path`, `name`, `parse_name` are per-language match arms. Replace with the `Source` roster (`lang/mod.rs:66 sources()`, `source_for`): each `Source` answers `name()` already; add `fn extract_lang(&self) -> ExtractLang` where the ast-grep shim needs a `SupportLang`, so the enum shrinks to the ast-grep bridge (`Sg(SupportLang)` + the tree-sitter arms) and no function switches on language names outside the roster. If a call site cannot be routed through the roster, STOP and hail with the line.

## Fail-first tests (`tests/6_kind_vocab.rs`)
1. `no_ext_tag_collides_with_a_core_tag` (build the full set from every language's constants).
2. `as_str_is_byte_stable_for_every_kind` (a golden list of every tag string that existed at 946460d75, checked in).
3. `a_single_language_kind_lives_in_its_language_file`: grep-based rail: `types.rs` contains no variant name that appears in only one `lang/` file.
4. `extract_lang_has_no_path_switch`: `from_path` is gone or delegates to `source_for`.
5. The JSONL golden diff above, as a test over `tests/fixtures/**` (cap 10 s; `#[ignore]` with measured time if over, run once by hand for the PR body).

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): full battery 0 failures; `6_kind_vocab` count.
- `git grep -n 'match .*ExtractLang\|ExtractLang::[A-Z][a-z]* =>' v6/sprefa-extract/src` prints only the ast-grep bridge in `extract_lang.rs`.
- The per-enum measurement table and the before/after core lists.
- `git diff 946460d75 --stat` shows only owned files; `cargo fmt`; no `eprintln!` in `src/**`; 10-second law.
- `bash v6/sprefa-engine-rs/grade.sh` unchanged versus a run on 946460d75 (the engine reads the wire tags): paste both counts.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Descriptive identifiers. Issue tick as its own commit.

## Delivery
One PR against `origin/main`, title `extract: kind enums core + per-language extension (option B)`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, core list sizes before/after>"`.
