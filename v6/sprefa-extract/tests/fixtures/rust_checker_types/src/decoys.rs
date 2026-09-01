//! Same-named type declarations in another file. Without them a corpus-wide
//! name match binds every candidate by name alone and no test can tell the
//! checker's answer apart from it.

pub struct Widget {
    pub weight: usize,
}

pub struct Config {
    pub weight: usize,
}

pub struct PathBuf;
