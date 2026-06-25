# Declarative `json` pattern (recover the v4 brace walker); keep string form as `jsonp`

Status: **PLAN ONLY** (for a parallel session/worktree). No code written yet.
Owner request (verbatim intent): v5 has pathstyle (the declarative `:scheme{...}`
form). The declarative JSON pattern was carried v2→v4 and got flattened to a dotted
STRING in v5. Bring it back as a first-class, non-string, syntax-highlightable form.
Keep the current dotted-string evaluator as **`jsonp`** (the old thing); make **`json`**
the declarative brace syntax.

This is christmas-list **#2** (`{$K:$V}` key capture + recursive path descent) and
overlaps **#6** (structural YAML). The substrate already exists in v5 — this is a
surface + binding recovery, not a from-scratch build.

## Current world state (v5, grounded)

- `json` op: `parse.rs::json()` (parse.rs:454) parses `json(path, rev, "jpath", out)`
  where `jpath` is a `Tok::Str`. AST: `BodyItem::Json { path, rev, jpath: String, out: Term }`
  (ast.rs:166).
- Evaluator: `datapath.rs::run_data(path, content, jpath) -> Vec<(String, usize, usize)>`
  (value text + byte span). Tree-sitter backed (json / yaml / toml grammars), span-aware
  (feeds the ref spine). `descend()` (datapath.rs:73) already iterates `entries(fmt, node)`
  yielding `(key_segs, value_node)` — **the key is in hand and discarded**. `*` = any
  key/index; TOML dotted keys consume N segments; multi-doc YAML.
- Engine run: `engine.rs:4703` `BodyItem::Json { jpath, out, .. } => run_data(...)`, binds
  ONE column (`out`) per hit.
- "pathstyle" = `Term::PathLit { scheme, body, span }` (ast.rs:101) lexed as
  `Tok::Scheme { scheme, body, span }` (lex.rs:18). `lex_scheme_body` (lex.rs:31) reads a
  `scheme:body` literal: fenced `` scheme:`...` `` OR bare `scheme:...` with balanced
  `()[]{}` and `\`-escapes. This is the carrier that makes a brace pattern a STRUCTURED
  token, not an opaque `Tok::Str` — the syntax-highlight win.
- The v5 lexer has NO bare `{`/`}`/`[`/`]` tokens (only inside scheme bodies + `${` interp).

## Source of truth to port (v4)

`v4/src/cst/dsls/json/` (3636 lines; do NOT bulk-copy — port the grammar + IR, reuse the
v5 tree-sitter substrate). Key files:
- `walk/brace_parse.rs` (634): the brace grammar + parser. Grammar (verbatim header):
  ```
  pattern    = annotation | object | array | capture | wildcard | value_glob
  object     = "{" (entry ("," entry)*)? "}"
  entry      = key ":" pattern
  key        = "**" | "$" SCREAMING | "$_" | "re:" REGEX | glob_str
  array      = "[" "..." pattern "]"
  capture    = "$" SCREAMING       # binds the VALUE
  wildcard   = "$_"
  value_glob = (not , } ] )+
  ```
- `walk/compile.rs` (147) + `compiled.rs`: `SelectStep` / `KeyMatcher` / `ObjectEntry` IR
  (the parsed pattern), compiled to `CompiledStep` (walker input). Relevant variants:
  `Any`, `AnyCapture{capture}` (recursive `**` that binds the dot-joined path),
  `Key{name, capture}`, `KeyMatch{pattern, capture}`, `Leaf{capture}`,
  `Object{entries}`, `Array{item}`. Drop the `Repo/Rev/Folder/File` context steps (v3+
  has separate ops). `KeyMatcher`: `Exact|Glob|Capture|Wildcard|RecursiveCapture`.
- `walk/walker.rs` (695): the AnyDataNode walker. **Do NOT port this** — v5's
  `datapath.rs` tree-sitter descent replaces it and gives real byte spans. Port only the
  *capture-binding* logic onto the v5 descent.

## The split (the headline)

| op      | 3rd arg            | binds            | evaluator |
|---------|--------------------|------------------|-----------|
| `jsonp` | `Tok::Str` dotted  | one value (`out`)| today's `run_data` verbatim |
| `json`  | `Term::PathLit` brace pattern | N named captures (keys AND values) | new `run_pattern` over the same descent |

`jsonp` = mechanical rename of today's `json` (zero behavior change). `json` = new.

## Design (CLAUDE.md protocol)

### 1. Type signatures

```rust
// ast.rs — split BodyItem::Json into two
BodyItem::JsonP { path: Term, rev: Term, jpath: String, out: Term }   // old, renamed
BodyItem::Json  { path: Term, rev: Term, pat: PathLit, caps: Vec<Cap> } // new declarative
// where the captured vars are discovered at parse time so typecheck/lower see them:
struct Cap { name: String, span: (u32, u32) }   // each $NAME in the pattern

// datapath.rs — pattern IR (ported, trimmed from v4)
enum Step {
    Any,
    AnyCapture { capture: String },              // ** binds dot-joined path
    Key { name: String, capture: Option<String> },
    KeyMatch { matcher: KeyMatcher, capture: Option<String> },
    ArrayItem,
    Leaf { capture: Option<String> },
    Object { entries: Vec<(KeyMatcher, Vec<Step>)> },
    Array  { item: Vec<Step> },
}
enum KeyMatcher { Exact(String), Glob(String), Capture(String), Wildcard, Recursive(String) }

// datapath.rs — parse + evaluate
fn parse_pattern(body: &str) -> Result<(Vec<Step>, Vec<String>), String>; // steps + capture names
fn run_pattern(path: &str, content: &str, steps: &[Step]) -> Vec<Bindings>;
// Bindings = one match = a map { capture_name -> (text, lo, hi) }, span-aware.
type Bindings = Vec<(String, String, usize, usize)>;
```

### 2. Pseudo-code

```
// parse.rs::json()  (new declarative)
//   ident "json" "(" term "," term "," PathLit "," ??? ")"
//   - 3rd arg is a Tok::Scheme/PathLit (NOT Tok::Str); bail if a string ("use jsonp")
//   - parse_pattern(pat.body) at parse time -> capture names
//   - the captures ARE the bound vars; NO trailing `out` term (each $NAME is a var,
//     same model as match's named groups). Final arg shape is a surface DECISION (below).
//
// engine.rs run (mirror the match arm, engine.rs ~4514):
//   for each Bind in binds:
//     for each match in run_pattern(path, content, steps):
//       ext = bind.clone()
//       for (cap, text, lo, hi) in match: ext.insert(cap_var, Value::Text(text))
//       (optionally push the leaf span into _where_bytes for the ref spine, like match's id)
//       next.push(ext)
//
// datapath.rs::run_pattern: same recursion as descend(), but walk `steps` not `segs`.
//   On Key{capture}/Leaf{capture}: record (capture, value_text, span) into the live binding.
//   On AnyCapture: recursive descent, accumulate the traversed key path, bind dot-joined.
//   entries(fmt,node) ALREADY yields keys -> bind the key when KeyMatcher::Capture.
```

### 3. Instance lifetimes (state)

- `Step` tree: immutable after parse; lives in the `BodyItem::Json`. One per json op.
- `Bindings`: per-match, short-lived inside `run_pattern`; flushed into the engine's
  `Vec<Bind>` (the same per-rule binding vector match/sg use), dropped after the tick's
  source-fact insert.
- No new persistent tables. Spans optionally flow to the existing `_where_bytes` (ref
  spine) exactly as the match `id` arg does — reuse `push_span` / `insert_spine_where_bytes`.

### 4. Storage / reads / writes / uniqueness

- Reads: the data file content (same `read_content(root, rev, path)` the op uses today).
- Writes: source-relation rows (one per `Bindings` match), through the normal source-rule
  reconcile path (`_prov`-tracked, batched). NO new write seam.
- Uniqueness: a rule head over the captures dedups in SQLite as usual (PK on head cols).
  Two matches yielding identical capture tuples collapse — intended.

## Open decisions (resolve in the implementing session)

1. **Surface of the 3rd arg + capture binding.** Two candidates:
   - (A) Scheme literal carrier: `json(p, rev, j:{ $k: $v })` — `j:` is the PathLit scheme,
     body parsed by `parse_pattern`. `$k`/`$v` become rule vars (like match groups). NO
     trailing out. Pro: reuses pathstyle verbatim, highlightable today. Con: needs a scheme
     word; pick one (`j:` / `json:` / reuse the format?).
   - (B) New bare brace token: extend the lexer with `{`/`}`/`[`/`]`/pattern tokens so
     `json(p, rev, { $k: $v })` parses with no scheme prefix. Pro: cleanest surface. Con:
     real lexer/grammar surgery; risk of colliding with future cons/`{}` work (see
     `project_cons_calling_unification` memory — `{}` is contested grammar).
   - **Recommendation: (A)** — it is literally "pathstyle coming back," lowest risk, and the
     user named pathstyle. Revisit (B) only if (A)'s scheme word grates.
2. **Capture → variable discovery.** Parse-time `Vec<Cap>` must reach typecheck (so the
   vars are known/typed) and lower (so they bind into the SELECT). Confirm how match's
   named-group vars are surfaced to typecheck (regex `capture_names`) and mirror it; the
   json captures come from `parse_pattern` instead of a regex.
3. **Value type of captures.** Today `out` is whatever the rule declares. Captures are all
   `text` (the value's source text) unless we add `int()`/`json()` coercion (christmas #29
   `int()` already exists). Keep text; coerce in the head.
4. **`**` / RecursiveCapture semantics** on the tree-sitter descent (v4 bound the dot-joined
   traversed path). Port faithfully; add a depth guard.
5. **YAML/TOML parity** (#6): the brace object pattern over YAML maps + TOML tables. The
   v5 `entries()` already abstracts the three formats, so the pattern walker is
   format-agnostic for free — verify with tests per format.

## Migration

- Rename every existing `json(p, rev, "…", out)` call site → `jsonp(...)`:
  `tests/data_ops.rs`, `tests/query_json.rs`, `tests/kotlin.rs`, any `examples/*.dl`,
  `v5/README.md`. Grep `json(` and `\bjson\b` in `.dl`.
- Reserve both `json` and `jsonp` op names; the op dispatch in `parse.rs::body_item`
  (the `if s == "json"` arm, parse.rs:303) gains a `jsonp` arm.
- Update the `i:sprefa-v5-new-extraction-op` skill checklist if the binding model
  (variable captures vs single out) is novel for the implementer.

## Sequencing (suggested, for the worktree)

1. **S** — rename: `BodyItem::Json` → `BodyItem::JsonP`, add `jsonp` parse arm + dispatch,
   move `run_data` call to the JsonP engine arm, migrate call sites. Green = no behavior
   change. (Lands the split cleanly before any new syntax.)
2. **M** — port `Step`/`KeyMatcher` IR + `parse_pattern` into `datapath.rs` (+ unit tests
   from v4's brace_parse tests).
3. **M** — `run_pattern` over the existing tree-sitter descent: object/key/leaf capture
   binding first (the `{$k:$v}` core, christmas #2), spans wired to ref spine.
4. **M** — new `json` parse arm taking a `PathLit`, capture-var discovery → typecheck →
   lower (the binding model). e2e test: `{ $k: $v }` binds both columns over a json file.
5. **S** — `**` recursive descent + `[...]` array spread + `re:`/glob keys.
6. **S** — YAML + TOML parity tests (#6).
7. **S** — docs (README op table, examples), reserve-name guard.

## Tests to write

- `parse_pattern` units (port v4 `brace_parse` tests): object, array spread, `**`,
  `re:`, `$_`, value glob, trailing-content error.
- e2e per format: `{ $k: $v }` over json/yaml/toml binds key+value with spans.
- `**` recursive path capture binds the dot-joined path.
- `jsonp` regression (old dotted string still works unchanged).
- Reserved-name guard for both `json` and `jsonp`.
- ref-spine join: a captured leaf's span resolves through `ref(id, _, f, lo, hi)`.
```
