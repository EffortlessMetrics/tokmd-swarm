//! Read-only GitHub.com transport for the release preflight verifier.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

const JSON_LIMIT: usize = 4 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const INSPECTION_TIMEOUT: Duration = Duration::from_secs(180);

pub(super) struct Client {
    deadline: Instant,
}

impl Client {
    pub(super) fn new() -> Self {
        Self {
            deadline: Instant::now() + INSPECTION_TIMEOUT,
        }
    }

    pub(super) fn json(&self, endpoint: &str) -> Result<Value> {
        let body = self.get(endpoint, JSON_LIMIT, "application/vnd.github+json")?;
        serde_json::from_slice(&body).context("invalid GitHub JSON response")
    }

    pub(super) fn bytes(&self, endpoint: &str, max: usize) -> Result<Vec<u8>> {
        self.get(endpoint, max, "application/vnd.github+json")
    }

    pub(super) fn raw_file(&self, endpoint: &str, max: usize) -> Result<Vec<u8>> {
        self.get(endpoint, max, "application/vnd.github.raw+json")
    }

    fn get(&self, endpoint: &str, max: usize, accept: &str) -> Result<Vec<u8>> {
        ensure!(
            (endpoint.starts_with("repos/EffortlessMetrics/tokmd-swarm/")
                || endpoint.starts_with("repos/EffortlessMetrics/tokmd/"))
                && !endpoint.contains(['\\', '{', '}'])
                && !endpoint.split('/').any(|part| matches!(part, "." | ".."))
                && endpoint.is_ascii()
                && !endpoint.bytes().any(|byte| byte.is_ascii_control()),
            "invalid preflight GitHub endpoint"
        );
        let now = Instant::now();
        ensure!(now < self.deadline, "GitHub preflight inspection timed out");
        let deadline = self.deadline.min(now + REQUEST_TIMEOUT);
        let mut child = Command::new("gh")
            .args([
                "api",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "--header",
            ])
            .arg(format!("Accept: {accept}"))
            .arg(endpoint)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env_remove("GH_FORCE_TTY")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("start GitHub CLI for preflight inspection")?;
        let result = (|| {
            let stdout = child.stdout.take().context("open GitHub CLI output")?;
            let (sender, receiver) = mpsc::sync_channel(1);
            thread::Builder::new()
                .spawn(move || {
                    let mut bytes = Vec::new();
                    let read = stdout
                        .take((max as u64).saturating_add(1))
                        .read_to_end(&mut bytes)
                        .map(|_| bytes);
                    let _ = sender.send(read);
                })
                .context("start GitHub response reader")?;
            let bytes = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .context("GitHub response timed out or closed")?
                .context("read GitHub response")?;
            ensure!(
                bytes.len() <= max,
                "GitHub response exceeded its byte limit"
            );
            loop {
                if let Some(status) = child.try_wait().context("wait for GitHub CLI")? {
                    ensure!(status.success(), "GitHub API request failed with {status}");
                    return Ok(bytes);
                }
                ensure!(Instant::now() < deadline, "GitHub API request timed out");
                thread::sleep(Duration::from_millis(10));
            }
        })();
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        result
    }

    /// Resolve the actual tag reference, peeling each annotated tag to its
    /// target. A caller-provided commit or a same-named branch cannot stand in
    /// for the tag on the inspected repository.
    pub(super) fn check_tag(&self, repository: &str, tag: &str, source_sha: &str) -> Result<()> {
        let encoded = encode_ref_component(tag);
        let reference = self.json(&format!("repos/{repository}/git/ref/tags/{encoded}"))?;
        ensure!(
            reference.get("ref").and_then(Value::as_str) == Some(&format!("refs/tags/{tag}")),
            "GitHub returned a different tag reference"
        );
        let mut response = reference;
        for _ in 0..16 {
            let kind = response.pointer("/object/type").and_then(Value::as_str);
            let sha = response
                .pointer("/object/sha")
                .and_then(Value::as_str)
                .context("GitHub tag target lacks an object SHA")?;
            ensure!(
                matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "invalid GitHub tag target SHA"
            );
            match kind {
                Some("commit") => {
                    ensure!(
                        sha.eq_ignore_ascii_case(source_sha),
                        "hosted tag commit differs from the verified source"
                    );
                    return Ok(());
                }
                Some("tag") => {
                    let object = self.json(&format!("repos/{repository}/git/tags/{sha}"))?;
                    ensure!(
                        object
                            .get("sha")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value.eq_ignore_ascii_case(sha)),
                        "hosted annotated tag object identity differs from its reference"
                    );
                    response = object;
                }
                _ => bail!("unsupported GitHub tag target"),
            }
        }
        bail!("hosted annotated tag chain exceeds the inspection limit")
    }
}

fn encode_ref_component(reference: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in reference.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            // Formatting into a String is infallible.
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}
