use crate::{SlotSpelling, Span};

#[derive(Clone, Debug)]
pub struct SyntaxModule {
    pub declarations: Vec<SyntaxDecl>,
}

#[derive(Clone, Debug)]
pub enum SyntaxDecl {
    Type(SyntaxTypeDecl),
    Pattern(SyntaxPatternDecl),
    Consumer(SyntaxConsumerDecl),
}

#[derive(Clone, Debug)]
pub struct SyntaxTypeDecl {
    pub name: String,
    pub expr: SyntaxTypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum SyntaxTypeExpr {
    Name(String, Span),
    Literal(String, Span),
    Record(Vec<SyntaxField>, Span),
    Union(Vec<SyntaxTypeExpr>, Span),
    Apply {
        constructor: String,
        args: Vec<SyntaxTypeExpr>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct SyntaxField {
    pub name: String,
    pub ty: SyntaxTypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SyntaxPatternDecl {
    pub name: String,
    pub template: SyntaxTemplate,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SyntaxTemplate {
    pub span: Span,
    pub parts: Vec<SyntaxTemplatePart>,
}

#[derive(Clone, Debug)]
pub enum SyntaxTemplatePart {
    Literal { text: String, span: Span },
    Slot(SyntaxSlot),
}

#[derive(Clone, Debug)]
pub struct SyntaxSlot {
    pub spelling: SlotSpelling,
    pub name: String,
    pub ty: Option<SyntaxTypeExpr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct SyntaxConsumerDecl {
    pub domain: String,
    pub operation: String,
    pub pattern: String,
    pub output: String,
    pub span: Span,
}
