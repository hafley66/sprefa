use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct EngineEvent {
    pub edges: u32,
    pub derived: u64,
    pub load_ms: u64,
    pub fixpoint_ms: u64,
    pub checksum: String,
    pub peak_rss_kb: i64,
}

fn take_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{}\":", key);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest
        .find(',')
        .or_else(|| rest.find('}'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn parse_event_line(line: &str, kind: &str) -> Result<EngineEvent, String> {
    let event = take_value(line, "event")
        .ok_or_else(|| format!("missing event in: {}", line))?
        .trim_matches('"');
    if event != kind {
        return Err(format!("expected event '{}' but got '{}'", kind, event));
    }
    let mut parsed = EngineEvent {
        edges: 0,
        derived: 0,
        load_ms: 0,
        fixpoint_ms: 0,
        checksum: String::new(),
        peak_rss_kb: 0,
    };
    match event {
        "loaded" => {
            parsed.edges = take_value(line, "edges")
                .ok_or_else(|| format!("missing edges in: {}", line))?
                .parse()
                .map_err(|_| format!("edges not a u32 in: {}", line))?;
            parsed.load_ms = take_value(line, "ms")
                .ok_or_else(|| format!("missing ms in: {}", line))?
                .parse()
                .map_err(|_| format!("ms not an int in: {}", line))?;
        }
        "fixpoint" => {
            parsed.derived = take_value(line, "derived")
                .ok_or_else(|| format!("missing derived in: {}", line))?
                .parse()
                .map_err(|_| format!("derived not an int in: {}", line))?;
            parsed.fixpoint_ms = take_value(line, "ms")
                .ok_or_else(|| format!("missing ms in: {}", line))?
                .parse()
                .map_err(|_| format!("ms not an int in: {}", line))?;
        }
        "done" => {
            parsed.checksum = take_value(line, "checksum")
                .ok_or_else(|| format!("missing checksum in: {}", line))?
                .trim_matches('"')
                .to_string();
            parsed.peak_rss_kb = take_value(line, "peak_rss_kb")
                .ok_or_else(|| format!("missing peak_rss_kb in: {}", line))?
                .parse()
                .map_err(|_| format!("peak_rss_kb not an int in: {}", line))?;
        }
        _ => return Err(format!("unknown event '{}'", event)),
    }
    Ok(parsed)
}

pub fn parse_events(stdout: &str) -> Result<(EngineEvent, EngineEvent, EngineEvent), String> {
    let mut loaded: Option<EngineEvent> = None;
    let mut fixpoint: Option<EngineEvent> = None;
    let mut done: Option<EngineEvent> = None;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("{\"event\":\"loaded\"") {
            loaded = Some(parse_event_line(line, "loaded")?);
        } else if line.starts_with("{\"event\":\"fixpoint\"") {
            fixpoint = Some(parse_event_line(line, "fixpoint")?);
        } else if line.starts_with("{\"event\":\"done\"") {
            done = Some(parse_event_line(line, "done")?);
        } else {
            return Err(format!("unexpected stdout line: {}", line));
        }
    }
    let loaded = loaded.ok_or_else(|| "missing 'loaded' event".to_string())?;
    let fixpoint = fixpoint.ok_or_else(|| "missing 'fixpoint' event".to_string())?;
    let done = done.ok_or_else(|| "missing 'done' event".to_string())?;
    Ok((loaded, fixpoint, done))
}

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct RunOutcome {
    pub event: EngineEvent,
    pub stderr: String,
}

pub fn run_engine(
    binary: &str,
    input_path: &str,
    timeout: Duration,
) -> Result<RunOutcome, String> {
    let mut child = Command::new(binary)
        .arg("--input")
        .arg(input_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot spawn '{}': {}", binary, error))?;
    let stderr_pipe = child.stderr.take().expect("stderr pipe");
    let stderr_reader = thread::spawn(move || {
        let mut buffer = String::new();
        let mut pipe = stderr_pipe;
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    });
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot poll child: {}", error))?
        {
            if !status.success() {
                let _ = child.wait();
                let stderr = stderr_reader.join().map_err(|_| "stderr thread panicked".to_string())?;
                return Err(format!("engine exited nonzero: {:?}\n{}", status, stderr));
            }
            let mut stdout_buffer = String::new();
            child
                .stdout
                .take()
                .expect("stdout pipe")
                .read_to_string(&mut stdout_buffer)
                .map_err(|error| format!("cannot read stdout: {}", error))?;
            let stderr = stderr_reader.join().map_err(|_| "stderr thread panicked".to_string())?;
            let (loaded, fixpoint, done) = parse_events(&stdout_buffer)?;
            let event = EngineEvent {
                edges: loaded.edges,
                load_ms: loaded.load_ms,
                derived: fixpoint.derived,
                fixpoint_ms: fixpoint.fixpoint_ms,
                checksum: done.checksum,
                peak_rss_kb: done.peak_rss_kb,
            };
            return Ok(RunOutcome { event, stderr });
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("engine timed out after {:?}", timeout));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_events() {
        let sample = "{\"event\":\"loaded\",\"edges\":3,\"ms\":1}\n{\"event\":\"fixpoint\",\"derived\":6,\"ms\":2}\n{\"event\":\"done\",\"checksum\":\"aabbccdd00112233\",\"peak_rss_kb\":4096}\n";
        let (loaded, fixpoint, done) = parse_events(sample).expect("parse");
        assert_eq!(loaded.edges, 3);
        assert_eq!(loaded.load_ms, 1);
        assert_eq!(fixpoint.derived, 6);
        assert_eq!(fixpoint.fixpoint_ms, 2);
        assert_eq!(done.checksum, "aabbccdd00112233");
        assert_eq!(done.peak_rss_kb, 4096);
    }

    #[test]
    fn rejects_foreign_line() {
        let sample = "garbage\n{\"event\":\"loaded\",\"edges\":3,\"ms\":1}\n";
        assert!(parse_events(sample).is_err());
    }

    #[test]
    fn rejects_missing_event() {
        let sample = "{\"event\":\"loaded\",\"edges\":3,\"ms\":1}\n";
        assert!(parse_events(sample).is_err());
    }
}
