//! Typed ast-grep rule requests. `cst {}` remains the Tree-sitter-query surface.
//!
//! YAML decodes into [`AstRuleRequest`], the only rule model. Accepted YAML has
//! `id`, `rule`, optional `utils`, and optional `fix`; callers must reduce
//! official ast-grep files carrying `language`, `severity`, `message`, `files`,
//! or `constraints` before this boundary.

use std::collections::BTreeMap;

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, NodeMatch};
use serde::Serialize;

use crate::lang::extract_lang::ExtractLang;
use crate::shape::{content_id_of, ContentId, Span};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AstRule {
    Pattern(String),
    Kind(String),
    Regex(String),
    Matches(String),
    All(Vec<AstRule>),
    Any(Vec<AstRule>),
    Not(Box<AstRule>),
    Inside {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Has {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Follows {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
    Precedes {
        rule: Box<AstRule>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_by: Option<StopBy>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum StopBy {
    End(String),
    Rule(Box<AstRule>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleRequest {
    pub id: String,
    pub rule: AstRule,
    #[serde(default)]
    pub utils: Vec<NamedAstRule>,
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NamedAstRule {
    pub id: String,
    pub rule: AstRule,
}

/// Match identity is content identity plus a half-open byte span. `path` is
/// display/routing data and never match identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMatch {
    pub record: &'static str,
    pub query: String,
    pub path: String,
    pub content: ContentId,
    pub span: Span,
    pub captures: Vec<AstRuleCapture>,
    pub proposal: Option<AstRuleMutationProposal>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct AstRuleCapture {
    pub name: String,
    pub text: String,
    pub span: Span,
}

/// A replacement proposal carries the source content identity and byte span.
/// The engine supplies the target `ActionSource` at the Soopy staging seam.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMutationProposal {
    pub query: String,
    pub content: ContentId,
    pub span: Span,
    pub replacement: String,
}

impl AstRuleMutationProposal {
    /// Combines deterministic edits for one source into Soopy's one-action
    /// transaction. `stage_mutations` remains the conflict authority.
    pub fn stage_request_batch(
        proposals: &[Self],
        root: soopy::SourceRootId,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> Result<soopy::StageRequest, &'static str> {
        let Some(first) = proposals.first() else {
            return Err("ast-rule stage request requires at least one proposal");
        };
        if proposals
            .iter()
            .any(|proposal| proposal.content != first.content)
        {
            return Err("ast-rule proposals for one source must share content identity");
        }
        let mut proposals = proposals.to_vec();
        proposals.sort_by(|left, right| {
            (&left.span, &left.query, &left.replacement).cmp(&(
                &right.span,
                &right.query,
                &right.replacement,
            ))
        });
        let edits = proposals
            .into_iter()
            .map(|proposal| soopy::TextEdit {
                range: soopy::ActionSpan {
                    source: source.clone(),
                    start: proposal.span.start.into(),
                    end: proposal.span.end().into(),
                },
                replacement: proposal.replacement.into_bytes(),
                producer: producer.clone().with_rule(proposal.query),
            })
            .collect();
        Ok(soopy::StageRequest::new(
            root,
            vec![soopy::SourceAction::Replace {
                source,
                expected: first.content.clone(),
                edits,
            }],
        ))
    }

    pub fn stage_request(
        &self,
        root: soopy::SourceRootId,
        source: soopy::ActionSource,
        producer: soopy::ActionProducer,
    ) -> soopy::StageRequest {
        let edit = soopy::TextEdit {
            range: soopy::ActionSpan {
                source: source.clone(),
                start: self.span.start.into(),
                end: self.span.end().into(),
            },
            replacement: self.replacement.as_bytes().to_vec(),
            producer: producer.with_rule(self.query.clone()),
        };
        soopy::StageRequest::new(
            root,
            vec![soopy::SourceAction::Replace {
                source,
                expected: self.content.clone(),
                edits: vec![edit],
            }],
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AstRuleError {
    NoGrammar(String),
    Utf8(String),
    Yaml(String),
    InvalidRule(String),
}

impl std::fmt::Display for AstRuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for AstRuleError {}

pub fn decode_ast_rule_yaml(yaml: &str) -> Result<AstRuleRequest, AstRuleError> {
    let value =
        serde_yaml::from_str(yaml).map_err(|error| AstRuleError::Yaml(error.to_string()))?;
    decode_request_value(value).map_err(AstRuleError::Yaml)
}

fn decode_request_value(value: serde_yaml::Value) -> Result<AstRuleRequest, String> {
    let mut map = yaml_map(value)?;
    let id = yaml_string(take_yaml(&mut map, "id")?)?;
    let rule = decode_rule(take_yaml(&mut map, "rule")?)?;
    let fix = match map.remove(&serde_yaml::Value::String("fix".into())) {
        Some(value) => Some(yaml_string(value)?),
        None => None,
    };
    let utils = match map.remove(&serde_yaml::Value::String("utils".into())) {
        Some(serde_yaml::Value::Mapping(utils)) => utils
            .into_iter()
            .map(|(id, rule)| {
                Ok(NamedAstRule {
                    id: yaml_string(id)?,
                    rule: decode_rule(rule)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        Some(_) => return Err("utils must be a mapping of name to rule".into()),
        None => Vec::new(),
    };
    if let Some((field, _)) = map.into_iter().next() {
        return Err(format!(
            "unsupported ast-rule YAML field {}",
            yaml_string(field)?
        ));
    }
    Ok(AstRuleRequest {
        id,
        rule,
        utils,
        fix,
    })
}

fn decode_rule(value: serde_yaml::Value) -> Result<AstRule, String> {
    let map = yaml_map(value)?;
    if map.len() != 1 {
        return Err("a rule must have exactly one operator".into());
    }
    let (operator, value) = map.into_iter().next().expect("len checked");
    let operator = yaml_string(operator)?;
    match operator.as_str() {
        "pattern" => Ok(AstRule::Pattern(yaml_string(value)?)),
        "kind" => Ok(AstRule::Kind(yaml_string(value)?)),
        "regex" => Ok(AstRule::Regex(yaml_string(value)?)),
        "matches" => Ok(AstRule::Matches(yaml_string(value)?)),
        "all" => Ok(AstRule::All(yaml_rules(value)?)),
        "any" => Ok(AstRule::Any(yaml_rules(value)?)),
        "not" => Ok(AstRule::Not(Box::new(decode_rule(value)?))),
        "inside" => relation_rule(value, |rule, stop_by| AstRule::Inside { rule, stop_by }),
        "has" => relation_rule(value, |rule, stop_by| AstRule::Has { rule, stop_by }),
        "follows" => relation_rule(value, |rule, stop_by| AstRule::Follows { rule, stop_by }),
        "precedes" => relation_rule(value, |rule, stop_by| AstRule::Precedes { rule, stop_by }),
        _ => Err(format!("unknown ast-rule operator {operator}")),
    }
}

fn relation_rule(
    value: serde_yaml::Value,
    constructor: impl FnOnce(Box<AstRule>, Option<StopBy>) -> AstRule,
) -> Result<AstRule, String> {
    let mut map = yaml_map(value)?;
    let stop_by = match map.remove(&serde_yaml::Value::String("stopBy".into())) {
        Some(serde_yaml::Value::String(value)) => Some(StopBy::End(value)),
        Some(value) => Some(StopBy::Rule(Box::new(decode_rule(value)?))),
        None => None,
    };
    Ok(constructor(
        Box::new(decode_rule(serde_yaml::Value::Mapping(map))?),
        stop_by,
    ))
}

fn yaml_rules(value: serde_yaml::Value) -> Result<Vec<AstRule>, String> {
    match value {
        serde_yaml::Value::Sequence(values) => values.into_iter().map(decode_rule).collect(),
        _ => Err("rule list must be a YAML sequence".into()),
    }
}

fn yaml_map(value: serde_yaml::Value) -> Result<serde_yaml::Mapping, String> {
    match value {
        serde_yaml::Value::Mapping(map) => Ok(map),
        _ => Err("rule must be a YAML mapping".into()),
    }
}

fn yaml_string(value: serde_yaml::Value) -> Result<String, String> {
    match value {
        serde_yaml::Value::String(value) => Ok(value),
        _ => Err("expected YAML string".into()),
    }
}

fn take_yaml(map: &mut serde_yaml::Mapping, name: &str) -> Result<serde_yaml::Value, String> {
    map.remove(&serde_yaml::Value::String(name.into()))
        .ok_or_else(|| format!("missing ast-rule YAML field {name}"))
}

pub fn query_ast_rule(
    path: &str,
    bytes: &[u8],
    request: &AstRuleRequest,
) -> Result<Vec<AstRuleMatch>, AstRuleError> {
    query_ast_rule_with_content(path, bytes, request, content_id_of(bytes))
}

/// Query bytes supplied by a source host while retaining the source host's
/// content identity in every emitted source/span proposal relation.
pub fn query_ast_rule_with_content(
    path: &str,
    bytes: &[u8],
    request: &AstRuleRequest,
    content: ContentId,
) -> Result<Vec<AstRuleMatch>, AstRuleError> {
    let language =
        ExtractLang::from_path(path).ok_or_else(|| AstRuleError::NoGrammar(path.into()))?;
    let source =
        std::str::from_utf8(bytes).map_err(|error| AstRuleError::Utf8(error.to_string()))?;
    let config_yaml = serde_yaml::to_string(&ConfigWire::from_request(request, language))
        .map_err(|error| AstRuleError::Yaml(error.to_string()))?;
    let configs: Vec<RuleConfig<ExtractLang>> =
        from_yaml_string(&config_yaml, &GlobalRules::default())
            .map_err(|error| AstRuleError::InvalidRule(error.to_string()))?;
    let config = configs
        .into_iter()
        .next()
        .ok_or_else(|| AstRuleError::InvalidRule("empty rule config".into()))?;
    let root = AstGrep::<StrDoc<ExtractLang>>::new(source, language);
    let fixer = config
        .get_fixer()
        .map_err(|error| AstRuleError::InvalidRule(error.to_string()))?
        .into_iter()
        .next();
    let mut matches = root
        .root()
        .find_all(&config.matcher)
        .map(|matched| {
            make_match(
                path,
                content.clone(),
                request,
                fixer.as_ref(),
                &config.matcher,
                matched,
            )
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        (&left.query, &left.path, left.span, &left.captures).cmp(&(
            &right.query,
            &right.path,
            right.span,
            &right.captures,
        ))
    });
    matches.dedup();
    Ok(matches)
}

fn make_match(
    path: &str,
    content: ContentId,
    request: &AstRuleRequest,
    fixer: Option<&ast_grep_config::Fixer>,
    matcher: &ast_grep_config::RuleCore,
    matched: NodeMatch<StrDoc<ExtractLang>>,
) -> AstRuleMatch {
    let range = matched.range();
    let span = Span {
        start: range.start as u32,
        len: (range.end - range.start) as u32,
    };
    let mut captures = matched
        .get_env()
        .get_matched_variables()
        .filter_map(|variable| {
            let name = match variable {
                MetaVariable::Capture(name, _) | MetaVariable::MultiCapture(name) => name,
                _ => return None,
            };
            matched.get_env().get_match(&name).map(|node| {
                let range = node.range();
                AstRuleCapture {
                    name,
                    text: node.text().into(),
                    span: Span {
                        start: range.start as u32,
                        len: (range.end - range.start) as u32,
                    },
                }
            })
        })
        .collect::<Vec<_>>();
    captures.sort();
    AstRuleMatch {
        record: "ast_rule",
        query: request.id.clone(),
        path: path.into(),
        content: content.clone(),
        span,
        captures,
        proposal: fixer.map(|fixer| {
            let edit = matched.make_edit(matcher, fixer);
            AstRuleMutationProposal {
                query: request.id.clone(),
                content,
                span: Span {
                    start: edit.position as u32,
                    len: edit.deleted_length as u32,
                },
                replacement: String::from_utf8(edit.inserted_text)
                    .expect("ast-grep UTF-8 fixer replacement"),
            }
        }),
    }
}

#[derive(Serialize)]
struct ConfigWire {
    id: String,
    language: ExtractLang,
    rule: RuleWire,
    utils: BTreeMap<String, RuleWire>,
    fix: Option<String>,
}
#[derive(Serialize)]
#[serde(untagged)]
enum RuleWire {
    Map(BTreeMap<String, serde_yaml::Value>),
}

impl ConfigWire {
    fn from_request(request: &AstRuleRequest, language: ExtractLang) -> Self {
        Self {
            id: request.id.clone(),
            language,
            rule: rule_wire(&request.rule),
            utils: request
                .utils
                .iter()
                .map(|rule| (rule.id.clone(), rule_wire(&rule.rule)))
                .collect(),
            fix: request.fix.clone(),
        }
    }
}

fn rule_wire(rule: &AstRule) -> RuleWire {
    use AstRule::*;
    let mut map = BTreeMap::new();
    match rule {
        Pattern(value) => put(&mut map, "pattern", value),
        Kind(value) => put(&mut map, "kind", value),
        Regex(value) => put(&mut map, "regex", value),
        Matches(value) => put(&mut map, "matches", value),
        All(rules) => put(
            &mut map,
            "all",
            &rules.iter().map(rule_wire).collect::<Vec<_>>(),
        ),
        Any(rules) => put(
            &mut map,
            "any",
            &rules.iter().map(rule_wire).collect::<Vec<_>>(),
        ),
        Not(rule) => put(&mut map, "not", &rule_wire(rule)),
        Inside { rule, stop_by } => relation(&mut map, "inside", rule, stop_by),
        Has { rule, stop_by } => relation(&mut map, "has", rule, stop_by),
        Follows { rule, stop_by } => relation(&mut map, "follows", rule, stop_by),
        Precedes { rule, stop_by } => relation(&mut map, "precedes", rule, stop_by),
    }
    RuleWire::Map(map)
}

fn put<T: Serialize>(map: &mut BTreeMap<String, serde_yaml::Value>, key: &str, value: &T) {
    map.insert(
        key.into(),
        serde_yaml::to_value(value).expect("typed rule serialization"),
    );
}
fn relation(
    map: &mut BTreeMap<String, serde_yaml::Value>,
    key: &str,
    rule: &AstRule,
    stop_by: &Option<StopBy>,
) {
    let mut inner = match rule_wire(rule) {
        RuleWire::Map(map) => map,
    };
    if let Some(stop_by) = stop_by {
        put(&mut inner, "stopBy", stop_by);
    }
    put(map, key, &RuleWire::Map(inner));
}
