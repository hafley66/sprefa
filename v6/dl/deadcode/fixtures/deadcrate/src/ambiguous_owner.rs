// rustc: silent. rail: MUST STAY SILENT. lib.rs calls `.refresh()` on a Widget
// through a receiver, and a receiver call carries no callee_path, so the name
// is all a call site offers. `refresh` is also defined in ambiguous_other.rs,
// so a rule that demands a unique name finds no live call and reports this file
// dead while 19 real call sites exist. That was the _3a_files.rs defect.
pub struct Widget;

impl Widget {
    pub fn refresh(&self) -> usize { self.reload() }
    fn reload(&self) -> usize { self.repaint() }
    fn repaint(&self) -> usize { self.settle() }
    fn settle(&self) -> usize { self.finish() }
    fn finish(&self) -> usize { 16 }
}
