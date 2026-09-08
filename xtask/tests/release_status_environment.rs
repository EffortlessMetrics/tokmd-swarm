//! Local release facts must describe the discovered workspace, even when the
//! parent process carries Git settings for a different repository.

use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::process::Command;

fn fixture_git(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(name);
    }
    let output = command.output().context("run fixture Git")?;
    ensure!(
        output.status.success(),
        "fixture Git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn tagged_workspace(root: &Path, marker: &str) -> Result<String> {
    std::fs::create_dir(root)?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion = \"1.15.1\"\n",
    )?;
    std::fs::write(root.join("identity.txt"), marker)?;
    fixture_git(root, &["init", "--quiet"])?;
    fixture_git(
        root,
        &["config", "core.hooksPath", "disabled-fixture-hooks"],
    )?;
    fixture_git(root, &["add", "--", "Cargo.toml", "identity.txt"])?;
    fixture_git(
        root,
        &[
            "-c",
            "user.name=Release fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            marker,
        ],
    )?;
    fixture_git(root, &["tag", "v1.15.1"])?;
    fixture_git(root, &["rev-parse", "HEAD"])
}

#[test]
fn release_status_ignores_ambient_repository_overrides() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = temp.path().join("workspace");
    let foreign = temp.path().join("foreign");
    let expected = tagged_workspace(&workspace, "workspace identity")?;
    let foreign_sha = tagged_workspace(&foreign, "foreign identity")?;
    ensure!(expected != foreign_sha, "fixture commits must differ");
    let receipt_path = temp.path().join("status.json");

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(&workspace)
        .args(["release-status", "--tag", "v1.15.1", "--json"])
        .arg(&receipt_path)
        .env("GIT_DIR", foreign.join(".git"))
        .env("GIT_WORK_TREE", &foreign)
        .env("GIT_COMMON_DIR", foreign.join(".git"))
        .env("GIT_INDEX_FILE", foreign.join(".git/index"))
        .env("GIT_OBJECT_DIRECTORY", foreign.join(".git/objects"))
        .output()
        .context("run just-built release-status with foreign Git environment")?;
    ensure!(
        output.status.success(),
        "release-status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
    ensure!(
        receipt
            .pointer("/source/sha")
            .and_then(serde_json::Value::as_str)
            == Some(expected.as_str()),
        "release source must be the discovered workspace, not foreign commit {foreign_sha}: {receipt}"
    );
    ensure!(
        receipt
            .pointer("/source/state")
            .and_then(serde_json::Value::as_str)
            == Some("passed")
    );
    ensure!(receipt.get("complete").and_then(serde_json::Value::as_bool) == Some(false));
    Ok(())
}
