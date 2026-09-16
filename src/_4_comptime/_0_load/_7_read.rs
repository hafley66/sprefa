//! The byte edge. The only `std::fs` in this folder; everything past it is a
//! function from owned values to owned values.

use std::io;
use std::path::{Path, PathBuf};

/// SWI's `read_line_to_string/2` drops a trailing `\r\n` as well as a trailing
/// `\n` and still yields the last partial line of a file with no terminator.
pub fn split_lines(text: &str) -> Vec<String> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() && text.is_empty() {
        return Vec::new();
    }
    body.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

pub fn read_tsi_stream(path: &Path) -> io::Result<Vec<String>> {
    Ok(split_lines(&std::fs::read_to_string(path)?))
}

pub fn read_source_fact_files(paths: &[PathBuf]) -> Vec<(PathBuf, io::Result<String>)> {
    paths
        .iter()
        .map(|path| (path.clone(), std::fs::read_to_string(path)))
        .collect()
}

/// The `.dl7` paths under a root, sorted. `2_compiler.pl:267` owns the real
/// project walk; this edge exists so the loader can be driven from a
/// directory in a test without one.
pub fn read_dir_tree(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "dl7") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}
