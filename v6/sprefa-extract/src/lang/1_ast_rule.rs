//! Typed ast-grep rule requests. `cst {}` remains the Tree-sitter-query surface;
//! this module owns ast-grep's recursive rule-config surface.

use ast_grep_config::{from_yaml_string, GlobalRules, RuleConfig};
use ast_grep_core::{AstGrep, Language, NodeMatch};
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::SupportLang;
use serde::{Deserialize, Serialize};

/// Recursive, programmatic ast-grep rule algebra. Serialization is restricted
/// to the YAML/JSON boundary and is compiled by ast-grep-config.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AstRule {
    Pattern(String), Kind(String), Regex(String), Matches(String),
    All(Vec<AstRule>), Any(Vec<AstRule>), Not(Box<AstRule>),
    Inside { rule: Box<AstRule>, #[serde(skip_serializing_if = "Option::is_none")] stop_by: Option<StopBy> },
    Has { rule: Box<AstRule>, #[serde(skip_serializing_if = "Option::is_none")] stop_by: Option<StopBy> },
    Follows { rule: Box<AstRule>, #[serde(skip_serializing_if = "Option::is_none")] stop_by: Option<StopBy> },
    Precedes { rule: Box<AstRule>, #[serde(skip_serializing_if = "Option::is_none")] stop_by: Option<StopBy> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StopBy { End(String), Rule(Box<AstRule>) }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AstRuleRequest { pub id: String, pub rule: AstRule, #[serde(default)] pub utils: Vec<NamedAstRule>, pub fix: Option<String> }
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NamedAstRule { pub id: String, pub rule: AstRule }
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMatch { pub record: &'static str, pub query: String, pub path: String, pub start: u32, pub end: u32, pub captures: Vec<AstRuleCapture>, pub mutation: Option<AstRuleMutation> }
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleCapture { pub name: String, pub text: String, pub start: u32, pub end: u32 }
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstRuleMutation { pub start: u32, pub end: u32, pub replacement: String }
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AstRuleError { NoGrammar(String), Utf8(String), Yaml(String), InvalidRule(String) }
impl std::fmt::Display for AstRuleError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") } }
impl std::error::Error for AstRuleError {}

pub fn decode_ast_rule_yaml(yaml: &str) -> Result<AstRuleRequest, AstRuleError> { serde_yaml::from_str(yaml).map_err(|e| AstRuleError::Yaml(e.to_string())) }

pub fn query_ast_rule(path: &str, content: &[u8], request: &AstRuleRequest) -> Result<Vec<AstRuleMatch>, AstRuleError> {
    let lang = SupportLang::from_path(path).ok_or_else(|| AstRuleError::NoGrammar(path.into()))?;
    let source = std::str::from_utf8(content).map_err(|e| AstRuleError::Utf8(e.to_string()))?;
    let config_yaml = serde_yaml::to_string(&ConfigWire::from_request(request, lang)).map_err(|e| AstRuleError::Yaml(e.to_string()))?;
    let configs: Vec<RuleConfig<SupportLang>> = from_yaml_string(&config_yaml, &GlobalRules::default()).map_err(|e| AstRuleError::InvalidRule(e.to_string()))?;
    let config = configs.into_iter().next().ok_or_else(|| AstRuleError::InvalidRule("empty rule config".into()))?;
    let root = AstGrep::<StrDoc<SupportLang>>::new(source, lang);
    let mut out = root.root().find_all(&config.matcher).map(|matched| make_match(path, request, matched)).collect::<Vec<_>>();
    out.sort_by(|a,b| (&a.query, &a.path, a.start, a.end).cmp(&(&b.query, &b.path, b.start, b.end)));
    out.dedup(); Ok(out)
}
fn make_match(path: &str, request: &AstRuleRequest, matched: NodeMatch<StrDoc<SupportLang>>) -> AstRuleMatch {
    let range = matched.range(); let mut captures = matched.get_env().get_matched_variables().filter_map(|name| { let name=match name { MetaVariable::Capture(name,_)|MetaVariable::MultiCapture(name)=>name, _=>return None }; matched.get_env().get_match(&name).map(|node| { let r=node.range(); AstRuleCapture { name, text: node.text().into(), start:r.start as u32, end:r.end as u32 } }) }).collect::<Vec<_>>();
    captures.sort_by(|a,b| (&a.name,a.start,a.end,&a.text).cmp(&(&b.name,b.start,b.end,&b.text)));
    AstRuleMatch { record:"ast_rule", query:request.id.clone(), path:path.into(), start:range.start as u32, end:range.end as u32, captures, mutation:request.fix.as_ref().map(|replacement| AstRuleMutation { start:range.start as u32,end:range.end as u32,replacement:replacement.clone() }) }
}
#[derive(Serialize)] struct ConfigWire { id:String, language:SupportLang, rule: RuleWire, utils: std::collections::BTreeMap<String, RuleWire>, fix: Option<String> }
#[derive(Serialize)] #[serde(untagged)] enum RuleWire { Map(std::collections::BTreeMap<String, serde_yaml::Value>) }
impl ConfigWire { fn from_request(r:&AstRuleRequest, language:SupportLang)->Self { Self{id:r.id.clone(),language,rule:rule_wire(&r.rule),utils:r.utils.iter().map(|u|(u.id.clone(),rule_wire(&u.rule))).collect(),fix:r.fix.clone()} } }
fn rule_wire(rule:&AstRule)->RuleWire { use AstRule::*; let mut m=std::collections::BTreeMap::new(); match rule { Pattern(v)=>put(&mut m,"pattern",v), Kind(v)=>put(&mut m,"kind",v), Regex(v)=>put(&mut m,"regex",v), Matches(v)=>put(&mut m,"matches",v), All(v)=>put(&mut m,"all",&v.iter().map(rule_wire).collect::<Vec<_>>()), Any(v)=>put(&mut m,"any",&v.iter().map(rule_wire).collect::<Vec<_>>()), Not(v)=>put(&mut m,"not",&rule_wire(v)), Inside{rule,stop_by}=>rel(&mut m,"inside",rule,stop_by), Has{rule,stop_by}=>rel(&mut m,"has",rule,stop_by), Follows{rule,stop_by}=>rel(&mut m,"follows",rule,stop_by), Precedes{rule,stop_by}=>rel(&mut m,"precedes",rule,stop_by) }; RuleWire::Map(m) }
fn put<T: Serialize>(m:&mut std::collections::BTreeMap<String,serde_yaml::Value>, key:&str, value:&T) { m.insert(key.into(),serde_yaml::to_value(value).expect("typed rule serialization")); }
fn rel(m:&mut std::collections::BTreeMap<String,serde_yaml::Value>, key:&str, rule:&AstRule, stop_by:&Option<StopBy>) { let mut inner=match rule_wire(rule) { RuleWire::Map(v)=>v }; if let Some(stop_by)=stop_by { put(&mut inner,"stopBy",stop_by); } put(m,key,&RuleWire::Map(inner)); }
