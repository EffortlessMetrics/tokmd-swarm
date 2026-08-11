//! Read-only, fail-closed release state inspection.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(test)]
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::cli::ReleaseStatusArgs;

const SCHEMA: &str = "tokmd.release_status.v1";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReleaseState {
    Missing,
    Pending,
    Passed,
    Failed,
    Unavailable,
    NotSupported,
    NotRun,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct StateFact {
    state: ReleaseState,
    detail: Option<String>,
    evidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct SourceFact {
    state: ReleaseState,
    workspace_version: Option<String>,
    expected_version: String,
    sha: Option<String>,
    detail: Option<String>,
    evidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct PublicationFact {
    state: ReleaseState,
    merge_sha: Option<String>,
    parent_count: Option<u32>,
    publication_ahead: Option<u64>,
    swarm_ahead: Option<u64>,
    detail: Option<String>,
    evidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct ReleaseStatusReceipt {
    schema: String,
    schema_version: u32,
    tag: String,
    source: SourceFact,
    publication: PublicationFact,
    git_tag: StateFact,
    github_release: StateFact,
    assets: StateFact,
    registry: StateFact,
    ghcr_exact: StateFact,
    ghcr_aliases: StateFact,
    action_exact: StateFact,
    action_alias: StateFact,
    consumer_proof: StateFact,
    nix: StateFact,
    wasm: StateFact,
    finalization: StateFact,
    complete: bool,
}

pub fn run(args: ReleaseStatusArgs) -> Result<()> {
    let receipt = match args.fixture.as_deref() {
        Some(path) => load_fixture(path, &args.tag)?,
        None => inspect_local(&args.tag)?,
    };

    if let Some(path) = args.json.as_deref() {
        write_json(path, &receipt)?;
    }

    println!(
        "release status: tag={} complete={} source={} registry={} consumer_proof={}",
        receipt.tag,
        receipt.complete,
        state_name(receipt.source.state),
        state_name(receipt.registry.state),
        state_name(receipt.consumer_proof.state),
    );
    Ok(())
}

fn inspect_local(tag: &str) -> Result<ReleaseStatusReceipt> {
    let workspace_root = find_workspace_root()?;
    inspect_local_at(&workspace_root, tag)
}

fn inspect_local_at(workspace_root: &Path, tag: &str) -> Result<ReleaseStatusReceipt> {
    validate_tag(tag)?;
    let expected_version = tag.strip_prefix('v').unwrap_or(tag).to_string();
    let workspace_version = workspace_version(workspace_root)?;
    let tag_sha = local_tag_sha(workspace_root, tag)?;
    let head_sha = local_head_sha(workspace_root)?;
    let source_matches = matches!(
        (&workspace_version, &tag_sha, &head_sha),
        (Some(version), Some(tag_sha), Some(head_sha))
            if source_matches_tag_commit(version, &expected_version, tag_sha, head_sha)
    );
    let source_state = match (&workspace_version, &tag_sha, &head_sha) {
        (Some(_), Some(_), Some(_)) if source_matches => ReleaseState::Passed,
        (Some(_), Some(_), Some(_)) => ReleaseState::Failed,
        (Some(_), Some(_), None) => ReleaseState::Missing,
        (_, None, _) => ReleaseState::Missing,
        (None, Some(_), _) => ReleaseState::Unavailable,
    };
    let source_detail = match (&workspace_version, &tag_sha, &head_sha) {
        (Some(_), Some(_), Some(_)) if source_matches => Some(
            "workspace version matches the inspected tag and HEAD resolves to its commit"
                .to_string(),
        ),
        (Some(_), Some(_), None) => {
            Some("tag exists but HEAD cannot be resolved; source cannot be verified".to_string())
        }
        (Some(version), Some(_), Some(_)) => Some(format!(
            "workspace version {version} or current HEAD does not match inspected tag {expected_version}"
        )),
        (_, None, _) => Some("tag does not exist in the local repository".to_string()),
        (None, Some(_), _) => Some(
            "workspace version could not be read; local source status is unavailable".to_string(),
        ),
    };
    let source = SourceFact {
        state: source_state,
        workspace_version,
        expected_version,
        sha: tag_sha.clone(),
        detail: source_detail,
        evidence: Some("local git tag and workspace Cargo.toml".to_string()),
    };

    let receipt = ReleaseStatusReceipt {
        schema: SCHEMA.to_string(),
        schema_version: SCHEMA_VERSION,
        tag: tag.to_string(),
        source,
        publication: not_run_publication("publication receipt not supplied"),
        git_tag: state_fact(
            if tag_sha.is_some() {
                ReleaseState::Passed
            } else {
                ReleaseState::Missing
            },
            "local git tag inspection",
            Some(format!("git tag {tag}")),
        ),
        github_release: not_run("GitHub Release receipt not supplied"),
        assets: not_run("release asset receipt not supplied"),
        registry: not_run("registry inventory receipt not supplied"),
        ghcr_exact: not_run("exact GHCR receipt not supplied"),
        ghcr_aliases: not_run("mutable GHCR alias receipt not supplied"),
        action_exact: not_run("exact Action receipt not supplied"),
        action_alias: not_run("Action v1 receipt not supplied"),
        consumer_proof: not_run("consumer proof receipt not supplied"),
        nix: not_run("Nix consumer receipt not supplied"),
        wasm: not_run("WASM/browser receipt not supplied"),
        finalization: not_run("finalization receipt not supplied"),
        complete: false,
    };
    Ok(receipt)
}

fn load_fixture(path: &Path, expected_tag: &str) -> Result<ReleaseStatusReceipt> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read release status fixture {}", path.display()))?;
    let receipt: ReleaseStatusReceipt = serde_json::from_str(&content)
        .with_context(|| format!("parse release status fixture {}", path.display()))?;
    validate_fixture(&receipt, expected_tag, path)?;
    Ok(receipt)
}

fn validate_fixture(receipt: &ReleaseStatusReceipt, expected_tag: &str, path: &Path) -> Result<()> {
    if receipt.schema != SCHEMA {
        bail!(
            "release status fixture {} has schema `{}`; expected `{SCHEMA}`",
            path.display(),
            receipt.schema
        );
    }
    if receipt.schema_version != SCHEMA_VERSION {
        bail!(
            "release status fixture {} has schema_version {}; expected {SCHEMA_VERSION}",
            path.display(),
            receipt.schema_version
        );
    }
    if receipt.tag != expected_tag {
        bail!(
            "release status fixture tag `{}` does not match requested `{expected_tag}`",
            receipt.tag
        );
    }
    validate_tag(&receipt.tag)?;
    let expected_version = receipt.tag.strip_prefix('v').unwrap_or(&receipt.tag);
    if receipt.source.expected_version != expected_version {
        bail!(
            "release status fixture source expected_version `{}` does not match tag-derived version `{expected_version}`",
            receipt.source.expected_version
        );
    }
    if receipt.source.state == ReleaseState::Passed
        && (receipt.source.workspace_version.as_deref() != Some(expected_version)
            || receipt.source.sha.as_deref().is_none_or(str::is_empty))
    {
        bail!(
            "passed source evidence must contain the tag-derived workspace version and a non-empty SHA"
        );
    }
    if receipt.source.state == ReleaseState::Passed {
        validate_sha("source", receipt.source.sha.as_deref())?;
    }
    for (name, fact) in [
        ("git_tag", &receipt.git_tag),
        ("github_release", &receipt.github_release),
        ("assets", &receipt.assets),
        ("registry", &receipt.registry),
        ("ghcr_exact", &receipt.ghcr_exact),
        ("ghcr_aliases", &receipt.ghcr_aliases),
        ("action_exact", &receipt.action_exact),
        ("action_alias", &receipt.action_alias),
        ("consumer_proof", &receipt.consumer_proof),
        ("nix", &receipt.nix),
        ("wasm", &receipt.wasm),
        ("finalization", &receipt.finalization),
    ] {
        validate_passed_state_fact(name, fact)?;
    }
    if receipt.source.state == ReleaseState::Passed
        && receipt.source.evidence.as_deref().is_none_or(str::is_empty)
    {
        bail!("passed source evidence must include a non-empty evidence field");
    }
    if receipt.source.state == ReleaseState::Passed
        && receipt.source.detail.as_deref().is_none_or(str::is_empty)
    {
        bail!("passed source evidence must include a non-empty detail field");
    }
    if receipt.publication.state == ReleaseState::Passed
        && receipt
            .publication
            .merge_sha
            .as_deref()
            .is_none_or(str::is_empty)
    {
        bail!("passed publication evidence must contain a non-empty merge SHA");
    }
    if receipt.publication.state == ReleaseState::Passed {
        validate_sha(
            "publication merge",
            receipt.publication.merge_sha.as_deref(),
        )?;
    }
    if receipt.publication.state == ReleaseState::Passed
        && !publication_graph_is_aligned(&receipt.publication)
    {
        bail!("passed publication evidence must include parent_count=2 and graph counters of 0/0");
    }
    if receipt.publication.state == ReleaseState::Passed
        && receipt
            .publication
            .evidence
            .as_deref()
            .is_none_or(str::is_empty)
    {
        bail!("passed publication evidence must include a non-empty evidence field");
    }
    if receipt.publication.state == ReleaseState::Passed
        && receipt
            .publication
            .detail
            .as_deref()
            .is_none_or(str::is_empty)
    {
        bail!("passed publication evidence must include a non-empty detail field");
    }
    let computed = is_complete(receipt);
    if receipt.complete != computed {
        bail!(
            "release status fixture complete={} disagrees with component states (computed {computed})",
            receipt.complete
        );
    }
    Ok(())
}

fn is_complete(receipt: &ReleaseStatusReceipt) -> bool {
    receipt.source.state == ReleaseState::Passed
        && receipt.publication.state == ReleaseState::Passed
        && publication_graph_is_aligned(&receipt.publication)
        && receipt.git_tag.state == ReleaseState::Passed
        && receipt.github_release.state == ReleaseState::Passed
        && receipt.assets.state == ReleaseState::Passed
        && receipt.registry.state == ReleaseState::Passed
        && receipt.ghcr_exact.state == ReleaseState::Passed
        && receipt.ghcr_aliases.state == ReleaseState::Passed
        && receipt.action_exact.state == ReleaseState::Passed
        && receipt.action_alias.state == ReleaseState::Passed
        && receipt.consumer_proof.state == ReleaseState::Passed
        && receipt.nix.state == ReleaseState::Passed
        && receipt.wasm.state == ReleaseState::Passed
        && receipt.finalization.state == ReleaseState::Passed
}

fn publication_graph_is_aligned(publication: &PublicationFact) -> bool {
    publication.parent_count == Some(2)
        && publication.publication_ahead == Some(0)
        && publication.swarm_ahead == Some(0)
}

fn workspace_version(workspace_root: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(workspace_root.join("Cargo.toml"))
        .context("read workspace Cargo.toml")?;
    let manifest: toml::Value = toml::from_str(&content).context("parse workspace Cargo.toml")?;
    Ok(manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(str::to_string))
}

fn find_workspace_root() -> Result<PathBuf> {
    let mut directory = std::env::current_dir().context("read current directory")?;
    loop {
        let cargo_toml = directory.join("Cargo.toml");
        if cargo_toml.is_file() {
            let content = fs::read_to_string(&cargo_toml)
                .with_context(|| format!("read workspace candidate {}", cargo_toml.display()))?;
            if content.contains("[workspace]") {
                return Ok(directory);
            }
        }
        if !directory.pop() {
            bail!("could not find workspace root");
        }
    }
}

fn local_head_sha(workspace_root: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD^{commit}"])
        .current_dir(workspace_root)
        .output()
        .context("resolve current Git HEAD")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(128) && is_unresolved_head_error(&stderr) {
            return Ok(None);
        }
        bail!(
            "resolve current Git HEAD failed with status {}: {}",
            output.status,
            stderr.trim()
        );
    }
    Ok(Some(
        String::from_utf8(output.stdout)
            .context("current Git HEAD is not UTF-8")?
            .trim()
            .to_string(),
    ))
}

fn is_unresolved_head_error(stderr: &str) -> bool {
    stderr.contains("Needed a single revision")
}

fn source_matches_tag_commit(
    workspace_version: &str,
    expected_version: &str,
    tag_sha: &str,
    head_sha: &str,
) -> bool {
    workspace_version == expected_version && tag_sha == head_sha
}

fn validate_sha(name: &str, value: Option<&str>) -> Result<()> {
    let Some(value) = value else {
        bail!("passed {name} evidence must contain a SHA");
    };
    let valid_length = matches!(value.len(), 40 | 64);
    if !valid_length || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("passed {name} evidence has an invalid Git SHA `{value}`");
    }
    Ok(())
}

fn validate_passed_state_fact(name: &str, fact: &StateFact) -> Result<()> {
    if fact.state == ReleaseState::Passed
        && (fact.evidence.as_deref().is_none_or(str::is_empty)
            || fact.detail.as_deref().is_none_or(str::is_empty))
    {
        bail!("passed {name} evidence must include non-empty detail and evidence fields");
    }
    Ok(())
}

fn local_tag_sha(workspace_root: &Path, tag: &str) -> Result<Option<String>> {
    let reference = format!("refs/tags/{tag}");
    let output = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &reference])
        .current_dir(workspace_root)
        .output()
        .context("inspect local Git tag")?;
    if !output.status.success() {
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        bail!(
            "inspect local Git tag `{tag}` failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let commit_reference = format!("{reference}^{{commit}}");
    let output = Command::new("git")
        .args(["rev-parse", "--verify", &commit_reference])
        .current_dir(workspace_root)
        .output()
        .context("resolve local Git tag commit")?;
    if !output.status.success() {
        bail!(
            "local Git tag `{tag}` exists but does not resolve to a commit: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let sha = String::from_utf8(output.stdout).context("Git tag SHA is not UTF-8")?;
    Ok(Some(sha.trim().to_string()))
}

fn validate_tag(tag: &str) -> Result<()> {
    if tag.is_empty() || tag.chars().any(char::is_whitespace) || tag.contains('\0') {
        bail!("release tag must be a non-empty single Git reference name");
    }
    let reference = format!("refs/tags/{tag}");
    let output = Command::new("git")
        .args(["check-ref-format", "--allow-onelevel", &reference])
        .output()
        .context("validate Git tag reference")?;
    if !output.status.success() {
        bail!(
            "invalid Git tag `{tag}`: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn state_fact(state: ReleaseState, detail: &str, evidence: Option<String>) -> StateFact {
    StateFact {
        state,
        detail: Some(detail.to_string()),
        evidence,
    }
}

fn not_run(detail: &str) -> StateFact {
    state_fact(ReleaseState::NotRun, detail, None)
}

fn not_run_publication(detail: &str) -> PublicationFact {
    PublicationFact {
        state: ReleaseState::NotRun,
        merge_sha: None,
        parent_count: None,
        publication_ahead: None,
        swarm_ahead: None,
        detail: Some(detail.to_string()),
        evidence: None,
    }
}

fn state_name(state: ReleaseState) -> &'static str {
    match state {
        ReleaseState::Missing => "missing",
        ReleaseState::Pending => "pending",
        ReleaseState::Passed => "passed",
        ReleaseState::Failed => "failed",
        ReleaseState::Unavailable => "unavailable",
        ReleaseState::NotSupported => "not_supported",
        ReleaseState::NotRun => "not_run",
    }
}

fn write_json(path: &Path, receipt: &ReleaseStatusReceipt) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(receipt).context("serialize release status")?;
    fs::write(path, format!("{json}\n"))
        .with_context(|| format!("write release status {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

    fn inspect_local_from(directory: &Path, tag: &str) -> Result<ReleaseStatusReceipt> {
        let _current_dir_lock = CURRENT_DIR_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("current-directory test lock poisoned"))?;
        let original_dir = std::env::current_dir()?;
        std::env::set_current_dir(directory)?;
        let inspection = inspect_local(tag);
        std::env::set_current_dir(original_dir)?;
        inspection
    }

    fn passed(detail: &str) -> StateFact {
        state_fact(ReleaseState::Passed, detail, Some("fixture".to_string()))
    }

    fn complete_fixture() -> ReleaseStatusReceipt {
        ReleaseStatusReceipt {
            schema: SCHEMA.to_string(),
            schema_version: SCHEMA_VERSION,
            tag: "v1.15.1".to_string(),
            source: SourceFact {
                state: ReleaseState::Passed,
                workspace_version: Some("1.15.1".to_string()),
                expected_version: "1.15.1".to_string(),
                sha: Some("a".repeat(40)),
                detail: Some("fixture".to_string()),
                evidence: Some("fixture".to_string()),
            },
            publication: PublicationFact {
                state: ReleaseState::Passed,
                merge_sha: Some("b".repeat(40)),
                parent_count: Some(2),
                publication_ahead: Some(0),
                swarm_ahead: Some(0),
                detail: Some("fixture".to_string()),
                evidence: Some("fixture".to_string()),
            },
            git_tag: passed("tag"),
            github_release: passed("release"),
            assets: passed("assets"),
            registry: passed("registry"),
            ghcr_exact: passed("exact image"),
            ghcr_aliases: passed("aliases"),
            action_exact: passed("exact action"),
            action_alias: passed("action alias"),
            consumer_proof: passed("consumer"),
            nix: passed("nix"),
            wasm: passed("wasm"),
            finalization: passed("finalization"),
            complete: true,
        }
    }

    #[test]
    fn completion_requires_every_surface_and_graph_alignment() -> Result<()> {
        let mut fixture = complete_fixture();
        if !is_complete(&fixture) {
            bail!("complete fixture should satisfy the release contract");
        }
        fixture.ghcr_aliases.state = ReleaseState::Pending;
        if is_complete(&fixture) {
            bail!("pending aliases must keep the release incomplete");
        }
        Ok(())
    }

    #[test]
    fn fixture_validation_rejects_stale_complete_claims() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.action_alias.state = ReleaseState::Pending;
        let path = Path::new("fixture.json");
        if validate_fixture(&fixture, "v1.15.1", path).is_ok() {
            bail!("stale complete claim should be rejected");
        }
        Ok(())
    }

    #[test]
    fn fixture_validation_rejects_contradictory_source_evidence() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.source.expected_version = "1.15.0".to_string();
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("tag and source expected version must agree");
        }

        let mut fixture = complete_fixture();
        fixture.source.sha = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed source evidence must include a SHA");
        }
        Ok(())
    }

    #[test]
    fn fixture_validation_rejects_missing_publication_merge_sha() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.publication.merge_sha = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed publication evidence must include a merge SHA");
        }
        Ok(())
    }

    #[test]
    fn fixture_validation_rejects_malformed_passed_shas() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.source.sha = Some("not-a-sha".to_string());
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed source evidence must contain a valid Git SHA");
        }

        let mut fixture = complete_fixture();
        fixture.publication.merge_sha = Some("f".repeat(39));
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed publication evidence must contain a valid Git SHA");
        }
        Ok(())
    }

    #[test]
    fn fixture_validation_rejects_missing_passed_surface_evidence() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.github_release.evidence = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed surfaces must carry evidence");
        }

        let mut fixture = complete_fixture();
        fixture.publication.parent_count = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed publication must carry graph evidence");
        }

        let mut fixture = complete_fixture();
        fixture.source.detail = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed source must carry detail evidence");
        }

        let mut fixture = complete_fixture();
        fixture.publication.detail = None;
        if validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json")).is_ok() {
            bail!("passed publication must carry detail evidence");
        }
        Ok(())
    }

    #[test]
    fn source_status_requires_current_head_to_match_tag() -> Result<()> {
        let tag_sha = "a".repeat(40);
        let head_sha = "b".repeat(40);
        if source_matches_tag_commit("1.15.1", "1.15.1", &tag_sha, &head_sha) {
            bail!("a same-version post-tag checkout must not pass source validation");
        }
        Ok(())
    }

    #[test]
    fn only_git_unresolved_head_diagnostic_maps_to_missing() -> Result<()> {
        if !is_unresolved_head_error("fatal: Needed a single revision\n") {
            bail!("the supported unresolved HEAD diagnostic should be recognized");
        }
        if is_unresolved_head_error("fatal: bad object HEAD\n") {
            bail!("other Git failures must not be classified as an unresolved HEAD");
        }
        Ok(())
    }

    fn run_git(root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .with_context(|| format!("run git {:?}", args))?;
        if !output.status.success() {
            bail!(
                "git {:?} failed with status {}: {}",
                args,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        String::from_utf8(output.stdout).with_context(|| format!("decode git {:?} output", args))
    }

    #[test]
    fn inspect_local_rejects_same_version_checkout_after_tag() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\n[workspace.package]\nversion = \"1.15.1\"\n",
        )?;
        let crate_dir = temp.path().join("crates").join("fixture");
        fs::create_dir_all(&crate_dir)?;
        let marker = temp.path().join("marker.txt");
        fs::write(&marker, "tagged\n")?;
        run_git(temp.path(), &["init"])?;
        run_git(temp.path(), &["config", "user.email", "test@example.com"])?;
        run_git(temp.path(), &["config", "user.name", "Release Test"])?;
        run_git(temp.path(), &["add", "."])?;
        run_git(temp.path(), &["commit", "-m", "tagged source"])?;
        run_git(temp.path(), &["tag", "v1.15.1"])?;
        fs::write(marker, "post-tag\n")?;
        run_git(temp.path(), &["add", "."])?;
        run_git(temp.path(), &["commit", "-m", "post-tag source"])?;

        let receipt = inspect_local_from(&crate_dir, "v1.15.1")?;
        if receipt.source.state != ReleaseState::Failed {
            bail!(
                "same-version checkout after the tag must fail source validation, got {:?}",
                receipt.source.state
            );
        }
        Ok(())
    }

    #[test]
    fn inspect_local_reports_missing_tag_without_head() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\n[workspace.package]\nversion = \"1.15.1\"\n",
        )?;
        let crate_dir = temp.path().join("crates").join("fixture");
        fs::create_dir_all(&crate_dir)?;
        run_git(temp.path(), &["init"])?;

        let receipt = inspect_local_from(&crate_dir, "v1.15.1")?;
        if receipt.source.state != ReleaseState::Missing {
            bail!(
                "missing tag and HEAD must produce missing source state, got {:?}",
                receipt.source.state
            );
        }
        Ok(())
    }

    #[test]
    fn inspect_local_reports_existing_tag_without_head_as_missing() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::write(
            temp.path().join("Cargo.toml"),
            "[workspace]\n[workspace.package]\nversion = \"1.15.1\"\n",
        )?;
        let crate_dir = temp.path().join("crates").join("fixture");
        fs::create_dir_all(&crate_dir)?;
        fs::write(temp.path().join("marker.txt"), "tagged\n")?;
        run_git(temp.path(), &["init"])?;
        run_git(temp.path(), &["config", "user.email", "test@example.com"])?;
        run_git(temp.path(), &["config", "user.name", "Release Test"])?;
        run_git(temp.path(), &["add", "."])?;
        run_git(temp.path(), &["commit", "-m", "tagged source"])?;
        run_git(temp.path(), &["tag", "v1.15.1"])?;
        run_git(
            temp.path(),
            &["symbolic-ref", "HEAD", "refs/heads/unborn-fixture"],
        )?;

        let receipt = inspect_local_from(&crate_dir, "v1.15.1")?;
        if receipt.source.state != ReleaseState::Missing {
            bail!(
                "existing tag without HEAD must produce missing source state, got {:?}",
                receipt.source.state
            );
        }
        if !receipt
            .source
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("HEAD cannot be resolved"))
        {
            bail!("missing HEAD source detail should explain the unavailable evidence");
        }
        Ok(())
    }

    #[test]
    fn incomplete_fixture_states_remain_explicit() -> Result<()> {
        let mut fixture = complete_fixture();
        fixture.github_release.state = ReleaseState::Pending;
        fixture.assets.state = ReleaseState::Failed;
        fixture.registry.state = ReleaseState::Unavailable;
        fixture.consumer_proof.state = ReleaseState::NotRun;
        fixture.complete = false;
        validate_fixture(&fixture, "v1.15.1", Path::new("fixture.json"))?;
        if is_complete(&fixture) {
            bail!("incomplete fixture states must not produce a complete receipt");
        }
        Ok(())
    }

    #[test]
    fn state_serialization_is_stable_and_explicit() -> Result<()> {
        let cases = [
            (ReleaseState::Missing, "\"missing\""),
            (ReleaseState::Pending, "\"pending\""),
            (ReleaseState::Passed, "\"passed\""),
            (ReleaseState::Failed, "\"failed\""),
            (ReleaseState::Unavailable, "\"unavailable\""),
            (ReleaseState::NotSupported, "\"not_supported\""),
            (ReleaseState::NotRun, "\"not_run\""),
        ];
        for (state, expected) in cases {
            let json = serde_json::to_string(&state).context("serialize state")?;
            if json != expected {
                bail!("unexpected state JSON for {state:?}: got {json}, expected {expected}");
            }
        }
        Ok(())
    }
}
