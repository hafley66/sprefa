# JSON query language -- archaeology and grading

Paper lane. Zero production edits; no new syntax lands anywhere. Base
`6c3a7e2d`. Companion to the current-world record
`plans/2026-07-30-json-interop-lab.pl` (12 receipts, 5 open cards, locked
one-rel boundary).

Archives searched: `~/projects/sprefa-archive-20260701` (`v3/`, `v4/`,
`MAIN.md`, `TASKS.md`, `human-goals.md`, `llm-notes.md`) and
`~/projects/sprefa-archive-20260428`.

---

## 0. The correction that reframes the request

The brief assumed the json language is lost in v3/v4 and must be brought back.
It is not lost. It is **alive, shipped, and tested at the repo root**, because
the repo root *is* v5.

    src/datapath.rs:1     //! Data-file extraction over json/yaml/toml: `jsonp` is the dotted-path
    src/datapath.rs:2     //! evaluator (`jsonp(p, rev, "a.b.*", out)`); `json` is the declarative
    src/datapath.rs:3     //! brace-pattern walker (`json(p, rev, q:{ $k: $v })`). Both dispatch on file
    src/datapath.rs:4     //! extension; each hit carries a byte span for the ref spine.

The lineage is **five** generations, not two. The brace pattern is the single
longest-surviving idea in the project -- it has outlived four rewrites with its
inner grammar essentially intact, and only the *outer call syntax* has churned:

| gen | where | json surface | status |
|---|---|---|---|
| v1 | `archive-20260428/crates/` | `json({ image: { repository: $REPO, tag: $TAG } })` | archive only |
| v2 | `archive-20260428/v2/` | same grammar + `$$sigil($VAR)` scan annotations | archive only |
| v3 | `archive-20260701/v3/` | `json({ package: { name: ${N?} } })`, parens, extension dispatch | archive only |
| v4 | `archive-20260701/v4/` | ``json(:toml)`{ ... ${SLUG?} ... }` `` -- backtick body, explicit `:fmt`, adds `$$${PATH?}` | archive only |
| **v5 (repo root)** | `src/datapath.rs` | **`json(path, rev, q:{ ... })` + `jsonp(...)`** | **LIVE, tested** |
| v6 | `v6/prolog/` | `decode/2` over a **declared struct type** | narrowed; most of the surface refused |

Correction to the brief worth stating plainly: the `${N?}` spelling the user
associates with "v3/v4" was **never** v1/v2 syntax. v1/v2 ran bare `$NAME` and
the then-current parser *rejected* `?` inside `${...}`
(`chat_log/20260425.5.v3-json-brace-walker-and-ast-grep-ports.md:16`: "the
current brace parser rejects `?` in `${...}` names, so the smoke as-edited will
likely fail"). `${N?}` was a user hand-edit to a fixture that forced the
grammar change. The feature was demanded into existence, not designed in.

So the real question is not "recover it from v3/v4" but "**v6 dropped most of
v5's json surface -- which parts come back, and at what cost under the locked
one-rel boundary**". §5 grades exactly that.

`v3`'s sigil literally still parses in v5 -- the compatibility bridge survived
three rewrites:

    src/datapath.rs:1368-1374
        #[test]
        fn braced_capture_optional_suffix() {
            let (_, v) = obj_one_entry("{ n: ${N?} }");
            assert!(matches!(&v[0], Step::Leaf { capture: Some(ref n) } if n == "N"));
            let (_, c) = parse_pattern("{ n: ${N?} }").unwrap();
            assert_eq!(c, vec!["N".to_string()]);
        }

---

## 1. The mature design (v5, live) -- this is "the tight one"

### 1.1 Grammar, verbatim

`src/datapath.rs:445-467`:

    // ── declarative brace pattern (the `json` op) ──────────────────────────────
    //
    // Ported (trimmed) from v4's cst/dsls/json/walk brace grammar. Parses the body
    // of a `json` brace pattern into a `Step` tree plus the capture names it binds.
    // `run_pattern` walks the same tree-sitter descent that powers `run_data`, but
    // over a `Step` tree instead of dotted segments. Captures bind as rule vars,
    // mirroring match's named groups.
    //
    // Grammar:
    //   pattern    = object | array | capture | wildcard | value_glob | quoted
    //   object     = "{" (entry ("," entry)*)? "}"
    //   entry      = key ":" pattern
    //   key        = "**" | "$" NAME | "$_" | "re:" REGEX | glob_or_exact
    //   array      = "[" "..." pattern "]"
    //   capture    = "$" NAME        # value position, binds the value
    //   wildcard   = "$_"            # value position, matches any, no bind
    //   value_glob = (not , } ] )+   # literal value (exact match)
    //   quoted     = '"' ... '"'     # literal value, or LeafPattern if it has `$`
    //
    // Dropped from v4 (host-grammar artifacts): `$$sigil(...)` annotations, the
    // `${rule.$VAR}` cross-ref form, the `$$${PATH?}` recursive-capture key. `**`
    // recursive descent is retained; AnyCapture is reserved for a future naming
    // surface.

Nine productions. That is the whole language. **That is the tightness.**

### 1.2 The IR it compiles to

`src/datapath.rs:470-500`:

    pub enum Step {
        /// `**` recursive descent (binds nothing yet).
        Any,
        /// Recursive descent that binds the dot-joined traversed key path.
        AnyCapture { capture: String },
        /// Value position: matches any scalar/object/array; binds the value if `Some`.
        Leaf { capture: Option<String> },
        /// Value position: literal exact match (a value_glob or a quoted string).
        LeafEq { text: String },
        /// Value position: pattern over the value text (a quoted string containing `$`).
        LeafPattern { pattern: String },
        /// Object pattern: each entry is (key matcher, sub-pattern).
        Object { entries: Vec<(KeyMatcher, Vec<Step>)> },
        /// Array spread: `[... pattern]`.
        Array { item: Vec<Step> },
    }

    pub enum KeyMatcher {
        Exact(String),
        /// Glob or `re:` regex (matched in Step 5).
        Glob(String),
        /// `$NAME` — binds the key text.
        Capture(String),
        /// `$_` — matches any key, binds nothing.
        Wildcard,
        /// Reserved for a future recursive path-capture key.
        Recursive(String),
    }

`parse_pattern` returns `(Vec<Step>, Vec<String>)` -- steps plus **capture names
in first-seen order**, deduped; a repeated `$NAME` is the same var referenced
twice (`src/datapath.rs:503-504`).

### 1.3 Semantics, pinned by tests (verbatim assertions)

One row per match; every capture in the match binds simultaneously as a dl rule
var. Nesting is **descent inside one match**, not a join.

    src/datapath.rs:1502-1509   // multi-entry object, flat leaf AND nested descent, ONE row
        let json = r#"{"number": 7, "user": {"login": "alice"}}"#;
        let ms = run_json("{ number: $n, user: { login: $a } }", json);
        assert_eq!(ms.len(), 1, "{ms:?}");
        assert_eq!(ms[0]["n"], "7");
        assert_eq!(ms[0]["a"], "alice");

Array-of-objects fan-out with correlated sibling + nested fields -- one row per
element:

    src/datapath.rs:1512-1524
        let json = r#"[{"number":1,"user":{"login":"a"}},{"number":2,"user":{"login":"b"}}]"#;
        let mut got: Vec<(String, String)> =
            run_json("[... { number: $n, user: { login: $u } }]", json)
        assert_eq!(got, vec![("1".into(), "a".into()), ("2".into(), "b".into())]);

Key capture iterates entries:

    src/datapath.rs:1291-1298
        let (k, v) = obj_one_entry("{ $K: $V }");
        assert!(matches!(k, KeyMatcher::Capture(ref n) if n == "K"));
        assert!(matches!(v[0], Step::Leaf { capture: Some(ref n) } if n == "V"));

Recursive descent at any depth:

    src/datapath.rs:1487-1494
        // `{ **: { image: $i } }` finds the image key at any depth.
        let json = r#"{"a":{"b":{"image":"deep"}},"top":1}"#;
        let ms = run_json("{ **: { image: $i } }", json);
        assert!(images.contains(&"deep".to_string()), "{:?}", images);

Regex and glob keys:

    src/datapath.rs:1532-1542
        // `re:^v` matches keys starting with 'v'.
        let ms = run_json("{ re:^v: $val }", r#"{"v1":"x","v2":"y","other":"z"}"#);
        assert_eq!(vals.len(), 2, "{:?}", vals);

    src/datapath.rs:1543-1549
        // `*id` matches any key ending in 'id'.
        let ms = run_json("{ *id: $v }", r#"{"uid":"a","gid":"b","name":"c"}"#);
        assert_eq!(ms.len(), 2, "{:?}", ms);

Array spread binds each item:

    src/datapath.rs:1526-1531
        let ms = run_json("{ tags: [...$t] }", r#"{"tags":["a","b","c"]}"#);
        assert_eq!(tags, vec!["a", "b", "c"]);

Non-match is silent (no row, no error) -- `missing_key_yields_no_match`,
`src/datapath.rs:1460-1464`.

Bindings carry byte spans into the ref spine:

    src/datapath.rs:1467-1477
        fn bindings_carry_byte_spans() {
            let ms = run_pattern("d.json", r#"{"name":"x"}"#, &steps);
            let (_, _, lo, hi) = &ms[0][0];
            assert_eq!(&content[*lo..*hi], "x");
        }

Format dispatch is by extension over one tree-sitter descent -- the same pattern
grammar reads JSON, JSONL/NDJSON, YAML and TOML (`fmt_of`, `src/datapath.rs:18-25`);
`tests/it/data_ops.rs` pins `json_declarative_over_yaml` and
`json_declarative_over_toml` with `q:{ $k: $v }` unchanged across all three.

### 1.4 What it looks like in a real program

The flagship, `examples/gh-cache.dl:114-124` -- comment is the author's own:

    # A LIST endpoint (e.g. `gh api repos/cli/cli/pulls`) returns a JSON ARRAY of
    # objects. One `json` brace pattern normalizes the whole array into one row per
    # element, with sibling fields correlated and nested fields (user.login) descended
    # in the same match — the full ghcacher `pull_request` row, no Rust, no per-field
    # rule. Add `watch("repos/cli/cli/pulls").` to poll it.
    rel pull_request(ep: text, num: text, title: text, state: text, author: text).
    pull_request(ep, num, title, state, author) <-
        resp_current(ep, _, body),
        json(body, q:[... { number: $num, title: $title, state: $state,
                            user: { login: $author } } ]).

Schema inference off a live document, `examples/type-from-json.dl:22-25`:

    # key/value pairs: the declarative `json` pattern binds the key metavar $key and
    # the value metavar $value as dl vars (term form, over the bound `body` string).
    rel payload_kv(key: text, value: text).
    payload_kv(key, value) <- sample(body), json(body, q:{ $key: $value }).

### 1.5 Two forms, one op

- **FILE form** `json(path, rev, q:{...})` -- a source op, scans a file.
- **TERM form** `json(src, q:{...})` -- parses a **bound string column** (an HTTP
  response body), no file, no rev; runs in the hybrid join+extract pass
  (`src/ast.rs:767-785`, `tests/it/data_ops.rs:210-226`).

`jsonp` is the dotted-string sibling: `jsonp(p, rev, "paths.*.*.operationId", op)`
(`examples/flow-services.dl:42`). `*` matches any object key or array index.
Passing a string to `json(` is a parse error that redirects you to `jsonp`
(`src/parse/ops.rs:463-464`), which is why the two never blur.

Carrier is a `q:` PathLit, deliberately **not** a string, so it stays
structured and highlightable (`src/desc.rs:78`, `TASKS.md:164`).

### 1.6 Construction (the other half)

v5 also *builds* JSON, via sqlite json1 in head position -- `examples/json-out.dl`:

    rel group_rels(rel_group: text, names_json: text).
    group_rels(rel_group, json_group_array(rel_name)) <-
        rel_catalog(rel_name, rel_group, cols, doc).

    rel group_json(payload: text).
    group_json(json_object("group", rel_group, "rels", json(names_json))) <-
        group_rels(rel_group, names_json).

    rel catalog_json(payload: text).
    catalog_json(json_group_array(json(payload))) <- group_json(payload).

Its header states the design rule that matters for the current cards:

    # dl stays flat in storage; nesting is OUTPUT, built by SQLite's own json
    # functions. ... Composition is by STRATIFICATION across two derived
    # relations, no new value model.

That sentence is the v5 answer to `json_residency` and to the one-rel boundary,
written years before the lab asked the question.

### 1.7 Known gaps the author already logged

`TASKS.md` (archive root) is the v5 `feat/json-declarative-pattern` follow-up
list -- shipped Steps 1–7, suite green. Its own support matrix:

    | shape | works? | note |
    | `{ a: $x }` | yes | single exact key, leaf |
    | `{ $k: $v }` | yes | single capture key → iterates entries |
    | `{ a: { b: $x } }` | yes | single key, nested descent (any depth) |
    | `{ a: $a, b: $b }` | yes | conjunctive, **but only if every value is a capture leaf** |
    | `{ a: { b: $x }, c: $y }` | no | nested value in multi-entry → bails (T1) |
    | `{ $k: $v, kind: u }` | no | capture key mixed with exact in one object → bails (T3) |
    | `{ a: $x } OR { b: $x }` | no | no alternation in the grammar (T2) |
    | `{ a.b.c: $x }` | no | must nest; no key-path shorthand (T4) |

(T1 has since been fixed -- `object_mixes_flat_and_nested_captures`,
`src/datapath.rs:1496-1509`, is green with the comment "was leaf-only, dropped
the match".)

Decisions log, `TASKS.md:162-173`:

    - Carrier = `q:` PathLit (structured/highlightable, not a string). `json` is the
      declarative op; `jsonp` is the dotted-string rename.
    - v4 host-grammar artifacts dropped: `$$sigil(...)` annotations, `${rule.$VAR}`
      cross-refs, `$$${PATH?}` recursive-capture key. `**` kept.
    - Capture vars bind lowercase (dl convention); `$NAME` matches a rule var of
      the same name. Literal `$ref`-style keys (OpenAPI/JSON-Schema) **must be
      quoted** (`"$ref"`) — bare `$ref` binds a capture named `ref`. Quoted `"$ref"`
      classifies as Glob → regex-escaped → matches the literal.
    - Evaluator is tree-descent over the existing tree-sitter parse (json/yaml/toml
      by extension). No new tables; spans flow to the existing ref spine.

And the one that speaks directly to the user's graphql framing, `TASKS.md:86-89`:

    - [ ] **T15 (L)** Relation-graph target (GraphQL-ish brace selection over the
          call/ref/flow graph). Different evaluator (relation-join, not tree-descent);
          datalog already queries it. This would be a sugar over relations, not a
          tree walker. Separate project; do not fold into `run_pattern`.

---

## 2. v3 -- the pipeline-op ancestor

The op was one segment of a `>`-chained pipeline, and the whole brace pattern
sat inside parens like every other v3 op.

`v3/crates/server/fixtures/json_yaml_toml_smoke.sprf:7`:

    repo(${R?}) > rev(:HEAD) > fs(glob(crates/tree-sitter-sprefa/Cargo.toml)) > json({ package: { name: ${N?}, version: ${V?} } })

`v3/crates/pipeline/src/ops/json.rs:10-13` (module doc):

    json({ name: $N, version: $V })
    json({ deps: { $K: $V } })
    json({ **: { image: "$REPO:$TAG" } })
    json({ users: [...{ name: $N, age: $A }] })

The op's own `DOC` string, `v3/crates/pipeline/src/ops/json.rs:60-74` -- the
whole surface in six lines:

    - bare keys match exactly: `{ name: $N }`
    - captured keys: `{ $K: $V }`
    - `**` recurses through nested objects: `{ **: { image: $I } }`
    - arrays via `[...]`: `{ items: [...{ id: $ID }] }`
    - quoted leaf patterns: `{ image: "$REPO:$TAG" }`
    - regex / glob keys: `{ re:^dep_: $V }`, `{ "@$SCOPE/$NAME": $V }`

Real fixtures, `v3/crates/server/fixtures/golden_kitchen_sink.sprf:40,48,53,149`:

    json({ paths: { /users: { get: { operationId: ${OPID?} } } } })
    json({ package: { name: ${N?}, version: ${V?} } })
    json({ service: { name: ${SVC?}, port: ${PORT?} } })
    json({ paths: { /no-such-route: { get: { operationId: ${X?} } } } })   # intentionally empty

### The v3 row model, stated by the author

`v3/crates/pipeline/src/walk/walker.rs:1-8`:

    //! Object entries split into `row_field` (one Leaf/LeafPattern/CaptureAny
    //! at the immediate value) vs descents (anything else). Row fields join
    //! into one row per match; descents fan out into separate rows.

That one comment is the entire semantics: **leaves join, descents fan out.**
Everything v5 does is the same rule.

### `${N?}` meant nothing at runtime

`v3/crates/pipeline/src/walk/brace_parse.rs:220-223`:

    // Trailing `?` = Unbound binding marker (host-grammar `${NAME?}`).
    // Recorded structurally elsewhere; for the v0 walker `${X?}` and
    // `${X}` are the same capture name.
    let name = name_with_suffix.strip_suffix('?').unwrap_or(name_with_suffix);

The `?` was an LSP/diagnostic marker for "this term is not yet bound", stripped
before matching. Worth knowing before anyone tries to "bring back `${N?}`" --
there is no semantics under it to bring back.

### v3 extras that did not survive

- `$$sigil($VAR)` scan annotations -- `{ repository: $$repo($REPO), tag: $$rev($TAG) }`
  (`v3/crates/sprefa/src/walk/brace_parse.rs:616`). Live in the older crate
  (stamps `Tri::Claimed` scan-pointer metadata), already inert in the v3
  pipeline port: "parse but are not surfaced today" (`ops/json.rs:42-44`).
- `${rule.$VAR}` cross-rule reference -- `{ tag: ${base_rule.$TAG} }`
  (`v3/crates/sprefa/src/walk/brace_parse.rs:676`). Lowers to `Leaf` rather
  than `CaptureAny`, so a pre-bound name **constrains** instead of rebinding.
  Carried real diagnostics: `xref/unknown-rule`, `xref/unknown-capture`,
  `xref/cycle` (`v3/crates/sprefa/src/analysis.rs:1524-1554`).
- jq-path was **never authorable input**. `crates/sprefa/src/jq_path.rs` renders
  paths for LSP completion, and `jq_path_to_sprf_pattern` (`ops/json.rs:686-730`)
  translates a picked jq-path *back into brace-pattern source text* to insert in
  the editor. The brace pattern was always the only surface.

Design-doc reach that never parsed (flag before anyone quotes it as history):
`v3/crates/sprefa_parse/parse.md:62-65` shows a sub-pipe inside a hole,
`json({ ${re((devD|optionalD|peerD|d)ependencies) > $DEP_KIND}: { $DEP : $VER } })`.
The landed parser has no `>` inside `${...}`. Roadmap, not surface.

---

## 3. v4 -- the host-grammar maximum

v4 kept the pattern grammar as a **verbatim 1:1 port** and rebuilt everything
around it. `v4/src/cst/dsls/json/mod.rs:8-11`:

    //! Sprf-blind. Faithful 1:1 port from v3/crates/pipeline/src/walk/ +
    //! v3/crates/pipeline/src/data/. Sprf carveouts (Cursor, Capture span_backed,
    //! TermPosition, ext-routed dispatch via cursor.fs) are dropped at the
    //! adapter; the underlying walk + data crates stay verbatim.

### Two changes to the call syntax

**Backtick DSL body instead of parens.** Every pattern DSL became
``op(:args)`body` ``, one `Dsl`/`Compiled` trait pair for all of them
(`v4/src/cst/README.md:52-59`):

    | id     | strategy                | body grammar           | capture syntax            |
    | `re`   | tree-sitter grammar.js  | regex with extensions  | `(?<NAME>...)`            |
    | `glob` | tree-sitter grammar.js  | glob with extensions   | `$NAME` per segment       |
    | `json` | hand-rolled parser      | brace-pattern DSL      | `{X}` braces              |
    | `ast`  | borrowed engine         | ast-grep syntactic     | `$NAME`, `$$$NAME`        |

**Explicit format atom instead of extension sniffing**
(`v4/src/compile/lower/ops.rs:1209-1213`):

    const JSON_SPEC: &[ArgSig] = &[ArgSig {
        kind: ArgKind::Atom,
        name: "fmt",
        doc: "target format atom (:json, :yaml, :toml). Default :json.",
        required: false,
    }];

Whole v4 program, `v4/examples/json-extract.sprf`:

    # Implemented syntax. Match structured JSON from cursor.value.

    rule(:people, NAME?, AGE?);

    `{"name":"ada","age":37}`
      > json`{ name: $NAME, age: $AGE }`
      > people(NAME, AGE);

TOML with array explosion, `v4/tests/read_gates_bytes_smoke.rs:185`:

    > json(:toml)`{ repos: [ ... { slug: ${SLUG?}, root: ${ROOT?}, remote: ${REMOTE?} } ] }`

### The one genuinely new construct: `$$${PATH?}`

Recursive descent **that binds the traversed key path**.
`v4/src/cst/dsls/json/walk/brace_parse.rs:398-405`:

    // json `$$${NAME?}` / `$$${NAME}` carveout: the recursive-descent
    // path-capture key (the v1/v2 `**:` "arbitrary json path down" idea).

Pinned, `v4/tests/dsl_hole_grammar_target.rs:180-198`:

    "rule(:imgs, PATH?, IMG?) { fs > glob`**/*.json` > read \
     > json`{ $$${PATH?}: { image: ${IMG?} } }` }; imgs();",
    ...
    assert_eq!(rows[0].get("IMG").unwrap_or(""), "nginx");
    assert_eq!(rows[0].get("PATH").unwrap_or(""), "services.web");

This is `**` plus a name for where it went. v5 dropped it and kept the
`AnyCapture` IR variant as a placeholder for exactly this
(`src/datapath.rs:473-474`), never re-surfaced.

### The v4 peak: json output composed into rendered documents

`v4/examples/openapi-cardinality-markdown.sprf` -- three `json` extractions into
three rule tables, then a markdown render whose holes re-query those tables and
join a `sql` sub-query on the captured `PATH`/`METHOD`:

    rule(:api_ops, PATH?, METHOD?, OP?, SUMMARY?);

    `v4/examples/openapi-cardinality.json`
      > read
      > json`{ paths: { $PATH: { $METHOD: { operationId: $OP, summary: $SUMMARY } } } }`
      > api_ops(PATH, METHOD, OP, SUMMARY);

    ...
    ${ sql`
          SELECT input.__cursor_idx, api_responses.STATUS, api_responses.DESC
          FROM input
          JOIN api_responses
            ON api_responses.PATH = ${PATH}
           AND api_responses.METHOD = ${METHOD}
          ORDER BY api_responses.STATUS
        `
      > render.markdown`  - ${ STATUS > `**${&.value}**` }: ${DESC}
    `
    }

Nested-key fan-out (`$PATH` × `$METHOD`) producing the cross-product of rows in
one pattern is the shape v5's `[... {...} ]` later covers for arrays.

### The design question v4 never closed

`v4/docs/v4-terse-relational-query-sessions.md:731-753, 800` -- "JSON
Whole-Subtree Capture", marked "Need spec lock-in first":

    json`{ paths: { $PATH: $$NODE } }`

    Runtime:
    - `NODE` value is raw JSON slice for the matched subtree.
    - `NODE_LO`, `NODE_HI`, `NODE_FS` point at the subtree byte range.

    Spec Question: JSON whole-subtree capture syntax: use `$$NODE`, `${NODE?}`,
    or another marker for "bind the raw matched object/array slice plus byte range"?

Still open. The user's own note in `human-goals.md:693` says the same thing from
the other side:

    also json syntax has no wayy to capture the hole json object of something vs
    its elements, its one or the other.

Under the current one-rel boundary this question **dissolves**: a subtree is a
row plus an integer ref, and "the raw slice" is a rendered value, not a second
storage shape. Worth telling the user their oldest open json question was
answered by a ruling made for other reasons.

### v3 → v4 → v5 delta, compact

| axis | v3 | v4 | v5 (live) |
|---|---|---|---|
| call shape | `json({ ... })` | ``json(:toml)`{ ... }` `` | `json(p, rev, q:{ ... })` |
| pattern body | brace grammar | **same grammar, ported verbatim** | same grammar, trimmed |
| format | sniff by extension | explicit `:fmt` atom | sniff by extension (back to v3) |
| capture | `$N`, `${N}`, `${N?}` | + host-hole formalized cross-DSL | `$n`, `${N?}` still accepted |
| recursion | `**` | `**` + `$$${PATH?}` binding | `**` only |
| annotations | `$$sigil($V)` (inert in the port) | dropped | dropped |
| cross-rule ref | `${rule.$VAR}` | dropped | dropped |
| carrier | paren | backtick body | `q:` PathLit |
| dotted-path sibling | -- | -- | `jsonp(...)` |

v5 is the **trim**, not the peak: it took v3's parens and extension dispatch
back, kept `**` and arrays and pattern keys, threw away the three
host-grammar experiments, and added `jsonp` plus a structured `q:` carrier.
Every drop is recorded with a reason in `TASKS.md:162-173`.

---

## 4. 20260428 -- v1 and v2, where the brace pattern was born

Correction to the brief: this archive holds **v1 (`crates/`) and v2 (`v2/`)**.
There is no `v3/` tree in it; v3 appears only as narration in `chat_log/`.

The construct is already fully formed in v1. `EXAMPLES.sprf:13`:

    fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })

`sprefa-rules.sprf:6,10,14`:

    fs(**/Cargo.toml) > json({ package: { name: $NAME } })
    fs(**/Cargo.toml) > json({ re:^(dev-)?dependencies: { $NAME: $_ } })
    rev(main) > fs(**/Cargo.toml) > json({ workspace: { members: [...$MEMBER] } });

`README.md:42,45,78`:

    sprefa eval 'fs(**/Cargo.toml) > json({ package: { name: $N } })'
    sprefa eval 'json({ name: $N })' package.json
    fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } })

Bare `$NAME`, no `?`. `${NAME}` existed only for identifier adjacency
("when the capture is adjacent to identifier characters, e.g.
`use${ENTITY}Query`", `crates/rules/src/pattern.rs:87-88`).

`README.md:243,245` states the two properties that never changed:

    Works on JSON, YAML, and TOML. All three parse into the same tree structure.
    **Recursive descent** (`**`) is useful for deeply nested configs

v2 adds the scan annotation, `NOTES.md:612`:

    json({ ** : { image: $I, $$repo($R), $$rev($V) } }) binds the full
    provenance chain at scan time

### The rejected alternative -- and it matters for the current cards

v1 shipped a **second, non-brace surface**: a JSON-Schema-driven step array
(`sprefa-rules.json:5-16`), the same walk expressed as data:

    "select": [
      { "step": "file", "pattern": "**/Cargo.toml" },
      { "step": "key", "name": "package" },
      { "step": "key", "name": "name" },
      { "step": "leaf", "capture": "name" }
    ]

It lost to the brace pattern and never reappeared in v2/v3/v4/v5. That is a
five-generation verdict against "express the query as structured data instead
of as a shape that looks like the document" -- directly relevant if anyone
proposes reaching json through generated rows rather than a pattern.

### Storage in the OG coordinate model

Load-bearing for the one-rel boundary: **the OG model already stored nothing
nested.** `docs/db-schema.md:37-89` is three flat relations --

- `strings` -- deduplicated values, one row per unique value
- `refs` -- "A physical occurrence: string X at byte span [start, end) in file Y",
  carrying `span_start`, `span_end`, `parent_key_string_id`
  ("for key-value pairs (dep version -> dep name)"), and a `node_path TEXT`
  breadcrumb
- `matches` -- "Semantic interpretation of a ref. One ref can have multiple
  matches from different rules"

Nesting collapses to a **single FK hop** plus a stringly `node_path`. All
multi-level destructuring happens in memory during extraction and is flattened
before it is persisted (`memory/project_query_redesign.md:19`: "`MatchResult.captures`
HashMap flushes directly as one row per extraction event. No EAV shredding into
individual match rows.").

So the locked one-rel boundary is not a new constraint the json language must be
squeezed into. **It is the constraint the json language was born under**, in the
first generation, and it survived every rewrite. That is the strongest single
piece of evidence in this document.

---

## 5. Grading against the current v6 world

### 5.1 What v6 has today

`v6/prolog/compile/registry.pl:61-68, 101-102`:

    % STRUCT-AS-ROWS (SLOT-DECODE-SURFACE): decode/2 stays on the surface as sugar
    % ... an edge body is edge_body_needs_json_destructure (the untyped
    % ... decode_source_not_struct, a non-object pattern is decode_pattern_not_object,
    % and a key the type does not declare is decode_field_unknown.
    surface(decode/2,       guard,     no_refs, wrapper(expr_pair, lower),        live).
    surface(json_each/2,    guard,     no_refs, wrapper(expr_pair, refuse(goal)), refused).
    surface(json_array/1,   aggregate, no_refs, head(refuse(aggregate)),          refused).
    surface(json_object/2,  aggregate, no_refs, head(refuse(aggregate)),          refused).

The v6 surface spelling (`v6/prolog/conformance/fixtures/json_arm.pl:28-35`):

    fixture(decode_open_pattern_binds_nested,
      prog([],
           [ (repo_name(Name)   <- raw_doc(Body), decode(Body, {name: Name})),
             (repo_owner(Login) <- raw_doc(Body), decode(Body, {owner: {login: Login}})) ]),
      [ raw_doc({name: cli, owner: {login: octo}, langs: [go, rust]}) ],

Shape-wise this **is** the brace pattern, in prolog term form. Two hard
narrowings separate it from v5:

1. **decode requires a declared struct type.** Both untyped `json_arm` fixtures
   currently compile to a refusal -- `v6/prolog/compile/out/manifest.json:87-88`:

       {"name":"decode_open_pattern_binds_nested","bucket":"unsupported",
        "reason":"decode_source_not_struct(decode(_,{name:_}))"},
       {"name":"decode_missing_key_fails_quietly","bucket":"unsupported",
        "reason":"decode_source_not_struct(decode(_,{absent_key:_}))"},

2. **Edge bodies refuse it entirely** -- 9 fixtures in
   `edge_body_needs_json_destructure` (`v6/prolog/compile/SCOREBOARD.md:82`).

Doc drift found in passing, not fixed (paper lane):
`v6/prolog/compile/SYNTAX.md:73` still renders `decode/2` as
`wrapper(expr_pair,refuse(goal))` / `refused`, while `registry.pl:68` says
`wrapper(expr_pair, lower)` / `live`. The generated table is stale relative to
its own source.

### 5.2 Per-construct grading

Legend -- **(a)** expresses in v6 today; **(b)** expresses but ugly; **(c)** needs
the one-rel storage work already carded in the json lab; **(d)** genuinely needs
new surface syntax.

| # | v5 construct | v5 spelling | grade | current v6 spelling / blocker |
|---|---|---|---|---|
| 1 | exact-key leaf capture | `q:{ name: $n }` | **a** | `decode(Body, {name: Name})` -- same shape, requires a `type` decl |
| 2 | nested descent by exact keys | `q:{ engines: { node: $v } }` | **a** | `decode(Body, {owner: {login: Login}})` -- fixture-covered |
| 3 | multi-entry conjunctive, flat + nested, one row | `q:{ number: $n, user: { login: $a } }` | **a** | one `decode` goal, same row semantics |
| 4 | silent non-match | missing key → no row | **a** | `decode_missing_key_fails_quietly` fixture; same semantics |
| 5 | value wildcard | `q:{ x: $_ }` | **a** | `decode(Body, {x: _})` -- prolog anonymous var |
| 6 | literal value filter | `q:{ "role": "admin" }` | **a** | `decode(Body, {role: admin})` -- atom in value position |
| 7 | **key capture / entry iteration** | `q:{ $k: $v }` | **d** | **no v6 spelling.** `decode` keys must be declared fields (`decode_field_unknown`). This is the single most-used v5 form. |
| 8 | array spread, scalars | `q:{ tags: [...$t] }` | **c** | `json_each(Langs, Lang)` exists in the oracle, **refused in the compiler**; storage answer = lab card `array_storage` |
| 9 | **array-of-objects fan-out with correlation** | `q:[... { number: $n, user: { login: $u } } ]` | **c** | needs 8 plus nested decode under the element; card `array_storage` |
| 10 | recursive descent | `q:{ **: { image: $i } }` | **d** | no v6 spelling, no refusal name |
| 11 | regex key | `q:{ re:^v: $val }` | **d** | no v6 spelling |
| 12 | glob key | `q:{ *id: $v }` | **d** | no v6 spelling |
| 13 | value-text pattern | `q:{ image: "$REPO:$TAG" }` | **d** | `LeafPattern`; no v6 spelling (v5 matches it literally today -- `TASKS.md` T7 open) |
| 14 | dotted-path sibling | `jsonp(body, "a.*.b", out)` | **b** | expressible as a chain of `decode`s per level; loses `*` fan-out (that is #8) |
| 15 | format dispatch json/yaml/toml/jsonl | one grammar, extension dispatch | **b** | v6 reaches files through extraction hosts; nothing routes yaml/toml into `decode`. Not a syntax gap -- a host-decl gap. |
| 16 | byte spans on captures | `bindings_carry_byte_spans` | **a** | struct-as-rows landed spans as a declared struct (ARCH `struct_as_rows`) |
| 17 | term form over a bound column | `json(body, q:{...})` | **a** | `decode(Body, ...)` is exactly this |
| 18 | file form as a source op | `json(p, rev, q:{...})` | **b** | extraction host + `decode`; two constructs where v5 had one |
| 19 | construction: array aggregate | `json_group_array(x)` | **c** | `json_array/1` **refused**; ruled emittable by `json_ticklog_encoding`, unowned arc |
| 20 | construction: object aggregate | `json_object(k, v)` | **c** | `json_object/2` **refused**; same arc |
| 21 | construction: scalar wrap for nesting | `json(names_json)` | **c** | rides 19/20 |
| 22 | edge-body use of any of the above | any `json(...)` in a `<+` rule | **c** | `edge_body_needs_json_destructure`, 9 fixtures |

Totals: **8 (a)**, **4 (b)**, **6 (c)**, **5 (d)** across 22 constructs. Roughly
a third survives translation unchanged; a third is carded storage work; a quarter
is genuinely new surface.

### 5.2b Constructs that died before v5 -- do not re-import by accident

These appear in the archives and will show up in any grep the user runs. All
four were dropped deliberately, three of them with a recorded reason. None is a
candidate for v6 without a fresh argument.

| construct | last seen | why it died |
|---|---|---|
| `$$sigil($VAR)` scan annotations | v2 live, v3 inert | "parse but are not surfaced today" (`v3 ops/json.rs:42-44`); dropped in v5 as a "v4 host-grammar artifact" (`TASKS.md:166`) |
| `${rule.$VAR}` cross-rule reference | v3 | dropped in v5, same line. In v6 an ordinary join does this. |
| `$$${PATH?}` recursive path capture | v4 | dropped in v5, same line; IR variant `AnyCapture` kept as a placeholder and never re-surfaced |
| step-array query (`"select": [{"step":"key",...}]`) | v1 | lost to the brace pattern and never returned across four rewrites (§4) |

The `$$${PATH?}` drop is the one worth reconsidering **only if**
CARD-RECURSIVE-KEY lands -- `**` without a path binding tells you a value
matched but not where it lives, and every consumer of a recursive match in the
archives wanted the path.

The (d) rows are all in one family: **the key axis**. v5 lets the *key* be a
capture, a regex, a glob, or a recursive descent. v6's `decode` only ever
matches a key **exactly, against a declared field**. Everything v6 lost is
"the key is data too".

### 5.3 The (d) rows as decision cards (locked stop rule -- no implementation proposed)

**CARD-KEY-CAPTURE** -- `q:{ $k: $v }`
- Old spelling: `json(body, q:{ $key: $value })` binds key and value as rule vars,
  one row per object entry.
- Closest no-new-syntax encoding: none in-language. A host emits `kv(doc, key, value)`
  rows and the program joins them.
- Cost of the encoding: every entry-iterating program needs a bespoke host decl;
  the key axis leaves the language. Cost of the syntax: `decode` stops being
  "destructure a declared struct" and becomes two constructs under one word,
  which is what `decode_field_unknown` currently exists to prevent.

**CARD-RECURSIVE-KEY** -- `q:{ **: { image: $i } }`
- Old spelling: `**` matches at any depth; graded by `star_star_recursive_descent`.
- Closest encoding: an explicit recursive rule over the struct-as-rows dictionary,
  once dictionaries are program-visible.
- Cost: the dictionary is **boundary-invisible by ruling**
  (`plans/2026-07-29-struct-as-rows-header.md`), so the encoding requires
  un-ruling that, or a new refusal. Cost of the syntax: one production, plus a
  depth guard (`TASKS.md` T9 flags `**` has no depth cap even in v5).

**CARD-PATTERN-KEY** -- `q:{ re:^v: $val }` and `q:{ *id: $v }`
- Old spelling: `re:` regex key, or a bare glob key.
- Closest encoding: key capture (CARD-KEY-CAPTURE) plus an ordinary `=~` guard on
  the bound key. This is strictly more composable and costs one extra body goal.
- Cost: depends entirely on CARD-KEY-CAPTURE; alone it is not worth a production.
  **This card should be read as "subsumed if key capture lands".**

**CARD-VALUE-PATTERN** -- `q:{ image: "$REPO:$TAG" }`
- Old spelling: `LeafPattern`, a quoted value containing `$`.
- v5 status: parsed but matched **literally** -- `TASKS.md` T7 is still open, so
  this never actually shipped its semantics.
- Closest encoding: capture the value, then `=~` a regex.
- Cost: near-zero to skip. Recommend recording it as **not wanted** unless the
  user says otherwise.

---

## 6. Card-by-card evidence from the recovered design

Evidence, not decisions. Each row states what v3/v4/v5 practice *implies*.

### `json_residency`
**Implied option: `core_global`.** Every generation kept json as a first-class
op in the language core, never a module and never host-only: v3 `json(...)` is a
pipeline op beside `re`/`glob`/`ast` (`MAIN.md:192`); v5 registers it in the
same op catalog as `match`/`sg` (`src/engine/decls.rs:231-232`). The author's own
statement of the reason, `human-goals.md:502`:

    Unified capture surface: ${X?} works across regex, glob, ast, sql, json.
    Standard tools require composing rg + jq + ast-grep, each with its own capture
    syntax. One sigil, one mental model, one TermFlowGraph that knows about all of
    them. AI generation is better against one surface than against four.

`host_only` is directly contradicted by that. It is contradicted a second time,
independently, by v1: the step-array surface (§4) *was* the "structured data
instead of a pattern" option, it shipped alongside the brace pattern, and it lost
across four consecutive rewrites. `optional_additive_module` has no precedent in
any generation -- json was never separable from the op set.

Counter-pressure to record: v5's own `json-out.dl` header ("nesting is OUTPUT,
built by SQLite's own json functions") puts *construction* on the sqlite side
while *extraction* stays in-language. So "core_global" in the recovered design
means the **pattern** is core, not the document builder.

### `array_storage`
**Implied option: `host_flattened`, with `indexed_elements` as the fallback.**
No generation ever stored an array. v5 explodes arrays at the pattern boundary
into ordinary rows and stores only the projection -- `[... { number: $n } ]`
yields one flat row per element and nothing array-shaped survives into storage
(`examples/gh-cache.dl:114-124`). `json-out.dl` re-materializes arrays only as
output text via `json_group_array`, never as state. `cons_relations` has no
precedent in any shipped generation (it is the types-lab amendment-1 proposal,
`plans/2026-07-29-struct-as-rows-header.md:65`). `refuse_arrays` is contradicted
by the flagship program -- and by every earlier generation: v1 already shipped
`[...$MEMBER]` (`archive-20260428/sprefa-rules.sprf:14`), v3
`[...{ name: $N, age: $A }]`, v4 `[ ... { slug: ${SLUG?} } ]`. Array fan-out is
five generations old and has never once been stored as an array.

The OG storage model (§4) is the decisive evidence: `refs` collapses nesting to
one `parent_key_string_id` hop plus a `node_path` breadcrumb, with no array
representation at all. Arrays have always been a **traversal** concept, never a
storage one.

### `null_and_optional`
**Implied option: `row_absence`.** v5's non-match is silent and produces no row --
`missing_key_yields_no_match` (`src/datapath.rs:1460-1464`); `examples/goto-flows.dl:72`
documents the same for `jsonp` over a null/missing field. v6 already agrees at
the fixture level (`decode_missing_key_fails_quietly` expects `final(probe/1, [])`
for **both** an absent key and an explicit `none`). `TASKS.md` T5 records that
optional-key syntax `{ a: $x, b?: $y }` was considered and **not built** ("No
nullable support now"), so `explicit_variant` has no recovered support either.
Consequence to state plainly: missing and explicit-null are **indistinguishable**
in the recovered design, and that was a deliberate non-decision, not an oversight.

### `schema_import_boundary`
**Implied option: `metadata_only`.** The only recovered schema-shaped program
derives a relation schema *from data*, not from a schema file --
`examples/type-from-json.dl` infers columns and types off a JSON sample and feeds
`type_decl_row(shape, pos, col, type)`, which is metadata and nothing else. The
json-arm plan explicitly parks the real thing: `plans/2026-07-27-json-arm.md:76`
lists "JSON Schema import (TypeSpec-replacement far tier)" under **Not in this
arm**. `metadata_plus_rules` has no precedent; `host_pre_normalized` is
contradicted by the in-language stance under `json_residency`.

### `recursive_identity`
**Implied option: `refuse_cycles`.** No generation expressed a cyclic JSON value.
v5's `**` is descent over a finite tree with no cycle handling at all (and
`TASKS.md` T9 flags it has no depth guard). The current world already rules the
same way -- cycles refused on the value plane because content ids cannot express
them (`plans/2026-07-29-struct-as-rows-header.md:67`). `bounded_unroll` is the
only option with any recovered echo, and only negatively: T9 asks for a max-depth
cap on `**`, which is bounded traversal, not bounded storage.

`entity_relations` is contradicted at the storage layer by the OG model, which
had extrinsic ids available (`refs.id`) and still never used them to represent
a nested value -- nesting was flattened at extraction time, one row per fact
(`memory/project_query_redesign.md:19`).

---

## 7. Decision cards for the user

Ordered by what unlocks the most.

1. **CARD-KEY-CAPTURE** (§5.3) -- restore `q:{ $k: $v }` in some v6 spelling, or
   rule it out. **Unblocks 4 of the 5 (d) rows.** Highest leverage item here.
2. **CARD-ARRAY-FANOUT** -- lift the `json_each/2` compiler refusal so
   `q:[... {...} ]` has a v6 path. Blocked on lab card `array_storage`; the
   evidence above points at `host_flattened`.
3. **CARD-CONSTRUCTION** -- `json_array/1` + `json_object/2` are refused but
   already **ruled emittable** by `json_ticklog_encoding`. This is an unowned
   arc, not an open question; it needs a dispatch, not a ruling.
4. **CARD-EDGE-BODY-JSON** -- 9 fixtures sit behind
   `edge_body_needs_json_destructure`. Reactive json is unreachable until this
   moves.
5. **CARD-RECURSIVE-KEY** (§5.3) -- `**`. Independent of 1; costs a depth-guard
   decision v5 never made.
6. **CARD-PATTERN-KEY** (§5.3) -- subsumed by 1 if 1 lands. Recommend no separate
   ruling.
7. **CARD-VALUE-PATTERN** (§5.3) -- never actually shipped its semantics in v5.
   Recommend recording as not-wanted.
8. **CARD-FORMAT-DISPATCH** -- yaml/toml/jsonl reached one grammar in v5 by
   extension dispatch. In v6 this is a host-decl question, not syntax. Needs a
   yes/no on whether the alpha wants it.
9. **CARD-SUBTREE-CAPTURE** -- the oldest open json question in the project.
   - Old spelling: **none shipped.** v4 proposed ``json`{ paths: { $PATH: $$NODE } }` ``
     and parked it ("Need spec lock-in first",
     `v4/docs/v4-terse-relational-query-sessions.md:741`). The user stated the
     same gap in their own words, `human-goals.md:693`: "json syntax has no wayy
     to capture the hole json object of something vs its elements, its one or
     the other."
   - Closest no-new-syntax encoding under the current boundary: a subtree already
     **is** a row plus an integer ref; binding it is binding the ref column, and
     printing it is the memoized `rendered_text` join that struct-as-rows already
     built.
   - Cost: near zero on the storage side -- the ruling that closed
     `compound_storage` answered this question as a side effect. The open part is
     only whether the *pattern* gets a spelling for "bind this node, do not
     descend". Recommend surfacing this to the user as **already solved
     structurally, unspelled syntactically** -- it is the cheapest win in the list
     and it is the thing they have wanted longest.

Also noted, no ruling needed: `SYNTAX.md:73` is stale against `registry.pl:68`
for `decode/2` (§5.1).

---

## 8. What was searched

- `~/projects/sprefa-archive-20260701`: `MAIN.md`, `TASKS.md`, `human-goals.md`,
  `llm-notes.md`, `v3/` (fixtures + `crates/pipeline/src/ops/json.rs`,
  `crates/sprefa/src/ops/json.rs`, brace/pattern walkers), `v4/`
  (`examples/*.sprf`, `docs/v4-*.md`, `src/cst/dsls/json/`).
  Also `v3/crates/sprefa/src/{analysis,jq_path,path_expr}.rs`,
  `v4/src/cst/dsls/json/walk/{brace_parse,walker,compile}.rs`,
  `v4/src/compile/lower/ops.rs`, `v4/src/v2_ops.rs`, `v4/tests/*.rs`.
- `~/projects/sprefa-archive-20260428`: `EXAMPLES.sprf`, `self_check.sprf`,
  `sprefa-rules.sprf` + `.json` + `.schema.json`, `README.md`, `NOTES.md`,
  `SCOPE_*.ts`, `docs/db-schema.md`, `memory/`, `crates/rules/src/`, `v2/src/`,
  `chat_log/` (the only place v3's grammar decisions are narrated).
- Live tree at `6c3a7e2d`: `src/datapath.rs`, `src/parse/ops.rs`, `src/ast.rs`,
  `src/desc.rs`, `src/engine/decls.rs`, `tests/it/data_ops.rs`, `tests/it/jsonl.rs`,
  `examples/*.dl`, `v6/prolog/compile/{registry,lower,analyze}.pl`,
  `v6/prolog/conformance/fixtures/json_arm.pl`, `v6/prolog/compile/SCOREBOARD.md`,
  `v6/prolog/compile/out/manifest.json`, `v6/prolog/conformance/rulings.pl`.
