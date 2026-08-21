// The same names called from a test site AND a shipped site in ONE caller file
// (live_private.rs). rustc: silent, the shipped chain reaches every step. rail:
// MUST STAY SILENT. The filter is per SITE: dropping a NAME because some test
// site mentions it reports this file dead while a shipped call names it, and a
// dead-code rail may under-report and must never over-report.
pub(crate) fn mixed_one() -> usize { mixed_two() }
pub(crate) fn mixed_two() -> usize { mixed_three() }
fn mixed_three() -> usize { mixed_four() }
fn mixed_four() -> usize { mixed_five() }
fn mixed_five() -> usize { 6 }
