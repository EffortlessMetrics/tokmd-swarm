#![cfg(feature = "analysis")]

//! Parser-specific guidance must survive format words in resource paths.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn gate(dir: &Path, input: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(dir)
        .env_remove("TOKMD_CONFIG")
        .env_remove("TOKMD_PROFILE")
        .args(["gate", "--format", "json"])
        .arg(input);
    command
}

#[test]
fn malformed_json_under_toml_directory_can_be_repaired_and_retried() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let nested = dir.path().join("toml");
    std::fs::create_dir(&nested)?;
    let receipt = nested.join("receipt.json");
    let policy = dir.path().join("policy.toml");
    std::fs::write(&receipt, "{broken")?;
    std::fs::write(&policy, "rules = []\n")?;

    gate(dir.path(), &receipt)
        .arg("--policy")
        .arg(&policy)
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(receipt.display().to_string()))
        .stderr(predicate::str::contains("Failed to parse JSON"))
        .stderr(predicate::str::contains(
            "Ensure the file is a tokmd JSON receipt (produced by `tokmd run`, `tokmd export`, or `tokmd analyze`).",
        ))
        .stderr(predicate::str::contains(
            "If it was hand-edited or truncated, regenerate the receipt and retry.",
        ))
        .stderr(predicate::str::contains("Check TOML syntax").not());

    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    let output = gate(dir.path(), &receipt)
        .arg("--policy")
        .arg(&policy)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let result: serde_json::Value = serde_json::from_slice(&output)?;
    anyhow::ensure!(result["passed"] == true);
    Ok(())
}

#[test]
fn policy_and_ratchet_files_named_json_keep_toml_recovery() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let receipt = dir.path().join("receipt.json");
    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    for flag in ["--policy", "--ratchet-config"] {
        let config = dir.path().join("receipt-policy.json");
        std::fs::write(&config, "{broken")?;
        let mut command = gate(dir.path(), &receipt);
        command.arg(flag).arg(&config);
        if flag == "--ratchet-config" {
            command.arg("--baseline").arg(&receipt);
        }
        command
            .assert()
            .code(2)
            .stdout("")
            .stderr(predicate::str::contains(config.display().to_string()))
            .stderr(predicate::str::contains("Failed to parse policy TOML"))
            .stderr(predicate::str::contains(
                "Check TOML syntax and key names in the file named above, then retry.",
            ))
            .stderr(predicate::str::contains("regenerate the receipt").not());

        std::fs::write(&config, "rules = []\n")?;
        let mut retry = gate(dir.path(), &receipt);
        retry.arg(flag).arg(&config);
        if flag == "--ratchet-config" {
            retry.arg("--baseline").arg(&receipt);
        }
        let output = retry.assert().success().get_output().stdout.clone();
        let result: serde_json::Value = serde_json::from_slice(&output)?;
        anyhow::ensure!(result["passed"] == true);
    }
    Ok(())
}

#[test]
fn selected_toml_config_named_json_can_be_repaired_and_retried() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("receipt-config.json");
    let receipt = dir.path().join("receipt.json");
    let policy = dir.path().join("policy.toml");
    std::fs::write(&config, "broken = [")?;
    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    std::fs::write(&policy, "rules = []\n")?;
    gate(dir.path(), &receipt)
        .env("TOKMD_CONFIG", &config)
        .arg("--policy")
        .arg(&policy)
        .assert()
        .code(1)
        .stdout("")
        .stderr(predicate::str::contains(config.display().to_string()))
        .stderr(predicate::str::contains("Failed to load TOML config"))
        .stderr(predicate::str::contains(
            "Check TOML syntax and key names in the file named above, then retry.",
        ))
        .stderr(predicate::str::contains("regenerate the receipt").not());

    std::fs::write(&config, "")?;
    let output = gate(dir.path(), &receipt)
        .env("TOKMD_CONFIG", &config)
        .arg("--policy")
        .arg(&policy)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let result: serde_json::Value = serde_json::from_slice(&output)?;
    anyhow::ensure!(result["passed"] == true);
    Ok(())
}

#[cfg(unix)]
#[test]
fn missing_policy_with_parse_marker_in_path_can_be_created_and_retried() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let receipt = dir.path().join("receipt.json");
    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    for (index, flag) in ["--policy", "--ratchet-config"].into_iter().enumerate() {
        let parent = dir.path().join(format!("inputs-{index}: invalid TOML:"));
        let config = parent.join("policy.toml");
        let mut command = gate(dir.path(), &receipt);
        command.arg(flag).arg(&config);
        if flag == "--ratchet-config" {
            command.arg("--baseline").arg(&receipt);
        }
        command
            .assert()
            .code(2)
            .stdout("")
            .stderr(predicate::str::contains(config.display().to_string()))
            .stderr(predicate::str::contains(
                "Verify the input path exists and is readable.",
            ))
            .stderr(predicate::str::contains(
                "Use an absolute path to avoid working-directory confusion.",
            ))
            .stderr(predicate::str::contains("Check TOML syntax").not())
            .stderr(predicate::str::contains("regenerate the receipt").not());

        std::fs::create_dir(&parent)?;
        std::fs::write(&config, "rules = []\n")?;
        let mut retry = gate(dir.path(), &receipt);
        retry.arg(flag).arg(&config);
        if flag == "--ratchet-config" {
            retry.arg("--baseline").arg(&receipt);
        }
        let output = retry.assert().success().get_output().stdout.clone();
        let result: serde_json::Value = serde_json::from_slice(&output)?;
        anyhow::ensure!(result["passed"] == true);
    }
    Ok(())
}
