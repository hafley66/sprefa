//! v3 Unified Language Prototype.
//!
//! Demonstrates the Lispy unified grammar from `v2/docs/_b_v3-unified-language.md`:
//!
//!   1. Abstraction: `(name) expr` binds name in the env
//!   2. Application: `op[bracket](paren){brace}` invokes an op with slot args
//!   3. Reference: bare ident resolves through the env
//!   4. Field access: `rule.CAPTURE` projects a capture from a rule value
//!   5. Rename: `> TARGET` projects with a new local name
//!   6. Carveout: `${expr}` embeds host expression inside a sub-grammar body
//!   7. Casing dispatch: UPPER = term, lower = op/rule, `:` = atom
//!
//! The prototype runs the following example program:
//!
//!     (classes) ast[rust] { class ${N} }
//!     (calls) ast[rust] { new ${classes.N > TARGET}() }
//!
//! Output shows:
//!   - parsed AST
//!   - env contents (the unified name table)
//!   - resolved AST (names replaced with EntityRef)
//!   - carveout extraction from each ast body
//!   - nested carveout demonstration
//!
//! Run: `cargo run --bin unified`

#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt;

// -- source --------------------------------------------------------------------

const SRC: &str = r#"
(classes) ast[rust] { class ${N} }
(calls)   ast[rust] { new ${classes.N > TARGET}() }
"#;

// For the "nested carveout" demonstration at the end.
const NESTED_SRC: &str = r#"
(outer) sh[bash] { echo ${{HOME}}/${:pathspec.$DIR > LOCAL} }
"#;

// -- AST -----------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    NameRef(String),
    Atom(String),
    StringLit(String),
    Application { op: String, slots: Vec<Slot> },
    FieldAccess { base: Box<Expr>, field: String },
    Carveout { body: Box<Expr>, rename: Option<String> },
    Path { segments: Vec<PathSeg> },
}

#[derive(Debug, Clone)]
enum PathSeg {
    Head(String),      // rule or op name at head
    CaptureField(String), // .FIELD (UPPERCASE)
    Atom(String),      // :atom
}

#[derive(Debug, Clone)]
enum Slot {
    Bracket(String),   // body text, sub-grammar parsed later
    Paren(String),
    Brace(String),
}

#[derive(Debug, Clone)]
enum Stmt {
    Binding { name: String, expr: Expr },
    Expr(Expr),
}

// -- Env + resolver ------------------------------------------------------------

#[derive(Debug, Clone)]
enum Entity {
    /// Built-in op. Carries its name for display.
    Op(String),
    /// User-defined rule. Carries the statement index in the program.
    Rule { stmt_index: usize, captures: Vec<String> },
    /// Capture decl from an enclosing rule's body.
    Capture { rule_name: String, field: String },
    /// Scalar literal.
    Scalar(ScalarValue),
}

#[derive(Debug, Clone)]
enum ScalarValue {
    Atom(String),
    String(String),
}

#[derive(Debug, Default)]
struct Env {
    /// Flat view for this prototype. A full impl walks parent scopes.
    entries: HashMap<String, Entity>,
}

impl Env {
    fn with_builtins() -> Self {
        let mut e = Env::default();
        for name in ["ast", "json", "md", "re", "sh", "fs", "repo", "rev", "sha256", "count", "filter", "retry", "time"] {
            e.entries.insert(name.to_string(), Entity::Op(name.to_string()));
        }
        e
    }

    fn insert(&mut self, name: String, ent: Entity) {
        self.entries.insert(name, ent);
    }

    fn lookup(&self, name: &str) -> Option<&Entity> {
        self.entries.get(name)
    }
}

// -- resolved AST --------------------------------------------------------------

#[derive(Debug, Clone)]
enum ResolvedExpr {
    Entity(Entity),
    Application { op: Entity, slots: Vec<Slot> },
    FieldProjection { rule: Entity, field: String },
    CarveoutResolved { body: Box<ResolvedExpr>, rename: Option<String> },
    PathResolved(Vec<ResolvedPathSeg>),
    StringLit(String),
    Atom(String),
    Unresolved(String),
}

#[derive(Debug, Clone)]
enum ResolvedPathSeg {
    Head(Entity),
    CaptureField(String),
    Atom(String),
}

// -- parser (hand-rolled, lispy subset) ----------------------------------------

struct Parser<'a> {
    src:  &'a [u8],
    pos:  usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src: src.as_bytes(), pos: 0 }
    }

    fn eof(&self) -> bool { self.pos >= self.src.len() }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while let Some(c) = self.peek() {
                if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                    self.pos += 1;
                } else { break }
            }
            // line comment :-
            if self.src.get(self.pos..self.pos+2) == Some(b":-") {
                while let Some(c) = self.peek() {
                    if c == b'\n' { break }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Skip spaces and tabs only, preserving newline as a separator.
    fn skip_inline_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_program(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        self.skip_ws_and_comments();
        while !self.eof() {
            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            }
            self.skip_ws_and_comments();
        }
        stmts
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        self.skip_ws_and_comments();
        if self.peek() == Some(b'(') {
            // Binding: (name) expr  or  (name) = expr
            let saved = self.pos;
            self.pos += 1;
            self.skip_ws_and_comments();
            let name = self.parse_ident();
            self.skip_ws_and_comments();
            if self.peek() == Some(b')') {
                self.pos += 1;
                self.skip_ws_and_comments();
                // optional `=`
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    self.skip_ws_and_comments();
                }
                let expr = self.parse_expr();
                return Some(Stmt::Binding { name, expr });
            } else {
                self.pos = saved;
            }
        }
        if self.eof() { return None }
        let expr = self.parse_expr();
        Some(Stmt::Expr(expr))
    }

    fn parse_expr(&mut self) -> Expr {
        self.skip_ws_and_comments();
        // Peek first token: ident, :, ${, &{, string, number
        let Some(c) = self.peek() else { return Expr::NameRef("<eof>".into()) };

        // atom literal :name
        if c == b':' {
            self.pos += 1;
            let name = self.parse_ident();
            return Expr::Atom(name);
        }

        // carveout ${...}
        if c == b'$' && self.src.get(self.pos+1) == Some(&b'{') {
            return self.parse_carveout();
        }

        // string literal
        if c == b'"' {
            return self.parse_string();
        }

        // ident-based: application, path, field-access chain
        let first = self.parse_ident();
        self.skip_ws_and_comments();

        // dotted path: first.segment.segment...
        if self.peek() == Some(b'.') {
            let mut segs = vec![PathSeg::Head(first)];
            while self.peek() == Some(b'.') {
                self.pos += 1;
                if self.peek() == Some(b'$') {
                    self.pos += 1;
                    let cap = self.parse_ident();
                    segs.push(PathSeg::CaptureField(cap));
                } else {
                    let seg = self.parse_ident();
                    if is_upper(&seg) {
                        segs.push(PathSeg::CaptureField(seg));
                    } else {
                        segs.push(PathSeg::Head(seg));
                    }
                }
            }
            self.skip_ws_and_comments();
            // optional > RENAME
            let rename = self.parse_optional_rename();
            let mut expr = Expr::Path { segments: segs };
            if let Some(r) = rename {
                expr = Expr::Carveout { body: Box::new(expr), rename: Some(r) };
            }
            return expr;
        }

        // application: op followed by slots
        if let Some(c2) = self.peek() {
            if c2 == b'[' || c2 == b'(' || c2 == b'{' {
                let slots = self.parse_slots();
                return Expr::Application { op: first, slots };
            }
        }

        // bare ident = name ref (with possible rename)
        let rename = self.parse_optional_rename();
        if let Some(r) = rename {
            return Expr::Carveout {
                body: Box::new(Expr::NameRef(first)),
                rename: Some(r),
            };
        }
        Expr::NameRef(first)
    }

    fn parse_optional_rename(&mut self) -> Option<String> {
        self.skip_ws_and_comments();
        if self.peek() == Some(b'>') {
            self.pos += 1;
            self.skip_ws_and_comments();
            if self.peek() == Some(b'$') { self.pos += 1; }
            return Some(self.parse_ident());
        }
        None
    }

    fn parse_slots(&mut self) -> Vec<Slot> {
        let mut slots = Vec::new();
        loop {
            self.skip_inline_ws();
            let Some(c) = self.peek() else { break };
            match c {
                b'[' => slots.push(Slot::Bracket(self.parse_balanced(b'[', b']'))),
                b'(' => slots.push(Slot::Paren  (self.parse_balanced(b'(', b')'))),
                b'{' => slots.push(Slot::Brace  (self.parse_balanced(b'{', b'}'))),
                _    => break,
            }
        }
        slots
    }

    fn parse_balanced(&mut self, open: u8, close: u8) -> String {
        assert_eq!(self.peek(), Some(open));
        self.pos += 1;
        let start = self.pos;
        let mut depth = 1i32;
        while let Some(c) = self.peek() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    let body = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
                    self.pos += 1;
                    return body;
                }
            }
            self.pos += 1;
        }
        panic!("unbalanced delimiter at pos {}", start);
    }

    fn parse_carveout(&mut self) -> Expr {
        // ${ expr }
        assert_eq!(self.peek(), Some(b'$'));
        self.pos += 1;
        assert_eq!(self.peek(), Some(b'{'));
        let body_text = self.parse_balanced(b'{', b'}');
        // re-parse body as an expression
        let mut inner = Parser::new(&body_text);
        let expr = inner.parse_expr();
        inner.skip_ws_and_comments();
        Expr::Carveout { body: Box::new(expr), rename: None }
    }

    fn parse_string(&mut self) -> Expr {
        assert_eq!(self.peek(), Some(b'"'));
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'"' {
                let body = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
                self.pos += 1;
                return Expr::StringLit(body);
            }
            self.pos += 1;
        }
        panic!("unterminated string at pos {}", start);
    }

    fn parse_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string()
    }
}

fn is_upper(s: &str) -> bool {
    s.chars().next().map_or(false, |c| c.is_ascii_uppercase())
}

// -- resolver ------------------------------------------------------------------

fn build_env(stmts: &[Stmt]) -> Env {
    let mut env = Env::with_builtins();
    for (i, stmt) in stmts.iter().enumerate() {
        if let Stmt::Binding { name, expr } = stmt {
            // Detect capture decls inside this rule's body by scanning for
            // carveouts that are bare NameRef UPPERCASE at head.
            let caps = collect_capture_decls(expr);
            env.insert(name.clone(), Entity::Rule { stmt_index: i, captures: caps });
        }
    }
    env
}

fn collect_capture_decls(e: &Expr) -> Vec<String> {
    let mut caps = Vec::new();
    collect_caps_walk(e, &mut caps);
    caps
}

fn collect_caps_walk(e: &Expr, caps: &mut Vec<String>) {
    match e {
        Expr::Carveout { body, .. } => {
            if let Expr::NameRef(n) = body.as_ref() {
                if is_upper(n) { caps.push(n.clone()); }
            }
            collect_caps_walk(body, caps);
        }
        Expr::Application { slots, .. } => {
            for s in slots {
                let body = match s {
                    Slot::Bracket(t) | Slot::Paren(t) | Slot::Brace(t) => t,
                };
                for range in scan_carveout_ranges(body) {
                    let inner_src = &body[range.0 + 2 .. range.1 - 1];
                    let mut p = Parser::new(inner_src);
                    let inner_expr = p.parse_expr();
                    collect_caps_walk(&inner_expr, caps);
                }
            }
        }
        Expr::FieldAccess { base, .. } => collect_caps_walk(base, caps),
        Expr::Path { .. } | Expr::NameRef(_) | Expr::Atom(_) | Expr::StringLit(_) => {}
    }
}

/// Scan a string for ${...} carveout ranges using balanced-brace rules.
fn scan_carveout_ranges(src: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i+1) == Some(&b'{') {
            // skip escape sequence ${{...}}
            if bytes.get(i+2) == Some(&b'{') {
                // walk past ${{...}}
                let mut depth = 0i32;
                let mut j = i + 2;
                while j < bytes.len() {
                    if bytes[j] == b'{' { depth += 1; }
                    else if bytes[j] == b'}' {
                        depth -= 1;
                        if depth == 0 { j += 1; break; }
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            let start = i;
            let mut depth = 0i32;
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'{' { depth += 1; }
                else if bytes[j] == b'}' {
                    depth -= 1;
                    if depth == 0 { j += 1; break; }
                }
                j += 1;
            }
            out.push((start, j));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn resolve(expr: &Expr, env: &Env, local_rule: Option<&str>) -> ResolvedExpr {
    match expr {
        Expr::NameRef(n) => {
            if is_upper(n) {
                // Capture ref: resolves to the enclosing rule's capture slot
                if let Some(rule) = local_rule {
                    ResolvedExpr::Entity(Entity::Capture {
                        rule_name: rule.to_string(),
                        field: n.clone(),
                    })
                } else {
                    ResolvedExpr::Unresolved(n.clone())
                }
            } else {
                match env.lookup(n) {
                    Some(e) => ResolvedExpr::Entity(e.clone()),
                    None    => ResolvedExpr::Unresolved(n.clone()),
                }
            }
        }
        Expr::Atom(a) => ResolvedExpr::Atom(a.clone()),
        Expr::StringLit(s) => ResolvedExpr::StringLit(s.clone()),
        Expr::Application { op, slots } => {
            let op_ent = env.lookup(op).cloned()
                .unwrap_or_else(|| Entity::Op(format!("UNRESOLVED:{op}")));
            ResolvedExpr::Application { op: op_ent, slots: slots.clone() }
        }
        Expr::Path { segments } => {
            let mut out = Vec::new();
            for (i, seg) in segments.iter().enumerate() {
                match seg {
                    PathSeg::Head(name) if i == 0 => {
                        let ent = env.lookup(name).cloned()
                            .unwrap_or(Entity::Op(format!("UNRESOLVED:{name}")));
                        out.push(ResolvedPathSeg::Head(ent));
                    }
                    PathSeg::Head(name) => {
                        out.push(ResolvedPathSeg::CaptureField(name.clone()));
                    }
                    PathSeg::CaptureField(f) => {
                        out.push(ResolvedPathSeg::CaptureField(f.clone()));
                    }
                    PathSeg::Atom(a) => {
                        out.push(ResolvedPathSeg::Atom(a.clone()));
                    }
                }
            }
            ResolvedExpr::PathResolved(out)
        }
        Expr::FieldAccess { base, field } => {
            let base_r = resolve(base, env, local_rule);
            if let ResolvedExpr::Entity(ent) = base_r {
                ResolvedExpr::FieldProjection { rule: ent, field: field.clone() }
            } else {
                ResolvedExpr::Unresolved(format!(".{field}"))
            }
        }
        Expr::Carveout { body, rename } => {
            let inner = resolve(body, env, local_rule);
            ResolvedExpr::CarveoutResolved {
                body: Box::new(inner),
                rename: rename.clone(),
            }
        }
    }
}

// -- display helpers -----------------------------------------------------------

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::NameRef(n)   => write!(f, "{n}"),
            Expr::Atom(a)      => write!(f, ":{a}"),
            Expr::StringLit(s) => write!(f, "\"{s}\""),
            Expr::Application { op, slots } => {
                write!(f, "{op}")?;
                for s in slots {
                    match s {
                        Slot::Bracket(t) => write!(f, "[{t}]")?,
                        Slot::Paren(t)   => write!(f, "({t})")?,
                        Slot::Brace(t)   => write!(f, "{{{t}}}")?,
                    }
                }
                Ok(())
            }
            Expr::FieldAccess { base, field } => write!(f, "{base}.{field}"),
            Expr::Carveout { body, rename } => match rename {
                Some(r) => write!(f, "${{{body} > {r}}}"),
                None    => write!(f, "${{{body}}}"),
            },
            Expr::Path { segments } => {
                for (i, seg) in segments.iter().enumerate() {
                    if i > 0 { write!(f, ".")?; }
                    match seg {
                        PathSeg::Head(n) => write!(f, "{n}")?,
                        PathSeg::CaptureField(c) => write!(f, "${c}")?,
                        PathSeg::Atom(a) => write!(f, ":{a}")?,
                    }
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Entity::Op(n)   => write!(f, "Op({n})"),
            Entity::Rule { stmt_index, captures } => {
                write!(f, "Rule(#{stmt_index}, captures={captures:?})")
            }
            Entity::Capture { rule_name, field } => {
                write!(f, "Capture({rule_name}.{field})")
            }
            Entity::Scalar(ScalarValue::Atom(a))   => write!(f, ":{a}"),
            Entity::Scalar(ScalarValue::String(s)) => write!(f, "\"{s}\""),
        }
    }
}

// -- demo ---------------------------------------------------------------------

fn demo_program(label: &str, src: &str) {
    println!("\n============================================================");
    println!("  {label}");
    println!("============================================================");
    println!("source:");
    for line in src.lines() {
        if line.trim().is_empty() { continue }
        println!("    {line}");
    }

    let mut parser = Parser::new(src);
    let stmts = parser.parse_program();

    println!("\n[parse] {} statements:", stmts.len());
    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Binding { name, expr } => println!("  #{i}  bind {name} = {expr}"),
            Stmt::Expr(e)                => println!("  #{i}  expr = {e}"),
        }
    }

    let env = build_env(&stmts);
    println!("\n[env] {} entries:", env.entries.len());
    let mut keys: Vec<_> = env.entries.keys().collect();
    keys.sort();
    for k in keys {
        let e = env.entries.get(k).unwrap();
        let mark = if matches!(e, Entity::Op(_)) { "  " } else { "* " };
        println!("  {mark}{k:20} -> {e}");
    }

    println!("\n[carveouts per rule body]");
    for stmt in &stmts {
        if let Stmt::Binding { name, expr } = stmt {
            if let Expr::Application { slots, op } = expr {
                for (si, s) in slots.iter().enumerate() {
                    let body = match s {
                        Slot::Bracket(t) | Slot::Paren(t) | Slot::Brace(t) => t,
                    };
                    let ranges = scan_carveout_ranges(body);
                    println!("  rule {name:10} op={op} slot#{si} body={body:?}");
                    for (s, e) in ranges {
                        let carve = &body[s..e];
                        let inner = &body[s+2..e-1];
                        println!("      carveout {s}..{e}: {carve}  inner-expr: {inner}");
                    }
                }
            }
        }
    }

    println!("\n[resolved exprs]");
    for stmt in &stmts {
        if let Stmt::Binding { name, expr } = stmt {
            if let Expr::Application { slots, .. } = expr {
                for (si, s) in slots.iter().enumerate() {
                    let body = match s {
                        Slot::Bracket(t) | Slot::Paren(t) | Slot::Brace(t) => t,
                    };
                    for (rs, re) in scan_carveout_ranges(body) {
                        let inner_src = &body[rs+2..re-1];
                        let mut p = Parser::new(inner_src);
                        let inner_expr = p.parse_expr();
                        let resolved = resolve(&inner_expr, &env, Some(name));
                        println!("  {name}[slot{si}] ${{{inner_src}}}");
                        print_resolved(&resolved, 6);
                    }
                }
            }
        }
    }
}

fn print_resolved(r: &ResolvedExpr, indent: usize) {
    let pad = " ".repeat(indent);
    match r {
        ResolvedExpr::Entity(e) => println!("{pad}-> {e}"),
        ResolvedExpr::Application { op, slots } => {
            println!("{pad}-> Application op={op}, slots={}", slots.len());
        }
        ResolvedExpr::FieldProjection { rule, field } => {
            println!("{pad}-> FieldProjection rule={rule} field={field}");
        }
        ResolvedExpr::CarveoutResolved { body, rename } => {
            println!("{pad}-> Carveout rename={rename:?}");
            print_resolved(body, indent + 2);
        }
        ResolvedExpr::PathResolved(segs) => {
            println!("{pad}-> Path:");
            for seg in segs {
                match seg {
                    ResolvedPathSeg::Head(e)            => println!("{pad}  head: {e}"),
                    ResolvedPathSeg::CaptureField(f)    => println!("{pad}  .{f}"),
                    ResolvedPathSeg::Atom(a)            => println!("{pad}  :{a}"),
                }
            }
        }
        ResolvedExpr::StringLit(s) => println!("{pad}-> String({s:?})"),
        ResolvedExpr::Atom(a)      => println!("{pad}-> Atom(:{a})"),
        ResolvedExpr::Unresolved(n)=> println!("{pad}-> UNRESOLVED({n})"),
    }
}

fn main() {
    println!("v3 Unified Language Prototype");
    println!("==============================\n");
    println!("Grammar productions demonstrated:");
    println!("  - abstraction:  (name) expr");
    println!("  - application:  op[slot](slot){{slot}}");
    println!("  - reference:    bare ident");
    println!("  - field access: rule.CAPTURE");
    println!("  - path:         rule.$CAPTURE with explicit $");
    println!("  - rename:       > LOCAL");
    println!("  - carveout:     ${{expr}}");
    println!("  - escape:       ${{{{bash_var}}}}");
    println!("  - atom literal: :name");
    println!("  - string lit:   \"text\"");

    demo_program("Example 1: classes + calls with inline xref", SRC);
    demo_program("Example 2: shell with bash escape + dotted path", NESTED_SRC);

    // Verify nested carveouts: show that a carveout can contain another application.
    println!("\n============================================================");
    println!("  Example 3: nested carveout (${{re(\"^foo\")}} inside rust body)");
    println!("============================================================");
    let nested = r#"(imports) ast[rust] { use ${re("^crate")} }"#;
    demo_program("nested carveout", nested);

    println!("\n============================================================");
    println!("  Summary");
    println!("============================================================");
    println!("  - One env, polymorphic entries (Op | Rule | Capture | Scalar)");
    println!("  - Casing dispatches: UPPER = term/capture, lower = op/rule");
    println!("  - Carveouts extracted uniformly across every sub-grammar body");
    println!("  - Inline xref resolves at lower time to FieldProjection");
    println!("  - Name lookup is one table, one walk");
}
