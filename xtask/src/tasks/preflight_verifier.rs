//! Bind downloaded preflight evidence to a GitHub Actions dispatch and a verified tag.

use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::Value;

use super::{ReleaseState, StateFact, state_fact};

#[path = "preflight_github.rs"]
mod github;
#[path = "preflight_archive.rs"]
mod preflight_archive;

const WORKFLOW: &str = ".github/workflows/release-preflight.yml";
const WORKFLOW_BYTES: &[u8] = include_bytes!("../../../.github/workflows/release-preflight.yml");
const JOB: &str = "Exact-source release preflight";
const UPLOAD: &str = "Upload terminal release-preflight evidence";
const ENFORCE: &str = "Enforce terminal release-preflight decision";
const MAX_ARCHIVE: usize = 64 << 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    repository: String,
    tag: String,
    source_sha: String,
    affected_base_sha: String,
    release_kind: String,
    run_id: u64,
    run_attempt: u64,
    job_id: u64,
    artifact_id: u64,
    artifact_digest: String,
}

fn lower_hex(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn commit_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && lower_hex(value)
}

impl Manifest {
    fn read(path: &Path, tag: &str, sha: &str, version: &str) -> Result<Self> {
        ensure!(
            std::fs::metadata(path)?.is_file(),
            "expected a regular manifest file"
        );
        let mut options = OpenOptions::new();
        options.read(true);
        // A concurrent replacement with a FIFO must not stall the subsequent descriptor check.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let f = options.open(path).context("open preflight expectations")?;
        ensure!(f.metadata()?.is_file(), "expected a regular manifest file");
        let mut bytes = Vec::new();
        f.take(16385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16384, "oversized preflight manifest");
        let m: Self = serde_json::from_slice(&bytes).context("parse preflight expectations")?;
        ensure!(
            matches!(
                m.repository.as_str(),
                "EffortlessMetrics/tokmd-swarm" | "EffortlessMetrics/tokmd"
            ),
            "unsupported release repository"
        );
        ensure!(
            commit_id(&m.source_sha) && commit_id(&m.affected_base_sha),
            "invalid source or affected-base commit"
        );
        ensure!(
            m.tag == tag && m.source_sha == sha && m.tag.strip_prefix('v') == Some(version),
            "manifest does not identify the verified release source/tag"
        );
        ensure!(
            matches!(m.release_kind.as_str(), "stable" | "rc"),
            "unsupported release kind"
        );
        ensure!(
            [m.run_id, m.run_attempt, m.job_id, m.artifact_id]
                .iter()
                .all(|n| *n > 0),
            "missing Actions identifier"
        );
        let digest = m
            .artifact_digest
            .strip_prefix("sha256:")
            .context("expected a SHA-256 digest")?;
        ensure!(
            digest.len() == 64 && lower_hex(digest),
            "invalid SHA-256 digest"
        );
        Ok(m)
    }
}

pub(super) fn verify(path: &Path, tag: &str, source: &str, version: &str) -> StateFact {
    match Manifest::read(path, tag, source, version).and_then(|m| observe(&m, version)) {
        Ok(fact) => fact,
        Err(error) => state_fact(
            ReleaseState::Unavailable,
            &format!("Preflight evidence unavailable: {error:#}"),
            None,
        ),
    }
}

// Missing or ill-typed API fields fail the matching predicate.
fn field<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}

fn check_run(run: &Value, m: &Manifest) -> Result<()> {
    ensure!(
        field(run, "/id").as_u64() == Some(m.run_id)
            && field(run, "/run_attempt").as_u64() == Some(m.run_attempt),
        "wrong workflow run or attempt"
    );
    ensure!(
        field(run, "/path").as_str() == Some(WORKFLOW)
            && field(run, "/event") == "workflow_dispatch",
        "unsupported producer workflow or trigger"
    );
    ensure!(
        field(run, "/repository/full_name") == m.repository.as_str()
            && field(run, "/head_repository/full_name") == m.repository.as_str(),
        "foreign workflow repository"
    );
    let repo = field(run, "/repository/id")
        .as_u64()
        .context("missing repository ID")?;
    ensure!(
        repo > 0 && field(run, "/head_repository/id").as_u64() == Some(repo),
        "foreign workflow source"
    );
    ensure!(
        field(run, "/head_sha").as_str().is_some_and(commit_id),
        "missing immutable workflow revision"
    );
    // This provider-generated name comes from the dispatch inputs before checkout.
    ensure!(
        field(run, "/display_title")
            == format!("Release preflight {} ({})", m.source_sha, m.release_kind).as_str(),
        "dispatch source or kind differs from requested release"
    );
    Ok(())
}

fn check_job<'a>(list: &'a Value, m: &Manifest) -> Result<(&'a Value, u64)> {
    let jobs = field(list, "/jobs")
        .as_array()
        .context("missing attempt jobs")?;
    ensure!(
        field(list, "/total_count").as_u64() == Some(1) && jobs.len() == 1,
        "unexpected preflight attempt job set"
    );
    let job = jobs.first().context("missing preflight job")?;
    ensure!(
        field(job, "/id").as_u64() == Some(m.job_id)
            && field(job, "/run_id").as_u64() == Some(m.run_id)
            && field(job, "/name") == JOB
            && field(job, "/status") == "completed",
        "foreign or nonterminal preflight job"
    );
    let steps = field(job, "/steps")
        .as_array()
        .context("missing attempt steps")?;
    let mut uploads = steps.iter().filter(|s| field(s, "/name") == UPLOAD);
    let upload = uploads.next().context("missing artifact upload")?;
    ensure!(
        uploads.next().is_none()
            && field(upload, "/status") == "completed"
            && field(upload, "/conclusion") == "success",
        "artifact upload did not succeed uniquely"
    );
    let step = field(upload, "/number")
        .as_u64()
        .filter(|v| *v > 0)
        .context("missing upload step number")?;
    Ok((job, step))
}

fn check_artifact(a: &Value, m: &Manifest, repo_id: u64, run_head: &str) -> Result<()> {
    ensure!(
        field(a, "/id").as_u64() == Some(m.artifact_id)
            && field(a, "/name") == format!("release-preflight-{}", m.source_sha).as_str()
            && field(a, "/expired").as_bool() == Some(false),
        "missing, expired or different artifact"
    );
    let r = field(a, "/workflow_run");
    ensure!(
        field(r, "/id").as_u64() == Some(m.run_id)
            && field(r, "/repository_id").as_u64() == Some(repo_id)
            && field(r, "/head_repository_id").as_u64() == Some(repo_id)
            && field(r, "/head_sha") == run_head,
        "artifact belongs to a different run or repository"
    );
    ensure!(
        field(a, "/digest") == m.artifact_digest.as_str(),
        "service digest differs from the requested artifact digest"
    );
    Ok(())
}

fn check_upload_log(bytes: Vec<u8>, step: u64, m: &Manifest) -> Result<()> {
    let mut archive = preflight_archive::open_archive(bytes)?;
    // The provider separates step output in its attempt ZIP; other steps cannot supply this entry.
    let prefix = format!("{JOB}/{step}_");
    let mut members = Vec::new();
    for name in archive.file_names() {
        let name = name?;
        if let Some(rest) = name.strip_prefix(&prefix)
            && rest.ends_with(".txt")
            && !rest.contains('/')
        {
            members.push(name.into_owned());
        }
    }
    let [filename] = members.as_slice() else {
        bail!("missing or ambiguous attempt-scoped upload log");
    };
    let entry = archive.by_name(filename)?;
    ensure!(entry.size() <= 1 << 20, "upload log is too large");
    let mut log = String::new();
    entry.take((1 << 20) + 1).read_to_string(&mut log)?;
    ensure!(log.len() <= 1 << 20, "upload log is too large");
    let artifact = format!(
        "Artifact release-preflight-{} successfully finalized. Artifact ID {}",
        m.source_sha, m.artifact_id
    );
    let digest = format!(
        "SHA256 digest of uploaded artifact is {}",
        m.artifact_digest
            .strip_prefix("sha256:")
            .context("missing digest")?
    );
    let mut has_id = false;
    let mut has_digest = false;
    for line in log.lines() {
        if let Some((timestamp, message)) = line.split_once(' ')
            && chrono::DateTime::parse_from_rfc3339(timestamp.trim_start_matches('\u{feff}'))
                .is_ok()
        {
            has_id |= message == artifact;
            has_digest |= message == digest;
        }
    }
    ensure!(
        has_id && has_digest,
        "upload step did not attest the selected artifact ID and digest"
    );
    Ok(())
}

fn check_digest(blob: &[u8], expected: &str) -> Result<()> {
    let actual = format!("sha256:{:x}", Sha256::digest(blob));
    ensure!(expected == actual, "downloaded artifact digest mismatch");
    Ok(())
}

fn pending_fact(run: &Value, workflow: &[u8]) -> Result<Option<StateFact>> {
    ensure!(
        workflow == WORKFLOW_BYTES,
        "unrecognized producer workflow revision"
    );
    match field(run, "/status").as_str() {
        Some("completed") => Ok(None),
        Some("queued" | "in_progress" | "pending" | "waiting" | "requested") => {
            Ok(Some(state_fact(
                ReleaseState::Pending,
                "Preflight attempt has no terminal result",
                None,
            )))
        }
        _ => bail!("unknown run status"),
    }
}

fn observe(m: &Manifest, version: &str) -> Result<StateFact> {
    let client = github::Client::new();
    let repo = format!("repos/{}", m.repository);
    client.check_tag(&m.repository, &m.tag, &m.source_sha)?;
    let attempt = format!(
        "{repo}/actions/runs/{}/attempts/{}",
        m.run_id, m.run_attempt
    );
    let run = client.json(&attempt)?;
    check_run(&run, m)?;
    let run_head = field(&run, "/head_sha")
        .as_str()
        .context("missing workflow revision")?;
    let workflow = client.raw_file(
        &format!("{repo}/contents/{WORKFLOW}?ref={run_head}"),
        1 << 20,
    )?;
    if let Some(pending) = pending_fact(&run, &workflow)? {
        return Ok(pending);
    }
    let jobs = client.json(&format!("{attempt}/jobs?per_page=100"))?;
    let (job, step) = check_job(&jobs, m)?;
    let endpoint = format!("{repo}/actions/artifacts/{}", m.artifact_id);
    let artifact = client.json(&endpoint)?;
    let repo_id = field(&run, "/repository/id")
        .as_u64()
        .context("missing repository ID")?;
    check_artifact(&artifact, m, repo_id, run_head)?;
    let size = field(&artifact, "/size_in_bytes")
        .as_u64()
        .context("missing artifact size")?;
    ensure!(
        size > 0 && size <= MAX_ARCHIVE as u64,
        "artifact exceeds download budget or is empty"
    );
    check_upload_log(
        client.bytes(&format!("{attempt}/logs"), MAX_ARCHIVE)?,
        step,
        m,
    )?;
    let blob = client.bytes(&format!("{endpoint}/zip"), MAX_ARCHIVE)?;
    ensure!(
        blob.len() as u64 == size,
        "artifact download length mismatch"
    );
    check_digest(&blob, &m.artifact_digest)?;
    let mut fact = preflight_archive::verify_archive(blob, m, version)?;
    let enforcement = field(job, "/steps")
        .as_array()
        .context("missing producer steps")?
        .iter()
        .filter(|s| field(s, "/name") == ENFORCE)
        .collect::<Vec<_>>();
    if fact.state == ReleaseState::Passed
        && !(field(&run, "/conclusion") == "success"
            && field(job, "/conclusion") == "success"
            && matches!(enforcement.as_slice(),[s] if field(s, "/status")=="completed" && field(s, "/conclusion")=="success"))
    {
        bail!("receipt pass disagrees with the producer's terminal decision");
    }
    fact.evidence = Some(format!(
        "https://github.com/{}/actions/runs/{}/attempts/{} job={} artifact={} {}",
        m.repository, m.run_id, m.run_attempt, m.job_id, m.artifact_id, m.artifact_digest
    ));
    Ok(fact)
}

#[cfg(test)]
#[path = "preflight_verifier_tests.rs"]
mod tests;
