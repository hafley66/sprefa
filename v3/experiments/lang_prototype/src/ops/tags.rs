use super::*;
use crate::cursor::{Cursor, SprfPath, PathSeg};

pub fn run(ctx: &mut OpCtx, _modes: &[TermMode], input: Vec<Cursor>) -> Result<OpAction, Diags> {
    let mut out = Vec::new();
    let rows: Vec<_> = ctx.store.query_all().into_iter().cloned().collect();

    if input.is_empty() {
        // Source mode: emit one cursor per tag row
        for row in rows {
            let mut c = Cursor::new(
                SprfPath(vec![PathSeg::Step("tags".into())]),
                Arc::from(Vec::new()),
            );
            c.captures.insert(
                super::super::cursor::CaptureSlot(3), // kind
                super::super::lower::Scalar::String(row.kind),
            );
            c.captures.insert(
                super::super::cursor::CaptureSlot(2), // value
                row.value,
            );
            out.push(c);
        }
    } else {
        for c in input {
            for row in &rows {
                let mut next = c.clone();
                next.captures.insert(
                    super::super::cursor::CaptureSlot(3),
                    super::super::lower::Scalar::String(row.kind.clone()),
                );
                next.captures.insert(
                    super::super::cursor::CaptureSlot(2),
                    row.value.clone(),
                );
                out.push(next);
            }
        }
    }
    Ok(OpAction::Emit(out))
}
