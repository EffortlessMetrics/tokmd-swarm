//! Local release facts must describe the discovered workspace, even when the
//! parent process carries Git settings for a different repository.

use anyhow::{Context, Result, ensure};
use std::path::Path;
use std::process::Command;

fn fixture_git(root: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args);
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

fn tagged_workspace(root: &Path, marker: &str, version: &str) -> Result<String> {
    std::fs::create_dir(root)?;
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[workspace.package]\nversion = \"{version}\"\n"),
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
    fixture_git(root, &["tag", &format!("v{version}")])?;
    fixture_git(root, &["rev-parse", "HEAD"])
}

#[test]
fn release_status_ignores_ambient_repository_overrides() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let workspace = temp.path().join("workspace");
    let foreign = temp.path().join("foreign");
    let expected = tagged_workspace(&workspace, "workspace identity", "1.15.1")?;
    let foreign_sha = tagged_workspace(&foreign, "foreign identity", "1.15.1")?;
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

#[test]
fn release_status_rejects_staged_and_unstaged_source_changes_at_the_tag() -> Result<()> {
    for staged in [false, true] {
        let temp = tempfile::tempdir()?;
        let workspace = temp.path().join("workspace");
        tagged_workspace(&workspace, "clean tagged source", "1.15.1")?;
        std::fs::write(workspace.join("identity.txt"), "modified source")?;
        if staged {
            fixture_git(&workspace, &["add", "--", "identity.txt"])?;
        }
        let before = fixture_git(&workspace, &["diff", "HEAD", "--", "identity.txt"])?;
        let receipt_path = temp.path().join("status.json");
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(&workspace)
            .args(["release-status", "--tag", "v1.15.1", "--json"])
            .arg(&receipt_path)
            .output()?;
        ensure!(
            output.status.success(),
            "status inspection failed: {output:?}"
        );
        let receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(receipt_path)?)?;
        ensure!(
            receipt
                .pointer("/source/state")
                .and_then(serde_json::Value::as_str)
                == Some("failed"),
            "dirty source (staged={staged}) must not pass: {receipt}"
        );
        ensure!(
            receipt
                .pointer("/source/detail")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|detail| detail.contains("tracked changes")),
            "dirty source needs a specific diagnostic"
        );
        ensure!(fixture_git(&workspace, &["diff", "HEAD", "--", "identity.txt"])? == before);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn release_status_preflight_requires_live_evidence_and_rejects_stale_manifest_before_remote_lookup()
-> Result<()> {
    use serde_json::{Value, json};
    use std::fs;
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir()?;
    let workspace = temp.path().join("workspace");
    let sha = tagged_workspace(&workspace, "clean offline preflight fixture", "1.15.0")?;
    let status_path = temp.path().join("status.json");
    let fixture_path = temp.path().join("fixture.json");
    let manifest_path = temp.path().join("manifest.json");

    // Inspect real local Git, with no hosting client or credentials available.
    let tools = temp.path().join("tools");
    fs::create_dir(&tools)?;
    let path = std::env::var_os("PATH").context("fixture needs system Git")?;
    let git = std::env::split_paths(&path)
        .map(|dir| dir.join("git"))
        .find(|path| path.is_file())
        .context("locate fixture Git")?;
    symlink(fs::canonicalize(git)?, tools.join("git"))?;
    let cli = || {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.current_dir(&workspace)
            .env_clear()
            .env("PATH", &tools)
            .env("HOME", temp.path())
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["release-status", "--tag", "v1.15.0", "--json"])
            .arg(&status_path);
        cmd
    };
    let result = cli().output()?;
    ensure!(result.status.success(), "local status: {result:?}");
    let mut baseline: Value = serde_json::from_slice(&fs::read(&status_path)?)?;
    ensure!(baseline.pointer("/source/sha").and_then(Value::as_str) == Some(sha.as_str()));
    ensure!(baseline.pointer("/source/state").and_then(Value::as_str) == Some("passed"));
    ensure!(baseline.get("complete").and_then(Value::as_bool) == Some(false));
    ensure!(
        baseline.get("preflight").is_none(),
        "preflight requested implicitly"
    );

    fs::remove_file(&status_path)?;
    for claim in [
        Value::Null,
        json!({"state":"passed","detail":"offline fixture","evidence":"fixture"}),
    ] {
        baseline
            .as_object_mut()
            .context("local receipt shape")?
            .insert("preflight".into(), claim);
        fs::write(&fixture_path, serde_json::to_vec(&baseline)?)?;
        let result = cli().arg("--fixture").arg(&fixture_path).output()?;
        ensure!(
            !result.status.success(),
            "fixture supplied a live preflight"
        );
        ensure!(
            String::from_utf8_lossy(&result.stderr).contains("preflight"),
            "{result:?}"
        );
        ensure!(!status_path.exists());
    }

    let stale = "cccccccccccccccccccccccccccccccccccccccc";
    ensure!(stale != sha);
    fs::write(
        &manifest_path,
        serde_json::to_vec(&json!({
            "repository":"EffortlessMetrics/tokmd-swarm", "source_sha":stale,
            "affected_base_sha":sha, "tag":"v1.15.0", "release_kind":"stable",
            "run_id":1001, "run_attempt":2, "job_id":2002, "artifact_id":3003,
            "artifact_digest":format!("sha256:{}", "0".repeat(64)),
        }))?,
    )?;
    let conflict = cli()
        .arg("--fixture")
        .arg(&fixture_path)
        .arg("--preflight-manifest")
        .arg(&manifest_path)
        .output()?;
    ensure!(!conflict.status.success() && !status_path.exists());
    let message = String::from_utf8(conflict.stderr)?;
    ensure!(
        message.contains("--fixture") && message.contains("--preflight-manifest"),
        "{message}"
    );

    let local = cli()
        .arg("--preflight-manifest")
        .arg(&manifest_path)
        .output()?;
    ensure!(local.status.success(), "preflight inspection: {local:?}");
    let receipt: Value = serde_json::from_slice(&fs::read(&status_path)?)?;
    ensure!(
        receipt.pointer("/preflight/state").and_then(Value::as_str) == Some("unavailable"),
        "{receipt}"
    );
    ensure!(
        receipt
            .pointer("/preflight/detail")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("verified release source/tag")),
        "stale source should be rejected before a hosting client is needed: {receipt}"
    );
    ensure!(receipt.get("complete").and_then(Value::as_bool) == Some(false));

    // The original reader blocked in open before checking the file type.
    fs::remove_file(&status_path)?;
    let fifo = temp.path().join("preflight.fifo");
    ensure!(Command::new("mkfifo").arg(&fifo).status()?.success());
    let mut child = cli().arg("--preflight-manifest").arg(&fifo).spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("release-status blocked on a nonregular manifest");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    ensure!(status.success());
    let receipt: Value = serde_json::from_slice(&fs::read(status_path)?)?;
    ensure!(receipt.pointer("/preflight/state").and_then(Value::as_str) == Some("unavailable"));
    ensure!(
        receipt
            .pointer("/preflight/detail")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("regular manifest file"))
    );
    Ok(())
}
