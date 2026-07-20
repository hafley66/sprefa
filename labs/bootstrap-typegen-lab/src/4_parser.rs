use crate::syntax::*;
use crate::{SlotSpelling, Source, Span};

#[derive(Debug)]
pub struct ParseOutput {
    pub module: SyntaxModule,
    pub diagnostics: Vec<String>,
}

pub fn parse(source: &Source) -> ParseOutput {
    let mut parser = Parser {
        source,
        cursor: 0,
        diagnostics: Vec::new(),
    };
    let mut declarations = Vec::new();
    while parser.skip_space_and_comments() {
        let start = parser.cursor;
        let result = if parser.consume_keyword("type") {
            parser.type_decl(start).map(SyntaxDecl::Type)
        } else if parser.consume_keyword("pattern") {
            parser.pattern_decl(start).map(SyntaxDecl::Pattern)
        } else if parser.consume_keyword("consumer") {
            parser.consumer_decl(start).map(SyntaxDecl::Consumer)
        } else {
            parser.error("expected type, pattern, or consumer declaration");
            parser.recover();
            continue;
        };
        if let Some(declaration) = result {
            declarations.push(declaration);
        } else {
            parser.recover();
        }
    }
    ParseOutput {
        module: SyntaxModule { declarations },
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    source: &'a Source,
    cursor: usize,
    diagnostics: Vec<String>,
}

impl<'a> Parser<'a> {
    fn text(&self) -> &str {
        &self.source.text
    }
    fn span(&self, start: usize) -> Span {
        self.source.span(start, self.cursor)
    }

    fn skip_space_and_comments(&mut self) -> bool {
        loop {
            while self
                .text()
                .as_bytes()
                .get(self.cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.cursor += 1;
            }
            if self.text()[self.cursor..].starts_with("//") {
                while self.cursor < self.text().len()
                    && self.text().as_bytes()[self.cursor] != b'\n'
                {
                    self.cursor += 1;
                }
                continue;
            }
            return self.cursor < self.text().len();
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let end = self.cursor + keyword.len();
        if self.text().get(self.cursor..end) == Some(keyword)
            && !self
                .text()
                .as_bytes()
                .get(end)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.cursor = end;
            true
        } else {
            false
        }
    }

    fn identifier(&mut self) -> Option<String> {
        self.skip_inline_space();
        let start = self.cursor;
        while self
            .text()
            .as_bytes()
            .get(self.cursor)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            self.cursor += 1;
        }
        (start != self.cursor).then(|| self.text()[start..self.cursor].to_owned())
    }

    fn skip_inline_space(&mut self) {
        while self
            .text()
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }
    fn consume(&mut self, token: &str) -> bool {
        self.skip_inline_space();
        if self.text()[self.cursor..].starts_with(token) {
            self.cursor += token.len();
            true
        } else {
            false
        }
    }
    fn require(&mut self, token: &str) -> bool {
        if self.consume(token) {
            true
        } else {
            self.error(&format!("expected '{token}'"));
            false
        }
    }
    fn error(&mut self, message: &str) {
        self.diagnostics
            .push(format!("{}:{}: {}", self.source.id.0, self.cursor, message));
    }
    fn recover(&mut self) {
        while self.cursor < self.text().len() && self.text().as_bytes()[self.cursor] != b'\n' {
            self.cursor += 1;
        }
    }

    fn type_decl(&mut self, start: usize) -> Option<SyntaxTypeDecl> {
        let name = self.identifier()?;
        self.consume("=");
        let expr = self.type_expr()?;
        self.consume(";");
        Some(SyntaxTypeDecl {
            name,
            expr,
            span: self.span(start),
        })
    }

    fn type_expr(&mut self) -> Option<SyntaxTypeExpr> {
        let start = self.cursor;
        let mut items = vec![self.type_atom()?];
        while self.consume("|") {
            items.push(self.type_atom()?);
        }
        if items.len() == 1 {
            Some(items.remove(0))
        } else {
            Some(SyntaxTypeExpr::Union(items, self.span(start)))
        }
    }

    fn type_atom(&mut self) -> Option<SyntaxTypeExpr> {
        self.skip_inline_space();
        let start = self.cursor;
        if self.consume("{") {
            let mut fields = Vec::new();
            while !self.consume("}") {
                let field_start = self.cursor;
                let name = self.identifier()?;
                if !self.require(":") {
                    return None;
                }
                let ty = self.type_expr()?;
                fields.push(SyntaxField {
                    name,
                    ty,
                    span: self.span(field_start),
                });
                self.consume(",");
            }
            return Some(SyntaxTypeExpr::Record(fields, self.span(start)));
        }
        if self.text().as_bytes().get(self.cursor) == Some(&b'"') {
            self.cursor += 1;
            let value_start = self.cursor;
            while self.cursor < self.text().len() && self.text().as_bytes()[self.cursor] != b'"' {
                self.cursor += 1;
            }
            let value = self.text()[value_start..self.cursor].to_owned();
            self.require("\"");
            return Some(SyntaxTypeExpr::Literal(value, self.span(start)));
        }
        let name = self.identifier()?;
        if self.consume("<") {
            let mut args = Vec::new();
            loop {
                args.push(self.type_expr()?);
                if self.consume(">") {
                    break;
                }
                if !self.require(",") {
                    return None;
                }
            }
            Some(SyntaxTypeExpr::Apply {
                constructor: name,
                args,
                span: self.span(start),
            })
        } else {
            Some(SyntaxTypeExpr::Name(name, self.span(start)))
        }
    }

    fn pattern_decl(&mut self, start: usize) -> Option<SyntaxPatternDecl> {
        let name = self.identifier()?;
        if !self.require("=") {
            return None;
        }
        self.skip_inline_space();
        let quote = self.text().as_bytes().get(self.cursor).copied();
        if quote != Some(b'`') {
            self.error("pattern template must use backticks");
            return None;
        }
        let template_start = self.cursor;
        self.cursor += 1;
        let mut parts = Vec::new();
        let mut literal_start = self.cursor;
        while self.cursor < self.text().len() && self.text().as_bytes()[self.cursor] != b'`' {
            if self.text().as_bytes()[self.cursor] == b'{' {
                if literal_start < self.cursor {
                    parts.push(SyntaxTemplatePart::Literal {
                        text: self.text()[literal_start..self.cursor].to_owned(),
                        span: self.source.span(literal_start, self.cursor),
                    });
                }
                let slot_start = self.cursor;
                self.cursor += 1;
                let name = self.identifier()?;
                let ty = if self.consume(":") {
                    Some(self.type_expr()?)
                } else {
                    None
                };
                if !self.require("}") {
                    return None;
                }
                parts.push(SyntaxTemplatePart::Slot(SyntaxSlot {
                    spelling: SlotSpelling::Braces,
                    name,
                    ty,
                    span: self.source.span(slot_start, self.cursor),
                }));
                literal_start = self.cursor;
            } else if self.text().as_bytes()[self.cursor] == b':' {
                if literal_start < self.cursor {
                    parts.push(SyntaxTemplatePart::Literal {
                        text: self.text()[literal_start..self.cursor].to_owned(),
                        span: self.source.span(literal_start, self.cursor),
                    });
                }
                let slot_start = self.cursor;
                self.cursor += 1;
                let name = self
                    .identifier()
                    .ok_or_else(|| {
                        self.error("expected slot name after ':'");
                        ()
                    })
                    .ok()?;
                parts.push(SyntaxTemplatePart::Slot(SyntaxSlot {
                    spelling: SlotSpelling::Colon,
                    name,
                    ty: None,
                    span: self.source.span(slot_start, self.cursor),
                }));
                literal_start = self.cursor;
            } else {
                self.cursor += 1;
            }
        }
        if literal_start < self.cursor {
            parts.push(SyntaxTemplatePart::Literal {
                text: self.text()[literal_start..self.cursor].to_owned(),
                span: self.source.span(literal_start, self.cursor),
            });
        }
        if !self.require("`") {
            return None;
        }
        self.consume(";");
        Some(SyntaxPatternDecl {
            name,
            template: SyntaxTemplate {
                span: self.source.span(template_start, self.cursor),
                parts,
            },
            span: self.span(start),
        })
    }

    fn consumer_decl(&mut self, start: usize) -> Option<SyntaxConsumerDecl> {
        let domain = self.identifier()?;
        self.consume("{");
        let operation = self.identifier()?;
        let pattern = self.identifier()?;
        if !self.require("->") {
            return None;
        }
        let output = self.identifier()?;
        self.consume("}");
        self.consume(";");
        Some(SyntaxConsumerDecl {
            domain,
            operation,
            pattern,
            output,
            span: self.span(start),
        })
    }
}
