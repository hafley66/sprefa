# simplify wave brief: deduped findings of the 4 opus reviewers (reuse/simplification/efficiency/altitude) over 934dcc4d..HEAD

QUEUES BEHIND the sol host-seam lane (shared files: emit_ts.pl, parse_dl.pl,
structPlane.ts, serve/1_hosts.ts, plunit_tests.pl). Dispatch after that merge.
Re-verify every file:line against the post-merge tree before editing.

ALREADY DONE by coordinator (6522f848): altitude #1 — aggregate_group_exprs
now shares group_expr/3's integer-literal wrap; fail-first fixture
groupby_aggregate_two_bare_integer_literals.

Validation after each group: sweep both modes (expect 99/97/0-wrong),
conformance 159, plunit 138, TEXT_DOOR 99, tsv2 tests, staleness gate. Rust
group: cargo test in v6/sprefa-extract + fixture snaps. Full battery at end.

## P0 — correctness-adjacent (do first)

1. **`__dict_` prefix sniff** (lower.pl:1767 `dictionary_use/1` + `:678,:690`
   literals). A program rel named `__dict_*` silently loses its delta arms;
   this is the banned magic-name pattern. Fix: dictionary relplans get their
   own plan kind (`dictionary`) minted at dictionary_relplans/2:800; test on
   relplan kind; derive the DDL index name from dictionary_table_name/2.
   Fail-first: a user rel named `__dict_x` keeps its delta arm.
2. **Refusal umbrella misses bare throws** (0_refusal_messages.pl renders
   only unsupported_construct/1; 1_host_expand.pl throws 15 bare refusals —
   refused_host_decl/1:120, column_mismatch/2:126,147, template_mismatch/1:
   153-157, bind_mismatch/2:212, bind_and_rule_head/1:217, query_mismatch/1:
   234, unmapped_feature/2:242,244,296, probe_mismatch/1:331,360,366 — all
   print "Unknown message"). Key the message clause on a refusal-term
   inventory (multifile refusal_term/1 or derived), unsupported_construct/1
   one arm. Also: derive the module list instead of the 10 hardcoded
   refusal_source_module/1 rows (silent-drift hazard); collapse the
   per-signature rescan to one findall+keysort (0_refusal_messages.pl:48-63).
3. **watch vs enumerate digest incompatibility** (2_binds.ts:231 sha256 vs
   enumerate host git hash-object — two values under one `digest` column
   name; nothing asserts agreement). SMALLEST: 2_binds digestOf switches to
   `git hash-object` semantics (spawn or inline blob-sha1) so the column has
   one meaning, plus a test pinning watch-row digest == enumerate-row digest
   for the same bytes. If that trade is wrong, STOP AND REPORT with the cost
   table instead of picking.

## P1 — cross-file dedup (each flagged by 2+ reviewers)

4. `call_name_match` x5 verbatim (ts.rs:2655, rust.rs:741, go.rs:1541,
   kotlin.rs:1049, prolog/_0_source.rs:455) -> one free fn in seams.rs
   (uses only covering_def/def_named/corpus_defs + FamilyTag). While there:
   def lookup via prebuilt HashMap per file (def_named is a linear scan per
   site), bind corpus_defs once, dedup with a set not Vec::contains.
5. `canonicalizeJson` TS duplicate (structPlane.ts:71 vs ticklog.ts:18) ->
   one export, structPlane imports. Also skip the JSON.parse->canonicalize->
   stringify round-trip at log time for columns the plan knows are ref(_)
   (ticklog.ts:37, sweep.ts finalValueJson) — stored __rendered IS canonical.
6. `check_world_shapes/3` mirrored (conformance/engine.pl:460 vs
   compile/compile.pl:155) -> one gatherer in 0_type_plane.pl, each door
   wraps its own throw (the 0_program_check first_violation split).
7. `ScriptedWatchSource` x3, one already drifted (watchBootReconcile.test.ts:
   48, serveWatch.test.ts:46, watchCounts.test.ts:54) -> tests/serveHelpers.ts.
8. `FlatFact::Sig` owner span emitted 3x (types.rs:1290-1296, wire.rs:100-108):
   pick ONE spelling for the whole wire — flat pairs per the df-aux brief's
   own convention; drop nested `owner`; align param/arg records or state why
   they stay nested. Regen fixture snaps.

## P2 — structPlane write-path efficiency (one edit serves three findings)

9. collect() records the per-(arrival,column) semantic key while walking;
   rewriteRow() becomes a lookup (kills the double canonicalization :163/:183,
   the O(depth) re-render of nested values — parent text = concat of child
   rendered strings already in fields — and the impossible-error throw :186).
   Drop the `{childSemantic}` union wrapper (plan.refs[i] discriminates).
   Memoize the per-tick `new Map(types.map(...))` (:202).
10. Boot dedup in 2_binds.ts: process-level path->{mtime,size,digest} memo
    (boot re-hashes the corpus per subscribe/swap; overlapping globs re-hash);
    share the engine.rows read across globs.

## P3 — prolog mechanical (single-reviewer or dead-code, all precise)

11. lower.pl: incremental_json_select_exprs_from/3 dup (:915 vs :966);
    goals_conjunction/2 dup of 1_host_expand body_from_list/2 (:832);
    braces_pattern_pairs/2 vs conformance/body.pl braces_pairs/2 (split
    flatten from canon, import); dictionary_storage_kind/3 alias (:695);
    decode_slot/6 dead Acc (:884); dead Types==[] branches (:723,:2092);
    boot_column_slots shared fold (:1999-2002 vs :2026-2029);
    boot_statements/5 takes Plan not (Decls,RelPlans) (:2109, 4 call sites).
12. emit_ts.pl: 3 dead arity shims (:163,:1287,:1342); js_string dead call
    (:261); struct_ref_entry/column_type_ref_entry collapse by making
    dictionary_ref_type/3 emit ref(T)/none consistently (lower.pl:748).
13. 0_type_plane.pl: settled_prefix subsumes topological_rounds
    (type_topological_order = settled_prefix + same_length; drop the double
    traversal in type_cycle_witness); type_canonical_json/4 delegates to
    canonical_json_text/2 (delete type_field_json/3); narrow the export list
    (4 unimported exports); consider col(C,T) pairs over parallel lists.
14. parse_dl.pl: type_decl_columns/3 reuses decl_a_columns/3 (map column/2
    -> col/2, refuse T == none). [WAITS on sol merge — parse_dl is sol's.]
15. analyze.pl: thread Types through program_column_types/7 (:383); hoist the
    ref(_) test above the witness findall (:385-390). engine.pl:444 hoist
    type_definitions out of the per-tick maplist; 0_type_plane per-row Decl
    scans get a Ref->columns map.
16. level_eval.pl:127-141: finish the registry adoption — the nine hardcoded
    functor clauses duplicate registry no_refs/refs_of_arg rows the new
    clause already reads; keep the not/1 ordering constraint
    (0_body_walk.pl:63-69), kill the functor list. goal_list_rel_refs fold
    dedup (:144 vs :121).

## P4 — rust extractor + scripts + tests

17. df edge+aux pairing helpers (20 hand-paired sites, 4 langs): 
    df_arg_edge/df_param twins so slot rows are structural; per-file
    param-walk dedup (go.rs:707 vs :1080, kotlin.rs:573 vs :808 — kotlin
    pair ALREADY drifted on param_pos increment: verify which is right
    against a fixture first, then dedup). ProjectEdge call_site
    non-optional on CallF (types.rs:697).
18. Scripts: extraction-live.sh reload_program helper (5 copies);
    staleness-gate.sh drop the bash-4 assoc array (macOS /bin/bash 3.2);
    shared ensure_binary resolver for the 6 copies of binary paths+build
    lines (staleness-gate:152, flagship-callgraph:147,164,
    extraction-live:91, lsp-diags:131,145, lsp-v5-bridge:63).
19. Tests: structPlane span(start,end) helper (8 casts); extract the 7th
    hand-rolled ScratchStore boot->run harness into a tests helper (7 files).

## Explicitly NOT in this wave
- 2_binds trackedPaths -> enumerate-host unification beyond the digest fix
  (#3): the A12 one-shot crossing is sanctioned in the 2_binds.ts header;
  re-architecting it is a design call, not a cleanup.
- golden.test.ts storesOpened probe removal: it is live diagnostics for the
  open flake hunt (ARCH golden_flake_hunt); leave until that closes.
- Prolog canonical-JSON triplication (ticklog.pl script copy): disclosed,
  pinned, structural reason stands.

## Added post-review (coordinator, flow-rig contact 2026-07-29)
- SLOT-BIND-SPELLING (user ruling): `:=` is the bind-goal spelling (registry.pl:72,
  16 fixture files) but is not an rxjs/prolog/SQL word — the vocabulary-law class
  review-B8 flagged. Prolog's own candidates: `is` (arith) or `=` (unification).
  Rename = registry + parse/print + engine op + 16 fixtures, mechanical.
- `Var = expr` (prolog's `=`) is unregistered and dies as unbound_head_var —
  wrong name, no mention of `=`. Whatever the ruling, `=` must refuse (or bind)
  BY NAME; it is the first spelling a prolog reader types (terra typed it).
- SLOT-TYPE-DECL-DISTINGUISHABILITY (user 2026-07-29: "types and rels are
  indistinguishable to a fucking human"): `type span(start: int, end: int).`
  and `rel file(path: text, digest: text).` share one visual shape with
  opposite semantics (value shape vs fact table); use sites are equally
  blind (`at: span` reads like `path: text`). Candidate spellings to price:
  (a) braces for value shapes: `type span = {start: int, end: int}` (JSON
  word for a value, parens stay tables); (b) uppercase type names + a
  lint (`at: Span` — rust/TS convention, zero grammar change); (c) status
  quo + editor semantic tokens only (weakest). Decl AND use-site legibility
  both graded. The type keyword itself was agent-chosen, never ruled.
