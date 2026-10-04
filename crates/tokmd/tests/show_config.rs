//! Integration tests for the global `--show-config` diagnostic surface.
//!
//! See `docs/specs/config-explainability.md` for the behavior contract.

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn tokmd_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(dir)
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_CONFIG");
    cmd
}

#[test]
fn show_config_prints_report_and_exits_without_scanning() -> TestResult {
    let tmp = tempfile::tempdir()?;
    tokmd_in(tmp.path())
        .arg("--show-config")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokmd configuration"))
        .stdout(predicate::str::contains(
            "Config sources (in precedence order):",
        ))
        .stdout(predicate::str::contains("Active profile:"))
        .stdout(predicate::str::contains("Resolved values:"));
    Ok(())
}

#[test]
fn show_config_flags_unmatched_profile() -> TestResult {
    let tmp = tempfile::tempdir()?;
    tokmd_in(tmp.path())
        .arg("--show-config")
        .arg("--profile")
        .arg("tokmd-no-such-profile-xyz")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "name:           tokmd-no-such-profile-xyz (from --profile)",
        ))
        .stdout(predicate::str::contains("did not match"));
    Ok(())
}

#[test]
fn show_config_is_available_after_subcommand() -> TestResult {
    let tmp = tempfile::tempdir()?;
    tokmd_in(tmp.path())
        .arg("module")
        .arg("--show-config")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokmd configuration"));
    Ok(())
}

#[test]
fn normal_run_does_not_print_config_report() -> TestResult {
    let tmp = tempfile::tempdir()?;
    tokmd_in(tmp.path())
        .arg("lang")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokmd configuration").not());
    Ok(())
}

#[test]
fn malformed_local_config_fails_before_machine_output() -> TestResult {
    let tmp = tempfile::tempdir()?;
    std::fs::write(tmp.path().join("tokmd.toml"), "[scan\n")?;
    std::fs::write(tmp.path().join("sample.rs"), "fn main() {}\n")?;

    tokmd_in(tmp.path())
        .args(["--format", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("tokmd.toml"))
        .stderr(predicate::str::contains("TOML"));
    Ok(())
}

#[test]
fn malformed_explicit_config_does_not_fall_back_to_local_config() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let selected = tmp.path().join("selected.toml");
    std::fs::write(&selected, "[scan\n")?;
    std::fs::write(tmp.path().join("tokmd.toml"), "[lang]\ntop = 3\n")?;

    tokmd_in(tmp.path())
        .env("TOKMD_CONFIG", &selected)
        .arg("--show-config")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("selected.toml"));
    Ok(())
}
