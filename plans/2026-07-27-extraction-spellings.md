# extraction_syntax: verdict on the last big surface gap

Lab: `v6/prolog/labs/extraction_syntax.pl`. 48 PASS, exit 0, empty stderr. DELETED
per lab protocol; last lives at commit 2fff3f61, recover via
`git show 2fff3f61:v6/prolog/labs/extraction_syntax.pl`.

Scope: AUDIT finding 17, the largest open surface gap (139 of 173 v5 files use extraction,
the candidate surface has no syntax for it). This is a SPELLING lab. The semantics are
already ruled (transform law: extraction rels are lazy world-fed rels keyed by
(digest, pattern); input-digest salt; the two-salt law; `->` splits program from world
columns; aggregate heads are reserved forms). The machinery behind those rels (git rev/blob/
tree crawling, ast extraction, watchers) is solved v5 rust that enters v6 as link-time binds.
Nothing here re-derives any of it.

## Verdict

**Extraction costs zero new grammar constructs, and it closes a second AUDIT row for free.**

Everything it needs already exists in AGGREGATE's keep-list: the quoted DSL region
`{|lang|| ... |}` (T1), `match(subject, Pattern)` as a body relation over an already-checked
pattern value (T1), named-column atoms with the body-omission-is-wildcard rule (T0),
`from world` / `->` (T1/T5), `Key(Type)` (T0), dot access (T0), struct literals (T0).

What extraction adds is a **library of seven world-fed rels** and **three static laws**, not
syntax. The construct count stays at 28. The still-missing row "regex + path literals"
(AGGREGATE 1a, T1) never becomes a construct: a regex literal is a `{|re|| ... |}` region and
a path literal is a `{|path|| ... |}` region, which is strictly better than `/.../` because
the v5 corpus already contains the escaping pathology `/\/\/.*@eprintln-ok:/` that the region
form spells with zero escapes (graded, `region_body_carries_slashes_unescaped`).

One hole the adversarial grader found and forced closed: the grammar parameter
(`{|sg:rust|| ... |}`) must be a CLOSED set registered by the link-time grammar import, or a
one-character typo silently produces a legal region with a different grammar
(`{|sg:rest|| ... |}` lexed fine before the fix). Closing it forces the rename off ast-grep's
short tags, because `ts` and `js` are one character apart. Graded three ways
(`grammar_names_pairwise_hamming_two`, `ast_grep_short_tags_would_break_the_law`,
`region_rejects_unknown_grammar`).

---

## 1. THE SPELLING TABLE

Format follows AGGREGATE section 2. "v5" is the shape being replaced; "v6" is the proposal.

| # | job | v5 | v6 candidate | why |
|---|---|---|---|---|
| a | select files, live tree | `scan("WORK", "src/**/*.rs", path, rev)` | `file({\|glob\|\|src/**/*.rs\|}, source_file)` | live tree IS the world, so no world argument; no rev out-column, because a live file has no rev, it has a digest carried inside `File`; the glob is a demand column (S4, argued below) |
| b | select files, pinned tree | `scan(repo, "HEAD", "go.mod", path, rev)` | `tree_file(rev, {\|glob\|\|go.mod\|}, manifest)` | `rev` is a variable bound by a body atom over data (`repo_ref`, a pin row, a ratchet row). There is no Rev literal syntax at all, so a rev coordinate is unspellable, not merely discouraged |
| c | regex line extraction | `match_line(path, rev, /^module\s+(?<mod>\S+)/, line)` plus implicit `mod` injection | `match(manifest, {\|re\|\|^module\s+(?<module_path>\S+)\|}, at: span, module_path: module_path)` | one subject value instead of (path, rev); capture names are output column names, bound by explicit named columns |
| d | ast / structural | `match_ast(path, rev, :rust, "$X.unwrap()", line, col, end_line, end_col)` | `match(source_file, {\|sg:rust\|\|$RECEIVER.unwrap()\|}, at: span, receiver: receiver)` | the target grammar is IN the tag because the pattern text must be parsed with that grammar (astgrep_patterns.md:99-110); four span columns collapse into one `at` |
| d' | tree-sitter query | `ast(path, rev, :rust, "(query) @cap", line, end)` | `match(source_file, {\|ts:rust\|\|(call_expression function: (identifier) @callee)\|}, at: span, callee: callee)` | same rel, different pattern language; `@cap` names are the output names |
| e | json path | `jsonp(path, rev, "paths.*.*.operationId", out)` | `match(spec, {\|jsonpath\|\|paths.*.*.operationId\|}, value: op)` | the one unnamed output gets the reserved name `value`; the brace form is `{\|json\|\|{ image: $image }\|}` and its `$name` captures bind the same way |
| f | comments | `comment(path, rev, /open/[, /close/], l0, l1, label)`, arity-overloaded | `comment_span(source_file, at: comment_at)` then `match(comment_at, {\|re\|\|@eprintln-ok:\|})`; paired form is `comment_region(source_file, {\|re\|\|BEGIN: (?<name>.*)\|}, {\|re\|\|END:\|}, at: block_at, label: name)` | v5's one op did two jobs selected by argument count. Split into a lexical view rel and a paired-marker rel, and a regex over a span is just `match` with a span subject |
| g | where the location lives | five loose columns per rel (`line`, `col`, `end_line`, `end_col`, `id`), threaded by hand | every extraction atom exposes `at: FileSpan`; `FileSpan { file, start, end }` with BYTE offsets | one location value; `line_of(span, line: n, col: c)` is a view for the presentation edge only |

Supporting library rels, all `->`-adorned and world-filled, none of them syntax:

    rel file(glob: Glob) -> File;                       // live tree
    rel tree_file(rev: GitRev, glob: Glob) -> File;     // pinned tree
    rel content(digest: Digest) -> Str;                 // lazy blobs
    rel match(subject: File | FileSpan, pattern: Pattern(Lang)) -> ...;
    rel comment_span(subject: File) -> (at: FileSpan);
    rel comment_region(subject: File, open: Pattern(re), close: Pattern(re))
        -> (at: FileSpan, label: Str);
    rel line_of(at: FileSpan) -> (line: Int, col: Int);

`match`'s output column set is pattern-dependent, which is why the pattern must be a
compile-time constant: the checker computes the legal output names from the parsed pattern
and an unknown name is a compile error, never a silent wildcard
(`unknown_output_name_is_an_error`).

### The three static laws this rests on

1. **Capture names ARE output column names.** Regex `(?<name>)`, ast-grep `$NAME`,
   tree-sitter `@name`, json `$name` all mean the same thing: an output column called `name`.
   v5 injected same-named variables implicitly; here the binding is written, so adding a
   capture to a pattern cannot mint or shadow a rule variable at a distance.
   Metavariables lowercase (`$RECEIVER` -> `receiver`) to obey the descriptive-name law
   (`captures_sg_metavars_lowercased`, `captures_ts_at_captures`, `captures_regex_named_groups`).
2. **`at` is reserved.** Every extraction atom exposes `at: FileSpan`; a pattern whose own
   capture is named `at` is refused (`at_is_a_reserved_capture_name`,
   `every_extraction_exposes_at`).
3. **The pattern language set and the grammar set are both CLOSED at link time**, and both
   are chosen so equal-length names are pairwise at Hamming distance >= 2. This is what makes
   the adversarial law hold on the tag; it is not decoration
   (`language_tags_pairwise_hamming_two`, `grammar_names_pairwise_hamming_two`).

### S4 answered: glob residency is a DEMAND COLUMN, not program-text residency

Proposal: the glob is a column on the enumeration rel, on the left of `->`, so it is an
ordinary demand value.

Arguments:

- **500 repos.** Program-text residency makes the walk plan a whole-program static union of
  every glob in every loaded program, recomputed per tick. Demand columns make it exactly the
  set of globs some live rule is currently demanding, with content-addressed dedup already
  ruled: two rules writing the same glob text produce one demand row and one walk, which the
  v5 engine had to implement as a special per-repo-per-rev union pass. Graded:
  `same_pattern_text_one_kernel_rel`.
- **Teardown is already solved.** A glob demand row lives under the demanding scope, so scope
  exit is the ruled range-DELETE, and the walk stops. Program-text residency has no
  corresponding retraction, which is exactly how v5 accumulated walks it no longer needed.
- **A glob can then come from a row.** v5 spelled the multi-repo fan as a magic string
  `scan("*", ...)` that the engine special-cased. With globs as data, a config-driven scan set
  is a plain join and needs no construct.
- **The counter-argument, stated:** a demand column means the walk is one tick behind the
  rule that wants it on first demand. That is the same latency contract the effect model
  already ships and that v5's `rev_cmp_want`/`rev_behind` pair already had (the pin-skew
  comment says "run twice / serve"), so it introduces no new class of surprise.

### Does scan-shaped sugar survive? No.

`scan` dies and is not replaced by sugar. File selection is a plain body atom over a lazy rel.
The receipt is v5's own arity defaulting: `scan/3` = (glob, path, rev_out), `scan/4` =
(rev, glob, path, rev_out), `scan/5` = (repo, rev, glob, path, rev_out). Deleting one argument
from a legal call leaves another legal call whose columns mean different things, with no
diagnostic. Same shape in `comment/6` vs `comment/7` (one regex = sequential dividers, two =
paired BEGIN/END). Both measured: `v5_scan_arity_defaults_are_ambiguous`,
`v5_comment_arity_defaults_are_ambiguous`. The v6 replacement is two differently NAMED rels of
different arity, so no deletion reaches another legal call
(`v6_selection_rels_have_no_arity_overlap`).

---

## 2. DESUGARING INTO THE RULED KERNEL SHAPES

Every spelling above lowers to: a world-filled lazy rel + its key columns + its salt + body
goals. Graded by unification, one check per row.

| surface | kernel rel | key | salt | check |
|---|---|---|---|---|
| `file(G, out)` | `enumerate(live, PatId)(pattern, found)` | `pattern` | `watch` | `desugar_live_selection` |
| `tree_file(Rev, G, out)` | `enumerate(tree, PatId)(rev, pattern, found)` | `rev, pattern` | `none` | `desugar_pinned_selection` |
| `match(File, P, ...)` | `extract(Lang, PatId)(content, pattern, start, end, ...captures)` | `content, pattern` | `input_digest` | `desugar_regex_extraction` |
| `match(Span, P, ...)` | `extract(Lang, PatId)(content, window_start, window_end, pattern, start, end, ...)` | `content, window_start, window_end, pattern` | `input_digest` | `desugar_span_subject_keys_the_window` |
| `comment_span(File, ...)` | `comment_span(content, start, end)` | `content` | `input_digest` | (in `desugar_span_subject_keys_the_window`) |
| `comment_region(File, Open, Close, ...)` | `comment_region(OpenId, CloseId)(content, open, close, start, end, label)` | `content, open, close` | `input_digest` | `desugar_comment_region_two_pattern_columns` |
| `line_of(Span, ...)` | `line_of(content, offset, line, col)` | `content, offset` | `input_digest` | (desugar path exercised by the rail transcription) |

Salt law, per family, consistent with the ruled two-salt law (clock bucket = time recurrence,
input digest = change recurrence, arrival-tick salt rejected):

- live-tree enumeration: `watch`. The bind pushes arrivals; there is no salt column, because
  the recurrence is an outside event, not a program-computed key.
- pinned-tree enumeration: `none`. A rev's tree is immutable, so the demand never re-fires.
  This is the strongest argument for splitting `file` from `tree_file` (S2): they have
  different salts, and one rel cannot have two (`pinned_tree_enumeration_never_refires`).
- extraction: `input_digest`. Re-extract exactly on content change
  (`live_tree_salt_is_input_digest`).

### The FileSpan mint point, and why dedup is correct

No kernel rel has a `FileSpan` column. The kernel returns content-relative `start`/`end`, and
the FileSpan is constructed **at the join**:

    kbind(digest_2, source_file.digest)
    katom(extract(re, PatId), [digest_2, PatId, start_2, end_2])
    kbind(at, FileSpan { file: source_file, start: start_2, end: end_2 })

That is what makes content-addressed dedup correct rather than merely cheap. Measured against
canned world rows: three files, two digests, one shared between two of them. Result is exactly
two distinct kernel extract rows consumed and three distinct FileSpans emitted
(`two_files_one_digest_share_one_extract_row`, `two_files_one_digest_get_distinct_spans`,
`changed_digest_demands_a_new_extract_row`, `filespan_minted_at_the_join`). Had the kernel rel
carried the File, the same content in N checkouts would be N extraction runs, which at the
500-repo target is the difference the digest-dedup line in the spine plan is asking for.

### The adversarial law, and its honest scope

"No single-character perturbation of a legal spelling silently yields a different legal
spelling." Graded exhaustively over every fence position of three region spellings, under
deletion and under substitution from an 18-character alphabet
(`fence_perturbation_never_silently_legal`).

Scope stated as a deviation: the law applies to the OUTER grammar, the fence and the tag.
Inside a region the sublanguage owns its own meanings, and a one-character edit to a regex
legitimately changes what it matches, exactly as inside any string literal. The grader
excludes body positions on purpose.

The first run of this check FAILED, which is the lab's most useful output: the grammar
parameter was an open identifier, so `{|sg:rust||` -> `{|sg:rest||` lexed to a different legal
region. The fix is the closed grammar set above.

---

## 3. THE THREE TRANSCRIPTIONS

Every construct used below is on AGGREGATE's keep-list or carries a proposal row in section 4.
Mechanically checked: `eprintln_rail_constructs_all_budgeted`,
`pin_skew_constructs_all_budgeted`, `flow_services_constructs_all_budgeted`. No `WORK` and no
`HEAD` appears anywhere (`no_rev_or_world_literal_anywhere`).

### 3a. `.dl/no-new-eprintln.dl` (lint rail: scan + match_line + comment waivers + aggregates + ratchet)

```
enum Severity { Error, Warning }

rel eprintln_hit(at: FileSpan);
eprintln_hit(at) <-
    file({|glob||src/**/*.rs|}, source_file),
    match(source_file, {|re||eprintln!|}, at: at);

rel eprintln_waiver(at: FileSpan);
eprintln_waiver(at) <-
    file({|glob||src/**/*.rs|}, source_file),
    comment_span(source_file, at: comment_at),
    match(comment_at, {|re||@eprintln-ok:|}, at: at);

rel eprintln_waived(at: FileSpan);
eprintln_waived(hit_at) <-
    eprintln_hit(hit_at),
    eprintln_waiver(waiver_at),
    hit_at.file == waiver_at.file,
    line_of(hit_at, line: hit_line),
    line_of(waiver_at, line: waiver_line),
    waiver_line >= hit_line - 1,
    waiver_line <= hit_line;

rel eprintln_counted(at: FileSpan);
eprintln_counted(at) <- eprintln_hit(at), !eprintln_waived(at);

rel eprintln_count(source_file: File, hits: Int);
eprintln_count(source_file, count(at)) <-
    eprintln_counted(at),
    source_file := at.file;

rel eprintln_baseline(path: Key(Path), allowed: Int);
eprintln_baseline({|path||src/config.rs|}, 1);
eprintln_baseline({|path||src/daemon/client.rs|}, 2);
eprintln_baseline({|path||src/setup/vscode.rs|}, 1);
eprintln_baseline({|path||src/setup/wire.rs|}, 1);

diag(at: FileSpan { file: source_file, start: 0, end: 0 },
     severity: Warning,
     code: "eprintln-exceeded",
     msg: "eprintln! use grew past this file's grandfathered baseline") <-
    eprintln_count(source_file, hits),
    baseline_path := source_file.path,
    eprintln_baseline(baseline_path, allowed),
    hits > allowed;

diag(at: at,
     severity: Warning,
     code: "eprintln-new-file",
     msg: "new eprintln! outside the grandfathered baseline") <-
    eprintln_counted(at),
    baseline_path := at.file.path,
    !eprintln_baseline(baseline_path, _);

diag_stage("eprintln-exceeded", "agent-turn");
diag_stage("eprintln-exceeded", "commit");
diag_stage("eprintln-new-file", "agent-turn");
diag_stage("eprintln-new-file", "commit");

? diag(at, severity, code, msg);
```

Three things changed shape, all of them arguable improvements and all of them stated:

1. **v5's two waiver rules collapse to one.** v5 needed `comment(...)` for whole-line waivers
   AND a separate `match_line(/\/\/.*@eprintln-ok:/)` for trailing ones, because
   `src/comment.rs` keys on a line-leading marker. Its own comment records the incident (the
   axum-arc gap, two daemon_cmd sites that needed whole-line conversion to take effect). In
   v6, `comment_span` is a lexical view over comment regions, leading or trailing, so one rule
   covers both and the v5 defect is not expressible.
2. **The baseline is keyed on `Path`, not `File`.** A `File` carries a digest, so a
   `File`-keyed ratchet row would invalidate on every edit. `at.file.path` is the join key.
   This is a real consequence of the spine's type shape and it is easy to get wrong.
3. **The line range join costs two `line_of` atoms.** The waiver rule thinks in lines
   (same line or the line above), and FileSpan is bytes. Cost: two extra body atoms per rail
   that reasons about line adjacency. See ambiguity 8.

### 3b. `examples/pin-skew.dl` (cross-repo: manifests -> pins -> demand -> rev_behind)

```
enum RevLookup { Found { rev: GitRev }, Unresolvable { reason: Str } }

rel git_repo(repo: Key(GitRepo));
rel repo_default_ref(repo: Key(GitRepo), ref_name: RefName) from world;
rel repo_ref(repo: Key(GitRepo, 1), ref_name: Key(RefName, 2), rev: GitRev) from world;
rel resolve_rev(repo: GitRepo, ref_text: Str) -> RevLookup;
rel rev_behind(repo: GitRepo, rev: GitRev, base: GitRev) -> (behind: Int, ahead: Int);

rel default_tree(repo: GitRepo, rev: GitRev);
default_tree(repo, rev) <-
    git_repo(repo),
    repo_default_ref(repo, ref_name),
    repo_ref(repo, ref_name, rev);

rel module_id(repo: GitRepo, module_path: Str);
module_id(repo, module_path) <-
    default_tree(repo, rev),
    tree_file(rev, {|glob||go.mod|}, manifest),
    match(manifest, {|re||^module\s+(?<module_path>\S+)|}, module_path: module_path);

rel gomod_pin(consumer: GitRepo, module_path: Str, version: Str);
gomod_pin(consumer, module_path, version) <-
    default_tree(consumer, rev),
    tree_file(rev, {|glob||go.mod|}, manifest),
    match(manifest, {|re||(?<module_path>\S+/\S+)\s+(?<version>v[0-9]\S*)|},
          module_path: module_path, version: version);

rel pin(consumer: GitRepo, dep: GitRepo, ref_text: Str);
pin(consumer, dep, version) <-
    gomod_pin(consumer, module_path, version),
    module_id(dep, module_path),
    consumer != dep;

rel pinned_rev(dep: GitRepo, ref_text: Str, rev: GitRev);
pinned_rev(dep, ref_text, rev) <-
    pin(_, dep, ref_text),
    resolve_rev(dep, ref_text, Found { rev: rev });

rel unresolvable_pin(dep: GitRepo, ref_text: Str, reason: Str);
unresolvable_pin(dep, ref_text, reason) <-
    pin(_, dep, ref_text),
    resolve_rev(dep, ref_text, Unresolvable { reason: reason });

rel stale_pin(consumer: GitRepo, dep: GitRepo, ref_text: Str, behind: Int);
stale_pin(consumer, dep, ref_text, behind) <-
    pin(consumer, dep, ref_text),
    pinned_rev(dep, ref_text, pinned),
    default_tree(dep, tip),
    rev_behind(dep, pinned, tip, behind: behind),
    behind > 0;

rel diverged_pin(consumer: GitRepo, dep: GitRepo, ref_text: Str);
diverged_pin(consumer, dep, ref_text) <-
    pin(consumer, dep, ref_text),
    pinned_rev(dep, ref_text, pinned),
    default_tree(dep, tip),
    rev_behind(dep, pinned, tip, ahead: ahead),
    ahead > 0;

? stale_pin(consumer, dep, ref_text, behind);
? diverged_pin(consumer, dep, ref_text);
? unresolvable_pin(dep, ref_text, reason);
```

Four differences from v5, each one doing work:

1. **`rev_cmp_want` disappears.** v5 needed a separate demand rel and a convention ("the
   built-in `rev_behind` fills next tick"). In v6 the effect rel IS the demand rel: a body
   atom over `rev_behind` with its program columns bound and its world columns unbound is the
   request. The convention became the type. This is the spine plan's own prediction, spelled.
2. **`"HEAD"` is gone.** v5 passed the string `"HEAD"` twice as a rev coordinate. v6 joins
   `repo_default_ref` (world-fed from config or discovery), so the default branch is a value
   somebody configured, not a token in a rail. `default_tree` names the join once.
3. **The prose limit became an arm.** v5's honest-limits comment says a go.mod pseudo-version
   is used verbatim and "those pairs skip loudly". In v6 the envelope enum makes it a value:
   `Unresolvable { reason }` is a row with its own ask, so "skips loudly" is a query result
   instead of a paragraph. Failure-is-a-value, applied to an already-existing v5 comment.
4. **`resolve_rev` is a separate hop.** v5 handed a raw ref string to `rev_behind`; v6
   resolves text to a `GitRev` first, so `rev_behind`'s columns are typed revs and the
   unresolvable case has exactly one place to live.

### 3c. `examples/flow-services.dl` (openapi jsonp extraction + cross-repo wire hop)

```
use "std/flow";

rel service_op(op: Str);
service_op(op) <-
    file({|glob||**/openapi.yaml|}, spec),
    match(spec, {|jsonpath||paths.*.*.operationId|}, value: op);

rel op_endpoint(op: Str, fn_bare: Str, sym: Str);
op_endpoint(op, fn_bare, sym) <-
    service_op(op),
    call_name(sym, op),
    fn_bare := replace_re(sym, {|re||^[^:]*::|}, "");

rel wire_call(op: Str, at: FileSpan);
wire_call(op, at) <- service_op(op), call_node(_, op, at);

flow_edge(argument, parameter) <-
    service_op(op),
    call_node(call, op, _),
    df_arg(call, position, argument),
    op_endpoint(op, fn_bare, _),
    df_node(parameter, kind: "param", fn: fn_bare),
    df_param(parameter, position);

flow_edge(returned, call) <-
    service_op(op),
    call_node(call, op, _),
    op_endpoint(op, fn_bare, _),
    df_node(returned, kind: "ret", fn: fn_bare);

rel service_reach(from_node: Str, to_node: Str);
service_reach(from_node, to_node) <- closure(flow_edge);

? service_op(op);
? wire_call(op, at);
? op_endpoint(op, fn_bare, sym);
? service_reach(from_node, to_node);
```

Notes:

1. **The multi-repo fan needs no program change.** v5's own header instructs the reader to
   edit `scan("WORK", ...)` into `scan("*", ...)` for the cross-repo run. In v6 the live tree
   IS the configured world, so the same two-line rule fans over one repo or five hundred, and
   which repos are live is a deployment fact. Consequence, stated as ambiguity 10: a
   deliberately single-repo rail now has to say so.
2. **`call_node` loses two columns and `df_node` loses four.** v5's `call_node(c, op, f, l)`
   becomes `call_node(call, name, at)`; v5's `df_node(param, "param", _, fnb, _, _, _)` with
   five underscores becomes a named-column body atom binding two columns. That is spelling (g)
   applied to the builtin catalog, and it is where the FileSpan collapse pays off most.
3. **v5's `src` rel is dead code** (declared, filled by a `scan`, read by nothing). Dropped;
   it is counted as removed in the line table below, not as a v6 saving.

---

## 4. LINE COUNTS AND CONSTRUCT BUDGET

### Line counts (non-blank, non-comment)

| program | v5 | v6 | delta | why |
|---|---|---|---|---|
| `.dl/no-new-eprintln.dl` | 53 | 50 | -3 | one fewer waiver rule; two extra `line_of` atoms; four `{\|path\|\| \|}` fact lines the same length as v5's strings |
| `examples/pin-skew.dl` | 15 | 52 | +37 | see decomposition below |
| `examples/flow-services.dl` | 30 | 30 | 0 | 5 spec-scan lines become 2; `src` dead rel removed; the `df_node` underscore padding removed; `use` and asks unchanged |

Extraction alone is a wash or better. The pin-skew growth is NOT extraction and it should not
be charged to this lab; decomposed:

| cause | lines | charge |
|---|---|---|
| declaring the library rels v5 left as undeclared engine builtins (`git_repo`, `repo_default_ref`, `repo_ref`, `resolve_rev`, `rev_behind`) | 5 | belongs in a std library, not per program |
| v5 packs 3 goals per physical line; v6 as written puts one goal per line | ~14 | formatting, not language |
| `default_tree` replacing the `"HEAD"` literal | 5 | the rev-spine inversion's real cost |
| the `Unresolvable` arm + enum + ask (content v5 had only as a comment) | 8 | new capability, not overhead |
| `resolve_rev` as a separate hop + `pinned_rev` rel | 5 | typing revs instead of passing ref strings |

Extraction-only comparison, which is the number this lab owes: going from nothing to a file's
captures costs **2 body goals in both v5 and v6**
(`scan(...)`, `match_line(...)` versus `file(...)`, `match(...)`), and v6 threads **one**
value between them instead of two (`source_file` instead of `path` + `rev`).

### New constructs and their budget cost

Budget stands at 28 after today's cuts. This lab proposes **zero grammar constructs**, so it
stays at 28, and it closes two AUDIT rows.

| proposal | grammar cost | what it actually is | argument |
|---|---|---|---|
| pattern language tag set `{re, sg, ts, json, jsonpath, glob, path}` | 0 | values in a construct AGGREGATE already keeps | closes the "regex + path literals still missing" T1 row without minting a construct; the region form needs zero escapes where `/.../` needs two on the real corpus regex |
| grammar tag parameter `{\|sg:rust\|\| ... \|}` | 0 | micro-syntax inside the existing tag | the target grammar must be known to PARSE the pattern text, so it cannot be a separate argument outside the region (astgrep_patterns.md:99-110) |
| closed grammar name set, Hamming >= 2 | 0 | a link-time obligation on the already-kept grammar-import construct | the adversarial grader found the hole; without closure a one-character typo is a silent re-spelling. Forces the rename off ast-grep's `ts`/`js` short tags |
| reserved output name `at` | 0 | one reserved name | FileSpan in every extraction output |
| capture-names-are-column-names | 0 | a static law | replaces v5's implicit variable injection; makes a typo a compile error |
| left-of-`->` IS the demand key, in declaration order | 0 | a reading that REMOVES `Key()` wrappers from effect declarations | feeds ruling Q8; see ambiguity 6 |
| multiplicity as a link-time bind obligation | 0 | avoids a `Set(T)` result wrapper next to `Stream`/`Tail` | LANG.md:72's open question answered without a type; see ambiguity 7 |
| seven stdlib rels (`file`, `tree_file`, `content`, `match`, `comment_span`, `comment_region`, `line_of`) | 0 | library names | the whole point: extraction is a library over ruled constructs |

Checked mechanically: `new_constructs_cost_zero_grammar`.

---

## 5. DEVIATIONS FROM LANG.md AND AGGREGATE

1. LANG.md's Surface section has no file, path, glob, regex or AST anything. This lab does not
   add to that section; it adds to the T1 quoted-region row and to the stdlib. If the reader
   expected new keywords, that expectation is the deviation.
2. The adversarial law is graded on the OUTER grammar only. Body positions inside a region are
   excluded on purpose and the exclusion is in the grader, not hidden in prose.
3. AGGREGATE's grammar-import row does not state a closed-set obligation. This lab adds one,
   with a graded receipt, and it is the only place the lab makes a construct's contract
   stricter rather than looser.
4. `Key()` wrappers are dropped from the left of `->` in the transcriptions. AGGREGATE's Q8
   ruling says both live; this lab reads the arrow as already carrying the key and would
   otherwise be respelling the same fact twice. Flagged, not settled (ambiguity 6).
5. The `.pl` grades spellings, desugarings and one tiny world evaluation. It does not
   implement extraction, and it does not mock a regex engine, an ast-grep, or git.

---

## 6. AMBIGUITIES (numbered)

1. **Glob residency (S4).** Proposed: demand column on the enumeration rel, argued above.
   The rejected alternative is per-rule program-text residency with an engine-side union walk
   (v5's shape). Cost of the proposal: first-demand latency of one tick.
2. **Capture-name binding syntax.** Proposed: explicit named-column outputs
   (`module_path: module_path`). Alternatives: implicit same-name injection (v5, rejected
   because a new capture silently mints or shadows a rule variable) and a punning shorthand
   (`match(f, P, :module_path)`, not proposed because it is a fourth way to write a binding).
   The repetition when names match is real and is the price.
3. **How a region names its language and grammar.** Proposed: `{|engine:grammar|| ... |}`,
   both from closed sets. Alternatives: grammar inside the body (unparseable without knowing
   it first), grammar as a separate atom argument (v5's `:rust`, same problem), grammar
   inferred from the subject file's extension (breaks the compile-time check, since the
   subject is a runtime value).
4. **Fence escape.** A region body containing `|}` is unspellable; the lexer reports
   `text_after_close_fence`. No fence-extension form (a counted or custom delimiter) is
   proposed, because no corpus pattern needs one today. If one ever does, the form is a design
   decision, not a bug fix.
5. **Does scan-shaped sugar survive?** Proposed: no. File selection is a body atom over a
   lazy rel, no arity defaults ever, distinct names for distinct shapes. Cost: `tree_file` and
   `file` cannot share a name, so a program that wants to be rev-agnostic writes both.
6. **`Key()` on the left of `->`.** Is the arrow's left side automatically the demand key in
   declaration order (this lab's reading, which removes wrappers), or must uniqueness be
   respelled with `Key(T, n)` (the spine plan's own examples do it inconsistently)? Feeds
   ruling Q8.
7. **Many rows per demand.** Enumeration and extraction both return a SET per demand row. Is
   that a mode discharged at link time by the bind (proposed, zero syntax), or a third result
   wrapper `Set(T)` beside `Stream(Item, End)` and `Tail(Item)`? LANG.md:72 leaves it open and
   shell_stream only ruled the streaming case.
8. **`span.line` sugar.** The waiver range join costs two `line_of` atoms. Should
   `at.line` desugar to the view join? Argument against: it would be the only dot access that
   is a join rather than a projection, which breaks what dot access means everywhere else.
   Argument for: line-adjacency rails are common and two atoms per rail is a visible tax.
9. **Metavariable case.** Proposed: `$RECEIVER` binds `receiver`, lowercased, to obey the
   descriptive-name law. Open: what happens when a pattern language is case-sensitive about
   metavariables and `$FOO` and `$foo` are different (ast-grep treats lowercase `$foo` as a
   literal, so the collision may be vacuous, but this was not verified against ast-grep source
   in this lab).
10. **Default repo scope of the live tree.** `file(glob, out)` ranges over the whole
    configured world, so a single-repo rail must join `this_repo` to say so. v5's default was
    the opposite (`"."` self, `"*"` opt-in fan) and 90% of the corpus is single-repo. Which
    default is right is a deployment-ergonomics call, not a semantics call.
11. **The reserved name `at`.** One reserved output name, chosen for brevity at every call
    site. Alternatives considered and not argued here: `span`, `loc`, or making the FileSpan
    the atom's first positional output instead of a named one.
12. **Is `from world` just the nullary-demand case of `->`?** `rel repo_ref(...) from world`
    and `rel file(glob) -> File` differ only in whether there are program columns to the left
    of the arrow. If they unify, T1 loses a construct and the budget drops to 27. Not
    proposed, because `from world` reads better on rels nobody demands and the merge is a
    separate argument from this lab's.
13. **Runtime-built patterns.** astgrep_patterns.md keeps `parse_pattern` at runtime for
    patterns held in rows, with an `ok/bad` refusal channel. Such a pattern cannot be
    monomorphized, so it cannot have a compile-time output column set. Does a row-held pattern
    get to key a kernel extract rel at all, and if so what are its output columns? Not
    resolved here.
14. **`comment_span` sees trailing comments.** Proposed yes, which is what collapses v5's two
    waiver rules into one and makes the axum-arc gap inexpressible. This is a behavior claim
    about the lexical view, and it needs a bind that can actually deliver it (v5's
    `src/comment.rs` cannot; it keys on a line-leading marker).

---

## 7. WHAT THIS MEANS FOR THE TIER ORDER

- **T1 is unblocked and does not grow.** AUDIT's ranked list puts extraction first: "until the
  language can name a file, every other decision is being made on a program shape the language
  cannot host." The answer is that T1 already had the constructs; what was missing was the
  library and three laws. The T1 row count stays at 4.
- **AUDIT finding 17's three resolution options are all rejected as stated.** Not a bound
  effect with a `Files { paths }` envelope (that makes the glob a streaming envelope and buys
  nothing over a demand column); not a builtin rel family with no surface (that loses the
  compile-time pattern check, which is the one thing worth having); not "extraction stays in
  rust and the language consumes its output rels" (that is true of the MACHINERY and false of
  the SPELLING, and conflating them is what left the gap open). The fourth answer is the one
  above: ruled constructs plus a library.
- **surface_dcg's owed work grows by one item and shrinks by one.** It still owes the raw-text
  region token (already owed, astgrep). It no longer owes regex or path literal tokens. It
  newly owes the closed-set check on tag and grammar names, which is a link-time table lookup,
  not lexing.
- **The grammar-import row (T1) gains an obligation.** It must register the grammar NAME set,
  not only the node types, and the registered names must satisfy the Hamming property. That is
  a checker, and it is cheap.
- **Ruling Q8 gains a receipt.** Every extraction rel in this lab is `->`-adorned with its
  demand key on the left and no `Key()` wrapper. If Q8 rules that Key must be respelled on
  effect rels, every declaration in section 1 grows wrappers and nothing else changes, so this
  lab does not force the ruling, it just prefers one side loudly.
- **The spine plan's S2 gets an argument.** `file` and `tree_file` must be separate rels
  because they have different SALTS (`watch` versus `none`), not merely different keys. A
  single rel cannot carry two recurrence rules. That is a stronger reason than the one in
  plans/2026-07-27-fs-rev-spine.md, which framed the split as a modelling preference with a
  join cost.
