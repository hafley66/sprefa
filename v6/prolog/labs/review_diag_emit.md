# Review: diag_emit (post-lab, 2026-07-27)

Reviewer run: `swipl -q -l v6/prolog/labs/diag_emit.pl -g go -g halt` observed
**14 PASS, exit 0**, matching diag_emit.md:3. Live db re-queried directly
(`out/diag.sqlite`): 8 rows, including the two banned-word lines the .md
predicts on LANG.md (0-based lines 88 and 89, i.e. 1-based 89 and 90) and the
no-go-entrypoint flags on src/checks.pl and src/kernel.pl that limitation 2
predicts. grader.pl escapes exactly as claimed (its line 2 quotes the string
`go :- run(check).`). The DDL in the emitted db carries no PRIMARY KEY, no
UNIQUE, no temporal column (verified via `SELECT sql FROM sqlite_master`).

## 1. Construct cardinality

Headline: this build adds ZERO new constructs to the surface. Every idea it
touched is either an instance of an AUDIT.md keep row or fresh pressure on a
row AUDIT already marks "add". The build's whole weight lands on the adds.

| construct / idea | where | verdict | AUDIT reconciliation |
|---|---|---|---|
| DELETE-then-INSERT scoped to scanned paths | emit_diags/3, diag_emit.pl:183-188 | **collapse** into `<-` level rule | confirms "`<-` level rule: keep" and strengthens it: the hand version is coarser than IVM (refresh unit = path, so a deleted source file leaks rows forever, limitation 6) and needs a second input (the scanned-path list) that body membership provides for free |
| the scanned-set declaration ("what I looked at" as distinct from "what I found") | emit_over_files/2, diag_emit.pl:191-194 | **collapse**, same construct as above | under `<-` this IS the body's file-set atoms; a separate declaration exists only because membership is maintained by hand |
| diag row shape (9 columns, rel_diag + diag_v5 view) | diag_emit.pl:48-56, :151-156; src/lsp.rs:399-409, :545 | **instance** of a convention, no syntax | confirms tier doc T3 "convention, not syntax"; ambiguity 6 (table name is convention, view name + column order is the contract) sharpens it. Weakens AUDIT's "diag sink: add" toward "bind + view name + a plain rel decl", no new head construct needed |
| keyed diag proposal, key (path, line, col, code) | diag_emit.md:144-147 | **flag: unbacked growth** | the lab's own evidence never needed it (14 PASS, zero keys); a key on that tuple also collapses two distinct findings sharing a site and code. AUDIT's Key keep row (cache chains) is untouched; this paragraph should be cut or marked unproven |
| waiver marker (`@banned-ok` line skip) | diag_emit.pl:223, :231 | **instance** of the v5 waiver-comment pattern (`@eprintln-ok` + comment() + range join, .dl/no-new-eprintln.dl) | confirms the AUDIT "add" rows it desugars into: negation + comment extraction. No new construct |
| banned-word table as data (halves assembly) | diag_emit.pl:213-219 | **instance** of "facts = bodiless clauses" (keep) | extends the facts row with the self-reference wrinkle: a lint whose rule table is plain text flags itself; the surface answer is a fact rel plus a scope filter or waiver rows, both existing shapes |
| shell bind to the sqlite3 CLI, one process per emit, BEGIN/COMMIT | sqlite_run/3 diag_emit.pl:101-119, emit_diags/3 :183-188, graded at :492-500 | **instance** of `bind X = shell { ... }` (keep) | extends the bind-obligation family from lab-consolidation proven item 3 (link-time finiteness): this bind carries two more dischargeable obligations, batching (one process per emit, graded by counter) and atomic commit (one data_version step per emit, the writer-side half of ruling R7). Obligation kinds grow; construct count does not |
| severity closed vocabulary | lsp_severity/1 diag_emit.pl:71-74 vs the silent coercion at src/lsp.rs:585-593 | **instance** of `enum` (keep) | a receipt for why enum earns its keep: an out-of-vocabulary severity renders as WARNING with no error anywhere; the type check is the only place the typo can surface |
| nullability discipline (hint Option; NULL code/msg kills the poll cycle) | ambiguity 2; src/lsp.rs:406-408, :502-513 | **instance** of required column types (keep) | the view COALESCEs only severity (diag_emit.pl:155-156); Option typing at the bind is the missing defense for code/msg |
| 0-based position convention | ambiguity 1; src/lsp.rs:395-398, :598-602 | not a construct | a units hazard; at most an argument for a Position newtype someday, not now |
| file read + line split + glob tree walk | diag_emit.pl:276-291, :301-319 | **instance** of scan + extraction | confirms AUDIT finding 17 (blocker) with force: the T3 flagship demo cannot be written in the candidate surface because the surface cannot read a file |
| scalar string fns + arithmetic | diag_emit.pl:235-236, :244 (`EndCol is Col + WordLength`); the .md's own sketch uses `contains`, `ends_with`, `col+1` (diag_emit.md:131) | **instance** of AUDIT "add" rows (comparison/arithmetic 166/173, scalar fns 28) | confirms |
| negation over a derived rel (check b in the candidate surface) | diag_emit.md:140-143 | **instance** of AUDIT "add negation" (86/173) | confirms |

Secretly-one findings: rows 1 and 2 are one construct (`<-`); the waiver marker
and check (b) are one construct (negation over an extraction rel); the batching
law and the atomicity law are one home (the bind).

## 2. Tier placement, derived independently

The loop has three parts. The writer needs T0 (level rules, negation,
comparison, scalar fns) plus T1 (extraction for file/line facts; the bind for
the sqlite sink). The sqlite file is the interface. The reader is finished rust
and contributes the loop's entire temporal behavior: data_version stepping
(src/lsp.rs:538-542) and retraction by absence via the last_published_paths
diff (src/lsp.rs:489-495). So:

- **T3 as mapped: confirmed**, with one tightening. The tier doc's
  "T3 <- T0 (+T1 in practice)" should read T0+T1 hard: the 54-file diag corpus
  is extraction-fed and even this toy needed file reads.
- **No T4 dependency: confirmed, with a stated qualifier.** No keyed or
  temporal semantics is smuggled: the DDL has no key (diag_emit.pl:152-156,
  verified in sqlite_master), no clock, one emit. Squiggle retraction needs no
  edge rules because the LSP contract does retraction by absence per publish
  cycle. The one temporal discipline the writer does carry is atomic
  commit-per-emit so data_version steps once; that is R7's boundary contract
  seen from the writer, and it is correctly housed in the bind as an
  obligation, not in program text.
- **No T5 dependency: confirmed at the payload level, overclaimed at the
  failure level.** No demand rows, no success envelope. But the sink does have
  a failure reply (see WRONG list, item 1); the placement survives because
  failure handling can live in the bind.
- **Manual level maintenance does not move the T0/T3 boundary.** T3 already
  depends on T0; emit_diags/3 is a receipt that the dependency is real (the
  hand version is weaker on exactly the axes IVM covers: retraction
  granularity and the deleted-file case). The kernel claim
  {ground_terms, rule, external_rel} + one shell bind holds for the plumbing;
  the surface claim understates (see item 2 below).

Tier doc changes earned:
1. T3 dependency line: `<- T0 (+T1 in practice)` becomes `<- T0, T1`.
2. Add to T3: retraction reaches the editor via the reader's absence diff
   (src/lsp.rs:489-495), so T3 carries no T4 dependency even for clearing
   squiggles; the `<-` arrow's payoff here is deleting the writer's hand-rolled
   refresh, not enabling retraction.
3. Bind obligations (T1) gain two kinds beyond finiteness: per-emit batching
   and atomic single-transaction commit (writer-side R7).

## 3. What the build got wrong or overclaimed

1. "it does not need the effect envelope machinery, because the sink has no
   interesting reply" (diag_emit.md:154-155). The sink replies with an exit
   status and stderr, and the lab handles that reply by throwing
   `sqlite3_failed` (diag_emit.pl:116-118). Failure is a value per LANG.md:26.
   The honest sentence is: no success payload; the failure channel is a bind
   obligation. Tier placement unaffected.
2. The tier-order paragraph (diag_emit.md:152-157) counts kernel elements but
   not surface constructs. The three checks, restated in the candidate surface
   by the .md itself (diag_emit.md:127-143), use extraction, negation,
   comparison/arithmetic, and scalar string fns; LANG.md has syntax for none of
   the four. All four are already AUDIT "add" rows, so nothing new breaks, but
   "needs nothing above {ground_terms, rule, external_rel}" is true only of the
   plumbing.
3. The keyed-diag paragraph (diag_emit.md:144-147) proposes a construct use the
   lab's own 14 checks never exercised, and the proposed key (path, line, col,
   code) would collapse two distinct findings differing only in msg. Cut or
   mark unproven.
4. Limitation 1 reports the two banned-word lines the live run flags in LANG.md
   and omits that the same run also flags two em dashes in LANG.md itself
   (0-based lines 0 and 34, verified in out/diag.sqlite), i.e. the spec file
   violates its own lab style law (LANG.md:90). One sentence would fix the
   omission.

## Disposition

Accept with notes. The code is verified green (14 PASS observed), every
line-number citation spot-checked against src/lsp.rs held, the no-keys and
no-temporal claims are true of the code, and the build's central observation
(the hand-written refresh in emit_diags/3 is the `<-` arrow's job) is both
correct and the strongest corpus-side receipt yet for T0's level semantics. The
notes are all in the .md, not the .pl: soften the no-reply claim, extend the
tier paragraph with the four surface "add" dependencies, cut or demote the
keyed-diag speculation, and add the self-flagging em dashes to limitation 1.
No rework of the lab code is warranted.
