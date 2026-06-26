//! Integration tests for machine-readable progress events on the scan-summary
//! command family (`lang`, `module`, `export`).
//!
//! See `docs/specs/progress-events.md` for the behavior contract. These tests
//! assert that, when `TOKMD_PROGRESS_EVENTS` is set, each command emits
//! `tokmd.progress` events to **stderr** while keeping **stdout** free of
//! progress noise, and that the events are absent when the variable is unset.

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const UPDATE_LINE: &str = r#"{"event":"tokmd.progress","kind":"update","message":"Scanning codebase...","schema_version":1}"#;
const FINISH_LINE: &str =
    r#"{"event":"tokmd.progress","kind":"finish","message":"done","schema_version":1}"#;

fn fixture_dir() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    std::fs::write(tmp.path().join("main.rs"), "fn main() {\n    // body\n}\n")?;
    Ok(tmp)
}

fn tokmd_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(dir)
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_CONFIG")
        .env_remove("TOKMD_NO_PROGRESS");
    cmd
}

/// The progress contract is identical across the scan-summary family, so assert
/// it uniformly: events on stderr, no progress noise on stdout.
fn assert_emits_progress(subcommand: &str) -> TestResult {
    let tmp = fixture_dir()?;
    tokmd_in(tmp.path())
        .arg(subcommand)
        .arg("--no-progress")
        .arg(".")
        .env("TOKMD_PROGRESS_EVENTS", "1")
        .assert()
        .success()
        .stderr(predicate::str::contains(UPDATE_LINE))
        .stderr(predicate::str::contains(FINISH_LINE))
        .stdout(predicate::str::contains("tokmd.progress").not());
    Ok(())
}

#[test]
fn lang_emits_progress_events_on_stderr() -> TestResult {
    assert_emits_progress("lang")
}

#[test]
fn module_emits_progress_events_on_stderr() -> TestResult {
    assert_emits_progress("module")
}

#[test]
fn export_emits_progress_events_on_stderr() -> TestResult {
    assert_emits_progress("export")
}

#[test]
fn lang_emits_no_progress_events_when_unset() -> TestResult {
    let tmp = fixture_dir()?;
    tokmd_in(tmp.path())
        .arg("lang")
        .arg(".")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .assert()
        .success()
        .stderr(predicate::str::contains("tokmd.progress").not());
    Ok(())
}

#[test]
fn export_emits_no_progress_events_when_unset() -> TestResult {
    let tmp = fixture_dir()?;
    tokmd_in(tmp.path())
        .arg("export")
        .arg(".")
        .env_remove("TOKMD_PROGRESS_EVENTS")
        .assert()
        .success()
        .stderr(predicate::str::contains("tokmd.progress").not());
    Ok(())
}
