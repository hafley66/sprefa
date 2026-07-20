use std::collections::HashMap;

use crate::Symbol;

#[derive(Debug, Default)]
pub struct SymbolTable {
    names: Vec<String>,
    ids: HashMap<String, Symbol>,
}

impl SymbolTable {
    pub fn intern(&mut self, text: &str) -> Symbol {
        if let Some(symbol) = self.ids.get(text) {
            return *symbol;
        }
        let symbol = Symbol(self.names.len() as u32);
        self.names.push(text.to_owned());
        self.ids.insert(text.to_owned(), symbol);
        symbol
    }

    pub fn resolve(&self, symbol: Symbol) -> &str {
        &self.names[symbol.0 as usize]
    }

    pub fn get(&self, text: &str) -> Option<Symbol> {
        self.ids.get(text).copied()
    }
}
