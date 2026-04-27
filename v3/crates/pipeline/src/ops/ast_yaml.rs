//! `ast_yaml[lang](raw_yaml)` — raw ast-grep RuleConfig YAML body.
//!
//! Escape hatch when the sprf-extended `ast[lang](pattern)` surface plus
//! bracket-DSL predicates (sprefa-9d1) and brace fan-out (sprefa-axf)
//! cannot reach a needed Rule shape. Body bytes between the parens are
//! handed verbatim to `serde_yaml::from_str::<SerializableRuleCore>` and
//! lowered into a `RuleCore<SupportLang>` once at compile time. The same
//! runtime path as `ast[lang]` runs the `find_all` (RuleCore implements
//! the Matcher trait), so per-cursor cost is identical to a plain ast op.
//!
//! First-pass scope (sprefa-5qb): no carveouts inside the YAML body. The
//! body is static; metavar captures come from ast-grep's native `$VAR`
//! syntax inside `pattern:` strings. Carveouts (`${X?}`) re-open this op
//! when sprefa-dkm lands.
//!
//! Example:
//!   ast_yaml[rs](
//!     pattern: 'fn $N() { $$$ }'
//!     not:
//!       inside: { kind: impl_item }
//!   )

use std::sync::Arc;

use ast_grep_config::{DeserializeEnv, RuleCore, SerializableRuleCore};
use ast_grep_core::{source::StrDoc, AstGrep};
use ast_grep_language::SupportLang;
use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

pub struct AstYamlOp {
    pub lang:           SupportLang,
    pub rule_core:      Arc<RuleCore<SupportLang>>,
    pub bound_caps:     Vec<Arc<str>>,
    pub _src:           Arc<str>,
    /// `${X?}` / `${X}` carveout token spans, absolute into the host
    /// source. Surfaced via `Op::term_positions()` so LSP hover
    /// recognises the cursor sitting inside a sprf metavar.
    pub term_positions: Arc<[crate::TermPosition]>,
}

impl std::fmt::Debug for AstYamlOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AstYamlOp")
            .field("lang", &self.lang)
            .field("bound_caps", &self.bound_caps)
            .field("src", &self._src)
            .finish()
    }
}

impl OpCtor for AstYamlOp {
    const NAME: &'static str = "ast_yaml";
    const BODY_GRAMMAR: &'static str =
        "ast-grep RuleConfig YAML (top-level rule fields); lang via `[rust|rs|ts|c]`";
    const DOC: &'static str = "\
`ast_yaml[lang](yaml)`

Match an ast-grep RuleConfig against the cursor's active bytes. The body
is parsed as YAML and deserialised into ast-grep's `SerializableRuleCore`
(top-level fields: `rule`, `constraints`, `utils`, `transform`, `fix`).

Use this op when the pattern needs Rule composition that the
`ast[lang](pattern)` surface does not yet expose: nested `inside`/`has`/
`follows`/`precedes`, `any`/`all`/`not`, `kind`/`regex` constraints,
named utility rules.

`lang` is `rust` (`rs`), `typescript` (`ts`), or `c`.

Captures bind under the metavar name in the cursor's capture set:
- ast-grep native `$VAR` / `$$$VAR` inside `pattern:` strings.
- sprf carveouts `${VAR?}` / `${VAR}` / `$$${VAR?}` anywhere in the
  YAML body; rewritten to native ast-grep metavars before deserialise.
";

    fn from_values(_values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Err(vec![PatternDiagnostic {
            code: "ast_yaml/missing-lang",
            message:
                "ast_yaml requires a bracket-arg form: ast_yaml[rust](rule: 'fn $N() {}')"
                    .into(),
            byte_range: 0..0,
        }])
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        Some(lower_ast_yaml_paren(inv_node, src))
    }
}

impl Op for AstYamlOp {
    fn name(&self) -> &'static str { "ast_yaml" }

    fn pipe<'a>(
        &'a self,
        ctx: &'a RtCtx,
        batch: Arc<[Cursor]>,
    ) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let parse_req = crate::effects::AstParseEffect {
                content:    c.content.clone(),
                byte_range: c.byte_range.clone(),
                lang:       self.lang,
            };
            let Some(grep) = ctx.put(parse_req).await else {
                return Vec::new();
            };
            scan_with_grep(self, &c, &grep)
        }))
    }

    fn bound_captures(&self) -> &[Arc<str>] { &self.bound_caps }

    fn term_positions(&self) -> &[crate::TermPosition] { &self.term_positions }

    fn cache_key(&self, h: &mut blake3::Hasher) -> bool {
        h.update(self.name().as_bytes());
        h.update(&[0u8]);
        h.update(self.lang.to_string().as_bytes());
        h.update(&[0u8]);
        h.update(self._src.as_bytes());
        true
    }
}

fn scan_with_grep(
    op:   &AstYamlOp,
    c:    &Cursor,
    grep: &AstGrep<StrDoc<SupportLang>>,
) -> Vec<Cursor> {
    let base = c.byte_range.start;
    let mut out: Vec<Cursor> = Vec::new();
    for nm in grep.root().find_all(&*op.rule_core) {
        let env = nm.get_env();
        let r = nm.range();
        let abs_match = (base + r.start)..(base + r.end);
        let mut next = c.clone();
        next.byte_range = abs_match;

        for cap_name in op.bound_caps.iter() {
            // Single-node capture preferred; fall back to multi.
            if let Some(node) = env.get_match(cap_name.as_ref()) {
                let nr = node.range();
                let span = (base + nr.start)..(base + nr.end);
                next.captures
                    .push(Capture::span_backed(cap_name.clone(), span));
                continue;
            }
            let multi = env.get_multiple_matches(cap_name.as_ref());
            if !multi.is_empty() {
                let joined: String = multi
                    .iter()
                    .map(|n| n.text().to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                next.captures.push(Capture::synthesized(
                    cap_name.clone(),
                    Arc::from(joined.as_str()),
                ));
            }
        }
        out.push(next);
    }
    out
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

fn lower_ast_yaml_paren(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<AstYamlOp, Vec<PatternDiagnostic>> {
    let bracket = inv_node.child_by_field_name("bracket").ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast_yaml/missing-lang",
            message:
                "ast_yaml requires a bracket-arg, e.g. ast_yaml[rust](rule: ...)".into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let br = bracket.byte_range();
    let lang_bytes = if br.end < br.start + 2 {
        &[][..]
    } else {
        &src[br.start + 1..br.end - 1]
    };
    let lang_name = std::str::from_utf8(lang_bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code: "ast_yaml/missing-lang",
            message: "ast_yaml bracket arg is not valid UTF-8".into(),
            byte_range: bracket.byte_range(),
        }])?
        .trim();
    let lang = parse_lang(lang_name).ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast_yaml/unknown-lang",
            message: format!(
                "ast_yaml language `{lang_name}` unknown; supported: rust, rs, typescript, ts, c"
            ),
            byte_range: bracket.byte_range(),
        }]
    })?;

    let paren = inv_node.child_by_field_name("paren").ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "ast_yaml/missing-body",
            message:
                "ast_yaml requires a paren body, e.g. ast_yaml[rust](rule: 'fn $N() {}')"
                    .into(),
            byte_range: inv_node.byte_range(),
        }]
    })?;
    let pr = paren.byte_range();
    let raw_bytes = if pr.end < pr.start + 2 { &[][..] } else { &src[pr.start + 1..pr.end - 1] };
    let raw = std::str::from_utf8(raw_bytes)
        .map_err(|_| vec![PatternDiagnostic {
            code: "ast_yaml/bad-yaml",
            message: "ast_yaml body is not valid UTF-8".into(),
            byte_range: paren.byte_range(),
        }])?;
    if raw.trim().is_empty() {
        return Err(vec![PatternDiagnostic {
            code: "ast_yaml/bad-yaml",
            message: "ast_yaml body is empty".into(),
            byte_range: paren.byte_range(),
        }]);
    }

    // Rewrite sprf carveouts (`${X?}`, `${X}`, `$$${X?}`, `$$${X}`) into
    // native ast-grep metavars (`$X`, `$$$X`) so the same authoring sigils
    // work here as in the `ast[lang]` op. The `?` suffix is stripped (its
    // unbound/bound semantics is for the v0 walker, not ast-grep). First
    // pass loses sub-token captures (no SPRFSLOTN regex trick) — sufficient
    // for whole-token metavars, which is the bulk of YAML rule authoring.
    let rewritten = rewrite_carveouts(raw);
    let (user_caps, body_rel_positions) = collect_carveout_names_and_positions(raw);
    // Lift body-relative carveout spans to absolute file offsets. The
    // body starts one byte after the paren's opening `(`.
    let body_origin = pr.start + 1;
    let term_positions: Vec<crate::TermPosition> = body_rel_positions
        .into_iter()
        .map(|(name, r)| crate::TermPosition {
            name,
            range: (body_origin + r.start)..(body_origin + r.end),
        })
        .collect();

    // Parse to a generic Value first so structural sugar ("did the user
    // wrap their rule under `rule:` or just write the rule fields at the
    // top?") falls out of YAML, not hand-rolled line scanning. If the
    // top-level mapping has any RuleCore envelope key (`rule`, `constraints`,
    // `utils`, `transform`, `fix`), pass the value through; otherwise wrap
    // it under `rule:` and re-deserialise.
    let value: serde_yaml::Value = serde_yaml::from_str(&rewritten).map_err(|e| {
        vec![PatternDiagnostic {
            code: "ast_yaml/bad-yaml",
            message: format!("ast_yaml YAML invalid: {e}"),
            byte_range: paren.byte_range(),
        }]
    })?;
    let core_value = wrap_under_rule_if_bare(value);
    let core: SerializableRuleCore = serde_yaml::from_value(core_value).map_err(|e| {
        vec![PatternDiagnostic {
            code: "ast_yaml/bad-yaml",
            message: format!("ast_yaml RuleCore deserialise failed: {e}"),
            byte_range: paren.byte_range(),
        }]
    })?;

    let env = DeserializeEnv::new(lang);
    let rule_core = core.get_matcher(env).map_err(|e| {
        vec![PatternDiagnostic {
            code: "ast_yaml/bad-rule",
            message: format!("ast_yaml rule build failed: {e}"),
            byte_range: paren.byte_range(),
        }]
    })?;

    // Author-facing capture set is the union of (a) what the user wrote
    // as `${X?}` carveouts in the source and (b) what ast-grep recovers
    // from the deserialised rule. Native `$X` mentions inside YAML
    // strings only show up via ast-grep's defined_vars().
    let mut bound_caps: Vec<Arc<str>> = user_caps;
    for v in rule_core.defined_vars() {
        let s: Arc<str> = Arc::from(v);
        if !bound_caps.iter().any(|c| c.as_ref() == s.as_ref()) {
            bound_caps.push(s);
        }
    }

    Ok(AstYamlOp {
        lang,
        rule_core:      Arc::new(rule_core),
        bound_caps,
        _src:           Arc::from(raw.trim()),
        term_positions: Arc::from(term_positions.into_boxed_slice()),
    })
}

fn parse_lang(s: &str) -> Option<SupportLang> {
    match s {
        "rust" | "rs"       => Some(SupportLang::Rust),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        "c"                 => Some(SupportLang::C),
        _                   => None,
    }
}

/// If the YAML body's top-level mapping uses a RuleCore envelope key,
/// hand it through; otherwise wrap the body under `rule:` so a bare
/// `pattern: ...` body deserialises as a full `SerializableRuleCore`.
fn wrap_under_rule_if_bare(value: serde_yaml::Value) -> serde_yaml::Value {
    const ENVELOPE_KEYS: &[&str] =
        &["rule", "constraints", "utils", "transform", "fix"];
    if let serde_yaml::Value::Mapping(m) = &value {
        let has_envelope = ENVELOPE_KEYS.iter().any(|k| {
            m.contains_key(serde_yaml::Value::String((*k).into()))
        });
        if has_envelope {
            return value;
        }
    }
    let mut wrapper = serde_yaml::Mapping::new();
    wrapper.insert(serde_yaml::Value::String("rule".into()), value);
    serde_yaml::Value::Mapping(wrapper)
}

/// Rewrite sprf carveouts to ast-grep native metavars in-place across the
/// whole YAML body. `${X?}` and `${X}` → `$X`; `$$${X?}` and `$$${X}` →
/// `$$$X`. Other `$` characters are preserved.
fn rewrite_carveouts(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let multi = bytes[i] == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let single = !multi
            && bytes[i] == b'$'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{';
        if multi || single {
            let inner_start = if multi { i + 4 } else { i + 2 };
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' { j += 1; }
            if j >= bytes.len() {
                // Unterminated; pass through as literal.
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }
            let inner = src[inner_start..j].trim();
            let name = inner.strip_suffix('?').unwrap_or(inner);
            if multi { out.push_str("$$$"); } else { out.push('$'); }
            out.push_str(name);
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Pull out names + body-relative spans of `${X?}` / `${X}` /
/// `$$${X?}` / `$$${X}` carveouts in source order. Names dedupe; spans
/// keep every occurrence so hover hits regardless of which mention the
/// user is parked on. Span covers the full carveout token (including
/// any outer `$$$` prefix).
fn collect_carveout_names_and_positions(
    src: &str,
) -> (Vec<Arc<str>>, Vec<(Arc<str>, std::ops::Range<usize>)>) {
    let bytes = src.as_bytes();
    let mut names: Vec<Arc<str>> = Vec::new();
    let mut positions: Vec<(Arc<str>, std::ops::Range<usize>)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let multi = bytes[i] == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let single = !multi
            && bytes[i] == b'$'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{';
        if multi || single {
            let token_start = i;
            let inner_start = if multi { i + 4 } else { i + 2 };
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' { j += 1; }
            if j >= bytes.len() { i += 1; continue; }
            let inner = src[inner_start..j].trim();
            let name = inner.strip_suffix('?').unwrap_or(inner);
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                let arc: Arc<str> = Arc::from(name);
                if !names.iter().any(|s| s.as_ref() == arc.as_ref()) {
                    names.push(arc.clone());
                }
                positions.push((arc, token_start..(j + 1)));
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    (names, positions)
}

