//! Integration tests for the global `--show-config` diagnostic surface.
//!
//! See `docs/specs/config-explainability.md` for the behavior contract.

use assert_cmd::Command;
use predicates::prelude::*;

fn tokmd_in(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(dir)
        .env_remove("TOKMD_PROFILE")
        .env_remove("TOKMD_CONFIG");
    cmd
}

#[test]
fn show_config_prints_report_and_exits_without_scanning() {
    let tmp = tempfile::tempdir().unwrap();
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
}

#[test]
fn show_config_flags_unmatched_profile() {
    let tmp = tempfile::tempdir().unwrap();
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
}

#[test]
fn show_config_is_available_after_subcommand() {
    let tmp = tempfile::tempdir().unwrap();
    tokmd_in(tmp.path())
        .arg("module")
        .arg("--show-config")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokmd configuration"));
}

#[test]
fn normal_run_does_not_print_config_report() {
    let tmp = tempfile::tempdir().unwrap();
    tokmd_in(tmp.path())
        .arg("lang")
        .assert()
        .success()
        .stdout(predicate::str::contains("tokmd configuration").not());
}
