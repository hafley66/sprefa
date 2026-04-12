# NOTES

## 2026-04-12: Feature ambitions dump

### Verbatim from session

> i want to take this sprefa repo and add every tool i have ever wanted, they all involve parsing so here they are. anyways, i have immediate refactor, parse capture language, i have 2 new ambitions: first class shell integrations, ability to take sh( echo and declare whatever i want $currentFile "\n" derp ) > line(re:$DERP_SPACEderp)
>
> etc. or maybe it has other apps, wehave a way to say "these cli forms are allowed", not unlike agent permission globs lol.
>
> i also want to take any kind of enum/set of types/set of strings/disc unions etc. and make it possible to say in 1 file, and basically get rust match arm semantic check or autofill/autoarm codegen, actually that may be nice part of codegen tool: This file's xyz pattern must be codegen'd/match or be represented in some ast matches pattern in this other file.
>
> so code gen, code gen validate, basically a shiv for codegen, not some hoopy doopy thing but also maybe something like "codegen into this comment marker range".
>
> We also need to add "variables" to sprf, with let's, but its like a literal string not an eval'd thing, takes bashisms, $1, $2, $3
>
> also, does there exist a bash flavored interpreter in rust where i can take any strings and just do string expansions of bash as first pass then do my own stuff.

---

### Feature 1: sh() tag — first-class shell integration

**What:** New tag `sh(command)` that runs a shell command and feeds its stdout into the downstream chain.

```sprf
rule(dynamic_versions) {
  sh(cat $currentFile | jq -r '.version') > line($VERSION)
};

rule(custom_extract) {
  fs(**/*.proto) > sh(protoc --decode_raw < $currentFile) > line(re:field\s+$FIELD)
};
```

**Shape:** sh() is a driver tag (like fs/repo/rev). It produces text content. Downstream tags (line/json/ast) consume that content. The shell command gets access to extract-time constants ($currentFile, $currentDir, etc).

**Permission model:** "these CLI forms are allowed" — allowlist of command patterns in sprefa.toml, similar to agent permission globs:

```toml
[shell]
allow = [
  "jq *",
  "protoc --decode_raw *",
  "echo *",
  "cat *",
]
```

Or pattern-based: `allow = ["jq **", "protoc **"]`. Anything not matching gets rejected at parse time or scan time.

**Open questions:**
- Does sh() run once per matched file (like a transform) or once globally (like a source)?
- Caching: same input file + same command = skip? Content hash of stdout?
- Security: even with allowlist, shell injection via captured variables is a risk. Maybe captured vars get shell-escaped automatically.

### Feature 2: codegen validation — enum/union exhaustiveness across files

**What:** Declare a set of variants in one place. Assert that another file's pattern matches all of them. Like `#[non_exhaustive]` + match arm checking but across files via sprf rules.

```sprf
-- source of truth: all event types
rule(event_types) {
  fs(src/events.rs) > ast[rust](pub enum EventType { $$$VARIANTS })
};

-- must be handled: switch/match must cover all event_types
check(event_handler_exhaustive) {
  SELECT et.variant
  FROM event_types et
  LEFT JOIN handler_arms ha ON ha.arm = et.variant
  WHERE ha.arm IS NULL
};
```

**Codegen mode:** Instead of just checking, generate the missing arms into a marker range:

```sprf
codegen(event_handler_arms) {
  source: event_types(variant: $V)
  target: fs(src/handler.rs) > marker("// BEGIN:event_arms", "// END:event_arms")
  template: "EventType::$V => handle_${V}(),"
};
```

**This is two features:**
1. **codegen validate** — check block that fails when a pattern set is incomplete
2. **codegen emit** — write generated code into marker-bounded regions

The validate part already works with existing check blocks + marker extraction. The emit part needs a new `codegen()` statement type and a write-back mechanism.

### Feature 2b: auto-watch sprf directives in comments

Any comment starting with `sprf` (trimmed) becomes a directive. Already expressible as a rule with existing language:

```sprf
rule(sprf_directives) {
  fs(**/*) > comment("sprf:") > line(re:(?P<DIRECTIVE>\w+)\s*(?P<ARGS>.*))
};

check(unhandled_sprf_directives) {
  SELECT d.directive, d.args, file_path(d.file_id)
  FROM sprf_directives d
  WHERE d.directive NOT IN ('codegen', 'check', 'todo', 'sync')
};
```

No new machinery needed. `comment()` (formerly `marker()`) + `line(re:)` + `check()` compose into this.

### Feature 3: let bindings — string variables in .sprf

**What:** Named string constants, not evaluated, just literal substitution.

```sprf
let PROTO_DIR = "src/proto";
let VERSION_PATTERN = "re:\\d+\\.\\d+\\.\\d+";

rule(proto_messages) {
  fs($PROTO_DIR/**/*.proto) > line(re:message\s+$NAME)
};

rule(semver_pins) {
  fs(**/package.json) > json({ version: "$VERSION_PATTERN" })
};
```

**Bash-ism positional args:** `$1`, `$2`, `$3` for parameterized .sprf files:

```bash
sprefa eval -f rules/check.sprf -- myorg/backend main
# $1 = myorg/backend, $2 = main
```

```sprf
let REPO = $1;
let REV = $2;

rule(targeted_scan) {
  repo($REPO) { rev($REV) { fs(**/deploy.yaml) > json({ image: $IMG }) } }
};
```

**Implementation:** Pre-pass before parsing. Walk the source, find `let NAME = "value";` lines, build a substitution map, expand all `$NAME` occurrences (non-SCREAMING already excluded from capture collection since captures require SCREAMING_CASE). Positional args from CLI injected into the map before expansion.

### Feature 4: bash string expansion in Rust

**Question:** Does a Rust crate exist that does bash-style string expansion?

**Known crates:**
- `shellexpand` — tilde and env var expansion (`~`, `$HOME`, `${VAR:-default}`). No globbing, no command substitution. Lightweight.
- `shell-words` — splits/joins shell-quoted strings. No expansion.
- `shlex` — POSIX shell lexing/quoting. No expansion.

**Missing:** Full bash parameter expansion (`${var//pattern/replace}`, `${var:offset:length}`, `${var%suffix}`, array syntax). No Rust crate does this completely.

**Options:**
1. `shellexpand` for simple `$VAR` / `${VAR}` / `${VAR:-default}` — covers 90% of use cases
2. Port bash parameter expansion rules into sprefa's own expander (the capture language already handles `$VAR` and `${VAR}`)
3. Actually shell out to `bash -c 'echo "expanded"'` for full bashism support (heaviest, most correct)

For let-bindings in .sprf, option 1 (`shellexpand`-style) is probably sufficient. The capture variable syntax already handles `$SCREAMING` and `${BRACED}`. Adding `${VAR:-default}` and `${VAR:+alt}` covers the useful bash-isms without needing a full bash interpreter.
