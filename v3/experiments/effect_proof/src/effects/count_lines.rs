//! Effect kind: count newlines in a byte slice.
//!
//! Second effect. Its existence proves: adding more effects only
//! adds files; it does not edit `src/lib.rs` or any other effect file.

use effect_runtime::{Batcher, BoxFuture, EffectKind};

pub struct CountLines {
    pub content: Vec<u8>,
}

impl EffectKind for CountLines {
    type Response = usize;
}

pub struct SimpleLineCounter;

impl Batcher<CountLines> for SimpleLineCounter {
    fn run(&self, req: CountLines) -> BoxFuture<'static, usize> {
        Box::pin(async move {
            let nl = req.content.iter().filter(|&&b| b == b'\n').count();
            // count newlines + 1 for content that does not end in newline
            if req.content.is_empty() {
                0
            } else if req.content.last() == Some(&b'\n') {
                nl
            } else {
                nl + 1
            }
        })
    }
}
