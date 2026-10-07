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
