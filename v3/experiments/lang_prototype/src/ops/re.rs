use super::*;
use crate::cursor::PathSeg;
use regex::Regex;

pub fn run(_ctx: &mut OpCtx, modes: &[TermMode], input: Vec<Cursor>) -> Result<OpAction, Diags> {
    let pat = modes.first().and_then(term_to_string).unwrap_or_default();
    let re = Regex::new(&pat)
        .map_err(|e| vec![super::super::diag::Diag::new(format!("re: bad regex: {e}"))])?;
    let capture_slot = super::super::cursor::CaptureSlot(1);
    let mut out = Vec::new();
    for c in input {
        let text = String::from_utf8_lossy(c.active());
        for (i, m) in re.captures_iter(&text).enumerate() {
            let full = m.get(0).unwrap();
            let mut next = c.narrow(full.start()..full.end());
            next.path.push(PathSeg::Step(format!("re:{i}").into()));
            if let Some(group) = m.get(1) {
                next.captures.insert(
                    capture_slot,
                    super::super::lower::Scalar::String(group.as_str().into()),
                );
            }
            out.push(next);
        }
    }
    Ok(OpAction::Emit(out))
}
