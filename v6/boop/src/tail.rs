//! Offset-resume tailing: read a transcript forward from a byte offset and
//! return only the complete lines, leaving a partial trailing line untouched.
//!
//! A transcript is appended to while it is read; a half-written JSON line is
//! normal, so a line with no terminating `\n` is never parsed and never counted
//! into `next_offset`. The next call re-reads it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

/// One complete line (terminated by `\n`) read from a file.
#[derive(Clone, Debug)]
pub struct CompleteLine {
    /// Byte offset of this line's first byte in the file.
    pub start: u64,
    /// The line bytes, without the trailing `\n`.
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ReadResult {
    pub lines: Vec<CompleteLine>,
    /// Byte offset to resume from: the byte just after the last complete line's
    /// `\n`. A trailing partial line is not included.
    pub next_offset: u64,
    /// True when the file was shorter than the requested offset.
    pub reset: bool,
}

/// Read complete lines from `file`, starting at the byte `offset`.
///
/// `offset` is a byte offset, never a line index. If the file is shorter than
/// `offset` (truncated or rotated), the read restarts at byte 0 and `reset` is
/// set. A line with no terminating `\n` is left unconsumed.
pub fn read_complete_lines(file: &mut File, offset: u64) -> anyhow::Result<ReadResult> {
    let len = file.metadata()?.len();
    if len < offset {
        let mut result = read_from(file, 0)?;
        result.reset = true;
        return Ok(result);
    }
    read_from(file, offset)
}

fn read_from(file: &mut File, offset: u64) -> anyhow::Result<ReadResult> {
    file.seek(SeekFrom::Start(offset))?;
    let len = file.metadata()?.len();
    let n = len.saturating_sub(offset);
    let mut body = vec![0u8; n as usize];
    file.read_exact(&mut body)?;

    // index of the last `\n`; bytes after it are a partial line to leave alone
    let last_newline = body.iter().rposition(|byte| *byte == b'\n');

    let chunk_end = match last_newline {
        Some(position) => position
            .checked_add(1)
            .map(|end| offset.saturating_add(end as u64))
            .unwrap_or(offset),
        None => offset,
    };

    let mut lines = Vec::new();
    if let Some(position) = last_newline {
        let mut line_start = 0usize;
        for (index, byte) in body[..=position].iter().enumerate() {
            if *byte == b'\n' {
                lines.push(CompleteLine {
                    start: offset.saturating_add(line_start as u64),
                    bytes: body[line_start..index].to_vec(),
                });
                line_start = index + 1;
            }
        }
    }

    Ok(ReadResult {
        lines,
        next_offset: chunk_end,
        reset: false,
    })
}

#[cfg(test)]
mod tests {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    use super::read_complete_lines;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_tail_{}_{}", std::process::id(), name))
    }

    fn overwrite(path: &PathBuf, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
    }

    fn append(path: &PathBuf, bytes: &[u8]) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(bytes).unwrap();
    }

    fn open(path: &PathBuf) -> File {
        File::open(path).unwrap()
    }

    #[test]
    fn tails_three_lines_from_zero() {
        let path = temp_path("t1");
        overwrite(&path, b"a\nb\nc\n");
        let result = read_complete_lines(&mut open(&path), 0).unwrap();
        assert_eq!(result.lines.len(), 3);
        assert_eq!(result.next_offset, 6);
        assert!(!result.reset);
    }

    #[test]
    fn picks_up_one_new_complete_line() {
        let path = temp_path("t2");
        overwrite(&path, b"a\nb\nc\n");
        let first = read_complete_lines(&mut open(&path), 0).unwrap();
        append(&path, b"d\n");
        let result = read_complete_lines(&mut open(&path), first.next_offset).unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.next_offset, 8);
        assert!(!result.reset);
    }

    #[test]
    fn leaves_a_partial_line_untouched() {
        let path = temp_path("t3");
        overwrite(&path, b"a\nb\nc\nd\n");
        let first = read_complete_lines(&mut open(&path), 0).unwrap();
        append(&path, b"part");
        let result = read_complete_lines(&mut open(&path), first.next_offset).unwrap();
        assert_eq!(result.lines.len(), 0);
        assert_eq!(result.next_offset, first.next_offset);
        assert!(!result.reset);
    }

    #[test]
    fn completes_a_partial_line() {
        let path = temp_path("t4");
        overwrite(&path, b"a\nb\nc\nd\n");
        let first = read_complete_lines(&mut open(&path), 0).unwrap();
        append(&path, b"part");
        let partial = read_complete_lines(&mut open(&path), first.next_offset).unwrap();
        append(&path, b"\n");
        let result = read_complete_lines(&mut open(&path), partial.next_offset).unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.next_offset, 13);
        assert!(!result.reset);
    }

    #[test]
    fn resets_when_truncated_below_offset() {
        let path = temp_path("t5");
        overwrite(&path, b"a\nb\nc\nd\npart\n");
        let full = read_complete_lines(&mut open(&path), 0).unwrap();
        overwrite(&path, b"a\nb");
        let result = read_complete_lines(&mut open(&path), full.next_offset).unwrap();
        assert!(result.reset);
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.next_offset, 2);
    }
}
