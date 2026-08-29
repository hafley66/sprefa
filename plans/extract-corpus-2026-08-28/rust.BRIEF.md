# Brief: sprefa-extract corpus battery, language = rust

Read `plans/extract-corpus-2026-08-28/COMMON.md` FIRST and follow it exactly.
Your lane name: `chore-extract-corpus-rust`.

## Your language and arm
- Language: **rust**, file glob `*.rs`.
- Arm you own (the ONLY src files you may edit): `v6/sprefa-extract/src/lang/rust.rs`, `rust_rehome.rs`, `rust_rename.rs`
- Tests you may add: `v6/sprefa-extract/tests/*rust*.rs` and
  `v6/sprefa-extract/tests/fixtures/rust/corpus_*.rs`.

## Corpus (read-only, never modify)
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` (2506 crates, 3.0G). Step 1 over ALL `.rs` files. Steps 3-4 over the 300 largest crates by file count. Step 5 (`scip`, rust-analyzer is installed at `~/.cargo/bin/rust-analyzer`) on `v6/sprefa-extract`, `v6/sprefa-engine-rs`, and one registry crate with a Cargo.toml (copy it to scratch first so the cache stays untouched).

## Scratch dir for logs and TSVs before commit
`/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust` (create it).

Extra rust checks: macro-heavy crates (`serde_derive`, `tokio-macros`, `syn`), `include!`, `#[cfg]`-gated mods, `mod.rs` vs `foo.rs` scope owner (see `tests/30_rust_mod_scope_owner.rs`), trait impl methods as callees, generic turbofish calls, closures.

## Sample commands
```
X=$PWD/v6/sprefa-extract/target/release/extract
find <ROOT> -name '*.rs' -type f > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/files.txt
wc -l /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/files.txt
nohup bash -c 'while read f; do s=$(date +%s%N); out=$(timeout 10 $X "$f" 2>/private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/err.tmp); rc=$?; e=$(( ($(date +%s%N)-s)/1000000 )); printf "%s\t%s\t%s\t%s\t%s\n" "$f" $rc $e $(printf "%s" "$out" | wc -l) "$(head -1 /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/err.tmp)"; done < /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/files.txt' > /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/efe90f72-55fb-4958-b159-4982236661fe/scratchpad/rust/runs.tsv 2>&1 &
```
Adapt for parallelism (split files.txt into 8 chunks, one loop each).
