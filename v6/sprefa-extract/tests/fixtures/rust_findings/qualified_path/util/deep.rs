// qualified_path/util/deep.rs: the relative qualifiers, resolved against THIS
// file's own module path (`.../qualified_path/util/deep`).
//
// EXPECTED, `extract --resolve --family call` over the qualified_path set:
//   super::helper() -> util/mod.rs::helper   (one segment popped)
//   self::local()   -> util/deep.rs::local   (this file's own module)
// OBSERVED at cec3d5c1d: `helper` is ambiguous corpus-wide (three defs), so
// both legs mint nothing at all.

pub fn helper() -> u32 {
    6
}

fn local() -> u32 {
    7
}

pub fn relative() -> u32 {
    super::helper() + self::local()
}
