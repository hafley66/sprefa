use super::*;

pub fn run(ctx: &mut OpCtx, modes: &[TermMode], input: Vec<Cursor>) -> Result<OpAction, Diags> {
    let kind = modes.get(1).and_then(term_to_string)
        .unwrap_or_else(|| "tag".into());
    let kind: Arc<str> = kind.into();

    let mut out = Vec::new();
    for c in input {
        match modes.first() {
            Some(TermMode::Bound(val)) => {
                // bound → write relation row
                ctx.store.write_tag(kind.clone(), val.clone(), c.path.clone());
                out.push(c);
            }
            Some(TermMode::Unbound) => {
                // unbound → read relation rows of this kind, emit cursors with term bound
                for row in ctx.store.query_kind(&kind) {
                    let mut next = c.clone();
                    next.captures.insert(
                        super::super::cursor::CaptureSlot(2),
                        row.value.clone(),
                    );
                    out.push(next);
                }
            }
            None => {
                ctx.diags.push(super::super::diag::Diag::new("tag: missing term arg"));
            }
        }
    }
    Ok(OpAction::Emit(out))
}
