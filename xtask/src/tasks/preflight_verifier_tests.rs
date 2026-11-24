//! Offline provenance checks. No release-status subprocess receives simulated GitHub evidence.

use super::{Manifest, ReleaseState, check_artifact, check_digest, check_run, preflight_archive};
use crate::{cli::ReleasePreflightArgs, tasks::release_preflight};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write},
};
use zip::{ZipWriter, write::SimpleFileOptions};

const REPO: &str = "EffortlessMetrics/tokmd-swarm";
const REPO_ID: u64 = 1243934419;
const VERSION: &str = "1.15.0";
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORKFLOW_HEAD: &str = "dddddddddddddddddddddddddddddddddddddddd";
const REQUIRED: &[&str] = &[
    "identity_check",
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
    "browser_tests",
    "browser_wasm_archive",
];
// Independent SHA-256 known-answer vector for the three bytes "abc".
const DIGEST: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn manifest() -> Result<Manifest> {
    Ok(serde_json::from_value(json!({
        "repository": REPO, "source_sha": SHA, "affected_base_sha": BASE, "tag": "v1.15.0",
        "release_kind": "stable", "run_id": 1001, "run_attempt": 2, "job_id": 2002,
        "artifact_id": 3003, "artifact_digest": DIGEST,
    }))?)
}

#[test]
fn exact_attempt_and_artifact_repository_are_checked_independently() -> Result<()> {
    let manifest = manifest()?;
    let repo = json!({"id": REPO_ID, "full_name": REPO});
    let mut run = json!({
        "id": 1001, "run_attempt": 2, "repository": repo, "head_repository": repo,
        "path": ".github/workflows/release-preflight.yml", "event": "workflow_dispatch",
        "status": "completed", "conclusion": "success", "head_sha": WORKFLOW_HEAD,
        "display_title": format!("Release preflight {SHA} (stable)")
    });
    check_run(&run, &manifest)?;
    let mut different_dispatch = run.clone();
    *different_dispatch
        .get_mut("display_title")
        .context("fixture display_title")? = json!(format!("Release preflight {BASE} (stable)"));
    ensure!(
        check_run(&different_dispatch, &manifest).is_err(),
        "workflow dispatch approved a different source"
    );
    *run.get_mut("run_attempt").context("fixture run_attempt")? = json!(1);
    ensure!(
        check_run(&run, &manifest).is_err(),
        "wrong attempt accepted"
    );
    let mut artifact = json!({
        "id": 3003, "name": format!("release-preflight-{SHA}"), "expired": false,
        "digest": DIGEST, "workflow_run": {
            "id": 1001, "repository_id": REPO_ID, "head_repository_id": REPO_ID, "head_sha": WORKFLOW_HEAD
        }
    });
    check_artifact(&artifact, &manifest, REPO_ID, WORKFLOW_HEAD)?;
    *artifact
        .pointer_mut("/workflow_run/repository_id")
        .context("fixture artifact owner")? = json!(1);
    ensure!(
        check_artifact(&artifact, &manifest, REPO_ID, WORKFLOW_HEAD).is_err(),
        "foreign artifact accepted"
    );
    Ok(())
}

#[test]
fn changed_download_bytes_reject_original_digest() -> Result<()> {
    check_digest(b"abc", DIGEST)?;
    ensure!(
        check_digest(b"abd", DIGEST).is_err(),
        "changed bytes accepted"
    );
    Ok(())
}

struct Contents(BTreeMap<String, Vec<u8>>);

impl Contents {
    fn produced(sha: &str, skip: Option<&str>) -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let input = temp.path().join("input.json");
        let output = temp.path().join("receipt.json");
        let mut members = BTreeMap::new();
        let mut commands = Vec::new();
        for &id in REQUIRED {
            let log = format!("logs/{id}.log");
            members.insert(log.clone(), b"offline validator fixture\n".to_vec());
            if Some(id) != skip {
                commands.push(json!({"id":id,"log":log,"status":"passed","duration_ms":1}));
            }
        }
        let bytes = serde_json::to_vec(&json!({
            "schema": "tokmd.release_preflight_input.v2", "schema_version": 2,
            "source_sha": sha, "affected_base_sha": BASE, "resolved_source_sha": sha,
            "resolved_affected_base_sha": BASE, "source_object_type": "commit",
            "affected_base_object_type": "commit", "expected_version": VERSION,
            "release_kind": "stable", "commands": commands
        }))?;
        fs::write(&input, &bytes)?;
        let result = release_preflight::run(ReleasePreflightArgs {
            input,
            output: output.clone(),
        });
        ensure!(
            result.is_ok() == skip.is_none(),
            "unexpected producer result: {result:?}"
        );
        members.insert("input.json".into(), bytes);
        members.insert("receipt.json".into(), fs::read(output)?);
        Ok(Self(members))
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in &self.0 {
            archive.start_file(name.as_str(), SimpleFileOptions::default())?;
            archive.write_all(bytes)?;
        }
        Ok(archive.finish()?.into_inner())
    }
}

#[test]
fn archive_requires_the_requested_source_and_each_command_log() -> Result<()> {
    let manifest = manifest()?;
    let mut contents = Contents::produced(SHA, None)?;
    let fact = preflight_archive::verify_archive(contents.bytes()?, &manifest, VERSION)?;
    ensure!(
        fact.state == ReleaseState::Passed,
        "valid offline archive rejected"
    );
    contents
        .0
        .remove("logs/cargo_deny.log")
        .context("fixture missing baseline log")?;
    let missing = preflight_archive::verify_archive(contents.bytes()?, &manifest, VERSION);
    ensure!(missing.is_err(), "absent command log accepted");

    let stale = Contents::produced(BASE, None)?;
    ensure!(
        preflight_archive::verify_archive(stale.bytes()?, &manifest, VERSION).is_err(),
        "different source accepted"
    );
    Ok(())
}

#[test]
fn missing_required_command_cannot_be_overridden_by_a_passed_summary() -> Result<()> {
    let manifest = manifest()?;
    let mut contents = Contents::produced(SHA, Some("cargo_deny"))?;
    let fact = preflight_archive::verify_archive(contents.bytes()?, &manifest, VERSION)?;
    ensure!(
        fact.state == ReleaseState::NotRun,
        "missing command must be not_run"
    );
    let existing = contents
        .0
        .get_mut("receipt.json")
        .context("fixture receipt")?;
    let mut receipt: Value = serde_json::from_slice(existing)?;
    *receipt.get_mut("overall").context("fixture overall")? = json!("passed");
    *existing = serde_json::to_vec(&receipt)?;
    ensure!(
        preflight_archive::verify_archive(contents.bytes()?, &manifest, VERSION).is_err(),
        "contradictory aggregate accepted"
    );
    Ok(())
}

#[test]
fn artifact_identity_must_appear_in_the_attempts_upload_step() -> Result<()> {
    let manifest = manifest()?;
    let stamp = "2026-11-24T09:39:49.1234567Z";
    let record = format!(
        "{stamp} SHA256 digest of uploaded artifact is {}\n{stamp} Artifact release-preflight-{} successfully finalized. Artifact ID {}\n",
        DIGEST.strip_prefix("sha256:").context("fixture digest")?,
        SHA,
        manifest.artifact_id
    );
    let path = format!("{}/12_upload.txt", super::JOB);
    let mut contents = Contents(BTreeMap::from([(path.clone(), record.as_bytes().to_vec())]));
    super::check_upload_log(contents.bytes()?, 12, &manifest)?;
    ensure!(super::check_upload_log(contents.bytes()?, 11, &manifest).is_err());
    contents.0.insert(
        format!("{}/12_second.txt", super::JOB),
        record.as_bytes().to_vec(),
    );
    ensure!(super::check_upload_log(contents.bytes()?, 12, &manifest).is_err());
    let wrong_id = record.replace("Artifact ID 3003", "Artifact ID 9001");
    let contents = Contents(BTreeMap::from([(path, wrong_id.into_bytes())]));
    ensure!(super::check_upload_log(contents.bytes()?, 12, &manifest).is_err());
    Ok(())
}
