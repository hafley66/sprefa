---
created: 2026-08-25
updated: 2026-08-25
type: task
reporter: hafley66
status: testing
priority: high
epic: extract-astgrep-soopy
labels:
- pkg:extract
---

# Arc A: ExtractLang implements ast-grep Language for dl6, prolog, markdown

## Description

Plan section: PLAN.md '## Arc A'. ExtractLang enum wrapping SupportLang plus Dl6/Prolog/Md variants, delegating Language + LanguageExt; expando_char 'µ' for the three (dl6 $X is a language hole, parse_dl_dcg.pl:1688). Owns: src/lang/extract_lang.rs (new), src/lang/mod.rs, src/lang/astgrep.rs, src/lang/1_ast_rule.rs, tree-sitter corpus tests. Forbidden: src/0_move.rs, src/drain.rs, Cargo.toml deps beyond what the plan names. Gate: cargo test --release --features cli in v6/sprefa-extract, plus the plan's Arc A tests.
