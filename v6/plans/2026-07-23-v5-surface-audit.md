# v5 surface audit vs the v6 direction — adversarial, living table (2026-07-23)

Method: every construct row has a source anchor (read this session), a grep count
over `examples/*.dl + .dl/*.dl + std/*.dl` (171 files), and a verdict against the four
v6 docs (in-literal-rxjs + its review verdict, transports-as-rels, rest-epic plan,
DECISIONS.md) plus the owner rulings in force. Counts are occurrence counts unless
marked "lines" or "files"; grep patterns are approximate where noted (hedged inline).
Verdicts: KEEP / DISSOLVES into X / KILL / UNDECIDED (Frontier item or open question).

v6 read-side reality check used throughout: the v6 TS AST today
(`v6/sprefa-store/js/src/lower/ast.ts`) expresses: Var/Lit (string|number|boolean|null),
Wild **in negation args only** (ast.ts:60), RelRef, Compare with `eq/ne/lt/le/gt/ge`
only (ast.ts:63), NegRelRef, head aggregates `max/min/sum/count` (ast.ts:109), untyped
column names on RelDecl (ast.ts:132). Nothing else. Everything a row below marks as a
hole is a claim against THAT surface plus the four docs, not against imagination.

---

## Part 1 — the living table

### 1a. Rule-temporal + boundary markers (the `@` family — sigil ruled dead)

| construct | anchor | what it does (verified) | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `@next` | src/ast.rs:473, parse/mod.rs:379 | stages the head into the NEXT tick's seed instead of this tick's fixpoint — a rule at tick t reads facts derived at t-1 | 18 | DISSOLVES into a tick column + self-join: `etag(ep, tag, tick)` read at `tick - 1`. rx spells "previous value" several equivalent ways (`scan`, `pairwise`, BehaviorSubject feedback edge) — all subscription-local, so none is definitional; the store column is the mechanism | The review verdict already caught the failure: EVERY subscription-local spelling of previous-value state (pairwise, scan, a Subject latch) reseeds after refcount churn and reports the whole relation as new (etag re-fetch storm). The store spelling is mandatory. All 18 uses are in the gh-cache family + chaos-soak — the canonical demo depends on this working. |
| `@async` | src/ast.rs:473, engine/tick.rs:292 | body fires a one-shot effect; response lands as a fact at a later tick | 32 | DISSOLVES into host rel (transports kill table: "asynchrony is the host rel's private business") | Heaviest temporal marker. If host-rel modes (F4) slip, 32 sites across gh-cache/npm/chat-marks have no spelling. The E4 veto explicitly deferred this to F10/F4 — it is currently expressible in NEITHER surface. |
| `@stream` | src/ast.rs:473, engine `drain_streams` (engine/mod.rs:653) | body opens a long-lived `sh*` subscription; rows append over many ticks | 3 | DISSOLVES into host rel whose rows include a `seq` column (transports doc, ShellKind row) | Only 3 uses (npm dep walker). Cheap to lose, but the npm example is the only long-lived-subprocess receipt; if the `seq`-column story is wrong there is no second test case. |
| `@in(class)` / `@out(class)` + `PortDir` + `Port::envelope` | src/ast.rs:144-173, parse/mod.rs:293 | marks a rel as a boundary port; serving loop injects `@in` rows pre-tick, drains `@out` post-tick; `class` (only `rpc` implemented) fixes the column envelope | 4 + 4 | DISSOLVES into fact/response rel pair (transports doc, explicit kill-table row) | Clean dissolution — the transports doc's `http_request`/`http_response` is the same envelope with the class made columns. What the pair model does NOT keep: `Port::envelope` was a CHECKED contract (wrong columns = load error, ast.rs:166). The v6 pair is convention; nothing typechecks that a route's reqRel has an id column. E6's RouteDecl re-introduces the check as data — verify it lands. |

### 1b. Core rule syntax

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `<-` neck (Rule) | src/ast.rs:476, parse/mod.rs:362 | derived rule / ground fact (empty body) | 2146 | KEEP | — |
| `!` negation (BodyItem::Neg) | src/ast.rs:370, parse/mod.rs:503 | stratified anti-join; `_` wildcards allowed | 247 (`![ident](` pattern) | KEEP — E4 is exactly this, at v5 parity | E4 is negation-only after the veto, so this is the one temporal-adjacent thing v6 has actually scheduled. Non-stratifiable diagnostic must name the cycle (E4 4.1.2). |
| `?` query | src/ast.rs:629, parse/mod.rs:478 | one-shot demand query; filtering by nesting (no `where`) | 490 (128 files) | DISSOLVES into `firstValueFrom`/take(1) reading the store (review verdict: queries read the store, never interior pipes) | The reviewers found quiescent `firstValueFrom` HANGS on cold pipes; the resolution (demand-pull reads the store) is asserted, not implemented — no epic owns "the query surface" as code. 490 uses is the single most-exercised construct after `<-`. |
| named args `col: term` (Atom::named) | src/ast.rs:314-321, parse/mod.rs:426 | by-name binding resolved to positional slots at load; unnamed columns pad `Term::Wild`/NULL | ~1727 (pattern `ident(word: `, includes op kwargs; hedged) | KEEP — owner style law already mandates named-arg dl snippets; transports doc examples use it | The NULL-padding half (partial heads) is load-bearing for `diag` (9 cols, name 3). v6 has no NULL story at all — LitValue has `null` but Rule heads are positional HeadTerms with no padding semantics. |
| `_` wildcard (Term::Wild) | src/ast.rs:291, parse/mod.rs:839 | don't-care in body atoms; NULL-projection in non-recursive heads | ~1525 (pattern `(_`/`, _` boundaries; hedged) | **CANNOT YET EXPRESS in positive body atoms** — v6 `Arg = Var \| Lit` (ast.ts:46); Wild exists only in `NegArg` (ast.ts:60) | Trivial to add, but today it is a real gap: almost every nontrivial v5 body atom has a `_`. Any parity claim (E4 "v5 shapes green") that skips this is testing toy programs. |
| cmp `= != < <= > >=` (CmpOp) | src/ast.rs:256 | SQL comparison in bodies | != 142, <= 117, >= 64 (+ = / < / > uncounted, ubiquitous) | KEEP — v6 Compare has all six | — |
| `=~` regex match (CmpOp::Match) | src/ast.rs:264, parse/mod.rs:758 (regex literal RHS only) | `REGEXP` filter; `/re/` literal with `$NAME` holes desugared (parse/desugar) | 100 | **CANNOT YET EXPRESS** — v6 CmpOp stops at `ge` | 100 uses across rails/examples. A regex filter inside `map` is trivial in TS, but the AST has no node for it, so E5's json-rx and E7's grammar can't represent it either. Nobody has claimed this row. |
| `~~` glob match (CmpOp::Glob) | src/ast.rs:264, lex.rs:134 | `GLOB` filter (string RHS — `/` is a path char) | 5 | Same hole as `=~`, lower stakes | Could KILL into `=~` if regex lands; 5 sites. |
| int arithmetic `+ - * / %` (Term::Arith) | src/ast.rs:301, parse/mod.rs:773 | head + comparison positions only; never binding | `+` on 281 lines, `*` 25, `%` 1 (line counts, comment noise possible; hedged) | **CANNOT YET EXPRESS** — no Arith in v6 AST | Two constructs hiding in one token: `+` on textish operands is STRING CONCAT (lower.rs:152 branches on `textish()`), used in the wild (`type_decl_row("partial_" + rel, ...)`, examples/type-from-json.dl:67). The transports candidate grammar itself writes `tick: tick - 1` — the v6 docs USE arithmetic the v6 AST cannot represent. |
| `Term::Call` scalar fns | src/ast.rs:307, whitelist src/engine/decls.rs:166-186 | pure string/cast fns in heads + comparisons: split/replace/replace_re/lower/upper/lcfirst/ucfirst/trim/strip_prefix/strip_suffix/norm/int/sym/json_object/json_array/json | replace_re 77, split 43, replace 26, int 17, trim 13, strip_prefix 11, misc ≤2 each — ≈190 total | **CANNOT YET EXPRESS** — no Call in v6 AST | The E5 pure-kernel plan (`kernel.ts`) is where these would live, but no epic lists a scalar-function surface. `replace_re` alone (77) outweighs whole feature families below. gh-cache-config uses `split`; rails use `replace_re` heavily. |
| `Term::Interp` `${var}` strings | src/ast.rs:292, lex.rs:165 | string templates with variable holes, in heads/comparisons; also feeds sh/cmd/gen templates | 253 | **CANNOT YET EXPRESS** — no template node in v6 AST; the transports executor tail (`sh\`{command}\``) implies but does not define one | Endpoint construction in gh-cache (`"repos/${slug}"`) is interp — the canonical program again. Dissolvable into concat + Call, but only once Arith/Call exist. |
| head aggregates count/sum/min/max | src/ast.rs:329, parse/mod.rs:438 | GROUP BY on non-agg head terms | count 315, min 37, max 22, sum 6 (patterns include prose false positives; hedged) | KEEP — v6 AggFn has exactly these four (ast.ts:109); DECISIONS bookmark: lowers into SQL GROUP BY at the dirty boundary | — |
| json_group_array / json_group_object | src/ast.rs:329 (AggFn), Rule::agg_args2 src/ast.rs:489 | build JSON array/object per group (SQLite native); the only 2-arg agg | 10 combined (3 + 1 in rules, rest prose; hedged) | **CANNOT YET EXPRESS** — absent from v6 AggFn | Small weight, but this is how a v5 program builds an http/mcp RESPONSE BODY. The transports doc writes `{stars: star_count}` term syntax that exists in NO grammar, v5 or v6. Serving (E6) needs an answer here before the ghcacher demo returns JSON. |
| `key(...)` decl qualifier | src/ast.rs:179-184, parse/mod.rs:267 | functional dependency / choice domain: conflict target is the key subset, first-wins without merge (Soufflé choice) | 8 | UNDECIDED — no v6 word. Nearest: DECISIONS' "latest-by-gen lowers into SQL GROUP BY + LIMIT" bookmark covers the read side, not the upsert lattice | The v6 store is pure set-semantics with Z-set weights. `key` changes what a "row" IS (FD identity, not full-row identity). Weight arithmetic over a keyed rel is undefined — does replacing a row's non-key columns retract weight on the old row? Nobody has asked. |
| `merge(MaxBy(col))` / `MinBy` | src/ast.rs:110-141, lowers to ON CONFLICT DO UPDATE WHERE excluded.col > col | lattice row-selection on key conflict; MinBy = shortest-path lattice (kills the node×path-length product in depth recursions) | 11 + 11 | UNDECIDED — same gap as `key` | MinBy is not sugar: it is what makes depth-tracking recursion TERMINATE small in v5. The v6 recursive stratum (naive fixpoint in lower.ts, SQL delta loop) has no lattice, so a depth recursion re-derives every (node, depth) pair. This is a semantics gap AND a Big-O gap. |

### 1c. Declarations and the type-system half (adversarial item 1)

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `rel name(col: ty, ...)` | src/ast.rs:176, parse/mod.rs:223 | typed rel decl; qualifiers key/merge/@in/@out | 1499 | KEEP — v6 RelDecl exists, but columns are UNTYPED strings (ast.ts:134) | The entire column-type layer (next 3 rows) rides on this and has no v6 landing spot. |
| Type enum text/int/path/file/dir/repo/rev (+`sym` alias, `node` coord) | src/ast.rs:5-35, Col src/ast.rs:47-101 | storage class (TEXT/INTEGER + interning), and the path family drives scan coordinates + path-escape/coerce diagnostics (TypeDiag codes, ast.rs:789) | column decls: text 1828, int 950, file 711, path 28, rev 21, node 18, repo 10, dir 1, sym 1 | UNDECIDED — v6 has LitValue string/number/boolean/null and nothing else. The trinity doc's "rel-kind algebra IS the type system" is about rx LIFECYCLE typing (hot/cold/replay), a DIFFERENT type system than v5's data-column typing. Both are real; only one is designed. | `file`(711)/`rev`(21)/`repo`(10) are not decoration: they are what lets the engine know a column joins the spine, resolves against a scan root, and survives a rev checkout. The E2 FactLine schema types spine rows structurally, which covers EDB — but derived rel decls in v6 hold zero type info, so `coerce-text-path` / `path-escapes-root` diagnostics (src/ast.rs:789) have no v6 home. The 0723.2 doc says "lift v5 typecheck.rs" as a task with no epic. |
| `type X <: parent.` brand (BrandDecl, Lt2 token) | src/ast.rs:645, parse/mod.rs:176, lex.rs:145 | nominal subtype, typecheck-time only, storage stays text; drives check_rule_types unification | **0** in corpus (tested in tests/it/type_decls.rs only) | KILL as surface, keep the IDEA for the F-open type bootstrap | The honest reading: brands shipped and no real program adopted them. Killing costs zero programs today. BUT builtin enum brands (Col::branded, ast.rs:98 — e.g. `type_edge.kind` closed vocab) are engine-minted and DO gate typos in 169 diag-writing rails; that half is a builtin-catalog feature, not surface syntax, and needs a v6 equivalent wherever builtin rels land. |
| `type X = "a" \| "b".` enum brand (Pipe) | src/ast.rs:645 (variants), parse/mod.rs:182 | closed literal set; literal outside the set = enum-variant-unknown error | **0** in corpus | KILL as user surface (same caveat: engine-minted enum brands live) | — |
| `type X(col: ty, ...).` shape (ShapeDecl) + `rel r: X.` (shape_ref) | src/ast.rs:653, src/ast.rs:201, parse/mod.rs:192 | reusable column list, expanded at load | 0 shape decls written by hand; 1 shape_ref (examples/type-from-json.dl:57) | KILL as hand-written surface; UNDECIDED for the computed half (next row) | — |
| `type_decl_row` computed-shape sink | src/engine/decls.rs:416-430, used examples/type-from-json.dl:44,67 | a DERIVED rule emits (shape, pos, col, type) rows; next tick those become checked shapes — dl programs minting their own schemas from data (JSON sample -> typed rel; `partial_<rel>` per builtin) | 4 | UNDECIDED — no v6 concept of reflective/staged schema at all | This is the one genuinely novel v5 type feature with a live example. It is also tiny (one example file). Decide deliberately: kill it and say the demo dies, or name it a far-frontier item. Do not let it dissolve silently. |
| `anchor name = fs:...` (AnchorDecl) | src/ast.rs:636, parse/mod.rs:145 | named filesystem anchor; only `~` default ever resolved | **0** | KILL — dead on arrival in v5 itself (v1 accepted, refs deferred per ast.rs:633) | Nothing depends on it. Safe. |
| `use "path".` (Import) | src/ast.rs:749, parse/mod.rs:117 | module inclusion against include roots (program dir, $SPREFA_STD, exe/../std); diamond dedup | 16 | UNDECIDED — no module story in any v6 doc; E7's grammar scope ("the v6 surface only") does not mention `use` | std/ is 9 files (arch/callgraph/entry/flow/measures/strings/suppress/...) reached ONLY via `use`. If v6 has no import, there is no stdlib mechanism. Cheap to spec, currently unowned. |
| `def name(params) <- body.` (RuleTemplate) | src/ast.rs:757, parse/mod.rs:86 | parameterized rule template, inlined + alpha-renamed at call sites | **0** | KILL | Unused. The E5 json-rx graph is a better macro layer anyway. Safe. |

### 1d. Source/extraction body items (adversarial item 3)

v6's stated home: a Rust extractor CLI writes fact jsonl over the E2 ingest seam
(FactLine `{t:"rel", name, row}` arm exists). The per-construct question is whether
the op's semantics survive that move.

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `scan(repo?, rev?, glob, path, rev_out?)` | src/ast.rs:371, parse/ops.rs:29 | glob file enumeration at a repo+rev COORDINATE (defaults `.`/WORK); the root of every extraction rule | 414 | DISSOLVES into extractor CLI + E2 ingest — but only the `(".", WORK)` point of its coordinate space is covered | The hole: `scan` takes ARBITRARY revs (git history) and OTHER repos (registered slugs). E2/E3 re-extract changed WORK files only. "time = repo_revs is data" (axis law) asserts the query side; NOTHING produces multi-rev facts on the v6 side. Real users: chaos-soak scans revs; the repo/checkout sinks (1f) pull repos dynamically. Either the extractor grows `--rev`/`--repo` args driven by program demand (undesigned), or v6 silently drops history queries. |
| `match_line(path, rev, /re/, line, [id, col, end_col])` (+deprecated `match`) | src/ast.rs:381, parse/ops.rs:94 | line-regex over flat text; named `$NAME` holes bind captures; optional spine id for codemods | 87 (legacy `match(` 0 — rename complete) | Survives as extractor capability — SAME semantics, different home. The undesigned part is DEMAND: the regex lives in the dl program; who ships it to the CLI? | The extractor worktree (F2) is pinned on spine fact framing, not on program-driven patterns. Per-rule regex extraction means the CLI takes (pattern, glob) pairs computed FROM the program set — that plumbing appears in no epic. Also: the `id` output threads into `ref(id,...)` for gen anchors; if gen dies (1e) that output is dead weight, if gen lives the extractor must mint spine ids. |
| `ast(path, rev, :lang, "ts-query", line, [end, id])` | src/ast.rs:383, parse/ops.rs:143 | tree-sitter query captures | 48 | Survives as extractor capability; same demand-plumbing hole as match_line | — |
| `match_ast(path, rev, :lang, "pat", spans..., id?)` file form (+deprecated `sg`) | src/ast.rs:405, parse/ops.rs:195 | ast-grep structural pattern | 81 (legacy `sg(` 0) | Same as ast | — |
| `match_ast(:lang, src, "pat", spans...)` TERM form | src/ast.rs:394-397 (rev:None), parse/ops.rs:243 | ast-grep over a BOUND STRING VALUE (embedded language: css-in-js, md fences, response bodies); runs in the join+extract hybrid pass, region-relative spans | 7 | **CANNOT YET EXPRESS** — see the term-extract block below the table | — |
| `ast_yaml(path, rev, :lang, \`yaml\`, spans...)` | src/ast.rs:412, parse/ops.rs:277 | ast-grep relational RuleCore (inside/has/not) — the superset pattern form | 6 | Survives as extractor capability | Low weight; folds into match_ast's story. |
| `jsonp(path, rev, "a.b.*", out, id?)` FILE form | src/ast.rs:422, parse/ops.rs:310 | dotted-path evaluator over json/yaml/toml files | ~10 (of 28 total jsonp) | Survives as extractor capability | — |
| `jsonp(src, "a.b", out)` TERM form | src/ast.rs:419-421 (rev:None) | dotted-path over a bound string VALUE | **18** of 28 | **CANNOT YET EXPRESS** — the biggest term-extract user | gh-cache.dl:110 `stars(ep, n) <- resp_current(ep, _, body), jsonp(body, "stargazers_count", n).` — the CANONICAL v6 demo program (0723.2 reading-order item 6, E6's ghcacher) extracts from an http response body mid-program. The ingest seam ("extractor writes files->facts") never sees this string; it is born inside the dataflow. v6 needs a scalar/table-valued extraction word usable in rule bodies (SQLite `json_each` is even named in the transports doc as a host rel the engine "already uses" — that is the shape of the fix, but nobody has connected it to term extraction). Sized: 18 + 11 + 7 = **36 term-extract sites, including every http-consuming example**. |
| `json(path\|src, rev?, q:{ $k: $v })` | src/ast.rs:429, parse/ops.rs:355, pattern parser src/datapath.rs | declarative brace-pattern over json/yaml/toml; binds key AND value captures | 17 total, 11 term-form | FILE form: extractor capability. TERM form: same hole as jsonp | The `q:` scheme literal (desc.rs SCHEMES) exists solely as this op's carrier. |
| `cmd(path, rev, "tool {file}", line, out)` | src/ast.rs:433, parse/ops.rs:385 | shell-out per matched file, one row per stdout line, content-cached | 1 | DISSOLVES into host rel (`host lines(command, ...) = sh\`...\``, transports doc) — the transports doc's own example IS cmd | Weight 1; the dissolution is already written. Verify the per-FILE caching contract (re-run only on content change) survives as host-rel tabling keyed on (cmd, file digest). |
| `comment(path, rev, /open/[, /close/], l0, l1, label)` | src/ast.rs:438, parse/ops.rs:407 | comment-marker regions (sequential or paired) | 58 | Survives as extractor capability | Feeds gen `:zone`/Splice coordinates — coupled to the gen decision. |

**The term-extract block (the real hole, sized):** 36 sites (jsonp 18, json 11,
match_ast 7) extract structured data from string VALUES produced by other rels —
mostly effect response bodies. The v6 extraction story ("Rust CLI writes facts over
the SQLite/jsonl seam", 0723.2; E2) covers file-origin facts only. Term extraction is
a DERIVED-rule capability: input rows in, more rows out, pure, incremental. In v6
vocabulary it is a pure table-valued function inside a stratum — nothing exotic —
but no doc names it, the v6 AST cannot hold it, and the flagship demo (gh-cache ->
E6 ghcacher) breaks without it. This is the loudest single finding of the audit.
Note v5 learned the hazard the hard way (CLAUDE.md style note): a term-extract rule
cannot share a head with a derived rule, and cannot feed `@next` directly — the v6
design should get the staging right natively instead of inheriting the bail.

### 1e. Codegen surface — gen + SpliceMode (adversarial item 2)

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `gen("path{var}", "tmpl") <- body` File form (+`:append` concat) | src/ast.rs:657-665, parse/mod.rs:553 | render rows through a template into a whole file; byte-identical write skipped (converged tick = no-op) | 9 file-form (`gen("` at line start); 168 `gen(` total | **UNDECIDED — no v6 doc mentions code generation at all** | — |
| `gen(p, l0, l1, "tmpl")` Splice (line markers) | src/ast.rs:667 | splice between comment-op marker lines | subset of 62 var-leading gen( | same | — |
| `gen(:replace\|:append\|:prepend\|:wrap\|:delete, p, lo, hi, ...)` Cursor + SpliceMode | src/ast.rs:673, :693, parse/mod.rs:600 | byte-accurate splice at ref-spine offsets (v4 write_cursor port); right-to-left batch application | 85 (`gen(:mode`) | same | — |
| `gen(:zone, p, name, "tmpl")` Zone | src/ast.rs:679 | replace between named BEGIN/END markers, comment-prefix tolerant | 15 | same | — |

**The collision, named honestly:** gen is dl WRITING the same files scan READS. That
is a feedback edge through the filesystem — in v5 the tick loop closes it, with
convergence guaranteed by the byte-identical-write skip (ast.rs:711: "a converged
tick is a no-op"). Under FRP-unidirectional the only legal spelling is: gen = an
outbound effect (a host rel that writes), the written file re-enters through the
watcher/extractor ingest, and the loop terminates by E2 idempotence (identical rows
-> zero changed cells -> no tick — the same E2 property the review verdict uses to
terminate effect feedback). So the model CAN express it — the cycle crosses ticks on
disk, exactly like the gh-cache etag loop. But: (a) nobody has designed it, (b) the
at-most-once/idempotence guard for a WRITE effect keyed on (path, lo, hi, payload
digest) has to exist before the first gen rule runs or v6 mints an infinite
write-extract loop on any non-converging template, and (c) the byte-offset Cursor
form depends on the ref spine minting ids at extract time (extractor obligation, see
match_line row). 168 uses — this is the auto-refactor/codemod half of the product,
plus every README table the repo generates. Silence in the v6 docs is not a verdict;
this row demands an owner ruling: port (as write-host-rel + ingest loop), or cut the
codemod capability from v6 scope explicitly.

### 1f. Effects, shell, transports

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `sh name(params) -> (cols) = \`tmpl\`.` (ShellFn, Read) | src/ast.rs:736, parse/mod.rs:316 | named typed effect template; name = effect kind; cached/deduped/at-least-once | 12 sh decls total | DISSOLVES into `host` decl (mode columns + executor tail) — explicit in transports kill table | The dissolution is the best-designed part of v6. Residual: v5 `sh` params are UNTYPED holes; the host decl types them (`endpoint: text in`) — strictly better, no loss. |
| `sh!` Mutate | src/ast.rs:726 | idempotency-keyed, exactly-once, never cached | 0 in corpus (grep `^sh!`) | DISSOLVES: tabling key IS the at-most-once guard (transports doc) | Zero corpus uses, but gen-as-write-effect (1e) needs exactly this semantics — do not delete the mechanism while its only future consumer is undecided. |
| `sh*` Stream | src/ast.rs:726 | long-lived subprocess, rows over ticks | 1 (`sh* npm_deps`) | DISSOLVES: seq-column host rel | — |
| effect call `name(args) -> (outs)` (BodyItem::Effect) | src/ast.rs:455, parse/mod.rs:538 | body-position effect invocation inside @async/@stream rules; desugars to ONE runtime model | 14 | DISSOLVES with `->`: outs become plain columns after the `in` block | The session precedent confirmed: `->` bundled mode+tabling+host-boundary; the host decl unbundles it. All 14 sites are the @async examples. |
| `collect(var, n)` in effect args | src/effect.rs:450-466 | request BATCHING: N pending request rows collapse into one effect call with a joined arg (gh pr_batch: 20 PRs per API call) | 2 | **CANNOT YET EXPRESS** — the review verdict names admission/backpressure as an engine gap; request COALESCING is a third thing nobody named | Tiny weight, big principle: this is the Haxl/DataLoader collapse at the effect seam, the anti-N+1 law applied to http. A v6 host rel is probed per bound-key; batching across keys has no vocabulary. Flag for F4. |
| `clock(secs, bucket)` / `every(secs)` builtin rels | src/engine/declare.rs:1190, decls.rs:934 | tick-bucket clock salt driving @async polling | 16 + 3 | DISSOLVES into `interval` host rel with no inputs (in-literal-rxjs vocabulary table) | Review verdict: `interval`'s counter is subscription-local — the store-owned tick counter resolution must cover clock buckets too, or a refcount cycle resets the poll cadence. |
| `diag(path, line, msg[, col, end_line, end_col, severity, code, hint])` sink + Severity | src/engine/decls.rs:249-286, ast.rs:777 | THE diagnostic sink: 9-col by-name envelope; feeds --lsp and --check exit codes; severity error fails --check | 169 | DISSOLVES into a drained rel (transports LSP section shows `diagnostic(...)` push rel) — the LSP wire half is designed; the `--check` EXIT-CODE half (error vs warn, Severity ast.rs:777) is not | 169 uses = every rail in .dl/. The rails are the repo's own CI. Verify E6/--inline defines "run program, exit nonzero iff error-severity rows exist" or the entire rail corpus has no v6 migration path. |
| `repo` / `checkout` head sinks | src/ast.rs:597-598, engine/rpc.rs:471 | derived rows drained post-fixpoint to clone+register repos / check out revs (dynamic corpus pull) | 1 + 1 | UNDECIDED — dynamic corpus growth from inside the program; touches the scan-coordinate hole (1d) | npm-graph.dl pulls package repos it discovers. Multi-repo is the 500-repo thesis; the v6 ingest seam assumes a fixed corpus list (E3 `runCorpus(repos: string[])`). Program-driven corpus expansion is unowned. |

### 1g. Graph builtins (adversarial item 5)

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `closure(edge_rel)` | src/ast.rs:441, parse/ops.rs:431 | transitive closure builtin; Tarjan condensation cached per edge rel | 31 | DISSOLVES — split verdict: the SEMANTICS is just a recursive rule (v6 recursive strata express it today); the IMPLEMENTATION maps to the store's Reach namespace (multi_source_walk) | Same thing? NO — three-way distinction: (a) v5 `closure()` = a cached builtin with a condensation shortcut; (b) v6 reach ns = engine plumbing for reachability-prune retraction (prune=reached, DECISIONS unification C); (c) a v6 recursive stratum = the honest lowering. The reach ns is not a user-facing rel. The dissolution is (c) with (b) as the fast path — but the condensation cache made v5 closure O(condensed) on cyclic graphs; a naive recursive stratum is not. Perf receipt needed (the graph-libs arc already measured the SQL side). |
| `scc(edge_rel)` | src/ast.rs:443, parse/ops.rs:439 | user-visible SCC membership rows (representative, member); shares closure's Tarjan run | 11 | **CANNOT YET EXPRESS** — plain datalog cannot compute SCC membership (needs argmin over cycles or the two-pass); v6 store's `scc_scope/scc_frontier` TEMP tables are RETRACTION internals (DECISIONS: created and referenced by zero other lines), not a query surface | 11 uses (arch conformance, flow panels). Either scc survives as a builtin (host-rel-shaped: edge rel in, membership out — fits the transports model cleanly) or those programs die. Do not confuse the engine's scc retraction machinery for this feature; they share only the name. |
| `node2vec(edge_rel)` | src/ast.rs:448, parse/ops.rs:447 | random-walk skip-gram embedding, top-k cosine pairs; vectors persist in `_node_embeddings` | 2 | KILL or host rel — 2 uses; as a host rel (edge rows in, similarity rows out, tabled on edge digest) it costs v6 nothing to keep the door open | Recompute-guard law applies (CLAUDE.md): v5 guards it with a digest skip; host-rel tabling gives that for free. |

### 1h. Literals and lexical forms

| construct | anchor | what it does | weight | v6 verdict | adversarial note |
|---|---|---|---|---|---|
| `fs:`/`glob:` scheme literals (PathLit) | src/lex.rs:15/Scheme, desc.rs:69-77 | typed path literals with anchor resolution + escape diagnostics | fs: 7, glob: 1 | KILL as literal syntax (plain strings + the type system express it) unless the F4 grammar wants them; near-decorative by count | Their DIAGNOSTICS (path-escapes-root, unknown-anchor) matter more than the syntax; those ride the Type-enum decision (1c). |
| `q:{...}` scheme | desc.rs:74-77 | carrier for the json brace op | 6 | dies or lives with `json()` (1d) | — |
| `/regex/` literals + `$NAME` hole desugar | lex.rs:228-261, parse/desugar.rs | JS-style value-position regex; holes become named captures, repeats dedupe to non-capturing | pervasive (carrier of match_line 87 + comment 58 + `=~` 100) | KEEP wherever `=~`/extraction land — E7 grammar must lex it | The hole-desugar (repeat -> non-capturing, parse/mod.rs tests:882) is subtle earned behavior; port the tests, not just the idea. |
| raw strings `r"..."`, `r#"..."#`, backtick fenced | lex.rs:110-123, :279-308 | escape-free bodies for patterns/yaml/shell | r" 120; backticks pervasive (sh/ast_yaml carriers) | KEEP (parser concern, E7) | — |
| unary minus, parenthesized exprs | parse/mod.rs:807, :848 | `-1` = 0-x desugar (split's negative idx); `(a+b)*2` | low | rides Arith | — |

---

## Part 2 — adversarial findings (the six assignments)

### 2.1 The type system half
Receipts: `<:` brands 0 uses, enum brands 0, hand-written shapes 0, anchors 0, defs 0
— the DECLARATION half of the v5 type system is unadopted and killable today. But
the COLUMN-TYPE half is everywhere (text 1828 / int 950 / file 711 column decls) and
does real work: interning class, spine-join identity (`node` coord cols, ast.rs:57-65),
scan-coordinate typing (repo/rev), and the TypeDiag family (ast.rs:789). v6 has two
unreconciled type stories: the rel-kind algebra (rx lifecycle — hot/cold/replay/origin,
tasks.d.ts) and "lift v5 typecheck.rs" (0723.2 open task, no epic). Neither covers
column types. The E7 parser will freeze a grammar (F4) that must either include `col: ty`
or drop typed decls; that decision is currently being made by default, silently.

### 2.2 Gen/SpliceMode
See 1e. Summary: expressible in the v6 model as write-host-rel + watcher re-ingest +
E2-idempotence termination — the same loop shape as gh-cache etags — but undesigned,
unowned by any epic, and 168 uses deep. The FRP-unidirectional collision dissolves
once the cycle is spelled as crossing ticks on disk; the remaining honest risks are
the write-effect at-most-once guard and extractor-minted spine ids for byte-accurate
Cursor splices. Needs an explicit port-or-cut ruling.

### 2.3 Extraction ops
File-form ops (scan/match_line/ast/match_ast/ast_yaml/jsonp-file/json-file/cmd/comment,
~750 combined uses) survive as extractor capabilities — same semantics, different home
— MODULO two undesigned seams: (a) program-driven demand (per-rule regexes/patterns/
globs must reach the CLI; F2 pins only the spine fact framing), and (b) the scan
coordinate space (arbitrary rev, other repos, program-driven corpus growth via the
repo/checkout sinks) which E2/E3 do not cover. Term-form ops (36 sites) do NOT fit
the files->facts model at all; they are mid-dataflow value extraction, the canonical
demo depends on them, and no v6 word exists. Sized and flagged in 1d.

### 2.4 Interp / PathLit / Call / head arithmetic
Highest-weight: Interp 253, Call ~190 (replace_re 77 top), Arith `+` ~281 lines
(including the text-concat dual role). Decorative: PathLit (fs: 7, glob: 1). The v6
AST expresses none of the heavy three; the transports candidate grammar
already writes `tick - 1`, i.e. the v6 design is authoring programs its own AST
cannot represent. This family is the cheapest big win: pure scalar kernels, no
semantics risk, blocks E5 round-trip fidelity for any real program until landed.

### 2.5 Closure/Scc vs the reach namespace
Different things wearing one vocabulary. closure() dissolves into recursive strata
(semantics) with the store's reach/cascade as implementation; scc() has NO v6
equivalent — the engine's scc_* tables are retraction internals, and SCC membership
is not expressible in stratified datalog without a builtin. 11 scc uses need a ruling.

### 2.6 One-token-many-things and many-tokens-one-thing (found this audit)
- `+` is TWO constructs: int add and text concat, dispatched on inferred type
  (lower.rs:152). If v6 adds arithmetic without the textish branch, 281-line grep
  surface silently changes meaning. Split them in the v6 grammar (`++` or `concat()`).
- `->` was THREE (mode+tabling+host-boundary) — already unbundled by the host decl
  (transports doc). Confirmed against parse sites: ThinArrow serves sh decls (12) and
  effect calls (14) only.
- `:` is FOUR surface jobs on one token: column typing (`col: ty`), named args
  (`col: term`), scheme literals (`fs:x`, space-sensitive lex.rs:315), and op language
  tags (`:rust`, `:zone` mode tags). E7's grammar inherits this ambiguity budget;
  the space-after-colon rule (parse/mod.rs:694) is the current disambiguator.
- `match`/`sg` vs `match_line`/`match_ast`: one construct each, two spellings
  (legacy_name aliases, both grep 0 — the rename is fully absorbed; v6 can drop the
  aliases for free).
- key()+merge() and `@next`-with-MaxBy are secretly ONE feature family:
  lattice/argmax state. v5 spells it three ways (decl qualifier, `@next` rule +
  latest-by-gen, MinBy recursion); the v6 "latest-by-gen lowers into SQL" bookmark
  touches only one spelling. Design them together or the same feature returns thrice.

---

## Part 3 — summaries

### (a) KILL list (nothing replaces it; why safe)
| construct | weight | why safe |
|---|---|---|
| `anchor` decls | 0 | never adopted; refs were deferred in v5 itself |
| `type X <: Y` brands (hand-written) | 0 | zero corpus uses; only tests/it exercise it. Engine-minted enum brands are a builtin-catalog concern, kept separately |
| `type X = "a" \| "b"` enum brands (hand-written) | 0 | same |
| hand-written shapes + `rel r: shape` | 0 + 1 | one example file; the example's real feature is type_decl_row (ruled separately, UNDECIDED) |
| `def` rule templates | 0 | unused; json-rx graphs are the better macro layer |
| `match`/`sg` legacy spellings | 0 | rename fully absorbed |
| `fs:`/`glob:` literal syntax | 8 | near-decorative; strings + column types hold the meaning (keep the path diagnostics with the type decision) |
| `~~` glob cmp | 5 | folds into `=~` when regex-cmp lands |
| node2vec | 2 | keep-the-door-open as a host rel is free; as syntax it can go |

### (b) CANNOT-YET-EXPRESS, ranked by usage weight (the deliverable)
| rank | construct family | weight | one-line hole statement |
|---|---|---|---|
| 1 | extraction demand + coordinates (scan 414 + match_line 87 + match_ast 88 + comment 58 + ast 48 + ast_yaml 6) | ~700 | dissolution named (extractor CLI) but program->CLI pattern plumbing, multi-rev/multi-repo coordinates, and spine-id minting are all undesigned |
| 2 | scalar term computation: Interp 253 + Arith ~307 lines + Call ~190 | ~750 combined (hedged: line counts) | no template/arith/function nodes in the v6 AST; the v6 docs themselves write `tick - 1`; `+` is secretly two ops (add/concat) |
| 3 | `_` wildcard in positive body atoms | ~1525 | v6 `Arg = Var \| Lit` — trivial fix, blocks all real-program parity claims until made |
| 4 | gen codegen (File/Splice/Cursor/Zone + SpliceMode) | 168 | zero v6 words for code generation; port-as-write-host-rel is expressible but undesigned; the codemod product surface hangs on it |
| 5 | diag `--check` exit semantics (Severity error/warn) | 169 | LSP push rel designed; program-as-CI-gate (exit code) unowned — the entire .dl/ rail corpus needs it |
| 6 | `=~` regex comparison (+ `/re/` in the AST) | 100 | v6 CmpOp stops at ge; blocks the rails and half the examples |
| 7 | term-form extraction (jsonp 18 + json 11 + match_ast 7) | 36 | mid-dataflow value extraction; the files->facts seam never sees these strings; gh-cache/E6-ghcacher breaks without it |
| 8 | key()/merge() lattice (MaxBy/MinBy) | 34 | FD-keyed identity + argmax/argmin merge undefined under Z-set weights; MinBy is also the depth-recursion terminator |
| 9 | `use` module imports (stdlib mechanism) | 16 | no module story in any v6 doc; std/ is unreachable without it |
| 10 | scc() membership | 11 | not expressible in stratified datalog; engine scc_* tables are retraction internals, not this |
| 11 | json_group_array/object + json_object (response-body construction) | ~11 | serving JSON needs a constructor; transports doc's `{stars: n}` syntax exists in no grammar |
| 12 | type_decl_row computed shapes | 4 | reflective schema staging; kill loudly or frontier it |
| 13 | `collect(var, n)` effect batching | 2 | request coalescing (anti-N+1 at the http seam) has no host-rel vocabulary |
| 14 | dynamic corpus (repo/checkout sinks) | 2 | program-driven repo pull vs E3's fixed repo list; touches rank 1 |

Ranks 1-2 are dissolution-direction-known/design-missing; ranks 3, 6 are cheap AST
gaps; ranks 4, 5, 7, 8, 10 are genuine model gaps needing owner rulings.

### (c) Count row
| measure | count |
|---|---|
| v5 bespoke surface constructs audited (table rows above, incl. sub-forms) | **62** — 4 temporal/boundary markers, 13 core-rule forms, 9 decl/type forms, 12 extraction ops (file+term counted apart), 4 gen targets + SpliceMode, 7 effect/transport forms, 3 graph builtins, 5 literal forms, 5 builtin sinks/rels (diag, clock/every, repo, checkout, type_decl_row) |
| of which KILL (safe, unused) | 9 |
| of which KEEP as-is (already in v6's vocabulary) | ~10 (neck, neg, cmp-6, 4 aggs, named args, rel decl, facts, raw strings) |
| of which DISSOLVES into a NAMED v6 mechanism | ~18 (the @ family incl. @next previous-tick state, ports, sh/effects/->, clock, query, cmd, closure, jsonp-file...) |
| of which CANNOT-YET-EXPRESS / UNDECIDED | **~25** (list b + the type-system half + use + key/merge) |
| v6 concepts under the current direction | **~27** = 16 rx vocabulary rows (in-literal-rxjs table) + 5 engine-plane gaps (review verdict: transactional propagation, durable state, error/retry, admission, demand-pull/completion) + host decl (modes + executor tail) + fact/response pair + seq column + store-owned tick counter + idempotent-ingest termination + weight retract |
| the honest ratio | v6 currently covers the DATAFLOW core (~28 of 62 constructs keep/dissolve cleanly) and none of the codegen, term-extract, lattice, module, or column-type surfaces. The unification thesis holds for the engine; the LANGUAGE surface is roughly half-mapped. |

Every count above is reproducible: `grep -hoE '<pattern>' examples/*.dl .dl/*.dl std/*.dl | wc -l`
with the patterns as printed in the table rows. Hedges are inline where a pattern
over- or under-counts (prose hits, line-vs-occurrence, kwarg ambiguity).
