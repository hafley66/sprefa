// Five impls of one trait plus an inherent method of the same name. rustc:
// FIRES on the trait, the structs and the inherent method. rail: MUST FIRE on
// the file, since no call site anywhere names `render`.
pub trait Render { fn render(&self) -> usize; }
pub struct Alpha;
pub struct Beta;
pub struct Gamma;
pub struct Delta;
pub struct Epsilon;
impl Render for Alpha { fn render(&self) -> usize { 5 } }
impl Render for Beta { fn render(&self) -> usize { 6 } }
impl Render for Gamma { fn render(&self) -> usize { 7 } }
impl Render for Delta { fn render(&self) -> usize { 8 } }
impl Render for Epsilon { fn render(&self) -> usize { 9 } }
impl Epsilon { fn render_inherent(&self) -> usize { 10 } }
