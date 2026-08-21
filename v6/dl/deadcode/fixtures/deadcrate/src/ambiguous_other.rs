// The second definition of `refresh`, which is what makes the name ambiguous.
// rustc: silent, lib.rs calls Gauge::refresh too. rail: silent.
pub struct Gauge;

impl Gauge {
    pub fn refresh(&self) -> usize { self.sample() }
    fn sample(&self) -> usize { self.smooth() }
    fn smooth(&self) -> usize { self.clamp_value() }
    fn clamp_value(&self) -> usize { self.emit() }
    fn emit(&self) -> usize { 17 }
}
