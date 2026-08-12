use anyhow::{Context, Result, ensure};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().to_path_buf()
}

fn run_xtask(args: &[&str]) -> (String, String, bool) {
    let root = workspace_root();
    let output = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("-p")
        .arg("xtask")
        .arg("--")
        .args(args)
        .current_dir(&root)
        .output()
        .expect("failed to run cargo xtask");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

#[test]
fn affected_help_mentions_base_head_and_json() {
    let (stdout, stderr, success) = run_xtask(&["affected", "--help"]);

    assert!(success, "affected --help failed. stderr: {stderr}");
    assert!(stdout.contains("--base"), "stdout: {stdout}");
    assert!(stdout.contains("--head"), "stdout: {stdout}");
    assert!(stdout.contains("--json"), "stdout: {stdout}");
    assert!(stdout.contains("--json-output"), "stdout: {stdout}");
}

#[test]
fn affected_json_reports_no_changes_for_same_ref() {
    let (stdout, stderr, success) =
        run_xtask(&["affected", "--base", "HEAD", "--head", "HEAD", "--json"]);

    assert!(success, "affected --json failed. stderr: {stderr}");
    let value: serde_json::Value =
        serde_json::from_str(&stdout).expect("affected --json should emit JSON");

    assert_eq!(value["schema"], "tokmd.affected.v1");
    assert_eq!(value["ok"], true);
    assert_eq!(value["base"], "HEAD");
    assert_eq!(value["head"], "HEAD");
    assert!(value["changed_files"].as_array().unwrap().is_empty());
    assert!(value["scopes"].as_array().unwrap().is_empty());
    assert!(value["unknown_files"].as_array().unwrap().is_empty());
}

#[test]
fn affected_json_output_writes_report_artifact() {
    let root = workspace_root();
    let path = root
        .join("target")
        .join("affected-w91")
        .join("affected.json");
    if path.exists() {
        std::fs::remove_file(&path).expect("stale affected fixture should be removable");
    }

    let path_arg = path.to_string_lossy().to_string();
    let (stdout, stderr, success) = run_xtask(&[
        "affected",
        "--base",
        "HEAD",
        "--head",
        "HEAD",
        "--json",
        "--json-output",
        &path_arg,
    ]);

    assert!(success, "affected --json-output failed. stderr: {stderr}");
    assert!(stdout.contains("\"schema\": \"tokmd.affected.v1\""));
    assert!(path.exists(), "affected artifact should be written");

    let written = std::fs::read_to_string(&path).expect("affected artifact should be readable");
    let stdout_json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout affected report should be JSON");
    let written_json: serde_json::Value =
        serde_json::from_str(&written).expect("written affected report should be JSON");

    assert_eq!(written_json["schema"], "tokmd.affected.v1");
    assert_eq!(written_json, stdout_json);
}

#[test]
fn affected_bad_base_reports_git_error() {
    let (_stdout, stderr, success) = run_xtask(&[
        "affected",
        "--base",
        "definitely-not-a-real-ref",
        "--head",
        "HEAD",
        "--json",
    ]);

    assert!(!success, "affected should fail for an invalid base ref");
    assert!(
        stderr.contains("git diff") || stderr.contains("bad revision"),
        "stderr: {stderr}"
    );
}

fn run_git(repo: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn affected_fixture(changed_files: &[&str], policy: &Path) -> Result<serde_json::Value> {
    let temp = tempfile::tempdir().context("create affected fixture repo")?;
    let repo = temp.path();
    run_git(repo, &["init", "-q"])?;
    run_git(repo, &["config", "user.email", "fixture@example.invalid"])?;
    run_git(repo, &["config", "user.name", "Affected Fixture"])?;
    run_git(repo, &["config", "commit.gpgsign", "false"])?;
    run_git(repo, &["config", "tag.gpgsign", "false"])?;

    for path in [
        "AGENTS.md",
        "agents/shared/repo.md",
        "agents/shared/future-guidance.md",
        ".github/workflows/ci.yml",
    ] {
        let target = repo.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create fixture directory {}", parent.display()))?;
        }
        fs::write(&target, "before\n")
            .with_context(|| format!("write fixture file {}", target.display()))?;
    }
    run_git(repo, &["add", "."])?;
    run_git(repo, &["commit", "-q", "-m", "fixture base"])?;

    for path in changed_files {
        let target = repo.join(path);
        fs::write(&target, "after\n")
            .with_context(|| format!("update fixture file {}", target.display()))?;
    }
    run_git(repo, &["add", "."])?;
    run_git(repo, &["commit", "-q", "-m", "fixture head"])?;

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["affected", "--base", "HEAD^", "--head", "HEAD", "--policy"])
        .arg(policy)
        .arg("--json")
        .current_dir(repo)
        .output()
        .context("run affected fixture")?;
    ensure!(
        output.status.success(),
        "affected fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("parse affected fixture JSON")
}

fn scope<'a>(report: &'a serde_json::Value, name: &str) -> Result<&'a serde_json::Value> {
    report
        .get("scopes")
        .and_then(serde_json::Value::as_array)
        .context("affected report scopes should be an array")?
        .iter()
        .find(|candidate| candidate.get("name").and_then(serde_json::Value::as_str) == Some(name))
        .with_context(|| format!("affected report should include {name}"))
}

fn array_len(value: &serde_json::Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .with_context(|| format!("affected report {key} should be an array"))
}

#[test]
fn agent_guidance_scope_reduces_and_deduplicates_commands() -> Result<()> {
    let root = workspace_root();
    let before_policy = root
        .join("fixtures")
        .join("proof-policy")
        .join("agent-guidance-before.toml");
    let after_policy = root.join("ci").join("proof.toml");

    for changed in [
        vec!["AGENTS.md"],
        vec!["agents/shared/repo.md"],
        vec!["agents/shared/future-guidance.md"],
    ] {
        let before = affected_fixture(&changed, &before_policy)?;
        let after = affected_fixture(&changed, &after_policy)?;
        ensure!(array_len(&before, "unknown_files")? == 0);
        ensure!(array_len(&after, "unknown_files")? == 0);
        ensure!(array_len(scope(&before, "proof_control_plane")?, "proof")? == 29);
        ensure!(array_len(scope(&after, "agent_guidance_docs")?, "proof")? == 2);
    }

    let paired = affected_fixture(&["AGENTS.md", "agents/shared/repo.md"], &after_policy)?;
    let guidance = scope(&paired, "agent_guidance_docs")?;
    ensure!(array_len(guidance, "matched_files")? == 2);
    ensure!(array_len(guidance, "proof")? == 2);
    ensure!(array_len(&paired, "scopes")? == 1);
    ensure!(array_len(&paired, "unknown_files")? == 0);

    let workflow = affected_fixture(&[".github/workflows/ci.yml"], &after_policy)?;
    ensure!(array_len(scope(&workflow, "proof_control_plane")?, "proof")? == 29);
    ensure!(array_len(&workflow, "unknown_files")? == 0);
    Ok(())
}
