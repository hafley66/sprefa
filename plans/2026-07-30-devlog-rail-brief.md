# Devlog rail — brief (codex luna)

User ruling devlog_rail = approved_dogfood: "DOGFOOD DOCS." A dl6 program
reads the session ledgers and emits DEVLOG.md. Same rail class as self-map
(production rail, never graded against the oracle corpus).

## Sources (all prolog fact files, already structured)

- chat_log/*.pl — 7 session ledgers. Fact shapes seen today:
  lane_landed(Name, Gate, Story), in_flight(Name, Where, Story),
  finding(Name, Who, Story), answered(Name, Story),
  directive(Name, Story). Older files may vary — inventory them first and
  name any shape you skip.
- v6/prolog/ARCH.pl task/3 rows (state + comment) for cross-reference.

## The shape

ONE dl6 program: an sh host consults each .pl and emits JSONL facts
(swipl one-liner in the host template — the world door); decode/spread
into rels; derive per-day, per-category views; render markdown lines;
`group_concat(Line, '\n', Ordinal)` assembles; one write-file effect emits
DEVLOG.md. Text construction stays in dl6; only file I/O is host.

Output: single DEVLOG.md, newest session first, per-session sections:
landed lanes (name + one-line story), rulings answered, findings, open
in-flight at ledger close. Deterministic ordering (ordinal columns, never
hash order) — run-twice-identical is a receipt.

## Receipts required

- `just devlog` recipe; DEVLOG.md committed; run-twice-identical.
- The dl6-vs-glue split stated (self-map precedent); glue = named gap.
- Ledger-shape inventory: which fact shapes parsed, which skipped by name.
- Battery: conformance untouched, green exit 0 (report EPERM legs),
  staleness gate.

## Fences

- Touch: new devlog dl6 file + rail script + justfile recipe + DEVLOG.md.
- Do NOT touch: registry/parser/emitter (seq lane), self-map files
  (self-map lane), labs/**, chat_log/*.pl (READ ONLY — the ledgers are the
  coordinator's).
- Do NOT depend on seq (concurrent lane); use the cursor idiom or
  arithmetic ordinals.
- No-commit flow. STOP AND REPORT on blocked commands.
