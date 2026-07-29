# Struct-as-rows arc header (planner contract)

RULED 2026-07-29 morning (rulings.pl compound_storage = struct_as_rows):
declared struct values are rel rows referenced by content id; the inline
blob (json-vs-term-text double spelling) ends; decode/2 dissolves into
joins. Executes the types-as-rels lab design
(plans/2026-07-28-types-as-rels-verdict.md is the settled record; its
graded checks are named below and are REQUIREMENTS, not suggestions).

## The two edges the user asked worked out, and their answers

### Edge 1: the tick log must print values, never ids

Problem: resp stores body_id = 8f3a... ; a log line printing the id breaks
byte-grading against the oracle (which has real terms and no ids), breaks
cross-run diffs (counter ids are build-order dependent, lab check
counter_ids_order_dependent), and breaks every migration step (verdict Q6:
"if the log ever prints ids ... the fixture grade becomes unusable").

ANSWER (memoized rendering, write-once):
- The dictionary row for a struct value carries THREE columns per the
  surrogate-mate ruling plus one: semantic content hash, dense storage
  key, and rendered_text = the value's canonical JSON (sorted keys, no
  whitespace -- the ruled cross-target encoding).
- rendered_text is computed ONCE at intern time. Values are immutable and
  children intern before parents (the DAG theorem), so a parent's
  rendering is one concat over child rendered_texts. No recursion at read.
- Boundary reads of a ref(T) column JOIN T's dictionary and SELECT
  rendered_text. The existing boundarySql CASE renderer is the seam this
  replaces/extends.
- Oracle side: ticklog.pl already renders canonical JSON (json_ticklog
  ruling); byte identity follows.
- GRADE: the lab's rendered_text_stable_under_both_policies as a real
  fixture pair -- same values interned in two build orders produce
  byte-identical tick logs. Plus one migration fixture: a column flipped
  inline -> ref produces a byte-identical log (verdict Q6's exact claim).

### Edge 2: dictionary rels must not leak into boundary deltas

Problem: interning a new value inserts dictionary rows; if those print in
the tick log, the emitted log grows rels the oracle does not have.

ANSWER (storage plane is boundary-invisible):
- Dictionary tables are storage-plane internals like the frontier TEMP
  tables: no boundarySql, no tick-log entry, never named in a program.
- The log shows the VALUE plane only (ARCH callout: only deltas cross the
  coastline). __host_* rels keep printing because BOTH sides derive them
  (shared expansion); dictionaries exist on the emitted side only, which
  is exactly why they must be invisible.
- GRADE: a fixture asserting the tick log rel-name set is identical
  oracle-vs-emitted while sqlite contains the dictionary tables (assert
  via sqlite_master, the memory-soak stats seam shows how to read it).

## Settled by the lab, adopt not relitigate

- struct decl = rel + key(every content column); id = content id
  (content_hash over child SEMANTIC hashes; dense int is a storage mate
  only, never semantic identity -- surrogate-mate ruling).
- GC/domination = refCount support counting, COMPLETE on the value plane
  because interned graphs are DAGs by construction (verdict Q3: cascade
  dissolves; FK CASCADE is proven wrong + never emitted, finding 6).
- Per-column coexistence: relplan grows a third storage kind ref(Type)
  beside int|text (verdict Q6; analyze rel_column_types is the slot).
  Inline json1 remains ONLY for untyped json values.
- Lists = fixed-arity cons cells (amendment 1); list columns carry a list
  mode.
- Cycles refused on the value plane (content ids cannot express them);
  entity plane is OUT OF SCOPE this arc.

## Arc scope (oracle + compiler + fixtures SAME ARC, the A4 law)

1. Decl spelling for a struct rel and a ref column (colon types exist;
   `col: type_name` where type_name is a declared struct rel is the
   natural reading -- SLOT-SPELLING below if it fights the parser).
2. Oracle: engine.pl holds real terms already; it needs only the decl
   forms and the same refusals (cycle refusal, untyped-json boundary).
3. Compiler: intern-at-arrival (world rows arrive as canonical JSON,
   post-order intern children then parent), ref(Type) column plan,
   dictionary DDL + memoized rendered_text, boundary joins for
   rendering, decode/2 lowering AS JOINS (the construct may stay as
   sugar or dissolve -- SLOT-DECODE-SURFACE).
4. Retraction: parent row dies -> support subtract on children (rides P3
   refCount SQL); graded shared-child fixture from the lab re-run as a
   compiled fixture.
5. The 20 held json fixtures: regrade; the ghcacher stars normalization
   is the acceptance case. Span columns (extract's nested span) become
   int columns via a declared span struct or flat host columns --
   whichever the extraction decls express, stated in the report.

## Named slots (decide loudly, report, never silently)

- SLOT-SPELLING: ref column spelling if `col: struct_name` collides with
  primitive type names in the parser.
- SLOT-DECODE-SURFACE: decode/2 stays as sugar lowering to joins vs
  removed from the surface (registry consequences either way).
- SLOT-GC-TIMING: support-GC on dictionaries in this arc vs monotone
  dictionaries + a named debt row (v1 punt is acceptable; state sizes).
- SLOT-ARRIVAL-MALFORMED: a world row whose JSON does not match the
  declared struct shape = named refusal at the boundary (which name).

## Grading contract

Full battery (conformance 139/0 floor, sweep identical-only growth, both
modes), the two edge-grade fixtures above, fail-first receipts per
refusal, EXPLAIN SEARCH receipts on dictionary joins, count tests on
intern paths (statements flat vs value count), endurance (kill -9 mid
intern: the dictionary write and the parent row commit atomically or not
at all).
