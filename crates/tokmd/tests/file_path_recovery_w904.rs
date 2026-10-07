#![cfg(feature = "analysis")]

//! Local baseline paths must not be interpreted as upstream network failures.
//! Minimal JSON and empty policy rules isolate file loading and recovery; these
//! tests do not establish full receipt schema validity or ratchet evaluation.

use std::path::Path;
use std::process::Command;

fn gate(dir: &Path, receipt: &Path, policy: &Path, baseline: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(dir)
        .env("TOKMD_CONFIG", dir.join("tokmd.toml"))
        .env_remove("TOKMD_PROFILE")
        .args(["gate", "--format", "json"])
        .arg(receipt)
        .arg("--policy")
        .arg(policy)
        .arg("--baseline")
        .arg(baseline);
    command
}

fn missing_baseline_recovers_without_network_advice(directory_name: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    let receipt = dir.path().join("receipt.json");
    let policy = dir.path().join("policy.toml");
    let parent = dir.path().join(directory_name);
    let baseline = parent.join("baseline.json");
    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    std::fs::write(&policy, "rules = []\n")?;
    anyhow::ensure!(
        !baseline.exists(),
        "baseline failure fixture already exists"
    );

    let failure = gate(dir.path(), &receipt, &policy, &baseline).output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1),
        "expected baseline read failure code 1 for {directory_name}, got {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        failure.stdout.is_empty(),
        "baseline read failure emitted stdout"
    );
    anyhow::ensure!(
        stderr.contains(&format!(
            "Failed to read baseline from {}",
            baseline.display()
        )),
        "missing selected baseline read context: {stderr}"
    );
    for expected in [
        "Verify the input path exists and is readable.",
        "Use an absolute path to avoid working-directory confusion.",
    ] {
        anyhow::ensure!(
            stderr.contains(expected),
            "missing local recovery hint: {stderr}"
        );
    }
    for forbidden in [
        "The upstream service is limiting requests. Wait briefly, then retry.",
        "Honor provider retry windows such as `Retry-After` when available.",
        "Use a smaller input scope if this command contacts a remote service.",
        "This looks transient. Retry with backoff after network or service health recovers.",
        "Check network, VPN, or proxy settings if retries keep failing.",
    ] {
        anyhow::ensure!(
            !stderr.contains(forbidden),
            "local baseline path received network advice {forbidden:?}: {stderr}"
        );
    }

    std::fs::create_dir(&parent)?;
    std::fs::write(&baseline, r#"{"schema_version":2}"#)?;
    let retry = gate(dir.path(), &receipt, &policy, &baseline).output()?;
    anyhow::ensure!(
        retry.status.success(),
        "baseline recovery failed for {directory_name}: {}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&retry.stdout)?;
    anyhow::ensure!(
        result["passed"] == true,
        "recovered gate did not pass: {result}"
    );
    Ok(())
}

#[test]
fn missing_baseline_under_rate_limit_directory_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_baseline_recovers_without_network_advice("rate_limit")
}

#[test]
fn missing_baseline_under_timeout_directory_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_baseline_recovers_without_network_advice("timeout")
}

fn hint_lines(error: &anyhow::Error) -> Vec<String> {
    tokmd::format_error(error)
        .lines()
        .filter(|line| line.starts_with("- "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn localized_missing_read_paths_keep_only_input_recovery() -> anyhow::Result<()> {
    for context in [
        "Failed to read baseline from rate_limit/baseline.json",
        "Failed to read baseline from timeout/baseline.json",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ))
        .context(context);
        let hints = hint_lines(&error);
        anyhow::ensure!(
            hints
                == vec![
                    "- Verify the input path exists and is readable.",
                    "- Use an absolute path to avoid working-directory confusion.",
                ],
            "wrong local read recovery for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn localized_permission_denied_reads_have_no_network_hints() -> anyhow::Result<()> {
    for context in [
        "Failed to read baseline from rate_limit/baseline.json",
        "Failed to read baseline from timeout/baseline.json",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fichier inaccessible",
        ))
        .context(context);
        let hints = hint_lines(&error);
        let expected: Vec<String> = vec![];
        anyhow::ensure!(hints == expected, "wrong denied-read hints: {hints:?}");
    }
    Ok(())
}

#[test]
fn missing_output_path_keeps_only_parent_directory_recovery() -> anyhow::Result<()> {
    let error = anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "fichier introuvable",
    ))
    .context("Failed to write badge to rate_limit/timeout/badge.svg");
    let hints = hint_lines(&error);
    anyhow::ensure!(
        hints == vec!["- Create the parent directory for the output path named above, then retry."],
        "wrong local output recovery: {hints:?}"
    );
    Ok(())
}

#[test]
fn explicit_legacy_missing_paths_keep_existing_local_guidance() -> anyhow::Result<()> {
    for (message, expected) in [
        (
            "Path not found: rate_limit/baseline.json",
            vec![
                "- Verify the input path exists and is readable.",
                "- Use an absolute path to avoid working-directory confusion.",
            ],
        ),
        (
            "Input path does not exist: timeout/baseline.json",
            vec![
                "- Verify the input path exists and is readable.",
                "- Use an absolute path to avoid working-directory confusion.",
            ],
        ),
        (
            "Bounded path not found: rate_limit/timeout/baseline.json",
            vec![
                "- Check the path for the failed operation; for output files, ensure the parent directory exists.",
            ],
        ),
    ] {
        let hints = hint_lines(&anyhow::anyhow!(message));
        anyhow::ensure!(
            hints == expected,
            "wrong legacy guidance for {message}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn genuine_http_429_keeps_all_provider_recovery_hints() -> anyhow::Result<()> {
    let hints = hint_lines(&anyhow::anyhow!(
        "GitHub returned HTTP 429 Too Many Requests"
    ));
    anyhow::ensure!(
        hints
            == vec![
                "- The upstream service is limiting requests. Wait briefly, then retry.",
                "- Honor provider retry windows such as `Retry-After` when available.",
                "- Use a smaller input scope if this command contacts a remote service.",
            ],
        "lost provider recovery: {hints:?}"
    );
    Ok(())
}

#[test]
fn typed_timeout_under_baseline_read_keeps_transient_recovery() -> anyhow::Result<()> {
    let error = anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "request timed out",
    ))
    .context("Failed to read baseline from receipts/baseline.json");
    let hints = hint_lines(&error);
    anyhow::ensure!(
        hints
            == vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        "lost typed timeout recovery under read context: {hints:?}"
    );
    Ok(())
}

#[test]
fn textual_remote_read_and_load_failures_keep_network_recovery() -> anyhow::Result<()> {
    for (message, expected) in [
        (
            "Failed to load remote manifest: HTTP 429",
            vec![
                "- The upstream service is limiting requests. Wait briefly, then retry.",
                "- Honor provider retry windows such as `Retry-After` when available.",
                "- Use a smaller input scope if this command contacts a remote service.",
            ],
        ),
        (
            "Failed to read remote manifest: request timed out",
            vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        ),
    ] {
        let hints = hint_lines(&anyhow::anyhow!(message));
        anyhow::ensure!(
            hints == expected,
            "lost remote recovery for {message}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn localized_open_create_and_resolve_failures_keep_existing_local_guidance() -> anyhow::Result<()> {
    for context in [
        "Failed to open file: rate_limit/timeout/input.rs",
        "Failed to open rate_limit/timeout/input.rs",
        "Failed to create rate_limit/timeout/output.json",
        "Failed to create directory rate_limit/timeout/output",
        "Failed to resolve scan root rate_limit/timeout: fichier inaccessible",
        "Failed to resolve bounded path rate_limit/timeout/input.rs: fichier inaccessible",
    ] {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
        ] {
            let error = anyhow::Error::new(std::io::Error::new(kind, "fichier inaccessible"))
                .context(context);
            let hints = hint_lines(&error);
            let expected = if kind == std::io::ErrorKind::NotFound {
                vec![
                    "- Check the path for the failed operation; for output files, ensure the parent directory exists.",
                ]
            } else {
                vec![]
            };
            anyhow::ensure!(
                hints == expected,
                "wrong local open/create/resolve recovery for {context} ({kind:?}): {hints:?}"
            );
        }
    }
    Ok(())
}

// Explicit check-ignore arguments select input paths, including bare names.
// These journeys cover missing-path recovery and fixture .tokeignore matching.
fn check_ignore(dir: &Path, selected_path: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    command
        .current_dir(dir)
        .env("TOKMD_CONFIG", dir.join("tokmd.toml"))
        .env_remove("TOKMD_PROFILE")
        .args(["check-ignore", selected_path]);
    // The command's Git probes must discover only the fixture's location.
    // Remove Git-local overrides from this child without changing the caller.
    for name in [
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_OBJECT_DIRECTORY",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_IMPLICIT_WORK_TREE",
        "GIT_GRAFT_FILE",
        "GIT_INDEX_FILE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_REPLACE_REF_BASE",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(name);
    }
    command
}

fn missing_check_ignore_path_can_be_created_and_retried(selected_path: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    std::fs::write(dir.path().join(".tokeignore"), format!("{selected_path}\n"))?;
    let target = dir.path().join(selected_path);
    anyhow::ensure!(!target.exists(), "missing-path fixture already exists");

    let failure = check_ignore(dir.path(), selected_path).output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1),
        "expected missing-path runtime error code 1, got {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(failure.stdout.is_empty(), "missing path emitted stdout");
    anyhow::ensure!(
        stderr.contains(&format!("'{selected_path}'")),
        "missing selected-path diagnostic: {stderr}"
    );
    anyhow::ensure!(
        stderr.contains("does not exist"),
        "missing path-absence diagnostic: {stderr}"
    );
    for forbidden in [
        "The upstream service is limiting requests. Wait briefly, then retry.",
        "Honor provider retry windows such as `Retry-After` when available.",
        "Use a smaller input scope if this command contacts a remote service.",
        "This looks transient. Retry with backoff after network or service health recovers.",
        "Check network, VPN, or proxy settings if retries keep failing.",
        "Unrecognized subcommand",
        "Did you mean the subcommand",
        "Run `tokmd --help` to see a list of available subcommands.",
    ] {
        anyhow::ensure!(
            !stderr.contains(forbidden),
            "explicit check-ignore path received unrelated advice {forbidden:?}: {stderr}"
        );
    }
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints
            == vec![
                "- Verify the input path exists and is readable.",
                "- Use an absolute path to avoid working-directory confusion.",
            ],
        "wrong explicit-path recovery: {hints:?}"
    );

    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("owned check-ignore target must have a parent directory"))?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&target, "fn fixture() {}\n")?;
    // Construct a fresh process with identical arguments after creating the
    // selected file. Default nonverbose output proves the ignored result.
    let retry = check_ignore(dir.path(), selected_path).output()?;
    anyhow::ensure!(
        retry.status.code() == Some(0),
        "created-path retry failed: {}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let stdout = std::str::from_utf8(&retry.stdout)?;
    anyhow::ensure!(
        stdout == format!("{selected_path}: ignored\n"),
        "wrong nonverbose ignored result: {stdout:?}"
    );
    anyhow::ensure!(retry.stderr.is_empty(), "ignored retry emitted stderr");
    Ok(())
}

#[test]
fn missing_check_ignore_rate_limit_path_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_check_ignore_path_can_be_created_and_retried("rate_limit/missing.rs")
}

#[test]
fn missing_check_ignore_timeout_path_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_check_ignore_path_can_be_created_and_retried("timeout/missing.rs")
}

#[test]
fn missing_check_ignore_bare_typo_path_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_check_ignore_path_can_be_created_and_retried("anolyze")
}

#[test]
fn localized_missing_check_ignore_access_keeps_only_input_recovery() -> anyhow::Result<()> {
    for context in [
        "failed to access path 'rate_limit/missing.rs'",
        "failed to access path 'timeout/missing.rs'",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "fichier introuvable",
        ))
        .context(context);
        let hints = hint_lines(&error);
        anyhow::ensure!(
            hints
                == vec![
                    "- Verify the input path exists and is readable.",
                    "- Use an absolute path to avoid working-directory confusion.",
                ],
            "wrong missing access recovery for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn localized_denied_check_ignore_access_has_no_network_recovery() -> anyhow::Result<()> {
    for context in [
        "failed to access path 'rate_limit/missing.rs'",
        "failed to access path 'timeout/missing.rs'",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "fichier inaccessible",
        ))
        .context(context);
        let hints = hint_lines(&error);
        let expected: Vec<String> = vec![];
        anyhow::ensure!(
            hints == expected,
            "wrong denied access recovery for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn genuine_typed_timeout_under_check_ignore_access_keeps_transient_recovery() -> anyhow::Result<()>
{
    let error = anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "request timed out",
    ))
    .context("failed to access path 'timeout/missing.rs'");
    let hints = hint_lines(&error);
    anyhow::ensure!(
        hints
            == vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        "lost genuine access timeout recovery: {hints:?}"
    );
    Ok(())
}

#[test]
fn missing_check_ignore_base_ref_path_can_be_created_and_retried() -> anyhow::Result<()> {
    missing_check_ignore_path_can_be_created_and_retried("base ref/missing.rs")
}

#[test]
fn genuine_missing_git_base_ref_keeps_ref_recovery() -> anyhow::Result<()> {
    let error = anyhow::anyhow!("base ref 'origin/missing' not found and no fallback resolved");
    let hints = hint_lines(&error);
    anyhow::ensure!(
        hints
            == vec![
                "- Fetch refs (`git fetch --tags --prune`) and retry with `--base <ref>`.",
                "- You can also set `TOKMD_GIT_BASE_REF` to a valid default base ref.",
            ],
        "lost genuine Git-base recovery: {hints:?}"
    );
    Ok(())
}

#[test]
fn localized_stable_access_base_ref_paths_keep_only_local_recovery() -> anyhow::Result<()> {
    // Both phrase triggers live in the filename, so localization of the IO
    // cause cannot hide a false Git-base classification.
    for kind in [
        std::io::ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied,
    ] {
        let error = anyhow::Error::new(std::io::Error::new(kind, "fichier inaccessible"))
            .context("failed to access path 'base ref/not found.rs'");
        let hints = hint_lines(&error);
        let expected = if kind == std::io::ErrorKind::NotFound {
            vec![
                "- Verify the input path exists and is readable.",
                "- Use an absolute path to avoid working-directory confusion.",
            ]
        } else {
            vec![]
        };
        anyhow::ensure!(
            hints == expected,
            "wrong localized local base-ref recovery for {kind:?}: {hints:?}"
        );
    }
    Ok(())
}

fn context_bundle_command(dir: &Path, selected_root: &str, mode: &str) -> Command {
    // Reuse only the fixture-local environment from the existing helper; its
    // check-ignore argv is not copied to this separate context command.
    let fixture_command = check_ignore(dir, selected_root);
    let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
    for (name, value) in fixture_command.get_envs() {
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
    }
    command
        .current_dir(dir)
        .args([
            "--no-progress",
            "context",
            "--mode",
            mode,
            "--no-git",
            "--budget",
            "1000",
            "--max-file-tokens",
            "40",
            "--no-smart-exclude",
        ])
        .arg(selected_root);
    command
}

fn context_head_tail_fixture() -> String {
    (1..=20)
        .map(|line| format!("pub fn line_{line:02}() {{}}\n"))
        .collect()
}

fn prove_context_head_tail_selection(dir: &Path, selected_root: &str) -> anyhow::Result<()> {
    let output = context_bundle_command(dir, selected_root, "json").output()?;
    anyhow::ensure!(
        output.status.code() == Some(0) && output.stderr.is_empty(),
        "context selection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let files = receipt
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("context receipt must have files: {receipt}"))?;
    anyhow::ensure!(
        files.len() == 1,
        "expected one selected context file: {receipt}"
    );
    let file = files
        .first()
        .ok_or_else(|| anyhow::anyhow!("selected file must exist"))?;
    let expected_path = format!("{selected_root}/input.rs");
    anyhow::ensure!(
        file.get("path").and_then(serde_json::Value::as_str) == Some(expected_path.as_str()),
        "wrong selected context path: {file}"
    );
    anyhow::ensure!(
        file.get("policy").and_then(serde_json::Value::as_str) == Some("head_tail")
            && file.get("lines").and_then(serde_json::Value::as_u64) == Some(20)
            && file.get("tokens").and_then(serde_json::Value::as_u64) == Some(200)
            && file
                .get("effective_tokens")
                .and_then(serde_json::Value::as_u64)
                == Some(40),
        "fixture must select twenty short lines at 200 tokens with a 40-token head-tail cap: {file}"
    );
    Ok(())
}

fn missing_context_root_can_be_created_and_bundled(selected_root: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    let root = dir.path().join(selected_root);
    anyhow::ensure!(!root.exists(), "missing context root already exists");
    let failure = context_bundle_command(dir.path(), selected_root, "bundle").output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1) && failure.stdout.is_empty(),
        "missing-root bundle must fail with code1 and empty stdout: {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        stderr.contains(&format!("Path not found: {selected_root}")),
        "missing selected scan-root diagnostic: {stderr}"
    );
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints
            == vec![
                "- Verify the input path exists and is readable.",
                "- Use an absolute path to avoid working-directory confusion.",
            ],
        "wrong missing scan-root recovery: {hints:?}"
    );
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("input.rs"), context_head_tail_fixture())?;
    // This is a scan-root recovery control, not a missing-renderer regression.
    // Prove the actual selection before retrying the original bundle argv.
    prove_context_head_tail_selection(dir.path(), selected_root)?;
    let retry = context_bundle_command(dir.path(), selected_root, "bundle").output()?;
    anyhow::ensure!(
        retry.status.code() == Some(0) && retry.stderr.is_empty(),
        "recovered context bundle failed: {}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let stdout = std::str::from_utf8(&retry.stdout)?;
    let expected = format!(
        "// === {selected_root}/input.rs ===\npub fn line_01() {{}}\npub fn line_02() {{}}\npub fn line_03() {{}}\n// ... [16 lines omitted] ...\npub fn line_20() {{}}\n\n"
    );
    anyhow::ensure!(
        stdout == expected,
        "wrong recovered head-tail bundle: {stdout:?}"
    );
    Ok(())
}

#[test]
fn missing_context_rate_limit_root_can_be_created_and_bundled() -> anyhow::Result<()> {
    missing_context_root_can_be_created_and_bundled("rate_limit/missing_root")
}

#[test]
fn missing_context_timeout_root_can_be_created_and_bundled() -> anyhow::Result<()> {
    missing_context_root_can_be_created_and_bundled("timeout/missing_root")
}

#[test]
fn missing_context_base_ref_root_can_be_created_and_bundled() -> anyhow::Result<()> {
    missing_context_root_can_be_created_and_bundled("base ref/missing_root")
}

// Baseline decoding fails before JSON parsing. Empty policy rules and minimal
// JSON isolate decoding, parser acceptance, and recovery; they do not establish
// receipt schema validity or exercise ratchet evaluation.
fn gate_with_fixture_environment(
    dir: &Path,
    receipt: &Path,
    policy: &Path,
    baseline: &Path,
) -> Command {
    let fixture_command = check_ignore(dir, "environment-probe.rs");
    let mut command = gate(dir, receipt, policy, baseline);
    for (name, value) in fixture_command.get_envs() {
        match value {
            Some(value) => {
                command.env(name, value);
            }
            None => {
                command.env_remove(name);
            }
        }
    }
    command
}

fn invalid_utf8_baseline_can_be_rewritten_and_retried(selected_path: &str) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("tokmd.toml"), "")?;
    let receipt = dir.path().join("receipt.json");
    let policy = dir.path().join("policy.toml");
    let baseline = dir.path().join(selected_path);
    let parent = baseline
        .parent()
        .ok_or_else(|| anyhow::anyhow!("owned baseline fixture must have a parent directory"))?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(&receipt, r#"{"schema_version":2}"#)?;
    std::fs::write(&policy, "rules = []\n")?;
    std::fs::write(&baseline, [0xff_u8])?;
    anyhow::ensure!(baseline.is_file(), "corrupt baseline fixture is missing");

    let failure = gate_with_fixture_environment(dir.path(), &receipt, &policy, &baseline).output()?;
    let stderr = std::str::from_utf8(&failure.stderr)?;
    anyhow::ensure!(
        failure.status.code() == Some(1),
        "expected baseline decoding error code 1, got {}: {stderr}",
        failure.status
    );
    anyhow::ensure!(
        failure.stdout.is_empty(),
        "baseline decoding failure emitted stdout"
    );
    anyhow::ensure!(
        stderr.contains(&format!(
            "Failed to read baseline from {}",
            baseline.display()
        )),
        "missing selected baseline read context: {stderr}"
    );
    anyhow::ensure!(
        !stderr.contains("Failed to parse baseline JSON"),
        "invalid UTF-8 reached JSON parser recovery: {stderr}"
    );
    let hints = stderr
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        hints == vec!["- Save the baseline file named above as valid UTF-8 text, then retry."],
        "wrong baseline encoding recovery for {selected_path}: {hints:?}"
    );

    // Rewrite the same selected file and create a fresh child with identical
    // arguments. No selector or policy change can account for recovery.
    std::fs::write(&baseline, r#"{"schema_version":2}"#)?;
    let retry = gate_with_fixture_environment(dir.path(), &receipt, &policy, &baseline).output()?;
    anyhow::ensure!(
        retry.status.code() == Some(0),
        "rewritten-baseline retry failed: {}",
        String::from_utf8_lossy(&retry.stderr)
    );
    anyhow::ensure!(retry.stderr.is_empty(), "baseline retry emitted stderr");
    let result: serde_json::Value = serde_json::from_slice(&retry.stdout)?;
    anyhow::ensure!(
        result.get("passed").and_then(serde_json::Value::as_bool) == Some(true),
        "recovered gate did not pass: {result}"
    );
    Ok(())
}

#[test]
fn invalid_utf8_baseline_rate_limit_file_can_be_rewritten_and_retried() -> anyhow::Result<()> {
    invalid_utf8_baseline_can_be_rewritten_and_retried("rate_limit/baseline.json")
}

#[test]
fn invalid_utf8_baseline_timeout_file_can_be_rewritten_and_retried() -> anyhow::Result<()> {
    invalid_utf8_baseline_can_be_rewritten_and_retried("timeout/baseline.json")
}

#[test]
fn invalid_utf8_baseline_base_ref_file_can_be_rewritten_and_retried() -> anyhow::Result<()> {
    invalid_utf8_baseline_can_be_rewritten_and_retried("base ref/not found.json")
}

#[test]
fn localized_invalid_data_baseline_reads_keep_only_encoding_recovery() -> anyhow::Result<()> {
    for context in [
        "Failed to read baseline from rate_limit/baseline.json",
        "Failed to read baseline from timeout/baseline.json",
        "Failed to read baseline from base ref/not found.json",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fichier inaccessible",
        ))
        .context(context);
        let hints = hint_lines(&error);
        anyhow::ensure!(
            hints == vec!["- Save the baseline file named above as valid UTF-8 text, then retry."],
            "wrong localized baseline encoding recovery for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn invalid_data_outside_baseline_reads_keeps_http_recovery() -> anyhow::Result<()> {
    for context in [
        "Failed to load remote manifest: HTTP 429",
        "Remote manifest: Failed to read baseline from cache.json: HTTP 429",
        "Failed to read remote manifest: HTTP 429 (Failed to read baseline from cache.json)",
        "Failed to read baseline from: remote manifest HTTP 429",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fichier inaccessible",
        ))
        .context(context);
        let hints = hint_lines(&error);
        anyhow::ensure!(
            hints
                == vec![
                    "- The upstream service is limiting requests. Wait briefly, then retry.",
                    "- Honor provider retry windows such as `Retry-After` when available.",
                    "- Use a smaller input scope if this command contacts a remote service.",
                ],
            "lost remote recovery or gained baseline advice for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn baseline_encoding_recovery_requires_invalid_data_cause() -> anyhow::Result<()> {
    let untyped = anyhow::anyhow!("Failed to read baseline from baseline.json: fichier inaccessible");
    let hints = hint_lines(&untyped);
    anyhow::ensure!(
        hints.is_empty(),
        "untyped baseline read gained encoding advice: {hints:?}"
    );

    let timeout = anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "request timed out",
    ))
    .context("Failed to read baseline from receipts/baseline.json");
    let hints = hint_lines(&timeout);
    anyhow::ensure!(
        hints
            == vec![
                "- This looks transient. Retry with backoff after network or service health recovers.",
                "- Check network, VPN, or proxy settings if retries keep failing.",
            ],
        "lost typed timeout recovery or gained encoding advice: {hints:?}"
    );
    Ok(())
}

#[test]
fn invalid_data_toml_parse_cause_keeps_syntax_recovery() -> anyhow::Result<()> {
    let parse_error = match tokmd_settings::TomlConfig::parse("broken = [") {
        Ok(_) => anyhow::bail!("invalid TOML control unexpectedly parsed"),
        Err(error) => error,
    };
    // Match TomlConfig::from_file's actual error wrapping without filesystem
    // dependence. InvalidData alone does not mean a baseline encoding error.
    let error = anyhow::Error::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        parse_error,
    ))
    .context("Failed to load TOML config from tokmd.toml");
    let hints = hint_lines(&error);
    anyhow::ensure!(
        hints == vec!["- Check TOML syntax and key names in the file named above, then retry."],
        "lost TOML syntax recovery or gained baseline encoding advice: {hints:?}"
    );
    Ok(())
}

#[test]
fn invalid_utf8_baseline_git_keyword_files_can_be_rewritten_and_retried() -> anyhow::Result<()> {
    for selected_path in [
        "not inside a git repository/baseline.json",
        "git is not available on PATH/baseline.json",
        "requires the 'git' feature/baseline.json",
    ] {
        invalid_utf8_baseline_can_be_rewritten_and_retried(selected_path)?;
    }
    Ok(())
}

#[test]
fn localized_invalid_data_baseline_git_paths_keep_only_encoding_recovery() -> anyhow::Result<()> {
    for context in [
        "Failed to read baseline from git is not available on PATH/baseline.json",
        "Failed to read baseline from requires the 'git' feature/baseline.json",
        "Failed to read baseline from not inside a git repository/baseline.json",
    ] {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fichier inaccessible",
        ))
        .context(context);
        let hints = hint_lines(&error);
        anyhow::ensure!(
            hints == vec!["- Save the baseline file named above as valid UTF-8 text, then retry."],
            "baseline filename gained Git recovery for {context}: {hints:?}"
        );
    }
    Ok(())
}

#[test]
fn genuine_git_failures_keep_existing_recovery() -> anyhow::Result<()> {
    for (message, expected) in [
        (
            "git is not available on PATH",
            vec![
                "- Install git and verify it with `git --version`.",
                "- If git metrics are optional, disable them with `--no-git`.",
            ],
        ),
        (
            "operation requires the 'git' feature",
            vec![
                "- Install git and verify it with `git --version`.",
                "- If git metrics are optional, disable them with `--no-git`.",
            ],
        ),
        (
            "not inside a git repository",
            vec![
                "- Run the command from a git repository, or disable git-dependent behavior.",
                "- Initialize git first if needed: `git init`.",
            ],
        ),
    ] {
        let hints = hint_lines(&anyhow::anyhow!(message));
        anyhow::ensure!(
            hints == expected,
            "lost genuine Git recovery for {message}: {hints:?}"
        );
    }
    Ok(())
}
