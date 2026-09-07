use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};

fn workflow() -> Result<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask manifest has no workspace parent")?
        .to_path_buf();
    fs::read_to_string(root.join(".github/workflows/release-preflight.yml"))
        .context("read release-preflight workflow")
}

fn step_timeout(text: &str, step_name: &str) -> Result<u16> {
    let marker = format!("- name: {step_name}");
    let tail = text
        .split_once(&marker)
        .with_context(|| format!("step `{step_name}` missing"))?
        .1;
    let value = tail
        .lines()
        .take(8)
        .find_map(|line| line.trim().strip_prefix("timeout-minutes: "))
        .with_context(|| format!("step `{step_name}` has no nearby timeout"))?;
    value.parse().context("parse step timeout")
}

fn env_seconds(text: &str, name: &str) -> Result<u16> {
    let prefix = format!("{name}: ");
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .with_context(|| format!("environment value `{name}` missing"))?;
    value.parse().context("parse environment seconds")
}

fn job_timeout(text: &str, job_name: &str) -> Result<u16> {
    let marker = format!("name: {job_name}");
    let tail = text
        .split_once(&marker)
        .with_context(|| format!("job `{job_name}` missing"))?
        .1;
    let value = tail
        .lines()
        .take(5)
        .find_map(|line| line.trim().strip_prefix("timeout-minutes: "))
        .with_context(|| format!("job `{job_name}` has no nearby timeout"))?;
    value.parse().context("parse job timeout")
}

#[test]
fn release_preflight_reserves_finalization_and_uploads_failures() -> Result<()> {
    let text = workflow()?;
    let job_minutes = job_timeout(&text, "Exact-source release preflight")?;
    ensure!(text.contains("- name: Materialize terminal preflight input\n        if: always()"));
    ensure!(
        text.contains("- name: Aggregate terminal release-preflight receipt\n        if: always()")
    );
    ensure!(
        text.contains("- name: Upload terminal release-preflight evidence\n        if: always()")
    );
    ensure!(text.contains("if-no-files-found: error"));
    let upload = text
        .find("- name: Upload terminal release-preflight evidence")
        .context("upload step missing")?;
    let enforce = text
        .find("- name: Enforce terminal release-preflight decision")
        .context("enforcement step missing")?;
    ensure!(upload < enforce);
    let maximum_setup_minutes = [
        "Check out exact source SHA",
        "Verify immutable source and affected base",
        "Set up Rust",
        "Set up Node.js",
        "Install wasm-pack",
        "Install cargo-deny",
        "Cache Rust build artifacts (best effort)",
        "Record cache outcome",
    ]
    .into_iter()
    .map(|name| step_timeout(&text, name))
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .sum::<u16>();
    let proof_minutes = env_seconds(&text, "PROOF_BUDGET_SECONDS")? / 60;
    let proof_step_minutes = step_timeout(
        &text,
        "Run required commands and record terminal observations",
    )?;
    let finalization_minutes = [
        "Materialize terminal preflight input",
        "Aggregate terminal release-preflight receipt",
        "Write preflight summary",
        "Upload terminal release-preflight evidence",
        "Enforce terminal release-preflight decision",
    ]
    .into_iter()
    .map(|name| step_timeout(&text, name))
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .sum::<u16>();
    ensure!(proof_minutes < proof_step_minutes);
    ensure!(maximum_setup_minutes + proof_step_minutes + finalization_minutes < job_minutes);
    ensure!(text.contains(": > \"${PREFLIGHT_ROOT}/logs/${id}.log\""));
    Ok(())
}

#[test]
fn release_preflight_binds_cache_and_commit_identity() -> Result<()> {
    let text = workflow()?;
    ensure!(text.contains(
        "run-name: Release preflight ${{ inputs.source_sha }} (${{ inputs.release_kind }})"
    ));
    ensure!(text.contains("CARGO_TARGET_DIR: target/release-preflight-cache"));
    ensure!(text.contains("workspaces: . -> target/release-preflight-cache"));
    ensure!(text.matches("git cat-file -t").count() >= 2);
    ensure!(text.contains("affected base must be an ancestor of source"));
    ensure!(text.contains("release_kind must be rc or stable"));
    ensure!(text.contains("^([0-9a-f]{40}|[0-9a-f]{64})$"));
    ensure!(text.contains("identity_check"));
    Ok(())
}

#[test]
fn release_preflight_records_every_canonical_command_once() -> Result<()> {
    let text = workflow()?;
    for id in [
        "affected_plan",
        "proof_plan",
        "fmt_check",
        "gate_check",
        "version_consistency",
        "publish_surface",
        "doc_artifacts",
        "docs_check",
        "proof_policy",
        "no_panic",
        "workspace_tests",
        "clippy",
        "cargo_deny",
        "publish_dry_run",
        "browser_wasm_archive",
        "browser_tests",
    ] {
        ensure!(
            text.matches(&format!("run_command {id} ")).count() == 1,
            "workflow command `{id}` is missing or duplicated"
        );
    }
    Ok(())
}

#[test]
fn release_preflight_maps_timeouts_and_cancellation_to_non_passing() -> Result<()> {
    let text = workflow()?;
    ensure!(text.contains("--kill-after=30s"));
    ensure!(text.contains(r#""${exit_code}" -eq 124"#));
    ensure!(text.contains("status=unavailable"));
    ensure!(text.contains("status=cancelled"));
    ensure!(text.contains("global proof budget exhausted"));
    ensure!(text.contains("identity check did not pass"));
    Ok(())
}
