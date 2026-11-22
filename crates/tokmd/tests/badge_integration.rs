#![cfg(feature = "analysis")]

mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn tokmd_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    cmd.current_dir(common::fixture_root());
    cmd
}

#[test]
fn badge_lines_svg_stdout() {
    let mut cmd = tokmd_cmd();
    cmd.arg("badge")
        .arg("--metric")
        .arg("lines")
        .assert()
        .success()
        .stdout(predicate::str::contains("<svg"))
        .stdout(predicate::str::contains("lines"));
}

#[test]
fn badge_writes_to_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let out_file = dir.path().join("badge.svg");

    let mut cmd = tokmd_cmd();
    cmd.arg("badge")
        .arg("--metric")
        .arg("bytes")
        .arg("--out")
        .arg(&out_file)
        .assert()
        .success()
        .stdout("");

    let content = std::fs::read_to_string(&out_file)?;
    assert!(content.contains("<svg"));
    assert!(content.contains("bytes"));
    Ok(())
}

#[cfg(unix)]
fn badge_nonfile_output_recovers(parent_is_file: bool) -> anyhow::Result<()> {
    let dir = tempdir()?;
    let config = dir.path().join("selected.toml");
    let source = dir.path().join("sample.rs");
    std::fs::write(&config, "")?;
    std::fs::write(&source, "pub fn badge_fixture() {}\n")?;
    let parent = dir.path().join("rate_limit_timeout");
    let output = parent.join("badge.svg");
    if parent_is_file {
        std::fs::write(&parent, "occupied")?;
    } else {
        std::fs::create_dir(&parent)?;
        std::fs::create_dir(&output)?;
    }

    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_tokmd"))
            .current_dir(dir.path())
            .env("TOKMD_CONFIG", &config)
            .env_remove("TOKMD_PROFILE")
            .args(["badge", "--metric", "lines", "--no-git", "--output"])
            .arg(&output)
            .arg(&source)
            .output()
    };
    let failure = run()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1) && failure.stdout.is_empty(),
        "non-file badge output must fail with exit 1 and empty stdout: {stderr}"
    );
    anyhow::ensure!(
        stderr.starts_with(&format!(
            "Error: Failed to write badge to {}:",
            output.display()
        )),
        "selected badge output path was lost: {stderr}"
    );
    let expected = if parent_is_file {
        "- Replace the non-directory output parent with a directory, then retry."
    } else {
        "- The output path is a directory. Select a file path, then retry."
    };
    let hints: Vec<_> = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect();
    anyhow::ensure!(hints == [expected], "wrong badge recovery: {stderr}");

    if parent_is_file {
        anyhow::ensure!(std::fs::read_to_string(&parent)? == "occupied");
        std::fs::remove_file(&parent)?;
        std::fs::create_dir(&parent)?;
    } else {
        anyhow::ensure!(output.is_dir() && std::fs::read_dir(&output)?.next().is_none());
        std::fs::remove_dir(&output)?;
    }
    let success = run()?;
    anyhow::ensure!(
        success.status.code() == Some(0) && success.stdout.is_empty() && success.stderr.is_empty(),
        "same-argv badge retry failed: {}",
        String::from_utf8_lossy(&success.stderr)
    );
    let svg = std::fs::read_to_string(&output)?;
    anyhow::ensure!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    anyhow::ensure!(svg.contains("lines") && svg.contains(">1</text>"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn badge_directory_output_keeps_local_recovery_and_can_be_retried() -> anyhow::Result<()> {
    badge_nonfile_output_recovers(false)
}

#[cfg(unix)]
#[test]
fn badge_file_parent_keeps_local_recovery_and_can_be_retried() -> anyhow::Result<()> {
    badge_nonfile_output_recovers(true)
}

#[test]
fn badge_output_recovery_uses_native_error_kind() -> anyhow::Result<()> {
    use std::io::ErrorKind;
    for (kind, expected) in [
        (
            ErrorKind::IsADirectory,
            vec!["- The output path is a directory. Select a file path, then retry."],
        ),
        (
            ErrorKind::NotADirectory,
            vec!["- Replace the non-directory output parent with a directory, then retry."],
        ),
        (ErrorKind::StorageFull, vec![]),
        (ErrorKind::PermissionDenied, vec![]),
        (
            ErrorKind::TimedOut,
            vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        ),
    ] {
        let error = anyhow::Error::new(std::io::Error::new(kind, "opération impossible"))
            .context("Failed to write badge to rate_limit/timeout/badge.svg");
        let rendered = tokmd::format_error(&error);
        let hints: Vec<_> = rendered
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect();
        anyhow::ensure!(hints == expected, "wrong recovery for {kind:?}: {rendered}");
        anyhow::ensure!(
            error
                .downcast_ref::<std::io::Error>()
                .map(std::io::Error::kind)
                == Some(kind)
        );
        anyhow::ensure!(tokmd::exit_code(&error) == 1);
    }
    Ok(())
}
