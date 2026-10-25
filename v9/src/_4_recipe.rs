//! Which impl runs each end. Pure parse; `_1_effects::v9_system` reads the env var. Same text,
//! same canonical form, same system.

use crate::_2a_diag::Diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineKind {
    #[default]
    Dd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoreKind {
    #[default]
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WireKind {
    #[default]
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Recipe {
    pub engine: EngineKind,
    pub store: StoreKind,
    pub wire: WireKind,
}

const KEYS: &[&str] = &["engine", "store", "wire"];
const ENGINES: &[&str] = &["dd"];
const STORES: &[&str] = &["sqlite"];
const WIRES: &[&str] = &["http"];

impl Recipe {
    /// `key=value` parts split on ',' or ';', trimmed, lowercased; empty parts skipped; a missing
    /// key takes the default.
    pub fn parse(text: &str) -> Result<Recipe, Diag> {
        let mut recipe = Recipe::default();
        let mut seen = Vec::new();
        for part in text
            .split([',', ';'])
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let part = part.to_lowercase();
            let Some((key, value)) = part.split_once('=') else {
                return Err(Diag::Recipe {
                    problem: "no '='",
                    found: part,
                    allowed: KEYS,
                });
            };
            let (key, value) = (key.trim(), value.trim());
            if seen.contains(&key.to_string()) {
                return Err(Diag::Recipe {
                    problem: "duplicate key",
                    found: key.to_string(),
                    allowed: KEYS,
                });
            }
            seen.push(key.to_string());
            let unknown = |allowed| Diag::Recipe {
                problem: "unknown value",
                found: format!("{key}={value}"),
                allowed,
            };
            match key {
                "engine" => {
                    recipe.engine = match value {
                        "dd" => EngineKind::Dd,
                        _ => return Err(unknown(ENGINES)),
                    }
                }
                "store" => {
                    recipe.store = match value {
                        "sqlite" => StoreKind::Sqlite,
                        _ => return Err(unknown(STORES)),
                    }
                }
                "wire" => {
                    recipe.wire = match value {
                        "http" => WireKind::Http,
                        _ => return Err(unknown(WIRES)),
                    }
                }
                _ => {
                    return Err(Diag::Recipe {
                        problem: "unknown key",
                        found: key.to_string(),
                        allowed: KEYS,
                    })
                }
            }
        }
        Ok(recipe)
    }

    /// Every key, sorted, with its value: `engine=dd,store=sqlite,wire=http`.
    pub fn canonical(&self) -> String {
        let engine = match self.engine {
            EngineKind::Dd => "dd",
        };
        let store = match self.store {
            StoreKind::Sqlite => "sqlite",
        };
        let wire = match self.wire {
            WireKind::Http => "http",
        };
        format!("engine={engine},store={store},wire={wire}")
    }
}
