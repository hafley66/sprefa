use std::collections::HashMap;

use crate::patterns::*;
use crate::syntax::*;
use crate::types::*;
use crate::{ParseOutput, Source, Symbol, SymbolTable, TypeId};

#[derive(Clone, Debug)]
pub enum Declaration {
    Type(TypeId),
    Pattern(crate::PatternId),
}

#[derive(Debug)]
pub struct Store {
    pub symbols: SymbolTable,
    pub source: Source,
    pub types: Vec<Type>,
    pub patterns: Vec<Pattern>,
    pub declarations: HashMap<Symbol, Declaration>,
    pub consumers: Vec<(String, String, crate::PatternId, TypeId)>,
    pub diagnostics: Vec<String>,
}

impl Store {
    pub fn new(source: Source) -> Self {
        Self {
            symbols: SymbolTable::default(),
            source,
            types: Vec::new(),
            patterns: Vec::new(),
            declarations: HashMap::new(),
            consumers: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
    pub fn alloc_type(&mut self, ty: Type) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty);
        id
    }
    pub fn alloc_pattern(&mut self, pattern: Pattern) -> crate::PatternId {
        let id = crate::PatternId(self.patterns.len() as u32);
        self.patterns.push(pattern);
        id
    }
    pub fn type_name(&self, id: TypeId) -> String {
        match &self.types[id.0 as usize] {
            Type::Record(record) => self.symbols.resolve(record.name).to_owned(),
            Type::Alias { name, .. } => self.symbols.resolve(*name).to_owned(),
            Type::Primitive(p) => format!("{p:?}"),
            Type::Literal(value) => format!("{value:?}"),
            other => other.kind_name().to_owned(),
        }
    }
    pub fn lower(mut self, parsed: ParseOutput) -> Result<Self, Vec<String>> {
        self.diagnostics.extend(parsed.diagnostics);
        let mut pending = Vec::new();
        for declaration in &parsed.module.declarations {
            match declaration {
                SyntaxDecl::Type(decl) => {
                    let symbol = self.symbols.intern(&decl.name);
                    if self.declarations.contains_key(&symbol) {
                        self.diagnostics
                            .push(format!("duplicate declaration {}", decl.name));
                    } else {
                        let id = self.alloc_type(Type::Error);
                        self.declarations.insert(symbol, Declaration::Type(id));
                        pending.push((id, decl.clone()));
                    }
                }
                SyntaxDecl::Pattern(decl) => {
                    let symbol = self.symbols.intern(&decl.name);
                    if self.declarations.contains_key(&symbol) {
                        self.diagnostics
                            .push(format!("duplicate declaration {}", decl.name));
                    } else {
                        let id = self.alloc_pattern(Pattern {
                            name: Some(symbol),
                            parts: Vec::new(),
                            span: decl.span,
                        });
                        self.declarations.insert(symbol, Declaration::Pattern(id));
                    }
                }
                SyntaxDecl::Consumer(_) => {}
            }
        }
        for (id, decl) in pending {
            let mut lowered = self.lower_type_expr(&decl.expr)?;
            if let Type::Record(record) = &mut lowered {
                record.name = self.symbols.intern(&decl.name);
            }
            self.types[id.0 as usize] = lowered;
        }
        for declaration in parsed.module.declarations {
            match declaration {
                SyntaxDecl::Pattern(decl) => self.lower_pattern(&decl)?,
                SyntaxDecl::Consumer(decl) => self.lower_consumer(&decl)?,
                SyntaxDecl::Type(_) => {}
            }
        }
        if self.diagnostics.is_empty() {
            Ok(self)
        } else {
            Err(self.diagnostics)
        }
    }

    fn lower_type_expr(&mut self, expr: &SyntaxTypeExpr) -> Result<Type, Vec<String>> {
        match expr {
            SyntaxTypeExpr::Name(name, _) => {
                let symbol = self.symbols.intern(name);
                match self.declarations.get(&symbol) {
                    Some(Declaration::Type(id)) => Ok(Type::Alias {
                        name: symbol,
                        target: *id,
                    }),
                    _ => match name.as_str() {
                        "String" => Ok(Type::Primitive(Primitive::String)),
                        "Int" => Ok(Type::Primitive(Primitive::Int)),
                        "Bool" => Ok(Type::Primitive(Primitive::Bool)),
                        _ => Err(vec![format!("unknown type {name}")]),
                    },
                }
            }
            SyntaxTypeExpr::Literal(value, _) => Ok(Type::Literal(Value::String(value.clone()))),
            SyntaxTypeExpr::Record(fields, _) => {
                let mut lowered = Vec::new();
                for field in fields {
                    let field_type = self.lower_type_expr(&field.ty)?;
                    let name = self.symbols.intern(&field.name);
                    let ty = self.alloc_type(field_type);
                    lowered.push(Field {
                        name,
                        ty,
                        span: field.span,
                    });
                }
                Ok(Type::Record(RecordType {
                    name: self.symbols.intern("<anonymous>"),
                    fields: lowered,
                }))
            }
            SyntaxTypeExpr::Union(items, _) => {
                let ids = items
                    .iter()
                    .map(|item| self.lower_type_expr(item).map(|ty| self.alloc_type(ty)))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Union(ids))
            }
            SyntaxTypeExpr::Apply {
                constructor, args, ..
            } => {
                let ids = args
                    .iter()
                    .map(|arg| self.lower_type_expr(arg).map(|ty| self.alloc_type(ty)))
                    .collect::<Result<Vec<_>, _>>()?;
                match (constructor.as_str(), ids.as_slice()) {
                    ("Array", [item]) => Ok(Type::Array(*item)),
                    ("Map", [key, value]) => Ok(Type::Map {
                        key: *key,
                        value: *value,
                    }),
                    ("Optional", [item]) => Ok(Type::Optional(*item)),
                    _ => Err(vec![format!("unknown type constructor {constructor}")]),
                }
            }
        }
    }

    fn lower_pattern(&mut self, declaration: &SyntaxPatternDecl) -> Result<(), Vec<String>> {
        let symbol = self.symbols.intern(&declaration.name);
        let id = match self.declarations.get(&symbol) {
            Some(Declaration::Pattern(id)) => *id,
            _ => return Err(vec![format!("unknown pattern {}", declaration.name)]),
        };
        let mut position = 0;
        let mut parts = Vec::new();
        for part in &declaration.template.parts {
            match part {
                SyntaxTemplatePart::Literal { text, span } => parts.push(PatternPart::Literal {
                    text: text.clone(),
                    span: *span,
                }),
                SyntaxTemplatePart::Slot(slot) => {
                    let ty = slot
                        .ty
                        .as_ref()
                        .map(|expr| self.lower_type_expr(expr).map(|ty| self.alloc_type(ty)))
                        .transpose()?
                        .unwrap_or_else(|| self.alloc_type(Type::Primitive(Primitive::String)));
                    let name = self.symbols.intern(&slot.name);
                    if parts.iter().any(
                        |part| matches!(part, PatternPart::Slot(old) if old.name == Some(name)),
                    ) {
                        return Err(vec![format!("duplicate binding {}", slot.name)]);
                    }
                    parts.push(PatternPart::Slot(Slot {
                        name: Some(name),
                        position,
                        ty,
                        spelling: slot.spelling,
                        source: slot.name.clone(),
                        span: slot.span,
                    }));
                    position += 1;
                }
            }
        }
        self.patterns[id.0 as usize].parts = parts;
        Ok(())
    }

    fn lower_consumer(&mut self, declaration: &SyntaxConsumerDecl) -> Result<(), Vec<String>> {
        let pattern = self.symbols.intern(&declaration.pattern);
        let output = self.symbols.intern(&declaration.output);
        let pattern = match self.declarations.get(&pattern) {
            Some(Declaration::Pattern(id)) => *id,
            _ => {
                return Err(vec![format!(
                    "unknown consumer pattern {}",
                    declaration.pattern
                )])
            }
        };
        let output = match self.declarations.get(&output) {
            Some(Declaration::Type(id)) => *id,
            _ => {
                return Err(vec![format!(
                    "unknown consumer output {}",
                    declaration.output
                )])
            }
        };
        self.consumers.push((
            declaration.domain.clone(),
            declaration.operation.clone(),
            pattern,
            output,
        ));
        Ok(())
    }

    pub fn lookup_type(&self, name: &str) -> Option<TypeId> {
        match self.declarations.get(&self.symbols.get(name)?) {
            Some(Declaration::Type(id)) => Some(*id),
            _ => None,
        }
    }
    pub fn dump(&self) -> String {
        let mut lines = Vec::new();
        for (symbol, declaration) in &self.declarations {
            match declaration {
                Declaration::Type(id) => lines.push(format!(
                    "type {} = {}",
                    self.symbols.resolve(*symbol),
                    self.type_name(*id)
                )),
                Declaration::Pattern(id) => lines.push(format!(
                    "pattern {} = {}",
                    self.symbols.resolve(*symbol),
                    self.pattern_text(*id)
                )),
            }
        }
        lines.sort();
        lines.join("\n")
    }
    pub fn pattern_text(&self, id: crate::PatternId) -> String {
        self.patterns[id.0 as usize]
            .parts
            .iter()
            .map(|part| match part {
                PatternPart::Literal { text, .. } => text.clone(),
                PatternPart::Slot(slot) => match slot.spelling {
                    SlotSpelling::Braces => format!(
                        "{{{}: {}}}",
                        self.symbols.resolve(slot.name.unwrap()),
                        self.type_name(slot.ty)
                    ),
                    SlotSpelling::Colon => format!(":{}", self.symbols.resolve(slot.name.unwrap())),
                },
            })
            .collect()
    }
}
