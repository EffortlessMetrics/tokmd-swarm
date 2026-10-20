//! Non-UTF-8 tracked assets must not produce a complete, lossy inventory.
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde_json::Value;

fn stage(root: &Path) -> Result<()> {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(["add", "--all"])
        .current_dir(root)
        .output()?;
    ensure!(output.status.success(), "Git staging failed: {output:?}");
    Ok(())
}

fn analyze(root: &Path) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_tokmd"))
        .args([
            "--no-progress",
            "--config",
            "none",
            "analyze",
            ".",
            "--preset",
            "supply",
            "--format",
            "json",
        ])
        .env_remove("TOKMD_PROFILE")
        .current_dir(root)
        .output()?;
    ensure!(output.status.success(), "Analysis failed: {output:?}");
    ensure!(output.stderr.is_empty(), "Unexpected stderr: {output:?}");
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn non_utf8_git_discovery_is_explicitly_partial_and_rename_recovers() -> Result<()> {
    let root = tempfile::tempdir()?;
    let init = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(["init", "-q"])
        .current_dir(root.path())
        .output()?;
    ensure!(init.status.success(), "Git initialization failed: {init:?}");
    fs::write(root.path().join("main.rs"), b"fn main() {}\n")?;
    fs::write(root.path().join("raw-\u{fffd}.png"), b"valid-image")?;
    let invalid = root
        .path()
        .join(OsString::from_vec(b"raw-\xff.png".to_vec()));
    fs::write(&invalid, b"other-image")?;
    stage(root.path())?;

    let failure = analyze(root.path())?;
    assert_eq!(
        failure.get("status").and_then(Value::as_str),
        Some("partial")
    );
    ensure!(
        failure.get("assets").is_none_or(Value::is_null),
        "Must not advertise incomplete asset counts"
    );
    let warnings = failure
        .get("warnings")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing warnings"))?;
    ensure!(
        warnings
            .iter()
            .any(|warning| warning.as_str().is_some_and(|s| {
                s.contains("git ls-files")
                    && s.contains("not valid UTF-8")
                    && s.contains("ff")
                    && s.contains("rename")
            })),
        "Missing encoding failure and recovery: {warnings:?}"
    );

    fs::rename(&invalid, root.path().join("repaired.png"))?;
    stage(root.path())?;
    let repaired = analyze(root.path())?;
    assert_eq!(
        repaired.get("status").and_then(Value::as_str),
        Some("complete")
    );
    assert_eq!(repaired.get("warnings"), Some(&serde_json::json!([])));
    assert_eq!(
        repaired
            .pointer("/assets/total_files")
            .and_then(Value::as_u64),
        Some(2)
    );
    let paths: Vec<_> = repaired
        .pointer("/assets/top_files")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing asset files"))?
        .iter()
        .map(|row| {
            row.get("path")
                .and_then(Value::as_str)
                .context("asset path missing")
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        paths.contains(&"raw-\u{fffd}.png") && paths.contains(&"repaired.png"),
        "Wrong file identities: {paths:?}"
    );
    Ok(())
}
