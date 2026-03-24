# URTSL -- Unified Repo Tree Selector Language

## Tree model

Every addressable node in the system lives in one tree:

```
{branch} / {repo} / {rel/path/to/file} / {data-or-ast-node} / ... / {leaf}
```

Segments are separated by `>` (direct child) or ` ` (descendant, any depth).
The first three segments are always fs: branch glob, repo glob, file glob.
After the file segment the tree switches to either DATA mode (yaml/json/toml)
or AST mode (source files via tree-sitter node kind names).

## Selector syntax

```
branch > repo > file > node > node > leaf { properties }
```

Each segment is one of:

| Segment | Meaning |
|---------|---------|
| `*` | any single segment |
| `literal` | exact match |
| `{a,b,c}` | alternation |
| `*.ext` or `*.{a,b}` | glob on filename |
| `[*]` | any array index (data mode) |
| `[key]` | the key of a mapping entry (data mode) |
| `[value]` | the value of a mapping entry (data mode) |
| `node_kind` | tree-sitter node kind name (ast mode) |
| `[key=/regex/]` | mapping key matches regex (data mode) |
| `[value=/regex/]` | mapping value matches regex (data mode) |
| `segment:as($var)` | bind this node to `$var` for use in properties |

## Properties

```
ref-kind: string;                   what ref_kind to emit
parent-key: <selector>;             path from current scope to the parent_key node
                                    can reference $var bindings
parent-key: key;                    the key of the current mapping entry is the parent_key
scan-repos: value|key|false;        run repo-name scanner on value or key string
value: strip-version-operators | strip-leading-eq | split-first | split-last;
@preprocess: gotmpl | jsonnet | none;   applied at file node level
ecosystem: npm|cargo|pip|helm|gradle;   for pkg_identity refs
```

## Content detectors (pseudo-attribute on file segment)

```
file[@openapi]      yaml/json file detected as OpenAPI spec (has paths: + info.title)
```

## @match block (ast-grep escape hatch, ast mode only)

```
segment > @match { <ast-grep pattern> } {
    $VAR { ref-kind: ...; parent-key: ...; }
}
```

Metavariables: `$NAME` (single node), `$$$` (zero or more nodes)

## Ancestor reference in parent-key

```
parent-key: $var > child;       navigate from a bound ancestor to a child
```

## Mode switching

File extension determines mode:

| Extension | Mode |
|-----------|------|
| `.yaml` `.yml` `.json` `.toml` `.xml` `.sql` | DATA mode (serde parse then path walk) |
| everything else | AST mode (tree-sitter) |
| `.yaml.gotmpl` etc | @preprocess then DATA mode |

## Specificity

More specific selectors win (longer non-wildcard prefix).
Later rules win on equal specificity.

## Compilation target

Rules compile via build.rs codegen to static Rust functions.
Each unique (branch,repo,file) prefix compiles to a FileKind variant + dispatch arm.
Each path below the file compiles to nested `get()`/`as_sequence()` calls (data)
or `match node.kind()` arms (ast).
`@match` blocks compile to ast_grep pattern evaluations.
Captures (`:as`) compile to `let` bindings on the stack frame of the walk.

## Output type (same for all rules)

```rust
struct ExtractedRef {
    value: String,
    span_start: u32,
    span_end: u32,
    ref_kind: &'static str,
    parent_key: Option<String>,
}
```

## Examples

### Helm chart dependencies

```
* > * > {Chart,chart}.{yaml,yml} > dependencies > [*]:as($dep) > version {
    ref-kind: dep_version;
    parent-key: $dep > name;
}
```

### TypeScript imports

```
* > * > *.{ts,tsx} > @match { import { $NAME } from '$PATH' } {
    $PATH { ref-kind: import_path; }
    $NAME { ref-kind: import_name; parent-key: $PATH; }
}
```

### YAML version map

```
* > * > versions.yaml > versions > * {
    ref-kind: dep_version;
    parent-key: key;
    scan-repos: key;
}
```

### Dockerfile

```
* > * > Dockerfile{,.*} > @match { FROM $IMAGE:$TAG } {
    $IMAGE { ref-kind: dep_name; scan-repos: value; }
    $TAG   { ref-kind: dep_version; parent-key: $IMAGE; }
}
```

### gotmpl preprocessing

```
* > * > *.yaml.gotmpl {
    @preprocess: gotmpl;
}

* > * > *.yaml.gotmpl > releases > [*]:as($r) > version {
    ref-kind: dep_version;
    parent-key: $r > name;
}
```

---

That's the full spec. Parser, codegen, and runtime are all derivable from this. The open questions left for implementation are: regex compilation strategy (compile all at build.rs time or lazy static), gotmpl holes that land on a matched leaf get flagged (probably a TemplateHole variant on ExtractedRef.value), and whether @preprocess is a file-level rule or inherits to all descendant rules automatically (should inherit).

---

## Extension: non-tree-sitter languages

The gap is languages tree-sitter doesn't have grammars for, or formats that aren't structural enough for tree-sitter (line-oriented, indent-sensitive, bespoke).

Current examples already in the codebase that fall through:

| File | Issue |
|------|-------|
| `requirements.txt` | line scanner, not a grammar |
| `go.mod` | line scanner |
| `yarn.lock` | custom format |
| `Gemfile` | ruby but the dep pattern is a specific line shape |
| `Dockerfile` | line scanner (FROM, RUN, etc) |
| `Pipfile.lock` | json but weird |

These are currently hand-rolled in `deps.rs` as ad-hoc line scanners.

### Simplest extension: regex line grammar

Add a third mode alongside DATA and AST:

LINE mode: file is treated as a sequence of lines, rules match against lines.

```
* > * > requirements*.txt {
    @mode: line;
}

* > * > requirements*.txt > [line=/^([a-zA-Z][a-zA-Z0-9._-]+)/] {
    ref-kind: dep_name;
    value: capture(1);
}

* > * > requirements*.txt > [line=/[><=!]=?\s*(\d.*)+)/] {
    ref-kind: dep_version;
    value: capture(1);
    parent-key: ~ [ref-kind=dep_name];
}
```

`value: capture(N)` pulls the Nth regex capture group as the emitted value instead of the whole line.

### For genuinely bespoke formats: inline micro-grammar

When a line regex isn't enough (multi-line constructs, indented blocks, yarn.lock's custom format):

```
* > * > yarn.lock {
    @grammar: {
        entry  := key ":" "\n" (indent field "\n")*;
        key    := /[^\s:]+/;
        field  := indent /\w+/ ":" /[^\n]+/;
        indent := "  ";
    }
}

* > * > yarn.lock > entry:as($e) > key[/^"([^@]+)@/] {
    ref-kind: dep_name;
    value: capture(1);
}

* > * > yarn.lock > entry > field[key="version"] {
    ref-kind: dep_version;
    parent-key: ^ entry > key;
}
```

The `@grammar` block is a PEG grammar expressed inline. Small, not a full parser generator -- just enough to define what "entry", "field", and "indent" mean for this file. The output is still nodes you can select against with the same `>` combinators.

### What the PEG needs to support

Minimal set covering all current bespoke formats:

```
rule     := name ":=" expr ";"
expr     := seq | alt | repeat | optional | literal | regex | ref
seq      := expr expr
alt      := expr "|" expr
repeat   := expr "*" | expr "+"
optional := expr "?"
literal  := quoted string
regex    := "/" pattern "/"
ref      := rule name
```

That's ~15 combinator types. Covers requirements.txt, go.mod, yarn.lock, Gemfile, Dockerfile line patterns, Pipfile.lock quirks. The PEG result is a node tree with rule names as node kinds -- exactly what the selector engine already traverses.

### Compilation

LINE mode: compile to `source.lines().enumerate().filter_map(|line| ...)` with regex match arms.

GRAMMAR mode: compile the PEG to a recursive descent parser at build.rs time (since grammar is statically known). The generated parser emits the same node kind tree that tree-sitter would. Selector evaluation is identical downstream.

### Extension stack

| Mode | Backend | Scope |
|------|---------|-------|
| DATA mode | serde (yaml/json/toml) | structured config |
| AST mode | tree-sitter (28 languages) | code files |
| LINE mode | regex on lines | most bespoke formats (new) |
| GRAMMAR mode | inline PEG | everything else (new) |

And `@preprocess` composes with any mode -- gotmpl strips templates before handing off to DATA, LINE, or GRAMMAR.

---

## Prior art

People have done adjacent things:

### CSS selectors on non-DOM trees
- GitHub's tree-sitter queries use S-expression patterns `((import_declaration (string) @path))` -- same idea, uglier syntax
- jq is path navigation on JSON, no file/repo context
- ast-grep is the closest: structural pattern matching on AST with metavariables. But single-language, single-file, no data-file modes

### Unified query across code + config
- Semgrep: multi-language AST matching, rule YAML files, but AST only, no YAML/JSON path navigation, no fs context in the selector
- CodeQL: Datalog over AST facts, extremely powerful but not a selector language and requires full compilation
- Comby: structural matching across languages, no tree-sitter, no data files

### The specific combination nobody has:
- fs path as part of the selector (branch/repo/file as first-class nodes)
- seamless switch from fs -> data format -> AST within one selector
- inline PEG for bespoke formats
- build.rs codegen so the rules have zero runtime interpretation cost
- the repo/string index as the output target (not lint diagnostics, not transformations -- indexing)

The closest prior art is probably **Datalog-based code analysis** (Doop, Souffl&eacute;, CodeQL) where facts include file provenance. But those are query languages over pre-extracted facts, not selector languages that drive the extraction itself.

The framing of "CSS selector over a unified repo/file/AST/data tree that compiles to static Rust" appears to be new. The individual pieces exist. The composition doesn't.
