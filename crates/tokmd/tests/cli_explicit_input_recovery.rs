//! Explicit scan commands select input paths even when a bare name resembles
//! another command. Recovery expectations are literal user-facing contracts,
//! independent of the production error formatter and hint classifier.

use std::path::Path;
use std::process::Command;

const MISSING_INPUT_HINTS: [&str; 2] = [
    "- Verify the input path exists and is readable.",
    "- Use an absolute path to avoid working-directory confusion.",
];
const ROOT_COMMAND_TYPO: &str = "Error: Unrecognized subcommand 'anolyze'\n\nHints:\n- Did you mean the subcommand `analyze`?\n\n";

fn tokmd(dir: &Path, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(dir)
        .env("TOKMD_CONFIG", dir.join("tokmd.toml"))
        .env_remove("TOKMD_PROFILE")
        .args(arguments);
    command
}

fn missing_input_can_be_created_and_retried(
    arguments: &[&str],
    selected_path: &str,
    expected_mode: &str,
    expected_header: &str,
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    let selected = dir.path().join(selected_path);
    anyhow::ensure!(!selected.exists(), "missing input fixture already exists");

    let failure = tokmd(dir.path(), arguments).output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1),
        "missing input must exit 1 for {arguments:?}: {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        failure.stdout.is_empty(),
        "missing input emitted stdout for {arguments:?}"
    );
    anyhow::ensure!(
        stderr.lines().next() == Some(expected_header),
        "wrong selected-input diagnostic for {arguments:?}: {stderr:?}"
    );
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints == MISSING_INPUT_HINTS,
        "wrong selected-input recovery for {arguments:?}: {hints:?}"
    );
    for forbidden in ["Unrecognized subcommand", "Did you mean"] {
        anyhow::ensure!(
            !stderr.contains(forbidden),
            "selected input received command advice {forbidden:?}: {stderr}"
        );
    }

    std::fs::create_dir_all(&selected)?;
    std::fs::write(selected.join("main.rs"), "fn main() {}\n")?;
    let retry = tokmd(dir.path(), arguments).output()?;
    anyhow::ensure!(
        retry.status.success() && retry.stderr.is_empty(),
        "same-argv recovery failed for {arguments:?}: {}: {}",
        retry.status,
        String::from_utf8_lossy(&retry.stderr)
    );
    // Parse all stdout so a receipt mixed with progress or other prose fails.
    let receipt: serde_json::Value = serde_json::from_slice(&retry.stdout)?;
    anyhow::ensure!(receipt["mode"] == expected_mode, "wrong mode: {receipt}");
    anyhow::ensure!(
        receipt["scan"]["paths"] == serde_json::json!([selected_path]),
        "same-argv retry scanned another path: {receipt}"
    );
    let rows = receipt["rows"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("recovered receipt has no rows: {receipt}"))?;
    anyhow::ensure!(rows.len() == 1, "wrong recovered fixture rows: {receipt}");
    anyhow::ensure!(
        rows[0]["lang"] == "Rust" && rows[0]["code"] == 1,
        "recovered receipt did not scan the tiny Rust fixture: {receipt}"
    );
    if expected_mode == "export" {
        anyhow::ensure!(
            rows[0]["path"] == format!("{selected_path}/main.rs"),
            "export did not retain the selected fixture path: {receipt}"
        );
    } else {
        anyhow::ensure!(
            receipt["total"]["files"] == 1 && receipt["total"]["code"] == 1,
            "wrong recovered language totals: {receipt}"
        );
    }
    Ok(())
}

#[test]
fn explicit_lang_missing_bare_path_has_input_recovery() -> anyhow::Result<()> {
    missing_input_can_be_created_and_retried(
        &["lang", "anolyze", "--format", "json"],
        "anolyze",
        "lang",
        "Error: Failed to read command input: Path not found: anolyze",
    )
}

#[test]
fn explicit_export_missing_bare_path_has_input_recovery() -> anyhow::Result<()> {
    missing_input_can_be_created_and_retried(
        &["export", "anolyze", "--format", "json"],
        "anolyze",
        "export",
        "Error: Failed to read command input: Path not found: anolyze",
    )
}

#[test]
fn root_command_typo_retains_analyze_suggestion() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    anyhow::ensure!(
        !dir.path().join("anolyze").exists(),
        "root typo fixture unexpectedly names an existing input"
    );
    let failure = tokmd(dir.path(), &["anolyze"]).output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1) && failure.stdout.is_empty(),
        "root typo must exit 1 with empty stdout: {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        stderr == ROOT_COMMAND_TYPO,
        "root command typo lost its analyze suggestion: {stderr:?}"
    );
    Ok(())
}

#[test]
fn root_path_shaped_missing_input_has_same_argv_recovery() -> anyhow::Result<()> {
    missing_input_can_be_created_and_retried(
        &["missing/root", "--format", "json"],
        "missing/root",
        "lang",
        "Error: Path not found: missing/root",
    )
}
