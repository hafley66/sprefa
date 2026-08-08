//! The registry is the ONE place a harness is named. Adding a harness means
//! one line here and one file, nothing else; the CLI routes to the trait and
//! contains no `match` on harness id.

use crate::harness::Harness;
use crate::harness::claude::Claude;

pub struct Registry {
    harnesses: Vec<Box<dyn Harness>>,
}

impl Registry {
    /// Every built-in harness, in id order.
    pub fn discover() -> Self {
        let mut harnesses: Vec<Box<dyn Harness>> = vec![Box::new(Claude)];
        harnesses.sort_by_key(|harness| harness.id());
        Registry { harnesses }
    }

    pub fn all(&self) -> &[Box<dyn Harness>] {
        &self.harnesses
    }

    pub fn by_id(&self, id: &str) -> Option<&dyn Harness> {
        self.harnesses.iter().find(|harness| harness.id() == id).map(|boxed| boxed.as_ref())
    }
}
