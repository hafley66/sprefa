//! Dotted-path extraction over data files: the `json` op's evaluator. The op
//! keeps its name but dispatches on file extension (json / yaml,yml / toml),
//! the v4 `AnyDataNode` behavior. Each format parses with its tree-sitter
//! grammar so every extracted value carries a byte span for the ref spine.
//!
//! `*` matches any object key or array index. A TOML dotted key (`a.b = 1`,
//! `[a.b]`) consumes that many path segments. Multi-document YAML streams
//! match the path against every document.

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Json,
    Yaml,
    Toml,
}

fn fmt_of(path: &str) -> Fmt {
    match path.rsplit('.').next().unwrap_or("") {
        "yaml" | "yml" => Fmt::Yaml,
        "toml" => Fmt::Toml,
        _ => Fmt::Json,
    }
}

/// Extract leaf values along a dotted path from a json/yaml/toml file.
pub fn run_data(path: &str, content: &str, jpath: &str) -> Vec<(String, usize, usize)> {
    let fmt = fmt_of(path);
    let lang: tree_sitter::Language = match fmt {
        Fmt::Json => tree_sitter_json::LANGUAGE.into(),
        Fmt::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Fmt::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(content, None) { Some(t) => t, None => return vec![] };
    let src = content.as_bytes();
    let segs: Vec<&str> = jpath.split('.').collect();
    let mut hits: Vec<tree_sitter::Node> = Vec::new();
    for root in root_values(fmt, tree.root_node()) {
        descend(fmt, root, &segs, src, &mut hits);
    }
    hits.iter().map(|n| value_text_span(fmt, *n, content)).collect()
}

/// The top-level value node(s) under the grammar's document wrapper. YAML
/// yields one per document in the stream; TOML's root is itself the object.
fn root_values<'a>(fmt: Fmt, root: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    match fmt {
        Fmt::Json => {
            let mut c = root.walk();
            root.named_children(&mut c).take(1).collect()
        }
        Fmt::Toml => vec![root],
        Fmt::Yaml => {
            let mut out = Vec::new();
            let mut c = root.walk();
            for doc in root.named_children(&mut c) {
                if doc.kind() != "document" { continue; }
                let mut dc = doc.walk();
                for n in doc.named_children(&mut dc) {
                    if matches!(n.kind(), "block_node" | "flow_node") {
                        out.push(yaml_unwrap(n));
                    }
                }
            }
            out
        }
    }
}

/// Match the remaining path segments against a value node. Object entries may
/// consume several segments at once (TOML dotted keys); arrays consume one.
fn descend<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, segs: &[&str], src: &[u8],
    out: &mut Vec<tree_sitter::Node<'a>>,
) {
    if segs.is_empty() {
        out.push(node);
        return;
    }
    for (ksegs, v) in entries(fmt, node, src) {
        if ksegs.len() <= segs.len()
            && ksegs.iter().zip(segs).all(|(k, s)| *s == "*" || k == s)
        {
            descend(fmt, v, &segs[ksegs.len()..], src, out);
        }
    }
    let items = items(fmt, node);
    if segs[0] == "*" {
        for it in items { descend(fmt, it, &segs[1..], src, out); }
    } else if let Ok(idx) = segs[0].parse::<usize>() {
        if let Some(it) = items.get(idx) { descend(fmt, *it, &segs[1..], src, out); }
    }
}

/// Key/value pairs of an object node. Keys come pre-split into segments: one
/// for json/yaml, one per part for a TOML dotted key.
fn entries<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, src: &[u8],
) -> Vec<(Vec<String>, tree_sitter::Node<'a>)> {
    let mut out = Vec::new();
    match fmt {
        Fmt::Json => {
            if node.kind() != "object" { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if pair.kind() != "pair" { continue; }
                if let (Some(k), Some(v)) = (pair.child_by_field_name("key"), pair.child_by_field_name("value")) {
                    out.push((vec![json_str_value(k, src)], v));
                }
            }
        }
        Fmt::Yaml => {
            if !matches!(node.kind(), "block_mapping" | "flow_mapping") { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if !matches!(pair.kind(), "block_mapping_pair" | "flow_pair") { continue; }
                if let (Some(k), Some(v)) = (pair.child_by_field_name("key"), pair.child_by_field_name("value")) {
                    out.push((vec![yaml_scalar_text(yaml_unwrap(k), src)], yaml_unwrap(v)));
                }
            }
        }
        Fmt::Toml => match node.kind() {
            // the document's entries are its top-level pairs plus each [table]
            // / [[table]] header as a (possibly dotted) key for the table node
            "document" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    match child.kind() {
                        "pair" => out.extend(toml_pair(child, src)),
                        "table" | "table_array_element" => {
                            if let Some(k) = toml_key_node(child) {
                                out.push((toml_key_segs(k, src), child));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "table" | "table_array_element" | "inline_table" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    if child.kind() == "pair" { out.extend(toml_pair(child, src)); }
                }
            }
            _ => {}
        },
    }
    out
}

/// Item nodes of an array/sequence node; empty for anything else.
fn items<'a>(fmt: Fmt, node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut out = Vec::new();
    match fmt {
        Fmt::Json | Fmt::Toml => {
            if node.kind() != "array" { return out; }
            let mut c = node.walk();
            out.extend(node.named_children(&mut c));
        }
        Fmt::Yaml => {
            if !matches!(node.kind(), "block_sequence" | "flow_sequence") { return out; }
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                match child.kind() {
                    "block_sequence_item" => {
                        if let Some(inner) = child.named_child(0) { out.push(yaml_unwrap(inner)); }
                    }
                    "flow_node" => out.push(yaml_unwrap(child)),
                    _ => {}
                }
            }
        }
    }
    out
}

/// A matched leaf as (text, byte span). String-ish nodes strip their quotes so
/// `ref` points at the value, mirroring the json behavior.
fn value_text_span(fmt: Fmt, n: tree_sitter::Node, content: &str) -> (String, usize, usize) {
    let (lo, hi) = (n.start_byte(), n.end_byte());
    let raw = &content[lo..hi];
    match fmt {
        Fmt::Json if n.kind() == "string" && hi - lo >= 2 =>
            (json_unescape(&raw[1..raw.len() - 1]), lo + 1, hi - 1),
        Fmt::Yaml if matches!(n.kind(), "double_quote_scalar" | "single_quote_scalar") && hi - lo >= 2 =>
            (yaml_scalar_text(n, content.as_bytes()), lo + 1, hi - 1),
        Fmt::Toml if n.kind() == "string" => {
            let q = if raw.starts_with("\"\"\"") || raw.starts_with("'''") { 3 } else { 1 };
            if raw.len() >= 2 * q {
                (toml_unquote(raw), lo + q, hi - q)
            } else {
                (raw.to_string(), lo, hi)
            }
        }
        _ => (raw.to_string(), lo, hi),
    }
}

// ── json ─────────────────────────────────────────────────────────────────────

/// Text of a json string node with quotes stripped and escapes resolved.
fn json_str_value(n: tree_sitter::Node, src: &[u8]) -> String {
    let (lo, hi) = (n.start_byte(), n.end_byte());
    if n.kind() == "string" && hi - lo >= 2 {
        json_unescape(std::str::from_utf8(&src[lo + 1..hi - 1]).unwrap_or(""))
    } else {
        String::from_utf8_lossy(&src[lo..hi]).into_owned()
    }
}

fn json_unescape(s: &str) -> String {
    if !s.contains('\\') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' { out.push(c); continue; }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some(o) => { out.push('\\'); out.push(o); }
            None => out.push('\\'),
        }
    }
    out
}

// ── yaml ─────────────────────────────────────────────────────────────────────

/// Strip the grammar's block_node/flow_node wrapper (skipping anchor/tag) to
/// the payload mapping/sequence/scalar.
fn yaml_unwrap(n: tree_sitter::Node) -> tree_sitter::Node {
    if !matches!(n.kind(), "block_node" | "flow_node") { return n; }
    let mut c = n.walk();
    for child in n.named_children(&mut c) {
        if !matches!(child.kind(), "anchor" | "tag") { return child; }
    }
    n
}

fn yaml_scalar_text(n: tree_sitter::Node, src: &[u8]) -> String {
    let raw = || String::from_utf8_lossy(&src[n.start_byte()..n.end_byte()]).into_owned();
    match n.kind() {
        "double_quote_scalar" => {
            let s = raw();
            json_unescape(&s[1..s.len().saturating_sub(1)])
        }
        "single_quote_scalar" => {
            let s = raw();
            s[1..s.len().saturating_sub(1)].replace("''", "'")
        }
        "plain_scalar" => match n.named_child(0) {
            Some(c) => yaml_scalar_text(c, src),
            None => raw(),
        },
        _ => raw(),
    }
}

// ── toml ─────────────────────────────────────────────────────────────────────

/// A `pair` as (key segments, value node). The key may be dotted.
fn toml_pair<'a>(pair: tree_sitter::Node<'a>, src: &[u8]) -> Option<(Vec<String>, tree_sitter::Node<'a>)> {
    let k = toml_key_node(pair)?;
    let mut c = pair.walk();
    let v = pair.named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "bare_key" | "dotted_key" | "quoted_key" | "comment"))
        .last()?;
    Some((toml_key_segs(k, src), v))
}

/// The key node of a pair / table header (first key-kind child).
fn toml_key_node(n: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut c = n.walk();
    let found = n.named_children(&mut c)
        .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"));
    found
}

/// Key text split into path segments: `a.b."c d"` -> ["a", "b", "c d"].
fn toml_key_segs(k: tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let part = |n: tree_sitter::Node| {
        let raw = String::from_utf8_lossy(&src[n.start_byte()..n.end_byte()]).into_owned();
        if n.kind() == "quoted_key" { toml_unquote(&raw) } else { raw }
    };
    if k.kind() == "dotted_key" {
        let mut segs = Vec::new();
        let mut stack = vec![k];
        // dotted_key nests left-recursively: (dotted_key (dotted_key a b) c)
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            let kids: Vec<_> = n.named_children(&mut c).collect();
            for child in kids.into_iter().rev() {
                if child.kind() == "dotted_key" { stack.push(child); } else { segs.push(part(child)); }
            }
        }
        segs.reverse();
        segs
    } else {
        vec![part(k)]
    }
}

fn toml_unquote(s: &str) -> String {
    for (open, close) in [("\"\"\"", "\"\"\""), ("'''", "'''"), ("\"", "\""), ("'", "'")] {
        if s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            let inner = &s[open.len()..s.len() - close.len()];
            return if open.starts_with('"') { json_unescape(inner) } else { inner.to_string() };
        }
    }
    s.to_string()
}

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

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

/// Parse a brace pattern body into (top steps, capture names in first-seen order).
/// Capture names are deduped; a repeated `$NAME` is the same var referenced twice.
pub fn parse_pattern(body: &str) -> Result<(Vec<Step>, Vec<String>), String> {
    let mut p = PatParser { src: body, pos: 0, caps: Vec::new() };
    let steps = p.pattern()?;
    p.ws();
    let rest = p.src[p.pos..].trim();
    if !rest.is_empty() {
        return Err(format!("unexpected trailing content in json pattern: {:?}", rest));
    }
    let mut seen: Vec<String> = Vec::new();
    for c in p.caps {
        if !seen.contains(&c) { seen.push(c); }
    }
    Ok((steps, seen))
}

enum Dollar { Wildcard, Name(String) }

struct PatParser<'a> { src: &'a str, pos: usize, caps: Vec<String> }

impl<'a> PatParser<'a> {
    fn b(&self) -> &'a [u8] { self.src.as_bytes() }

    fn ws(&mut self) {
        while self.pos < self.src.len() && self.b()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> { self.b().get(self.pos).copied() }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        match self.peek() {
            Some(b) if b == expected => { self.pos += 1; Ok(()) }
            Some(b) => Err(format!("expected {:?}, found {:?}", expected as char, b as char)),
            None => Err(format!("expected {:?}, found end of input", expected as char)),
        }
    }

    fn record(&mut self, name: &str) { self.caps.push(name.to_string()); }

    fn pattern(&mut self) -> Result<Vec<Step>, String> {
        self.ws();
        if self.pos >= self.src.len() {
            return Err("unexpected end of json pattern".into());
        }
        match self.b()[self.pos] {
            b'{' => self.object(),
            b'[' => self.array(),
            b'$' => self.value_dollar(),
            b'"' => self.quoted_value(),
            _ => self.value_glob(),
        }
    }

    fn object(&mut self) -> Result<Vec<Step>, String> {
        self.expect(b'{')?;
        self.ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(vec![Step::Object { entries: Vec::new() }]);
        }
        let mut entries: Vec<(KeyMatcher, Vec<Step>)> = Vec::new();
        loop {
            self.ws();
            let (key, value) = self.entry()?;
            // `**` consumes the rest of the object as recursive descent.
            if matches!(key, KeyMatcher::Exact(ref s) if s == "**") {
                self.ws();
                if self.peek() == Some(b',') { self.pos += 1; }
                self.ws();
                self.expect(b'}')?;
                let mut steps = if entries.is_empty() { Vec::new() }
                                else { vec![Step::Object { entries }] };
                steps.push(Step::Any);
                steps.extend(value);
                return Ok(steps);
            }
            entries.push((key, value));
            self.ws();
            match self.peek() {
                Some(b',') => { self.pos += 1; }
                Some(b'}') => { self.pos += 1; break; }
                Some(c) => return Err(format!("expected `,` or `}}` in object, found {:?}", c as char)),
                None => return Err("unclosed `{` in json pattern".into()),
            }
        }
        Ok(vec![Step::Object { entries }])
    }

    fn entry(&mut self) -> Result<(KeyMatcher, Vec<Step>), String> {
        self.ws();
        let key = self.key()?;
        self.ws();
        self.expect(b':')?;
        self.ws();
        let value = self.pattern()?;
        Ok((key, value))
    }

    fn key(&mut self) -> Result<KeyMatcher, String> {
        self.ws();
        // Quoted key: "..." — classify the inner text.
        if self.peek() == Some(b'"') {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.src.len() && self.b()[self.pos] != b'"' { self.pos += 1; }
            if self.pos >= self.src.len() { return Err("unclosed `\"` in key position".into()); }
            let inner = self.src[start..self.pos].to_string();
            self.pos += 1;
            let m = key_classify(inner);
            if let KeyMatcher::Capture(ref n) = m { self.record(n); }
            return Ok(m);
        }
        // `**` recursive marker (valid only as the sole consuming key).
        if self.src[self.pos..].starts_with("**")
            && (self.pos + 2 >= self.src.len() || !self.b()[self.pos + 2].is_ascii_alphanumeric())
        {
            self.pos += 2;
            return Ok(KeyMatcher::Exact("**".to_string()));
        }
        // `re:` regex key: read until a `:` followed by a value delimiter, so a
        // colon inside the regex (not the entry separator) is preserved.
        if self.src[self.pos..].starts_with("re:") {
            self.pos += 3;
            let start = self.pos;
            while self.pos < self.src.len() {
                if self.b()[self.pos] == b':' {
                    let after = self.pos + 1;
                    if after >= self.src.len()
                        || matches!(self.b()[after], b' ' | b'\t' | b'\n' | b'{' | b'[' | b'$')
                    {
                        break;
                    }
                }
                self.pos += 1;
            }
            let re = self.src[start..self.pos].trim();
            return Ok(KeyMatcher::Glob(format!("re:{}", re)));
        }
        // `$NAME` / `$_` capture key.
        if self.peek() == Some(b'$') {
            let m = match self.dollar_read()? {
                Dollar::Name(n) => { self.record(&n); KeyMatcher::Capture(n) }
                Dollar::Wildcard => KeyMatcher::Wildcard,
            };
            return Ok(m);
        }
        // Bare key: read until the entry colon. Glob chars classify as Glob.
        let start = self.pos;
        while self.pos < self.src.len() && self.b()[self.pos] != b':' { self.pos += 1; }
        let s = self.src[start..self.pos].trim();
        if s.is_empty() { return Err("empty key in json pattern".into()); }
        let m = key_classify(s.to_string());
        if let KeyMatcher::Capture(ref n) = m { self.record(n); }
        Ok(m)
    }

    fn array(&mut self) -> Result<Vec<Step>, String> {
        self.expect(b'[')?;
        self.ws();
        if !self.src[self.pos..].starts_with("...") {
            return Err("expected `...` after `[` in array pattern".into());
        }
        self.pos += 3;
        let item = self.pattern()?;
        self.ws();
        self.expect(b']')?;
        Ok(vec![Step::Array { item }])
    }

    /// Value-position `$...`: `$_` (wildcard leaf) or `$NAME`/`${NAME}` (capture).
    fn value_dollar(&mut self) -> Result<Vec<Step>, String> {
        let leaf = match self.dollar_read()? {
            Dollar::Name(n) => { self.record(&n); Step::Leaf { capture: Some(n) } }
            Dollar::Wildcard => Step::Leaf { capture: None },
        };
        Ok(vec![leaf])
    }

    /// Reads starting at `$`. `$_` (not followed by an ident char) is a wildcard;
    /// `$NAME` and `${NAME}` (optional trailing `?`) are captures.
    fn dollar_read(&mut self) -> Result<Dollar, String> {
        self.pos += 1; // consume `$`
        if self.peek() == Some(b'{') {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.src.len() && self.b()[self.pos] != b'}' { self.pos += 1; }
            if self.pos >= self.src.len() { return Err("unclosed `${` in json pattern".into()); }
            let inner = self.src[start..self.pos].trim();
            self.pos += 1; // consume `}`
            let name = inner.strip_suffix('?').unwrap_or(inner);
            return Ok(Dollar::Name(validate_name(name)?.to_string()));
        }
        if self.peek() == Some(b'_')
            && (self.pos + 1 >= self.src.len() || !self.b()[self.pos + 1].is_ascii_alphanumeric())
        {
            self.pos += 1;
            return Ok(Dollar::Wildcard);
        }
        let start = self.pos;
        while self.pos < self.src.len()
            && (self.b()[self.pos].is_ascii_alphanumeric() || self.b()[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let name = &self.src[start..self.pos];
        if name.is_empty() { return Err("empty capture name after `$`".into()); }
        Ok(Dollar::Name(validate_name(name)?.to_string()))
    }

    fn quoted_value(&mut self) -> Result<Vec<Step>, String> {
        self.pos += 1; // consume opening `"`
        let start = self.pos;
        while self.pos < self.src.len() && self.b()[self.pos] != b'"' { self.pos += 1; }
        if self.pos >= self.src.len() { return Err("unclosed `\"` in json pattern".into()); }
        let content = self.src[start..self.pos].to_string();
        self.pos += 1; // consume closing `"`
        Ok(vec![if content.contains('$') {
            Step::LeafPattern { pattern: content }
        } else {
            Step::LeafEq { text: content }
        }])
    }

    fn value_glob(&mut self) -> Result<Vec<Step>, String> {
        let start = self.pos;
        while self.pos < self.src.len() {
            match self.b()[self.pos] {
                b',' | b'}' | b']' => break,
                _ => self.pos += 1,
            }
        }
        let text = self.src[start..self.pos].trim();
        if text.is_empty() { return Err("empty value in json pattern".into()); }
        Ok(vec![Step::LeafEq { text: text.to_string() }])
    }
}

/// Validate a capture identifier: first char alpha/`_`, rest alphanumeric/`_`.
fn validate_name(name: &str) -> Result<&str, String> {
    let b = name.as_bytes();
    if b.is_empty()
        || !(b[0].is_ascii_alphabetic() || b[0] == b'_')
        || !b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        return Err(format!("invalid capture name `{:?}`", name));
    }
    Ok(name)
}

/// Classify a bare/quoted key string: `$_` → Wildcard, `$SCREAMING` (no `/` or
/// `:`) → Capture, glob chars (`*`/`?`/`[`/`$`) → Glob, else Exact.
fn key_classify(s: String) -> KeyMatcher {
    if s == "$_" {
        KeyMatcher::Wildcard
    } else if s.starts_with('$') && s.len() > 1
        && s[1..].starts_with(|c: char| c.is_ascii_uppercase())
        && !s.contains('/') && !s.contains(':')
    {
        KeyMatcher::Capture(s[1..].to_string())
    } else if s.contains('*') || s.contains('?') || s.contains('[') || s.contains('$') {
        KeyMatcher::Glob(s)
    } else {
        KeyMatcher::Exact(s)
    }
}

/// One captured binding: (capture name, value text, byte lo, byte hi). The span
/// is byte offsets into the file content, fed to the ref spine like match's id.
pub type Binding = (String, String, usize, usize);

/// Walk a brace `Step` pattern over the tree-sitter parse of a data file. Each
/// returned `Vec<Binding>` is one match (capture name -> text + byte span),
/// mirroring how a regex match yields named groups. The target today is a
/// json/yaml/toml document (dispatched by extension, like `run_data`); the
/// tree-descent evaluator is target-agnostic, so an AST/graph target could back
/// the same `Step` IR later without touching the parser or this walker's core.
pub fn run_pattern(path: &str, content: &str, steps: &[Step]) -> Vec<Vec<Binding>> {
    let fmt = fmt_of(path);
    let lang: tree_sitter::Language = match fmt {
        Fmt::Json => tree_sitter_json::LANGUAGE.into(),
        Fmt::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Fmt::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(content, None) { Some(t) => t, None => return vec![] };
    let src = content.as_bytes();
    let mut out: Vec<Vec<Binding>> = Vec::new();
    for root in root_values(fmt, tree.root_node()) {
        walk_steps(fmt, root, steps, src, content, Vec::new(), &mut out);
    }
    out
}

fn walk_steps<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, steps: &[Step],
    src: &'a [u8], content: &str,
    binds: Vec<Binding>, out: &mut Vec<Vec<Binding>>,
) {
    if steps.is_empty() {
        out.push(binds);
        return;
    }
    match &steps[0] {
        Step::Object { entries } => walk_object(fmt, node, entries, src, content, binds, out),
        Step::Leaf { capture } => {
            let mut b = binds;
            if let Some(name) = capture {
                let (t, lo, hi) = value_text_span(fmt, node, content);
                b.push((name.clone(), t, lo, hi));
            }
            walk_steps(fmt, node, &steps[1..], src, content, b, out);
        }
        Step::LeafEq { text } => {
            let (t, _, _) = value_text_span(fmt, node, content);
            if t == *text { walk_steps(fmt, node, &steps[1..], src, content, binds, out); }
        }
        Step::LeafPattern { pattern } => {
            // A quoted value containing `$`: matched literally (including the
            // `$`) for now. A real interpolation matcher is a follow-up.
            let (t, _, _) = value_text_span(fmt, node, content);
            if t == *pattern { walk_steps(fmt, node, &steps[1..], src, content, binds, out); }
        }
        Step::Array { item } => {
            for elem in items(fmt, node) {
                walk_steps(fmt, elem, item, src, content, binds.clone(), out);
            }
        }
        Step::Any => {
            // `**` recursive descent: try the remaining pattern at this node and
            // every named descendant. Binds nothing itself.
            walk_steps(fmt, node, &steps[1..], src, content, binds.clone(), out);
            for desc in all_descendants(node) {
                walk_steps(fmt, desc, &steps[1..], src, content, binds.clone(), out);
            }
        }
        // AnyCapture is never produced: the `$$${PATH}` key was dropped from
        // the parser. Reserved for a future recursive path-capture surface.
        _ => {}
    }
}

/// Match an `Object` step against a node. A single-entry Capture/Wildcard key
/// iterates the node's entries (one match per entry, the `{$k:$v}` core); a
/// single Exact key descends into that key's value; a multi-entry object is
/// conjunctive (bind each Exact key's leaf value, emit one match). Mixed and
/// nested multi-entry patterns generalize in Step 5 via continuation-passing.
fn walk_object<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, entries: &[(KeyMatcher, Vec<Step>)],
    src: &'a [u8], content: &str,
    binds: Vec<Binding>, out: &mut Vec<Vec<Binding>>,
) {
    let kds = entries_kd(fmt, node, src);
    if entries.is_empty() {
        if is_container(fmt, node) { out.push(binds); }
        return;
    }
    if entries.len() == 1 {
        let (km, vpat) = &entries[0];
        match km {
            KeyMatcher::Capture(name) => {
                for (kt, ks, v) in &kds {
                    let mut b = binds.clone();
                    b.push((name.clone(), kt.clone(), ks.0, ks.1));
                    walk_steps(fmt, *v, vpat, src, content, b, out);
                }
            }
            KeyMatcher::Wildcard => {
                for (_, _, v) in &kds {
                    walk_steps(fmt, *v, vpat, src, content, binds.clone(), out);
                }
            }
            KeyMatcher::Exact(s) => {
                for (kt, _, v) in &kds {
                    if kt == s {
                        walk_steps(fmt, *v, vpat, src, content, binds, out);
                        break;
                    }
                }
            }
            KeyMatcher::Glob(g) => {
                // `re:REGEX` or a glob (`*`/`?`); match keys, descend each hit.
                let Some(re) = compile_key_matcher(g) else { return };
                for (kt, _, v) in &kds {
                    if re.is_match(kt) {
                        walk_steps(fmt, *v, vpat, src, content, binds.clone(), out);
                    }
                }
            }
            // RecursiveCapture keys never reach here (no parser surface yet).
            _ => {}
        }
        return;
    }
    // Conjunctive: every entry is an Exact key with a bare capture-leaf value.
    let mut b = binds;
    for (km, vpat) in entries {
        let key = match km { KeyMatcher::Exact(s) => s, _ => return };
        let Some((_, _, v)) = kds.iter().find(|(kt, _, _)| kt == key) else { return };
        let Some(Step::Leaf { capture: Some(name) }) = vpat.first() else { return };
        let (t, lo, hi) = value_text_span(fmt, *v, content);
        b.push((name.clone(), t, lo, hi));
    }
    out.push(b);
}

/// Whether `node` is an object/map for `fmt` (so `{}` matches it).
fn is_container(fmt: Fmt, node: tree_sitter::Node) -> bool {
    match fmt {
        Fmt::Json => node.kind() == "object",
        Fmt::Yaml => matches!(node.kind(), "block_mapping" | "flow_mapping"),
        Fmt::Toml => matches!(
            node.kind(),
            "document" | "table" | "table_array_element" | "inline_table"
        ),
    }
}

/// All named descendant nodes of `node` (pre-order, excluding `node` itself).
/// Backs the `**` recursive-descent step.
fn all_descendants<'a>(node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut c = n.walk();
        for child in n.named_children(&mut c) {
            out.push(child);
            stack.push(child);
        }
    }
    out
}

/// Compile a key matcher: `re:REGEX` compiles the regex verbatim; anything else
/// is a glob (`*`→`.*`, `?`→`.`, rest escaped), full-match anchored. Returns
/// None on a compile error (the caller treats it as no-match).
fn compile_key_matcher(g: &str) -> Option<regex::Regex> {
    let pat = if let Some(re) = g.strip_prefix("re:") {
        re.to_string()
    } else {
        let mut s = String::from("^");
        for c in g.chars() {
            match c {
                '*' => s.push_str(".*"),
                '?' => s.push('.'),
                c => s.push_str(&regex::escape(&c.to_string())),
            }
        }
        s.push('$');
        s
    };
    regex::Regex::new(&pat).ok()
}

/// Like `entries` but yields (joined key text, key byte span, value node) — the
/// key span feeds capture binding (a captured key binds its text AND its span).
fn entries_kd<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, src: &[u8],
) -> Vec<(String, (usize, usize), tree_sitter::Node<'a>)> {
    let mut out = Vec::new();
    match fmt {
        Fmt::Json => {
            if node.kind() != "object" { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if pair.kind() != "pair" { continue; }
                if let (Some(k), Some(v)) =
                    (pair.child_by_field_name("key"), pair.child_by_field_name("value"))
                {
                    out.push((json_str_value(k, src), (k.start_byte(), k.end_byte()), v));
                }
            }
        }
        Fmt::Yaml => {
            if !matches!(node.kind(), "block_mapping" | "flow_mapping") { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if !matches!(pair.kind(), "block_mapping_pair" | "flow_pair") { continue; }
                if let (Some(k), Some(v)) =
                    (pair.child_by_field_name("key"), pair.child_by_field_name("value"))
                {
                    let ku = yaml_unwrap(k);
                    let vu = yaml_unwrap(v);
                    out.push((yaml_scalar_text(ku, src), (ku.start_byte(), ku.end_byte()), vu));
                }
            }
        }
        Fmt::Toml => match node.kind() {
            "document" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    match child.kind() {
                        "pair" => { if let Some(r) = toml_pair_kd(child, src) { out.push(r); } }
                        "table" | "table_array_element" => {
                            if let Some(k) = toml_key_node(child) {
                                let kt = toml_key_segs(k, src).join(".");
                                out.push((kt, (k.start_byte(), k.end_byte()), child));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "table" | "table_array_element" | "inline_table" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    if child.kind() == "pair" {
                        if let Some(r) = toml_pair_kd(child, src) { out.push(r); }
                    }
                }
            }
            _ => {}
        },
    }
    out
}

/// A toml `pair` as (joined key text, key span, value node).
fn toml_pair_kd<'a>(
    pair: tree_sitter::Node<'a>, src: &[u8],
) -> Option<(String, (usize, usize), tree_sitter::Node<'a>)> {
    let k = toml_key_node(pair)?;
    let kt = toml_key_segs(k, src).join(".");
    let ks = (k.start_byte(), k.end_byte());
    let mut c = pair.walk();
    let v = pair
        .named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "bare_key" | "dotted_key" | "quoted_key" | "comment"))
        .last()?;
    Some((kt, ks, v))
}

#[cfg(test)]
mod pattern_tests {
    use super::*;

    fn obj_one_entry(body: &str) -> (KeyMatcher, Vec<Step>) {
        let (s, _) = parse_pattern(body).unwrap();
        match s.into_iter().next() {
            Some(Step::Object { entries }) => entries.into_iter().next().unwrap(),
            other => panic!("expected a single-entry object, got {:?}", other),
        }
    }

    #[test]
    fn flat_object_capture() {
        let (k, v) = obj_one_entry("{ name: $NAME }");
        assert!(matches!(k, KeyMatcher::Exact(n) if n == "name"));
        assert!(matches!(v[0], Step::Leaf { capture: Some(ref n) } if n == "NAME"));
        let (_, c) = parse_pattern("{ name: $NAME }").unwrap();
        assert_eq!(c, vec!["NAME".to_string()]);
    }

    #[test]
    fn captured_key_and_value() {
        let (k, v) = obj_one_entry("{ $K: $V }");
        assert!(matches!(k, KeyMatcher::Capture(ref n) if n == "K"));
        assert!(matches!(v[0], Step::Leaf { capture: Some(ref n) } if n == "V"));
        let (_, c) = parse_pattern("{ $K: $V }").unwrap();
        assert_eq!(c, vec!["K".to_string(), "V".to_string()]);
    }

    #[test]
    fn wildcard_value_binds_nothing() {
        let (k, v) = obj_one_entry("{ x: $_ }");
        assert!(matches!(k, KeyMatcher::Exact(ref n) if n == "x"));
        assert!(matches!(v[0], Step::Leaf { capture: None }));
        let (_, c) = parse_pattern("{ x: $_ }").unwrap();
        assert!(c.is_empty());
    }

    #[test]
    fn leaf_eq_value_glob() {
        let (_, v) = obj_one_entry("{ t: hello }");
        assert!(matches!(&v[0], Step::LeafEq { text } if text == "hello"));
    }

    #[test]
    fn leaf_eq_quoted() {
        let (_, v) = obj_one_entry(r#"{ t: "hi there" }"#);
        assert!(matches!(&v[0], Step::LeafEq { text } if text == "hi there"));
    }

    #[test]
    fn leaf_pattern_when_dollar_in_quotes() {
        let (_, v) = obj_one_entry(r#"{ image: "$REPO:$TAG" }"#);
        assert!(matches!(&v[0], Step::LeafPattern { pattern } if pattern == "$REPO:$TAG"));
    }

    #[test]
    fn empty_object() {
        let (s, _) = parse_pattern("{}").unwrap();
        assert!(matches!(s[0], Step::Object { ref entries } if entries.is_empty()));
    }

    #[test]
    fn array_spread() {
        let (k, v) = obj_one_entry("{ items: [...$X] }");
        assert!(matches!(k, KeyMatcher::Exact(ref n) if n == "items"));
        match &v[0] {
            Step::Array { item } => {
                assert!(matches!(item[0], Step::Leaf { capture: Some(ref n) } if n == "X"));
            }
            other => panic!("expected array, got {:?}", other),
        }
        let (_, c) = parse_pattern("{ items: [...$X] }").unwrap();
        assert_eq!(c, vec!["X".to_string()]);
    }

    #[test]
    fn star_key_expands_to_any() {
        let (s, _) = parse_pattern("{ **: { image: $I } }").unwrap();
        assert!(matches!(s[0], Step::Any));
        assert!(matches!(&s[1], Step::Object { entries } if entries.len() == 1));
    }

    #[test]
    fn nested_object() {
        let (k, v) = obj_one_entry("{ a: { b: $X } }");
        assert!(matches!(k, KeyMatcher::Exact(ref n) if n == "a"));
        match &v[0] {
            Step::Object { entries } => {
                let (k2, v2) = &entries[0];
                assert!(matches!(k2, KeyMatcher::Exact(ref n) if n == "b"));
                assert!(matches!(v2[0], Step::Leaf { capture: Some(ref n) } if n == "X"));
            }
            other => panic!("expected nested object, got {:?}", other),
        }
    }

    #[test]
    fn braced_capture_optional_suffix() {
        let (_, v) = obj_one_entry("{ n: ${N?} }");
        assert!(matches!(&v[0], Step::Leaf { capture: Some(ref n) } if n == "N"));
        let (_, c) = parse_pattern("{ n: ${N?} }").unwrap();
        assert_eq!(c, vec!["N".to_string()]);
    }

    #[test]
    fn trailing_content_errors() {
        assert!(parse_pattern("{ a: $A } junk").is_err());
    }

    #[test]
    fn re_key_classifies_as_glob() {
        let (k, _) = obj_one_entry("{ re:v+: $V }");
        assert!(matches!(k, KeyMatcher::Glob(ref g) if g.starts_with("re:")));
    }

    #[test]
    fn invalid_capture_name_errors() {
        assert!(parse_pattern("{ x: $1 }").is_err());
    }
}

#[cfg(test)]
mod run_pattern_tests {
    use super::*;
    use std::collections::HashMap;

    /// Run a brace pattern over a json doc, returning one map (capture → text) per match.
    fn run(body: &str, json: &str) -> Vec<HashMap<String, String>> {
        let (steps, _) = parse_pattern(body).unwrap();
        run_pattern("d.json", json, &steps)
            .into_iter()
            .map(|b| b.into_iter().map(|(n, t, _, _)| (n, t)).collect())
            .collect()
    }

    #[test]
    fn captured_key_and_value_iterates_entries() {
        let ms = run("{ $k: $v }", r#"{"name":"x","age":7}"#);
        assert_eq!(ms.len(), 2, "{:?}", ms);
        let by_key: HashMap<String, String> =
            ms.iter().map(|m| (m["k"].clone(), m["v"].clone())).collect();
        assert_eq!(by_key["name"], "x");
        assert_eq!(by_key["age"], "7");
    }

    #[test]
    fn exact_key_descends_and_binds() {
        let ms = run("{ name: $N }", r#"{"name":"x","other":9}"#);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0]["N"], "x");
    }

    #[test]
    fn conjunctive_multi_entry_one_match() {
        let ms = run("{ a: $A, b: $B }", r#"{"a":1,"b":2}"#);
        assert_eq!(ms.len(), 1, "{:?}", ms);
        assert_eq!(ms[0]["A"], "1");
        assert_eq!(ms[0]["B"], "2");
    }

    #[test]
    fn nested_object_descends() {
        let ms = run("{ a: { b: $X } }", r#"{"a":{"b":5}}"#);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0]["X"], "5");
    }

    #[test]
    fn wildcard_value_matches_without_binding() {
        let ms = run("{ x: $_ }", r#"{"x":42}"#);
        assert_eq!(ms.len(), 1);
        assert!(ms[0].is_empty(), "{:?}", ms[0]);
    }

    #[test]
    fn leaf_eq_matches_literal() {
        let ms = run("{ role: admin }", r#"{"role":"admin"}"#);
        assert_eq!(ms.len(), 1);
        assert!(ms[0].is_empty(), "{:?}", ms[0]);
    }

    #[test]
    fn leaf_eq_rejects_mismatch() {
        let ms = run("{ role: admin }", r#"{"role":"user"}"#);
        assert!(ms.is_empty(), "{:?}", ms);
    }

    #[test]
    fn missing_key_yields_no_match() {
        let ms = run("{ nope: $X }", r#"{"a":1}"#);
        assert!(ms.is_empty());
    }

    #[test]
    fn bindings_carry_byte_spans() {
        // The value "x" sits at bytes 9..10 inside {"name":"x"} (after the
        // opening quote at byte 8). A captured leaf binds that span.
        let (steps, _) = parse_pattern("{ name: $N }").unwrap();
        let content = r#"{"name":"x"}"#;
        let ms = run_pattern("d.json", content, &steps);
        assert_eq!(ms.len(), 1);
        let (_, _, lo, hi) = &ms[0][0];
        assert_eq!(&content[*lo..*hi], "x");
    }

    fn run_json(body: &str, json: &str) -> Vec<HashMap<String, String>> {
        let (steps, _) = parse_pattern(body).unwrap();
        run_pattern("d.json", json, &steps)
            .into_iter()
            .map(|b| b.into_iter().map(|(n, t, _, _)| (n, t)).collect())
            .collect()
    }

    #[test]
    fn star_star_recursive_descent() {
        // `{ **: { image: $i } }` finds the image key at any depth.
        let json = r#"{"a":{"b":{"image":"deep"}},"top":1}"#;
        let ms = run_json("{ **: { image: $i } }", json);
        let images: Vec<_> = ms.iter().map(|m| m["i"].clone()).collect();
        assert!(images.contains(&"deep".to_string()), "{:?}", images);
    }

    #[test]
    fn array_spread_binds_each_item() {
        let json = r#"{"tags":["a","b","c"]}"#;
        let ms = run_json("{ tags: [...$t] }", json);
        let tags: Vec<_> = ms.iter().map(|m| m["t"].clone()).collect();
        assert_eq!(tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn regex_key_matches() {
        // `re:^v` matches keys starting with 'v'.
        let json = r#"{"v1":"x","v2":"y","other":"z"}"#;
        let ms = run_json("{ re:^v: $val }", json);
        let vals: Vec<_> = ms.iter().map(|m| m["val"].clone()).collect();
        assert_eq!(vals.len(), 2, "{:?}", vals);
        assert!(vals.contains(&"x".to_string()));
        assert!(vals.contains(&"y".to_string()));
    }

    #[test]
    fn glob_key_matches() {
        // `*id` matches any key ending in 'id'.
        let json = r#"{"uid":"a","gid":"b","name":"c"}"#;
        let ms = run_json("{ *id: $v }", json);
        assert_eq!(ms.len(), 2, "{:?}", ms);
    }
}
