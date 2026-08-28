---
created: 2026-08-27
updated: 2026-08-27
type: improvement
assignee: chris
status: done
priority: normal
labels: [extract, refactor]
closed: 2026-08-27
---

# extract: family kind enums, core + per-language extension (option B)

## Description

leaky-types review rows 2-5; user decision option B 2026-08-27

## Comments

### 2026-08-27T19:19:54Z · @chris

Option B landed: core enums in v6/sprefa-extract/src/types.rs carry Ext(LangKind); single-language kinds moved to lang files (rust.rs BORROW/BREAK/MATCH/BLOCK/TRAIT, ts.rs COND/CONCAT/TEMPLATE); ExtractLang::from_path routes through the Source roster. Wire byte-identical vs 946460d75; battery 316 passed / 0 failed; grade.sh 449/322 unchanged.
