// rustc: silent. rail: MUST STAY SILENT. lib.rs calls `paint` through a `dyn
// Paint`, which names no impl, so one live call has to reach every impl. A
// rail that flags this file has a trait rule that is too strict.
pub trait Paint { fn paint(&self) -> usize; }
pub struct Red;
pub struct Green;
pub struct Blue;
pub struct Cyan;
pub struct Amber;
impl Paint for Red { fn paint(&self) -> usize { 11 } }
impl Paint for Green { fn paint(&self) -> usize { 12 } }
impl Paint for Blue { fn paint(&self) -> usize { 13 } }
impl Paint for Cyan { fn paint(&self) -> usize { 14 } }
impl Paint for Amber { fn paint(&self) -> usize { 15 } }
