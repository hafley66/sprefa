// Every shape the rust syntax tier claims, in one file: the three variant
// forms, a generic product, a const, a static, an inherent impl, a trait.

use std::fmt;

pub struct Error;

pub enum Step {
    Idle,
    Retry(u32, bool),
    Failed { reason: String, code: u32 },
}

pub struct Trail<T> {
    pub steps: Vec<Option<T>>,
    pub outcome: Result<u64, Error>,
    pub label: (String, u32),
    pub tag: Box<str>,
    pub rendered: std::fmt::Result,
}

pub const RETRY_LIMIT: u32 = 3;

pub static BANNER: &str = "trail";

impl<T> Trail<T> {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn clear(&mut self) -> () {}
}

pub trait Render {
    fn render(&self, into: &mut fmt::Formatter) -> bool;
}

impl<T> Render for Trail<T> {
    fn render(&self, into: &mut fmt::Formatter) -> bool {
        true
    }
}
