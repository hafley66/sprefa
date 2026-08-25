---
created: 2026-08-24
updated: 2026-08-24
type: bug
status: fixed
priority: high
closed: 2026-08-24
commits:
- hash: 0f721ae21
  summary: text identity compare reads characters
---

# text == literal on a computed value compares through __str ids and NULL-matches

## Description

Found 2026-08-24 in v6/dl/prolog_graph/import_graph.dl6. Rule: `First := substr(Raw, 1, 1), First == "'"`. Emitted SQL (cache .rs): `(SELECT __id FROM __str WHERE content = substr(<raw>, 1, 1)) IS (SELECT __id FROM __str WHERE content = '''')`. When the substring is not an interned string the left subquery is NULL; when the literal was never interned the right is NULL; `NULL IS NULL` is TRUE, so the quoted arm fired for every bare atom (`unquoted(analyze) = nalyz`) and the `\\==` arm never fired. Same shape on `Head := substr(Rest0,1,3), Head == "../"`. A probe over facts (scratch q.dl6) passed because its literals were interned. Expected: identity comparison on text compares CONTENT (or interns computed texts before the compare). Workaround used: integer `instr(...) == 1`. Rail: a conformance fixture comparing a substr result against a literal the program never states as a fact.
