use std::io::Write;

#[derive(Clone, Debug)]
pub struct EngineRow {
    pub name: String,
    pub is_reference: bool,
    pub derived: u64,
    pub fixpoint_ms: u64,
    pub load_ms: u64,
    pub peak_rss_kb: i64,
    pub runs_used: u32,
    pub dnf: bool,
}

#[derive(Clone, Debug)]
pub struct CaseStanding {
    pub family_name: String,
    pub scale: u32,
    pub edges: u64,
    pub derived: u64,
    pub params_label: String,
    pub rows: Vec<EngineRow>,
}

#[derive(Clone, Debug)]
pub struct BuildRow {
    pub name: String,
    pub size_bytes: u64,
    pub build_seconds: Option<f64>,
}

pub struct RunMeta {
    pub command_line: String,
}

impl CaseStanding {
    pub fn best_derived_per_sec(&self) -> f64 {
        let mut best = 0.0f64;
        for row in &self.rows {
            if row.is_reference {
                continue;
            }
            let seconds = row.fixpoint_ms as f64 / 1000.0;
            if seconds <= 0.0 {
                continue;
            }
            let throughput = row.derived as f64 / seconds;
            if throughput > best {
                best = throughput;
            }
        }
        best
    }
}

pub fn write_standings(cases: &[CaseStanding], builds: &[BuildRow], meta: &RunMeta) -> String {
    let mut output = String::new();
    output.push_str("# exec_shootout STANDINGS\n\n");
    output.push_str("Run command: `");
    output.push_str(&meta.command_line);
    output.push_str("`\n\n");
    output.push_str("THE number is derived rows/sec in the fixpoint phase, best of 3.\n\n");

    let mut grouped: Vec<&CaseStanding> = cases.iter().collect();
    grouped.sort_by(|left, right| {
        left.family_name
            .cmp(&right.family_name)
            .then_with(|| left.scale.cmp(&right.scale))
    });

    let mut current_family: Option<&str> = None;
    for standing in grouped {
        if current_family != Some(standing.family_name.as_str()) {
            if current_family.is_some() {
                output.push('\n');
            }
            output.push_str(&format!("## {}\n\n", standing.family_name));
            output.push_str(
                "| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |\n",
            );
            output.push_str("|---|---|---|---|---|---|---|---|---|---|\n");
            current_family = Some(standing.family_name.as_str());
        }
        let best = standing.best_derived_per_sec();
        let best_engine = standing
            .rows
            .iter()
            .filter(|row| !row.is_reference && !row.dnf)
            .min_by_key(|row| row.fixpoint_ms)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| "-".to_string());
        for (row_index, row) in standing.rows.iter().enumerate() {
            let seconds = row.fixpoint_ms as f64 / 1000.0;
            let throughput = if seconds > 0.0 {
                row.derived as f64 / seconds
            } else {
                0.0
            };
            let throughput_text = if row.dnf {
                "DNF".to_string()
            } else if row.is_reference {
                "(reference)".to_string()
            } else {
                format!("{:.0}", throughput)
            };
            let (load_show, fp_show, rss_show) = if row.is_reference {
                ("-".to_string(), "-".to_string(), "-".to_string())
            } else if row.dnf {
                ("-".to_string(), "-".to_string(), "-".to_string())
            } else {
                (
                    format!("{}", row.load_ms),
                    format!("{}", row.fixpoint_ms),
                    format!("{}", row.peak_rss_kb),
                )
            };
            let name = row.name.clone();
            let derived_text = if row.dnf {
                "DNF".to_string()
            } else {
                format!("{}", row.derived)
            };
            let scale_text = if row_index == 0 {
                format!("{}", standing.scale)
            } else {
                String::new()
            };
            let params_text = if row_index == 0 {
                standing.params_label.clone()
            } else {
                String::new()
            };
            let edges_text = if row_index == 0 {
                format!("{}", standing.edges)
            } else {
                String::new()
            };
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                scale_text,
                params_text,
                edges_text,
                name,
                derived_text,
                throughput_text,
                load_show,
                fp_show,
                rss_show,
                row.runs_used,
            ));
        }
        output.push_str(&format!(
            "| **best (THE number)** | | | | **{}** | **{:.0}** | | | | |\n",
            best_engine, best
        ));
    }

    output.push_str("\n## Engine builds\n\n");
    output.push_str("| engine | release binary size (bytes) | cold build seconds |\n");
    output.push_str("|---|---|---|\n");
    for build in builds {
        match build.build_seconds {
            Some(seconds) => {
                output.push_str(&format!(
                    "| {} | {} | {:.1} |\n",
                    build.name, build.size_bytes, seconds
                ));
            }
            None => {
                output.push_str(&format!(
                    "| {} | {} | n/a |\n",
                    build.name, build.size_bytes
                ));
            }
        }
    }

    output.push_str(
        "\n\nCorrectness: every engine agrees on (derived, checksum); the internal \
reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.\n",
    );
    output
}

pub fn write_standings_to(path: &str, cases: &[CaseStanding], builds: &[BuildRow], meta: &RunMeta) {
    let content = write_standings(cases, builds, meta);
    let mut file = std::fs::File::create(path)
        .unwrap_or_else(|error| panic!("cannot create {}: {}", path, error));
    file.write_all(content.as_bytes())
        .unwrap_or_else(|error| panic!("cannot write {}: {}", path, error));
}
