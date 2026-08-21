//! The emit_rust_harness flags that let a shell drive a program without
//! writing a schedule file and without reading JSON back: `--arrive`,
//! `--final`, `--final-only`, `--final-tsv`, `--final-rels`.
//!
//! Sabotage receipt for the TSV leg: dropping the `rel` prefix from the
//! `--final-tsv` line reds `tsv_rows_carry_the_rel_then_its_columns` with the
//! row read as `a.rs\t3\t9`, one column short of the four a caller splits.
//!
//! Sabotage receipt for the seed leg: sending the `--arrive` rows to a second
//! batch instead of the first reds `arrive_alone_needs_no_schedule_file` with
//! two tick lines where the fold is one arrival tick plus its drains.

use std::process::Command;

fn program(name: &str) -> String {
    format!(
        "{}/tests/fixtures/{name}.program.rs",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn harness(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_emit_rust_harness"))
        .args(args)
        .output()
        .expect("spawn harness")
}

#[test]
fn arrive_alone_needs_no_schedule_file() {
    let output = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "source_file=a.rs",
        "--arrive",
        "__host_response_look=witness|look|path:text=a.rs,0,a.rs,3,9",
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines[0].contains("\"source_file\""),
        "the first tick carries the seeded arrival, got {}",
        lines[0]
    );
    assert!(
        stdout.contains("\"spanned\""),
        "the host answer reaches the rule, got {stdout}"
    );
}

#[test]
fn tsv_rows_carry_the_rel_then_its_columns() {
    let output = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "source_file=a.rs",
        "--arrive",
        "__host_response_look=witness|look|path:text=a.rs,0,a.rs,3,9",
        "--final-only",
        "--final-tsv",
        "--final-rels",
        "spanned",
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "spanned\ta.rs\t3\t9\n", "{stdout}");
}

#[test]
fn final_only_drops_the_tick_lines_and_final_keeps_them() {
    let with_ticks = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "source_file=a.rs",
        "--arrive",
        "__host_response_look=witness|look|path:text=a.rs,0,a.rs,3,9",
        "--final",
        "--final-rels",
        "spanned",
    ]);
    let without = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "source_file=a.rs",
        "--arrive",
        "__host_response_look=witness|look|path:text=a.rs,0,a.rs,3,9",
        "--final-only",
        "--final-rels",
        "spanned",
    ]);
    let kept = String::from_utf8_lossy(&with_ticks.stdout);
    let dropped = String::from_utf8_lossy(&without.stdout);
    assert_eq!(
        dropped,
        r#"{"columns":["path","start","end"],"rel":"spanned","rows":[["a.rs",3,9]]}"#.to_owned()
            + "\n",
        "{dropped}"
    );
    assert!(kept.ends_with(dropped.as_ref()), "{kept}");
    assert!(kept.lines().count() > dropped.lines().count(), "{kept}");
}

#[test]
fn final_rels_names_the_read_order() {
    let output = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "source_file=a.rs",
        "--arrive",
        "__host_response_look=witness|look|path:text=a.rs,0,a.rs,3,9",
        "--final-only",
        "--final-tsv",
        "--final-rels",
        "spanned,source_file",
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout, "spanned\ta.rs\t3\t9\nsource_file\ta.rs\n", "{stdout}");
}

#[test]
fn a_schedule_and_arrive_share_the_first_tick() {
    let output = harness(&[
        &program("live_shell_probe"),
        &fixture("live_shell_probe.scripted-response.schedule.json"),
        "--arrive",
        "source_file=b.rs",
        "--final-only",
        "--final-tsv",
        "--final-rels",
        "source_file",
    ]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout, "source_file\ta.rs\nsource_file\tb.rs\n",
        "the schedule's first batch and the flag's row land together, got {stdout}"
    );
}

#[test]
fn an_undeclared_rel_and_a_wrong_arity_are_named_stops() {
    let unknown = harness(&[&program("live_shell_probe"), "--arrive", "no_such_rel=x"]);
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("no_such_rel"),
        "{unknown:?}"
    );

    let arity = harness(&[&program("live_shell_probe"), "--arrive", "source_file=a,b"]);
    assert_eq!(arity.status.code(), Some(2), "{arity:?}");
    let stderr = String::from_utf8_lossy(&arity.stderr);
    assert!(stderr.contains("2 values"), "{stderr}");
    assert!(stderr.contains("1 columns"), "{stderr}");
}

#[test]
fn an_int_column_stops_rather_than_read_a_cell_as_zero() {
    let output = harness(&[
        &program("live_shell_probe"),
        "--arrive",
        "__host_response_look=w,0,a.rs,3,nine",
    ]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("wants an integer"), "{stderr}");
    assert!(stderr.contains("nine"), "{stderr}");
}

#[test]
fn a_program_with_no_seed_and_no_schedule_prints_the_usage() {
    let output = harness(&[&program("live_shell_probe")]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: emit_rust_harness"), "{stderr}");
    assert!(stderr.contains("--arrive"), "{stderr}");
}
