//! `read_dl7/5` from v7 `src/0_reader/0_parser.pl`, control flow for control
//! flow. The first error wins: forms and source rows are dropped.

use super::tokens::{decoded_escape, integer_token, term_delimiter, valid_atom, valid_identifier};
use crate::_6_eval::{TermId, Universe};

/// Offsets, lines and columns count CHARACTERS: v7 reads through
/// `string_codes/2`. Lines and columns are 1-based, offsets 0-based.
#[derive(Copy, Clone, Debug)]
pub struct Pos {
    pub offset: usize,
    pub line: usize,
    pub col: usize,
}

pub struct Read {
    pub forms: Vec<TermId>,
    pub source_rows: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

pub struct Reader<'a> {
    pub u: &'a mut Universe,
    pub path: TermId,
    pub chars: Vec<char>,
    pub at: usize,
    pub pos: Pos,
    pub index: u32,
    pub rows: Vec<TermId>,
    pub vars: Vec<(String, TermId)>,
}

pub fn read(u: &mut Universe, path: &str, text: &str) -> Read {
    let path_atom = u.atom(path);
    let mut reader = Reader {
        u,
        path: path_atom,
        chars: text.chars().collect(),
        at: 0,
        pos: Pos {
            offset: 0,
            line: 1,
            col: 1,
        },
        index: 0,
        rows: Vec::new(),
        vars: Vec::new(),
    };
    match reader.top_forms() {
        Ok(forms) => Read {
            forms,
            source_rows: reader.rows,
            diagnostics: Vec::new(),
        },
        Err(diagnostic) => Read {
            forms: Vec::new(),
            source_rows: Vec::new(),
            diagnostics: vec![diagnostic],
        },
    }
}

impl Reader<'_> {
    pub fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    pub fn bump(&mut self) -> char {
        let c = self.chars[self.at];
        self.at += 1;
        self.pos.offset += 1;
        if c == '\n' {
            self.pos.line += 1;
            self.pos.col = 1;
        } else {
            self.pos.col += 1;
        }
        c
    }

    pub fn node_id(&mut self, index: u32) -> TermId {
        let path = self.path;
        let n = self.u.int(index as i64);
        self.u.compound("reader_node", vec![path, n])
    }

    pub fn node(&mut self, node_id: TermId, payload: TermId) -> TermId {
        self.u.compound("node", vec![node_id, payload])
    }

    pub fn pos_term(&mut self, at: Pos) -> TermId {
        let offset = self.u.int(at.offset as i64);
        let line = self.u.int(at.line as i64);
        let col = self.u.int(at.col as i64);
        self.u.compound("position", vec![offset, line, col])
    }

    pub fn source_row(&mut self, node_id: TermId, start: Pos, end: Pos) -> TermId {
        let path = self.path;
        let args = vec![
            node_id,
            path,
            self.u.int(start.offset as i64),
            self.u.int(end.offset as i64),
            self.u.int(start.line as i64),
            self.u.int(start.col as i64),
            self.u.int(end.line as i64),
            self.u.int(end.col as i64),
        ];
        self.u.compound("source", args)
    }

    pub fn push_row(&mut self, node_id: TermId, start: Pos, end: Pos) {
        let row = self.source_row(node_id, start, end);
        self.rows.push(row);
    }

    pub fn diagnostic(&mut self, node_id: TermId, code: TermId, at: Pos) -> TermId {
        let reader = self.u.atom("reader");
        let path = self.path;
        let position = self.pos_term(at);
        self.u
            .compound("diagnostic", vec![reader, path, node_id, code, position])
    }

    pub fn plain_error(&mut self, node_id: TermId, code: &str, at: Pos) -> TermId {
        let code = self.u.atom(code);
        self.diagnostic(node_id, code, at)
    }

    pub fn named_error(&mut self, node_id: TermId, code: &str, name: &str, at: Pos) -> TermId {
        let name = self.u.atom(name);
        let code = self.u.compound(code, vec![name]);
        self.diagnostic(node_id, code, at)
    }

    pub fn skip_layout(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some(';') => {
                    self.bump();
                    self.skip_comment();
                }
                _ => return,
            }
        }
    }

    pub fn skip_comment(&mut self) {
        while self.peek().is_some() {
            if self.bump() == '\n' {
                return;
            }
        }
    }

    pub fn take_token(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if term_delimiter(c) {
                break;
            }
            out.push(self.bump());
        }
        out
    }

    pub fn top_forms(&mut self) -> Result<Vec<TermId>, TermId> {
        let mut forms = Vec::new();
        loop {
            self.skip_layout();
            if self.peek().is_none() {
                return Ok(forms);
            }
            self.vars.clear();
            let top = self.index;
            let form = self.read_term(top)?;
            forms.push(form);
        }
    }

    pub fn read_term(&mut self, top: u32) -> Result<TermId, TermId> {
        let index = self.index;
        self.index += 1;
        let node_id = self.node_id(index);
        let start = self.pos;
        match self.peek() {
            Some('(') => self.read_form(top, node_id, start),
            Some('{') => self.read_query_term(node_id, start),
            Some(')') => Err(self.plain_error(node_id, "unexpected_closing_parenthesis", start)),
            Some('}') => Err(self.plain_error(node_id, "unexpected_closing_query_brace", start)),
            Some('"') => self.read_string_term(node_id, start),
            Some('\'') => self.read_symbol(node_id, start),
            Some('?') => self.read_variable(top, node_id, start),
            None => Err(self.plain_error(node_id, "expected_term", start)),
            Some(_) => self.read_bare(node_id, start),
        }
    }

    /// The form's own source row precedes its items', so the slot is reserved
    /// before the items are read.
    pub fn read_form(&mut self, top: u32, node_id: TermId, start: Pos) -> Result<TermId, TermId> {
        self.bump();
        let slot = self.rows.len();
        self.rows.push(node_id);
        let mut items = Vec::new();
        loop {
            self.skip_layout();
            match self.peek() {
                None => {
                    let at = self.pos;
                    return Err(self.plain_error(node_id, "unterminated_form", at));
                }
                Some(')') => {
                    self.bump();
                    break;
                }
                _ => {
                    let item = self.read_term(top)?;
                    items.push(item);
                }
            }
        }
        let end = self.pos;
        self.rows[slot] = self.source_row(node_id, start, end);
        let list = self.u.list(&items);
        let payload = self.u.compound("form", vec![list]);
        Ok(self.node(node_id, payload))
    }

    pub fn read_query_term(&mut self, node_id: TermId, start: Pos) -> Result<TermId, TermId> {
        self.bump();
        let text = match self.read_query() {
            Ok(text) => text,
            Err((code, at)) => return Err(self.plain_error(node_id, code, at)),
        };
        let end = self.pos;
        let text = self.u.string(&text);
        let query = self.u.compound("tree_sitter_query", vec![text]);
        let payload = self.u.compound("literal", vec![query]);
        self.push_row(node_id, start, end);
        Ok(self.node(node_id, payload))
    }

    /// Query text stays exact: escapes are copied, not decoded, and a `}`
    /// inside a quoted run does not close the query.
    pub fn read_query(&mut self) -> Result<String, (&'static str, Pos)> {
        let mut out = String::new();
        let mut inside_string = false;
        loop {
            match self.peek() {
                None => return Err(("unterminated_query", self.pos)),
                Some('}') if !inside_string => {
                    self.bump();
                    return Ok(out);
                }
                Some('"') => {
                    self.bump();
                    out.push('"');
                    inside_string = !inside_string;
                }
                Some('\\') if inside_string && self.at + 1 < self.chars.len() => {
                    out.push(self.bump());
                    out.push(self.bump());
                }
                Some(_) => out.push(self.bump()),
            }
        }
    }

    pub fn read_string_term(&mut self, node_id: TermId, start: Pos) -> Result<TermId, TermId> {
        self.bump();
        let text = match self.read_string() {
            Ok(text) => text,
            Err((code, at)) => return Err(self.plain_error(node_id, code, at)),
        };
        let end = self.pos;
        let text = self.u.string(&text);
        let payload = self.u.compound("literal", vec![text]);
        self.push_row(node_id, start, end);
        Ok(self.node(node_id, payload))
    }

    pub fn read_string(&mut self) -> Result<String, (&'static str, Pos)> {
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(("unterminated_string", self.pos)),
                Some('"') => {
                    self.bump();
                    return Ok(out);
                }
                Some('\\') => {
                    self.bump();
                    match self.peek() {
                        None => return Err(("unterminated_string", self.pos)),
                        Some(_) => {
                            let escape = self.bump();
                            decoded_escape(escape, &mut out);
                        }
                    }
                }
                Some(_) => out.push(self.bump()),
            }
        }
    }

    pub fn read_symbol(&mut self, node_id: TermId, start: Pos) -> Result<TermId, TermId> {
        self.bump();
        let name = self.take_token();
        if !valid_identifier(&name) {
            return Err(if name.is_empty() {
                self.plain_error(node_id, "expected_symbol_name", start)
            } else {
                self.named_error(node_id, "invalid_symbol_name", &name, start)
            });
        }
        let end = self.pos;
        let name = self.u.atom(&name);
        let symbol = self.u.compound("symbol", vec![name]);
        let payload = self.u.compound("literal", vec![symbol]);
        self.push_row(node_id, start, end);
        Ok(self.node(node_id, payload))
    }

    pub fn read_variable(
        &mut self,
        top: u32,
        node_id: TermId,
        start: Pos,
    ) -> Result<TermId, TermId> {
        self.bump();
        let name = self.take_token();
        if !valid_identifier(&name) {
            return Err(if name.is_empty() {
                self.plain_error(node_id, "expected_variable_name", start)
            } else {
                self.named_error(node_id, "invalid_variable_name", &name, start)
            });
        }
        let end = self.pos;
        let identity = self.variable_identity(top, node_id, &name);
        let name = self.u.atom(&name);
        let payload = self.u.compound("variable", vec![identity, name]);
        self.push_row(node_id, start, end);
        Ok(self.node(node_id, payload))
    }

    /// `_` is fresh per node; every other name is shared across one top form.
    pub fn variable_identity(&mut self, top: u32, node_id: TermId, name: &str) -> TermId {
        let atom = self.u.atom(name);
        if name == "_" {
            return self.u.compound("variable", vec![node_id, atom]);
        }
        if let Some((_, known)) = self.vars.iter().find(|(seen, _)| seen == name) {
            return *known;
        }
        let top_id = self.node_id(top);
        let identity = self.u.compound("variable", vec![top_id, atom]);
        self.vars.push((name.to_string(), identity));
        identity
    }

    pub fn read_bare(&mut self, node_id: TermId, start: Pos) -> Result<TermId, TermId> {
        let token = self.take_token();
        if token.is_empty() {
            return Err(self.plain_error(node_id, "expected_term", start));
        }
        let end = self.pos;
        let payload = if integer_token(&token) {
            match token.parse::<i64>() {
                Ok(n) => {
                    let n = self.u.int(n);
                    self.u.compound("literal", vec![n])
                }
                Err(_) => {
                    return Err(self.named_error(node_id, "integer_out_of_range", &token, start))
                }
            }
        } else if valid_atom(&token) {
            let name = self.u.atom(&token);
            self.u.compound("atom", vec![name])
        } else {
            return Err(self.named_error(node_id, "invalid_atom", &token, start));
        };
        self.push_row(node_id, start, end);
        Ok(self.node(node_id, payload))
    }
}
