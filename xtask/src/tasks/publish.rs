//! Publish crates to crates.io in dependency order.
//!
//! Safety guarantees:
//! - Only publishes workspace members (not external dependencies)
//! - Filters out non-publishable crates (publish = false)
//! - Validates exclusions don't break required dependencies
//! - Requires confirmation for actual publishing (or explicit --yes)

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::fs::File;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId};
use chrono::{DateTime, FixedOffset, Utc};
use petgraph::algo::toposort;
use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli::PublishArgs;

/// Result of attempting to publish a single crate.
#[derive(Debug)]
pub enum PublishResult {
    Success,
    AlreadyPublished,
    Failed(anyhow::Error),
}

/// Information about why a crate was included in the publish set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InclusionReason {
    /// Explicitly requested via --crates
    Explicit,
    /// Included as a transitive dependency of an explicit crate
    TransitiveDep(String),
    /// Included because no --crates filter was specified (all publishable)
    Default,
}

/// Information about why a crate was excluded from the publish set.
#[derive(Debug, Clone)]
pub enum ExclusionReason {
    /// Crate has publish = false in Cargo.toml
    NotPublishable,
    /// Crate is xtask (internal tooling)
    IsXtask,
    /// Crate is fuzz (testing infrastructure)
    IsFuzz,
    /// Explicitly excluded via --exclude
    ExplicitExclude,
    /// Not in the requested --crates set or their dependencies
    NotRequested,
}

/// The resolved publish plan.
#[derive(Debug)]
pub struct PublishPlan {
    /// Crates to publish, in topological order.
    pub publish_order: Vec<String>,
    /// Why each crate was included.
    pub inclusion_reasons: BTreeMap<String, InclusionReason>,
    /// Why each crate was excluded.
    pub exclusion_reasons: BTreeMap<String, ExclusionReason>,
    /// The workspace version (from [workspace.package].version).
    pub workspace_version: String,
    /// Direct internal predecessors for each planned crate.
    pub predecessors: BTreeMap<String, BTreeSet<String>>,
}

const PUBLISH_RECEIPT_SCHEMA: &str = "tokmd.publish_receipt.v3";
const PUBLISH_RECEIPT_VERSION: u32 = 3;
const PUBLISH_REGISTRY_ID: &str = "crates.io:https://crates.io";
static RECEIPT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn receipt_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublishReceiptState {
    Planned,
    InProgress,
    Published,
    AlreadyPresent,
    Yanked,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PublishRunState {
    Planned,
    InProgress,
    Complete,
    Incomplete,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct PublishCrateReceipt {
    name: String,
    version: String,
    state: PublishReceiptState,
    attempts: u32,
    registry_visible: Option<bool>,
    dependency_closure: Option<bool>,
    #[serde(default)]
    bootstrap: bool,
    reason: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct PublishIdentity {
    source_commit: String,
    source_tree: String,
    registry_id: String,
    workspace_version: String,
    plan_digest: String,
    options_digest: String,
    manifest_digests: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct PublishReceipt {
    schema: String,
    schema_version: u32,
    workspace_version: String,
    generation: u64,
    identity: PublishIdentity,
    state: PublishRunState,
    publish_order: Vec<String>,
    crates: Vec<PublishCrateReceipt>,
}

struct PublishReceiptLock {
    path: PathBuf,
    _file: File,
}

impl Drop for PublishReceiptLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_publish_receipt_lock(path: &Path) -> Result<PublishReceiptLock> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create publication receipt parent {}", parent.display()))?;
    }
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| {
            format!(
                "publication receipt lock {} already exists; another publisher may be active (remove it only after proving no publisher owns it)",
                lock_path.display()
            )
        })?;
    writeln!(file, "pid={}", std::process::id())?;
    file.sync_all()?;
    Ok(PublishReceiptLock {
        path: lock_path,
        _file: file,
    })
}

fn validated_publish_receipt_path(path: &Path, workspace_root: &Path) -> Result<PathBuf> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("publication receipt paths must not contain `..`");
    }

    let workspace_root = canonicalize_with_missing_tail(workspace_root)?;
    let target_root = canonicalize_with_missing_tail(&workspace_root.join("target"))?;
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    let resolved_path = canonicalize_with_missing_tail(&absolute_path)?;

    if path.is_absolute() {
        if path_is_within(&resolved_path, &workspace_root)
            && !path_is_within(&resolved_path, &target_root)
        {
            bail!(
                "a receipt inside the worktree must be under ignored target/; use target/publishing/publish-receipt.json or a path outside the worktree"
            );
        }
    } else {
        let mut components = path.components();
        if components
            .next()
            .is_none_or(|component| component.as_os_str() != "target")
        {
            bail!(
                "a relative publication receipt must be under ignored target/; use target/publishing/publish-receipt.json"
            );
        }
    }
    Ok(resolved_path)
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf> {
    let mut existing = path;
    let mut tail = Vec::new();
    while !existing.exists() {
        let name = existing.file_name().ok_or_else(|| {
            anyhow!(
                "publication receipt path has no existing ancestor: {}",
                path.display()
            )
        })?;
        tail.push(name.to_os_string());
        existing = existing.parent().ok_or_else(|| {
            anyhow!(
                "publication receipt path has no existing ancestor: {}",
                path.display()
            )
        })?;
    }
    let mut resolved = existing
        .canonicalize()
        .with_context(|| format!("canonicalize publication receipt path {}", path.display()))?;
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn path_is_within(path: &Path, parent: &Path) -> bool {
    #[cfg(windows)]
    {
        let folded_path = path.to_string_lossy().to_lowercase();
        let folded_parent = parent.to_string_lossy().to_lowercase();
        Path::new(&folded_path).starts_with(Path::new(&folded_parent))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(parent)
    }
}

/// Publish all workspace crates in dependency order.
pub fn run(args: PublishArgs) -> Result<()> {
    // Keep the historical hidden alias behavior identical to --dry-run.
    let mut args = normalize_publish_args(args);

    // Load workspace metadata
    // Use no_deps() for faster metadata loading - we only need workspace members
    // and their manifest-declared dependencies, not the full resolved graph
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to load cargo metadata")?;

    // Resolve the publish plan (workspace-scoped, validated)
    let plan = resolve_publish_plan(&metadata, &args)?;

    // Registry inventory is intentionally read-only and writes its receipt even
    // when the registry is incomplete. It must not run publish preflight or
    // mutate crates.io.
    if let Some(path) = args.registry_inventory.as_deref() {
        return run_registry_inventory(&metadata, &plan, path);
    }

    validate_publish_mode(&args)?;

    if let Some(path) = args.receipt.as_deref() {
        args.receipt = Some(validated_publish_receipt_path(
            path,
            metadata.workspace_root.as_std_path(),
        )?);
    }

    let _receipt_lock = args
        .receipt
        .as_deref()
        .map(acquire_publish_receipt_lock)
        .transpose()?;

    let bootstrap = validate_bootstrap_crates(&plan.publish_order, args.bootstrap.as_deref())?;
    let identity = args
        .receipt
        .as_ref()
        .map(|_| build_publish_identity(&metadata, &plan, &args, &bootstrap))
        .transpose()?;

    let mut receipt = match args.receipt.as_deref() {
        Some(path) if args.resume => Some(load_publish_receipt_for_identity(
            path,
            &plan,
            identity
                .as_ref()
                .ok_or_else(|| anyhow!("receipt identity was not constructed"))?,
        )?),
        Some(path) => {
            if path.exists() || has_publish_recovery_candidates(path)? {
                bail!(
                    "publication receipt or recovery candidate exists; {}",
                    resume_instruction(Some(path))
                );
            }
            Some(new_publish_receipt_with_identity(
                &plan,
                identity
                    .as_ref()
                    .ok_or_else(|| anyhow!("receipt identity was not constructed"))?,
            ))
        }
        None => None,
    };

    let start_idx = if let Some(ref from_crate) = args.from {
        plan.publish_order
            .iter()
            .position(|name| name == from_crate)
            .ok_or_else(|| anyhow!("Crate '{}' not found in publish order", from_crate))?
    } else {
        0
    };
    // Handle --plan mode: just print and exit
    if args.plan {
        print_plan(&plan, &args);
        return Ok(());
    }

    // Run pre-publish checks (unless skipped)
    if !args.skip_checks {
        run_pre_publish_checks(&args, &metadata, &plan)?;
        if let Some(receipt) = receipt.as_mut() {
            mark_dependency_closure_verified(receipt);
        }
    }

    if !args.dry_run && !bootstrap.is_empty() && args.receipt.is_none() {
        bail!("--bootstrap requires --receipt so the no-verify decision is auditable");
    }

    // A receipt-backed live run inventories every exact crate version before
    // the first upload. Resume never trusts persisted visibility on its own.
    if let (Some(path), Some(receipt)) = (args.receipt.as_deref(), receipt.as_mut()) {
        let inventory = inventory_publish_plan(&plan);
        reconcile_inventory(path, receipt, &inventory)?;
    }

    let crates_to_publish = crates_to_publish(&plan, start_idx, receipt.as_ref());
    if crates_to_publish.is_empty() {
        validate_no_work_resume(&args, receipt.as_ref())?;
        if let (Some(path), Some(receipt)) = (args.receipt.as_deref(), receipt.as_mut()) {
            receipt.state = completed_publish_run_state(receipt);
            write_publish_receipt(path, receipt)?;
        }
        println!("No crates require publication after live registry inventory.");
        return Ok(());
    }

    if let (Some(path), Some(receipt)) = (args.receipt.as_deref(), receipt.as_ref()) {
        write_publish_receipt(path, receipt)?;
    }

    // Print summary and require confirmation for real publishing
    print_pre_publish_summary(&plan, &args, start_idx);

    validate_publish_authorization(&args, io::stdin().is_terminal())?;
    if !args.dry_run && !args.yes && !confirm_publish()? {
        println!("\nPublish cancelled.");
        return Ok(());
    }

    // Execute publishing
    let (succeeded, failed) = execute_publish(
        &crates_to_publish,
        &args,
        &plan.workspace_version,
        &bootstrap,
        &plan.predecessors,
        args.receipt.as_deref(),
        receipt.as_mut(),
    )?;

    // Print summary
    println!("\n--- Summary ---");
    println!("Succeeded: {}", succeeded.len());
    if !failed.is_empty() {
        println!("Failed: {} ({:?})", failed.len(), failed);
    }

    // Create git tag if requested
    if args.tag && failed.is_empty() && !args.dry_run {
        create_git_tag(&args, &plan.workspace_version)?;
    }

    if !failed.is_empty() {
        if let (Some(path), Some(receipt)) = (args.receipt.as_deref(), receipt.as_mut()) {
            receipt.state = PublishRunState::Incomplete;
            write_publish_receipt(path, receipt)?;
        }
        bail!("{} crate(s) failed to publish", failed.len());
    }

    if let (Some(path), Some(receipt)) = (args.receipt.as_deref(), receipt.as_mut()) {
        receipt.state = PublishRunState::Complete;
        write_publish_receipt(path, receipt)?;
    }

    Ok(())
}

fn normalize_publish_args(mut args: PublishArgs) -> PublishArgs {
    args.dry_run |= args.verify;
    args.verify = false;
    args
}

fn validate_publish_mode(args: &PublishArgs) -> Result<()> {
    if args.dry_run && args.receipt.is_some() {
        bail!("--receipt cannot be combined with --dry-run or --verify");
    }
    if args.receipt.is_some() && args.skip_checks {
        bail!("--skip-checks cannot be used with receipt-backed publication");
    }
    if args.receipt.is_some() && args.skip_git_check {
        bail!("--skip-git-check cannot be used with receipt-backed publication");
    }
    if args.receipt.is_some() && args.from.is_some() {
        bail!(
            "--from cannot be used with a publication receipt; --resume derives work from live registry state"
        );
    }
    if args.receipt.is_some() && args.tag {
        bail!(
            "--tag cannot be used with receipt-backed publication; release finalization is a separate step"
        );
    }
    Ok(())
}

#[derive(Serialize)]
struct CanonicalPlanIdentity<'a> {
    domain: &'static str,
    workspace_version: &'a str,
    publish_order: &'a [String],
    predecessors: &'a BTreeMap<String, BTreeSet<String>>,
    manifest_digests: &'a BTreeMap<String, String>,
}

#[derive(Serialize)]
struct CanonicalOptionsIdentity<'a> {
    domain: &'static str,
    bootstrap: &'a BTreeSet<String>,
}

fn canonical_digest<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("serialize canonical publication identity")?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn git_identity_value(spec: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", spec])
        .output()
        .with_context(|| format!("resolve git identity {spec}"))?;
    if !output.status.success() {
        bail!(
            "cannot resolve immutable git identity {spec}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase())
}

fn build_publish_identity(
    metadata: &Metadata,
    plan: &PublishPlan,
    _args: &PublishArgs,
    bootstrap: &BTreeSet<String>,
) -> Result<PublishIdentity> {
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
        .context("inspect git worktree before receipt-backed publication")?;
    if !status.status.success() || !status.stdout.is_empty() {
        bail!("receipt-backed publication requires a clean, committed worktree");
    }

    let planned: BTreeSet<&str> = plan.publish_order.iter().map(String::as_str).collect();
    let root = metadata.workspace_root.as_std_path();
    let mut manifest_digests = BTreeMap::new();
    for package in &metadata.packages {
        if !planned.contains(package.name.as_str()) {
            continue;
        }
        let manifest = package.manifest_path.as_std_path();
        let bytes = fs::read(manifest).with_context(|| {
            format!(
                "read manifest for publication identity: {}",
                manifest.display()
            )
        })?;
        let relative = manifest.strip_prefix(root).unwrap_or(manifest);
        let key = format!(
            "{}:{}",
            package.name,
            relative.to_string_lossy().replace('\\', "/")
        );
        manifest_digests.insert(key, blake3::hash(&bytes).to_hex().to_string());
    }
    if manifest_digests.len() != plan.publish_order.len() {
        bail!("publication identity could not account for every planned crate manifest");
    }

    let plan_digest = canonical_digest(&CanonicalPlanIdentity {
        domain: "tokmd.publish.plan.v1",
        workspace_version: &plan.workspace_version,
        publish_order: &plan.publish_order,
        predecessors: &plan.predecessors,
        manifest_digests: &manifest_digests,
    })?;
    let options_digest = canonical_digest(&CanonicalOptionsIdentity {
        domain: "tokmd.publish.options.v1",
        bootstrap,
    })?;

    Ok(PublishIdentity {
        source_commit: git_identity_value("HEAD^{commit}")?,
        source_tree: git_identity_value("HEAD^{tree}")?,
        registry_id: PUBLISH_REGISTRY_ID.to_string(),
        workspace_version: plan.workspace_version.clone(),
        plan_digest,
        options_digest,
        manifest_digests,
    })
}

fn validate_no_work_resume(args: &PublishArgs, receipt: Option<&PublishReceipt>) -> Result<()> {
    if !args.resume {
        return Ok(());
    }
    let receipt = receipt.ok_or_else(|| anyhow!("--resume requires a publication receipt"))?;
    if receipt.state != PublishRunState::Complete
        && (receipt.crates.is_empty()
            || receipt
                .crates
                .iter()
                .any(|entry| !is_release_complete_entry(entry)))
    {
        bail!(
            "publication receipt is incomplete; no publishable crates remain, so inspect the receipt before retrying"
        );
    }
    Ok(())
}

fn new_publish_receipt_with_identity(
    plan: &PublishPlan,
    identity: &PublishIdentity,
) -> PublishReceipt {
    let now = Utc::now().to_rfc3339();
    PublishReceipt {
        schema: PUBLISH_RECEIPT_SCHEMA.to_string(),
        schema_version: PUBLISH_RECEIPT_VERSION,
        workspace_version: plan.workspace_version.clone(),
        generation: 0,
        identity: identity.clone(),
        state: PublishRunState::Planned,
        publish_order: plan.publish_order.clone(),
        crates: plan
            .publish_order
            .iter()
            .map(|name| PublishCrateReceipt {
                name: name.clone(),
                version: plan.workspace_version.clone(),
                state: PublishReceiptState::Planned,
                attempts: 0,
                registry_visible: None,
                dependency_closure: None,
                bootstrap: false,
                reason: None,
                updated_at: now.clone(),
            })
            .collect(),
    }
}

fn load_publish_receipt_for_identity(
    path: &Path,
    plan: &PublishPlan,
    identity: &PublishIdentity,
) -> Result<PublishReceipt> {
    let receipt = recover_publish_receipt(path, plan, identity)?;
    if receipt.schema != PUBLISH_RECEIPT_SCHEMA || receipt.schema_version != PUBLISH_RECEIPT_VERSION
    {
        bail!(
            "publication receipt {} has schema `{}` and schema_version {}; expected `{PUBLISH_RECEIPT_SCHEMA}` version {PUBLISH_RECEIPT_VERSION}",
            path.display(),
            receipt.schema,
            receipt.schema_version
        );
    }
    if &receipt.identity != identity {
        bail!(
            "publication receipt identity does not match the current committed source, registry, plan, manifests, or options"
        );
    }
    if receipt.workspace_version != plan.workspace_version {
        bail!(
            "publication receipt version {} does not match workspace version {}",
            receipt.workspace_version,
            plan.workspace_version
        );
    }
    if receipt.publish_order != plan.publish_order {
        bail!(
            "publication receipt order does not match the current publish plan; rebuild the receipt from the same workspace and filters"
        );
    }
    if receipt.crates.len() != plan.publish_order.len()
        || receipt
            .crates
            .iter()
            .zip(plan.publish_order.iter())
            .any(|(entry, name)| entry.name != *name || entry.version != plan.workspace_version)
    {
        bail!("publication receipt crate entries do not match the current publish plan");
    }
    for entry in &receipt.crates {
        validate_publish_receipt_entry(entry)?;
    }
    if receipt.state == PublishRunState::Complete
        && receipt
            .crates
            .iter()
            .any(|entry| !is_release_complete_entry(entry))
    {
        bail!("complete publication receipt contains a non-terminal crate entry");
    }
    Ok(receipt)
}

#[cfg(test)]
fn fixture_publish_identity(plan: &PublishPlan) -> PublishIdentity {
    PublishIdentity {
        source_commit: "fixture-commit".to_string(),
        source_tree: "fixture-tree".to_string(),
        registry_id: PUBLISH_REGISTRY_ID.to_string(),
        workspace_version: plan.workspace_version.clone(),
        plan_digest: "fixture-plan".to_string(),
        options_digest: "fixture-options".to_string(),
        manifest_digests: BTreeMap::new(),
    }
}

#[cfg(test)]
fn new_publish_receipt(plan: &PublishPlan) -> PublishReceipt {
    new_publish_receipt_with_identity(plan, &fixture_publish_identity(plan))
}

#[cfg(test)]
fn load_publish_receipt(path: &Path, plan: &PublishPlan) -> Result<PublishReceipt> {
    load_publish_receipt_for_identity(path, plan, &fixture_publish_identity(plan))
}

fn recovery_prefixes(path: &Path) -> Result<(PathBuf, String, String)> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("publication receipt path has no file name"))?
        .to_string_lossy();
    Ok((
        parent,
        format!(".{file_name}.backup-"),
        format!(".{file_name}.tmp-"),
    ))
}

fn has_publish_recovery_candidates(path: &Path) -> Result<bool> {
    let (parent, backup_prefix, temp_prefix) = recovery_prefixes(path)?;
    Ok(fs::read_dir(parent)?.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with(&backup_prefix) || name.starts_with(&temp_prefix)
    }))
}

fn recover_publish_receipt(
    path: &Path,
    plan: &PublishPlan,
    identity: &PublishIdentity,
) -> Result<PublishReceipt> {
    let (parent, backup_prefix, temp_prefix) = recovery_prefixes(path)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("publication receipt path has no file name"))?
        .to_string_lossy();
    let mut candidates = vec![path.to_path_buf()];
    if let Ok(entries) = fs::read_dir(&parent) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&backup_prefix) || name.starts_with(&temp_prefix) {
                candidates.push(entry.path());
            }
        }
    }

    let mut valid: Vec<(PathBuf, PublishReceipt, Vec<u8>)> = Vec::new();
    let mut canonical_error = None;
    for candidate in candidates {
        match fs::read(&candidate).and_then(|bytes| {
            serde_json::from_slice::<PublishReceipt>(&bytes)
                .map(|receipt| (receipt, bytes))
                .map_err(std::io::Error::other)
        }) {
            Ok((receipt, bytes)) => match validate_recovery_candidate(&receipt, plan, identity) {
                Ok(()) => valid.push((candidate, receipt, bytes)),
                Err(error) if candidate == path => canonical_error = Some(error),
                Err(_) => {}
            },
            Err(error) if candidate == path => canonical_error = Some(error.into()),
            _ => {}
        }
    }
    let max_generation = valid
        .iter()
        .map(|(_, receipt, _)| receipt.generation)
        .max()
        .ok_or_else(|| {
            anyhow!(
                "no valid publication receipt or recovery candidate for {}{}",
                path.display(),
                canonical_error
                    .as_ref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            )
        })?;
    let mut newest = valid
        .into_iter()
        .filter(|(_, receipt, _)| receipt.generation == max_generation);
    let chosen = newest
        .next()
        .ok_or_else(|| anyhow!("publication receipt recovery candidate disappeared"))?;
    for (_, _, bytes) in newest {
        if bytes != chosen.2 {
            bail!(
                "publication receipt recovery found divergent generation {max_generation} candidates"
            );
        }
    }
    if chosen.0 != path {
        let recovery_temp = parent.join(format!(
            ".{file_name}.tmp-recovery-{}-{}",
            std::process::id(),
            receipt_nonce()
        ));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&recovery_temp)?;
        file.write_all(&chosen.2)?;
        file.sync_all()?;
        drop(file);
        install_publish_receipt(&recovery_temp, path).with_context(|| {
            format!(
                "recover publication receipt {} from {}",
                path.display(),
                chosen.0.display()
            )
        })?;
    }
    Ok(chosen.1)
}

fn validate_recovery_candidate(
    receipt: &PublishReceipt,
    plan: &PublishPlan,
    identity: &PublishIdentity,
) -> Result<()> {
    if receipt.schema != PUBLISH_RECEIPT_SCHEMA || receipt.schema_version != PUBLISH_RECEIPT_VERSION
    {
        bail!("publication receipt schema does not match the current publisher");
    }
    if receipt.identity != *identity {
        bail!("publication receipt identity does not match the current publish run");
    }
    if receipt.workspace_version != plan.workspace_version
        || receipt.publish_order != plan.publish_order
        || receipt.crates.len() != plan.publish_order.len()
        || receipt
            .crates
            .iter()
            .zip(&plan.publish_order)
            .any(|(entry, name)| entry.name != *name || entry.version != plan.workspace_version)
    {
        bail!("publication receipt entries do not match the current publish plan");
    }
    for entry in &receipt.crates {
        validate_publish_receipt_entry(entry)?;
    }
    if receipt.state == PublishRunState::Complete
        && receipt
            .crates
            .iter()
            .any(|entry| !is_release_complete_entry(entry))
    {
        bail!("complete publication receipt contains a non-terminal crate entry");
    }
    Ok(())
}

fn write_publish_receipt(path: &Path, receipt: &PublishReceipt) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create publication receipt parent {}", parent.display()))?;
    }
    let prior_generation = if path.exists() {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PublishReceipt>(&bytes).ok())
            .map_or(0, |prior| prior.generation)
    } else {
        0
    };
    let mut snapshot = receipt.clone();
    snapshot.generation = prior_generation.max(receipt.generation).saturating_add(1);
    let json = serde_json::to_string_pretty(&snapshot).context("serialize publication receipt")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("publication receipt path has no file name"))?
        .to_string_lossy();
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{}-{}",
        std::process::id(),
        receipt_nonce(),
        RECEIPT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut temp = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .with_context(|| {
            format!(
                "create temporary publication receipt {}",
                temp_path.display()
            )
        })?;
    let write_result = (|| -> Result<()> {
        temp.write_all(format!("{json}\n").as_bytes())?;
        temp.sync_all()
            .context("sync temporary publication receipt")?;
        Ok(())
    })();
    drop(temp);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("write publication receipt {}", path.display()));
    }
    install_publish_receipt(&temp_path, path)
}

#[cfg(not(windows))]
fn install_publish_receipt(temp_path: &Path, path: &Path) -> Result<()> {
    fs::rename(temp_path, path)
        .with_context(|| format!("install publication receipt {}", path.display()))?;
    Ok(())
}

#[cfg(windows)]
fn install_publish_receipt(temp_path: &Path, path: &Path) -> Result<()> {
    if !path.exists() {
        fs::rename(temp_path, path)
            .with_context(|| format!("install publication receipt {}", path.display()))?;
        return Ok(());
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("publication receipt path has no file name"))?
        .to_string_lossy();
    let backup_path = parent.join(format!(
        ".{file_name}.backup-{}-{}-{}",
        std::process::id(),
        receipt_nonce(),
        RECEIPT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::rename(path, &backup_path)
        .with_context(|| format!("stage existing publication receipt {}", path.display()))?;

    if let Err(error) = fs::rename(temp_path, path) {
        let restore_result = fs::rename(&backup_path, path);
        let _ = fs::remove_file(temp_path);
        if let Err(restore_error) = restore_result {
            return Err(anyhow!(
                "install publication receipt failed: {error}; restoring the previous receipt also failed: {restore_error}; previous receipt remains at {}",
                backup_path.display()
            ));
        }
        return Err(error)
            .with_context(|| format!("install publication receipt {}", path.display()));
    }

    let _ = fs::remove_file(backup_path);
    Ok(())
}

fn validate_publish_receipt_entry(entry: &PublishCrateReceipt) -> Result<()> {
    let valid = match (&entry.state, entry.attempts, entry.reason.as_deref()) {
        (PublishReceiptState::Planned, 0, None) => true,
        (PublishReceiptState::InProgress, attempts, None) if attempts > 0 => true,
        (
            PublishReceiptState::Published | PublishReceiptState::AlreadyPresent,
            attempts,
            reason,
        ) if (attempts > 0 || entry.state == PublishReceiptState::AlreadyPresent)
            && reason.is_none_or(|reason| !reason.trim().is_empty()) =>
        {
            true
        }
        (PublishReceiptState::Yanked, _, Some(reason)) if !reason.trim().is_empty() => true,
        (PublishReceiptState::Failed, attempts, Some(reason))
            if attempts > 0 && !reason.trim().is_empty() =>
        {
            true
        }
        (PublishReceiptState::Blocked, _, Some(reason)) if !reason.trim().is_empty() => true,
        _ => false,
    };
    if !valid {
        bail!(
            "publication receipt entry `{}` has inconsistent state, attempts, or reason",
            entry.name
        );
    }
    if entry.dependency_closure == Some(false) {
        bail!(
            "publication receipt entry `{}` records a failed dependency-closure proof",
            entry.name
        );
    }
    let visibility_valid = match (&entry.state, entry.attempts, entry.registry_visible) {
        (_, _, None) => true,
        (
            PublishReceiptState::Planned
            | PublishReceiptState::InProgress
            | PublishReceiptState::Failed
            | PublishReceiptState::Blocked,
            _,
            Some(false),
        ) => true,
        (
            PublishReceiptState::Published | PublishReceiptState::AlreadyPresent,
            attempts,
            Some(_),
        ) if attempts > 0 => true,
        (PublishReceiptState::AlreadyPresent, 0, Some(true)) => true,
        (PublishReceiptState::Yanked, attempts, Some(false)) if attempts > 0 => true,
        (PublishReceiptState::Yanked, 0, Some(false)) => true,
        _ => false,
    };
    if !visibility_valid {
        bail!(
            "publication receipt entry `{}` has registry visibility evidence inconsistent with its state",
            entry.name
        );
    }
    Ok(())
}

fn inventory_publish_plan(plan: &PublishPlan) -> BTreeMap<String, RegistryVersionLookup> {
    let mut inventory = BTreeMap::new();
    for (index, name) in plan.publish_order.iter().enumerate() {
        if index > 0 {
            sleep(REGISTRY_REQUEST_DELAY);
        }
        inventory.insert(
            name.clone(),
            query_registry_version(name, &plan.workspace_version),
        );
    }
    inventory
}

fn reconcile_inventory(
    path: &Path,
    receipt: &mut PublishReceipt,
    inventory: &BTreeMap<String, RegistryVersionLookup>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let mut blockers = Vec::new();
    let was_complete = receipt.state == PublishRunState::Complete;
    for entry in &mut receipt.crates {
        let lookup = inventory
            .get(&entry.name)
            .ok_or_else(|| anyhow!("registry inventory omitted planned crate `{}`", entry.name))?;
        match lookup.state {
            "present" => {
                if entry.state != PublishReceiptState::Published {
                    transition_publish_receipt_entry(entry, PublishReceiptState::AlreadyPresent)?;
                }
                entry.registry_visible = Some(true);
                entry.reason = None;
                entry.updated_at = now.clone();
            }
            "missing" => {
                entry.registry_visible = Some(false);
                if matches!(
                    entry.state,
                    PublishReceiptState::Published
                        | PublishReceiptState::AlreadyPresent
                        | PublishReceiptState::InProgress
                        | PublishReceiptState::Yanked
                ) || was_complete
                {
                    blockers.push(format!(
                        "{} is missing although the receipt records {:?}; retry inventory later and do not re-upload blindly",
                        entry.name, entry.state
                    ));
                } else {
                    transition_publish_receipt_entry(entry, PublishReceiptState::Planned)?;
                    entry.reason = None;
                }
                entry.updated_at = now.clone();
            }
            "yanked" => {
                transition_publish_receipt_entry(entry, PublishReceiptState::Yanked)?;
                entry.registry_visible = Some(false);
                entry.reason = Some("exact registry version is yanked".to_string());
                entry.updated_at = now.clone();
                blockers.push(format!("{} exact version is yanked", entry.name));
            }
            state => {
                entry.registry_visible = None;
                entry.updated_at = now.clone();
                blockers.push(format!(
                    "{} registry inventory is unavailable ({state}): {}",
                    entry.name,
                    lookup.error.as_deref().unwrap_or("no detail")
                ));
            }
        }
    }
    receipt.state = completed_publish_run_state(receipt);
    write_publish_receipt(path, receipt)?;
    if !blockers.is_empty() {
        bail!(
            "publication stopped before mutation because live registry inventory did not pass:\n  - {}",
            blockers.join("\n  - ")
        );
    }
    Ok(())
}

fn crates_to_publish(
    plan: &PublishPlan,
    start_idx: usize,
    receipt: Option<&PublishReceipt>,
) -> Vec<String> {
    plan.publish_order
        .get(start_idx..)
        .unwrap_or_default()
        .iter()
        .filter(|name| {
            receipt.is_none_or(|receipt| {
                receipt
                    .crates
                    .iter()
                    .find(|entry| entry.name == **name)
                    .is_none_or(|entry| !is_terminal_publish_entry(entry))
            })
        })
        .cloned()
        .collect()
}

fn is_terminal_publish_entry(entry: &PublishCrateReceipt) -> bool {
    match &entry.state {
        PublishReceiptState::Published | PublishReceiptState::AlreadyPresent => {
            entry.registry_visible == Some(true)
        }
        // A yanked version is terminal for resume purposes, but it does not
        // prove a usable release and therefore keeps the run incomplete.
        PublishReceiptState::Yanked => true,
        _ => false,
    }
}

fn is_release_complete_entry(entry: &PublishCrateReceipt) -> bool {
    matches!(
        &entry.state,
        PublishReceiptState::Published | PublishReceiptState::AlreadyPresent
    ) && entry.registry_visible == Some(true)
}

fn completed_publish_run_state(receipt: &PublishReceipt) -> PublishRunState {
    if !receipt.crates.is_empty() && receipt.crates.iter().all(is_release_complete_entry) {
        PublishRunState::Complete
    } else {
        PublishRunState::Incomplete
    }
}

fn pending_visibility_state(
    receipt: Option<&PublishReceipt>,
    crate_name: &str,
) -> Option<PublishReceiptState> {
    receipt
        .and_then(|receipt| receipt.crates.iter().find(|entry| entry.name == crate_name))
        .and_then(|entry| match &entry.state {
            PublishReceiptState::Published | PublishReceiptState::AlreadyPresent
                if entry.registry_visible != Some(true) =>
            {
                Some(entry.state.clone())
            }
            _ => None,
        })
}

fn update_publish_receipt(
    path: &Path,
    receipt: &mut PublishReceipt,
    crate_name: &str,
    state: PublishReceiptState,
    reason: Option<String>,
    increment_attempt: bool,
) -> Result<()> {
    update_publish_receipt_entry(receipt, crate_name, state, reason, increment_attempt)?;
    write_publish_receipt(path, receipt)
}

fn update_publish_receipt_entry(
    receipt: &mut PublishReceipt,
    crate_name: &str,
    state: PublishReceiptState,
    reason: Option<String>,
    increment_attempt: bool,
) -> Result<()> {
    let entry = receipt
        .crates
        .iter_mut()
        .find(|entry| entry.name == crate_name)
        .ok_or_else(|| anyhow!("publication receipt is missing crate `{crate_name}`"))?;
    transition_publish_receipt_entry(entry, state)?;
    if increment_attempt {
        entry.attempts = entry.attempts.saturating_add(1);
    }
    entry.reason = reason;
    entry.updated_at = Utc::now().to_rfc3339();
    receipt.state = PublishRunState::InProgress;
    Ok(())
}

fn transition_publish_receipt_entry(
    entry: &mut PublishCrateReceipt,
    state: PublishReceiptState,
) -> Result<()> {
    if !valid_publish_receipt_transition(&entry.state, &state) {
        bail!(
            "publication receipt entry `{}` cannot transition from {:?} to {:?}",
            entry.name,
            entry.state,
            state
        );
    }
    entry.state = state;
    Ok(())
}

fn valid_publish_receipt_transition(
    current: &PublishReceiptState,
    next: &PublishReceiptState,
) -> bool {
    current == next
        || matches!(
            (current, next),
            (
                PublishReceiptState::Planned,
                PublishReceiptState::InProgress
                    | PublishReceiptState::AlreadyPresent
                    | PublishReceiptState::Yanked
                    | PublishReceiptState::Blocked
            ) | (
                PublishReceiptState::InProgress,
                PublishReceiptState::Published
                    | PublishReceiptState::AlreadyPresent
                    | PublishReceiptState::Yanked
                    | PublishReceiptState::Failed
                    | PublishReceiptState::Blocked
            ) | (
                PublishReceiptState::Published | PublishReceiptState::AlreadyPresent,
                PublishReceiptState::Yanked
            ) | (
                PublishReceiptState::Failed | PublishReceiptState::Blocked,
                PublishReceiptState::Planned
                    | PublishReceiptState::InProgress
                    | PublishReceiptState::AlreadyPresent
                    | PublishReceiptState::Yanked
            ) | (
                PublishReceiptState::Yanked,
                PublishReceiptState::AlreadyPresent
            )
        )
}

fn update_publish_receipt_visibility_entry(
    receipt: &mut PublishReceipt,
    crate_name: &str,
    lookup: &RegistryVersionLookup,
) -> Result<()> {
    let entry = receipt
        .crates
        .iter_mut()
        .find(|entry| entry.name == crate_name)
        .ok_or_else(|| anyhow!("publication receipt is missing crate `{crate_name}`"))?;
    entry.registry_visible = match lookup.state {
        "present" => Some(true),
        "missing" | "yanked" => Some(false),
        _ => None,
    };
    entry.updated_at = Utc::now().to_rfc3339();
    Ok(())
}

fn update_publish_receipt_with_visibility(
    path: &Path,
    receipt: &mut PublishReceipt,
    crate_name: &str,
    state: PublishReceiptState,
    reason: Option<String>,
    increment_attempt: bool,
    lookup: &RegistryVersionLookup,
) -> Result<()> {
    update_publish_receipt_entry(receipt, crate_name, state, reason, increment_attempt)?;
    update_publish_receipt_visibility_entry(receipt, crate_name, lookup)?;
    write_publish_receipt(path, receipt)
}

fn publish_bootstrap_decision(
    receipt: Option<&mut PublishReceipt>,
    crate_name: &str,
    selected: bool,
    dry_run: bool,
) -> Result<bool> {
    let Some(receipt) = receipt else {
        return Ok(selected);
    };
    let entry = receipt
        .crates
        .iter_mut()
        .find(|entry| entry.name == crate_name)
        .ok_or_else(|| anyhow!("publication receipt is missing crate `{crate_name}`"))?;
    if dry_run {
        return Ok(selected);
    }
    if entry.attempts == 0 && entry.state == PublishReceiptState::Planned {
        entry.bootstrap = selected;
        return Ok(selected);
    }
    if entry.bootstrap != selected {
        bail!(
            "publication receipt bootstrap decision for `{crate_name}` is {}; current selection is {selected}",
            entry.bootstrap
        );
    }
    Ok(entry.bootstrap)
}
#[derive(Debug, Serialize)]
struct RegistryInventoryReceipt {
    schema_version: &'static str,
    generated_at: String,
    workspace_version: String,
    dependency_check_scope: &'static str,
    status: &'static str,
    crates: Vec<RegistryCrateReceipt>,
}

#[derive(Debug, Serialize)]
struct RegistryCrateReceipt {
    name: String,
    version: String,
    state: &'static str,
    dependencies_resolvable: bool,
    published_at: Option<String>,
    error: Option<String>,
}

/// Schema version for the crates.io registry inventory receipt.
pub const PUBLISH_REGISTRY_SCHEMA_VERSION: &str = "tokmd.publish_registry.v1";

/// Delay between consecutive crates.io API requests.
///
/// crates.io asks API clients to rate-limit themselves. Without this the
/// 16-crate publish surface issues 16 back-to-back requests and reliably
/// earns HTTP 429, which would misreport published crates as `unavailable`.
const REGISTRY_REQUEST_DELAY: Duration = Duration::from_millis(1_000);
/// Maximum number of bounded visibility observations after an upload.
const REGISTRY_VISIBILITY_ATTEMPTS: u32 = 3;

/// Maximum number of retries for a single rate-limited crates.io request.
const REGISTRY_RATE_LIMIT_RETRIES: u32 = 3;

/// Fallback backoff when a rate-limit response omits a usable `Retry-After`.
const REGISTRY_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct RegistryVersionLookup {
    state: &'static str,
    published_at: Option<String>,
    error: Option<String>,
}

/// Query the crates.io registry for every crate in the resolved publish plan.
///
/// The receipt is deliberately written before the command returns an error.
/// `dependencies_resolvable` covers the internal publish-plan edges; a clean
/// external `cargo install` remains the proof for the complete dependency graph.
fn run_registry_inventory(
    metadata: &Metadata,
    plan: &PublishPlan,
    output_path: &Path,
) -> Result<()> {
    let mut receipts = Vec::with_capacity(plan.publish_order.len());
    for (index, name) in plan.publish_order.iter().enumerate() {
        if index > 0 {
            sleep(REGISTRY_REQUEST_DELAY);
        }
        let lookup = query_registry_version(name, &plan.workspace_version);
        receipts.push(RegistryCrateReceipt {
            name: name.clone(),
            version: plan.workspace_version.clone(),
            state: lookup.state,
            dependencies_resolvable: false,
            published_at: lookup.published_at,
            error: lookup.error,
        });
    }

    let present: BTreeSet<String> = receipts
        .iter()
        .filter(|receipt| receipt.state == "present")
        .map(|receipt| receipt.name.clone())
        .collect();
    let publishable: BTreeSet<String> = plan.publish_order.iter().cloned().collect();
    let packages: BTreeMap<&str, &Package> = metadata
        .packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

    for receipt in &mut receipts {
        if receipt.state != "present" {
            continue;
        }
        let Some(package) = packages.get(receipt.name.as_str()) else {
            receipt.error = Some("publish-plan package metadata is missing".to_string());
            continue;
        };
        receipt.dependencies_resolvable = package
            .dependencies
            .iter()
            .filter(|dependency| is_publish_dependency(&dependency.kind))
            .filter(|dependency| publishable.contains(dependency.name.as_str()))
            .all(|dependency| present.contains(dependency.name.as_str()));
    }

    let complete = registry_inventory_is_complete(&receipts);
    let receipt = RegistryInventoryReceipt {
        schema_version: PUBLISH_REGISTRY_SCHEMA_VERSION,
        generated_at: Utc::now().to_rfc3339(),
        workspace_version: plan.workspace_version.clone(),
        dependency_check_scope: "workspace_internal_publish_edges",
        status: if complete { "passed" } else { "failed" },
        crates: receipts,
    };

    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create registry inventory parent {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&receipt).context("serialize registry inventory")?;
    fs::write(output_path, format!("{json}\n"))
        .with_context(|| format!("write registry inventory {}", output_path.display()))?;
    println!(
        "Registry inventory written to {} ({})",
        output_path.display(),
        receipt.status
    );

    if !complete {
        bail!(
            "registry inventory is incomplete; inspect {}",
            output_path.display()
        );
    }
    Ok(())
}

/// Decide whether a registry inventory proves every planned crate is publishable.
///
/// An empty plan is deliberately *not* complete. `--crates`/`--exclude` filters
/// can reduce the publish order to nothing, and a receipt claiming `passed` over
/// zero crates would assert publishability the registry never confirmed.
fn registry_inventory_is_complete(receipts: &[RegistryCrateReceipt]) -> bool {
    !receipts.is_empty()
        && receipts
            .iter()
            .all(|receipt| receipt.state == "present" && receipt.dependencies_resolvable)
}

fn query_registry_version(crate_name: &str, version: &str) -> RegistryVersionLookup {
    if !crate_name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return RegistryVersionLookup {
            state: "unavailable",
            published_at: None,
            error: Some("crate name contains an unsafe URL character".to_string()),
        };
    }

    let mut attempt = 0;
    loop {
        let fetched = match fetch_registry_crate(crate_name) {
            Ok(fetched) => fetched,
            Err(error) => {
                return RegistryVersionLookup {
                    state: "unavailable",
                    published_at: None,
                    error: Some(error),
                };
            }
        };

        // 429 means "ask again later", not "this version does not exist".
        // Classifying it immediately would report a published crate as
        // unavailable and fail an otherwise-complete inventory.
        if is_registry_rate_limited(fetched.status) && attempt < REGISTRY_RATE_LIMIT_RETRIES {
            let backoff = fetched
                .retry_after
                .as_deref()
                .and_then(parse_retry_after)
                .unwrap_or(REGISTRY_RATE_LIMIT_BACKOFF * (attempt + 1));
            eprintln!(
                "crates.io rate-limited {crate_name} (HTTP {}); retrying in {}s",
                fetched.status,
                backoff.as_secs()
            );
            sleep(backoff);
            attempt += 1;
            continue;
        }

        return parse_registry_version_response(crate_name, version, fetched.status, &fetched.body);
    }
}

fn wait_for_registry_visibility(
    crate_name: &str,
    version: &str,
    interval: u64,
) -> RegistryVersionLookup {
    let mut lookup = RegistryVersionLookup {
        state: "unavailable",
        published_at: None,
        error: Some("registry visibility was not observed".to_string()),
    };
    for attempt in 0..REGISTRY_VISIBILITY_ATTEMPTS {
        lookup = query_registry_version(crate_name, version);
        if matches!(lookup.state, "present" | "yanked") {
            return lookup;
        }
        if attempt + 1 < REGISTRY_VISIBILITY_ATTEMPTS && interval > 0 {
            sleep(Duration::from_secs(interval));
        }
    }
    lookup
}

/// A single crates.io API response, reduced to what classification needs.
#[derive(Debug)]
struct RegistryFetch {
    status: u16,
    body: String,
    retry_after: Option<String>,
}

/// HTTP statuses crates.io uses to ask a client to slow down.
fn is_registry_rate_limited(status: u16) -> bool {
    matches!(status, 429 | 503)
}

/// Parse a `Retry-After` header value expressed in delta-seconds.
///
/// crates.io sends the delta-seconds form. The HTTP-date form is intentionally
/// unhandled: falling back to the caller's bounded backoff is safer than
/// trusting a parsed absolute time from a rate-limit response.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds: u64 = value.trim().parse().ok()?;
    // Cap the honored delay so a hostile or misconfigured header cannot stall
    // the inventory indefinitely.
    Some(Duration::from_secs(seconds.min(60)))
}

fn fetch_registry_crate(crate_name: &str) -> Result<RegistryFetch, String> {
    let curl = if cfg!(windows) { "curl.exe" } else { "curl" };
    let url = format!("https://crates.io/api/v1/crates/{crate_name}");
    let output = Command::new(curl)
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "20",
            "--max-time",
            "60",
            "--user-agent",
            "tokmd-xtask-registry-inventory",
            "--dump-header",
            "-",
            "--write-out",
            "\n%{http_code}",
            &url,
        ])
        .output()
        .map_err(|error| format!("failed to execute {curl}: {error}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let response = String::from_utf8_lossy(&output.stdout);
    let (headers_and_body, status) = response
        .trim_end()
        .rsplit_once('\n')
        .ok_or_else(|| "crates.io response omitted HTTP status".to_string())?;
    let status: u16 = status
        .parse()
        .map_err(|error| format!("invalid crates.io HTTP status: {error}"))?;

    let (headers, body) = split_registry_headers(headers_and_body);
    Ok(RegistryFetch {
        status,
        body: body.to_string(),
        retry_after: header_value(headers, "retry-after"),
    })
}

/// Split `--dump-header -` output from the response body.
///
/// `--location` can emit several header blocks, so the body starts after the
/// *last* blank line separating a header block from what follows it.
fn split_registry_headers(response: &str) -> (&str, &str) {
    response
        .rsplit_once("\r\n\r\n")
        .or_else(|| response.rsplit_once("\n\n"))
        .unwrap_or(("", response))
}

/// Case-insensitively read a header value from a raw header block.
fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().rev().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn parse_registry_version_response(
    crate_name: &str,
    version: &str,
    http_status: u16,
    body: &str,
) -> RegistryVersionLookup {
    if http_status == 404 {
        return RegistryVersionLookup {
            state: "missing",
            published_at: None,
            error: None,
        };
    }
    if http_status != 200 {
        return RegistryVersionLookup {
            state: "unavailable",
            published_at: None,
            error: Some(format!("crates.io returned HTTP {http_status}")),
        };
    }

    let parsed: Value = match serde_json::from_str(body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return RegistryVersionLookup {
                state: "unavailable",
                published_at: None,
                error: Some(format!(
                    "invalid crates.io response for {crate_name}: {error}"
                )),
            };
        }
    };
    let Some(versions) = parsed.get("versions").and_then(Value::as_array) else {
        return RegistryVersionLookup {
            state: "unavailable",
            published_at: None,
            error: Some("crates.io response omitted versions".to_string()),
        };
    };
    let Some(version_entry) = versions
        .iter()
        .find(|entry| entry.get("num").and_then(Value::as_str) == Some(version))
    else {
        return RegistryVersionLookup {
            state: "missing",
            published_at: None,
            error: None,
        };
    };
    // Every other unexpected response shape here fails closed. An absent or
    // non-boolean `yanked` field must too, otherwise the receipt would call a
    // crate publishable on evidence the registry never gave.
    let Some(yanked) = version_entry.get("yanked").and_then(Value::as_bool) else {
        return RegistryVersionLookup {
            state: "unavailable",
            published_at: None,
            error: Some("crates.io version entry omitted a boolean yanked field".to_string()),
        };
    };
    RegistryVersionLookup {
        state: if yanked { "yanked" } else { "present" },
        published_at: version_entry
            .get("created_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        error: None,
    }
}

/// Resolve the publish plan from workspace metadata.
///
/// This is the critical safety function that ensures we only consider
/// workspace members, not external dependencies.
fn resolve_publish_plan(metadata: &Metadata, args: &PublishArgs) -> Result<PublishPlan> {
    // Step 1: Get workspace members only (SAFETY: this is the critical filter)
    let workspace_member_ids: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let workspace_root = metadata.workspace_root.as_std_path();

    // Step 2: Build the set of publishable workspace packages
    let workspace_packages: Vec<&Package> = metadata
        .packages
        .iter()
        .filter(|pkg| workspace_member_ids.contains(&pkg.id))
        .collect();

    // Step 3: Determine which crates are publishable
    let mut publishable: BTreeSet<String> = BTreeSet::new();
    let mut exclusion_reasons: BTreeMap<String, ExclusionReason> = BTreeMap::new();

    for pkg in &workspace_packages {
        let name = pkg.name.as_str();

        // Check publish = false
        if pkg.publish.as_ref().is_some_and(|p| p.is_empty()) {
            exclusion_reasons.insert(name.to_string(), ExclusionReason::NotPublishable);
            continue;
        }
        if pkg
            .publish
            .as_ref()
            .is_some_and(|registries| !registries.iter().any(|registry| registry == "crates-io"))
        {
            bail!("workspace crate `{name}` does not allow publication to the crates-io registry");
        }

        // Skip xtask and fuzz by convention
        if name == "xtask" {
            exclusion_reasons.insert(name.to_string(), ExclusionReason::IsXtask);
            continue;
        }
        if name == "tokmd-fuzz" || name == "fuzz" {
            exclusion_reasons.insert(name.to_string(), ExclusionReason::IsFuzz);
            continue;
        }

        // Belt-and-suspenders: verify manifest is under workspace root
        let manifest_path = pkg.manifest_path.as_std_path();
        if !manifest_path.starts_with(workspace_root) {
            exclusion_reasons.insert(name.to_string(), ExclusionReason::NotPublishable);
            continue;
        }

        publishable.insert(name.to_string());
    }

    // Step 4: Validate publishable crates don't depend on non-publishable workspace crates
    validate_no_unpublishable_deps(&workspace_packages, &publishable, &exclusion_reasons)?;

    // Step 5: Compute topological order for publishable crates
    let publish_order = compute_publish_order(&workspace_packages, &publishable)?;

    // Step 6: Apply --crates filter (with transitive dependencies)
    let mut inclusion_reasons: BTreeMap<String, InclusionReason> = BTreeMap::new();
    let to_publish: BTreeSet<String> = if let Some(ref crates) = args.crates {
        let requested: HashSet<_> = crates.iter().cloned().collect();

        // Validate requested crates exist and are publishable
        for name in &requested {
            if !publishable.contains(name) {
                if let Some(reason) = exclusion_reasons.get(name) {
                    bail!("Crate '{}' cannot be published: {:?}", name, reason);
                }
                bail!(
                    "Crate '{}' is not a workspace member or does not exist",
                    name
                );
            }
        }

        let mut result = BTreeSet::new();

        // Add requested crates
        for name in &requested {
            result.insert(name.clone());
            inclusion_reasons.insert(name.clone(), InclusionReason::Explicit);
        }

        // Add transitive workspace dependencies
        for name in requested.iter() {
            add_transitive_deps(
                name,
                &workspace_packages,
                &publishable,
                &mut result,
                &mut inclusion_reasons,
            );
        }

        // Mark crates not in result as NotRequested
        for name in &publishable {
            if !result.contains(name) {
                exclusion_reasons.insert(name.clone(), ExclusionReason::NotRequested);
            }
        }

        result
    } else {
        // No filter: publish all publishable crates
        for name in &publishable {
            inclusion_reasons.insert(name.clone(), InclusionReason::Default);
        }
        publishable
    };

    // Step 7: Apply --exclude filter with validation
    let final_set: BTreeSet<String> = if let Some(ref excludes) = args.exclude {
        let exclude_set: HashSet<_> = excludes.iter().collect();

        // Validate exclusions don't break required dependencies
        for name in &to_publish {
            if exclude_set.contains(name) {
                continue;
            }
            let pkg = workspace_packages
                .iter()
                .find(|p| p.name == *name)
                .ok_or_else(|| anyhow!("Package {} not found in workspace", name))?;
            for dep in &pkg.dependencies {
                if !is_publish_dependency(&dep.kind) {
                    continue;
                }
                if exclude_set.contains(&dep.name) && to_publish.contains(&dep.name) {
                    bail!(
                        "Cannot exclude '{}': crate '{}' depends on it",
                        dep.name,
                        name
                    );
                }
            }
        }

        // Apply exclusions
        let mut result = to_publish.clone();
        for name in excludes {
            if result.remove(name) {
                inclusion_reasons.remove(name);
                exclusion_reasons.insert(name.clone(), ExclusionReason::ExplicitExclude);
            }
        }
        result
    } else {
        to_publish
    };

    // Step 8: Filter publish_order to final set
    let filtered_order: Vec<_> = publish_order
        .into_iter()
        .filter(|name| final_set.contains(name))
        .collect();
    let predecessors: BTreeMap<String, BTreeSet<String>> = filtered_order
        .iter()
        .map(|name| {
            let dependencies = workspace_packages
                .iter()
                .find(|package| package.name == *name)
                .map(|package| {
                    package
                        .dependencies
                        .iter()
                        .filter(|dependency| is_publish_dependency(&dependency.kind))
                        .filter(|dependency| final_set.contains(&dependency.name))
                        .map(|dependency| dependency.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            (name.clone(), dependencies)
        })
        .collect();

    // Get workspace version
    let workspace_version = metadata
        .packages
        .iter()
        .find(|p| p.name == "tokmd")
        .map(|p| p.version.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(PublishPlan {
        publish_order: filtered_order,
        inclusion_reasons,
        exclusion_reasons,
        workspace_version,
        predecessors,
    })
}

/// Print the publish plan (for --plan mode).
fn print_plan(plan: &PublishPlan, args: &PublishArgs) {
    println!("=== Publish Plan ===\n");
    println!("Workspace version: {}\n", plan.workspace_version);

    println!("Publish order ({} crates):", plan.publish_order.len());
    for (i, name) in plan.publish_order.iter().enumerate() {
        let reason = plan
            .inclusion_reasons
            .get(name)
            .map(|r| match r {
                InclusionReason::Explicit => " (explicit)".to_string(),
                InclusionReason::TransitiveDep(parent) => format!(" (dep of {})", parent),
                InclusionReason::Default => String::new(),
            })
            .unwrap_or_default();
        println!("  {:2}. {}{}", i + 1, name, reason);
    }

    if !plan.exclusion_reasons.is_empty() && args.verbose {
        println!("\nExcluded crates:");
        for (name, reason) in &plan.exclusion_reasons {
            println!("  - {}: {:?}", name, reason);
        }
    }

    println!("\nFlags:");
    println!("  --dry-run: {}", args.dry_run);
    println!("  --tag: {}", args.tag);
    if args.tag {
        println!(
            "  --tag-format: {} (would create: {})",
            args.tag_format,
            args.tag_format
                .replace("{version}", &plan.workspace_version)
        );
    }
    if let Some(ref bootstrap) = args.bootstrap {
        println!("  --bootstrap: {}", bootstrap.join(","));
    }
    if let Some(ref from) = args.from {
        println!("  --from: {}", from);
    }

    // Reconstruct the execution command from the current args (minus --plan, plus --yes)
    let exec_cmd = reconstruct_publish_command(args);
    println!("\nTo execute this plan:");
    println!("  {}", exec_cmd);
}

/// Reconstruct the publish command from args, removing --plan and ensuring --yes is present.
fn reconstruct_publish_command(args: &PublishArgs) -> String {
    let mut parts = vec!["cargo xtask publish".to_string()];

    // Scope filters (critical for matching the plan)
    if let Some(ref crates) = args.crates {
        parts.push(format!("--crates {}", crates.join(",")));
    }
    if let Some(ref exclude) = args.exclude {
        parts.push(format!("--exclude {}", exclude.join(",")));
    }
    if let Some(ref bootstrap) = args.bootstrap {
        parts.push(format!("--bootstrap {}", bootstrap.join(",")));
    }
    if let Some(ref from) = args.from {
        parts.push(format!("--from {}", from));
    }

    // Mode flags
    if args.dry_run {
        parts.push("--dry-run".to_string());
    }
    if args.tag {
        parts.push("--tag".to_string());
        if args.tag_format != "v{version}" {
            parts.push(format!("--tag-format \"{}\"", args.tag_format));
        }
    }

    // Skip flags (preserve if user specified them)
    if args.skip_checks {
        parts.push("--skip-checks".to_string());
    }
    if args.skip_tests {
        parts.push("--skip-tests".to_string());
    }
    if args.skip_git_check {
        parts.push("--skip-git-check".to_string());
    }
    if args.skip_changelog_check {
        parts.push("--skip-changelog-check".to_string());
    }
    if args.skip_version_check {
        parts.push("--skip-version-check".to_string());
    }

    // Timing flags (only if non-default)
    if args.interval != 10 {
        parts.push(format!("--interval {}", args.interval));
    }
    if args.retry_delay != 30 {
        parts.push(format!("--retry-delay {}", args.retry_delay));
    }
    if args.rate_limit_timeout != 7200 {
        parts.push(format!("--rate-limit-timeout {}", args.rate_limit_timeout));
    }

    // Always add --yes for non-dry-run (the whole point of this reconstruction)
    if !args.dry_run {
        parts.push("--yes".to_string());
    }

    if args.verbose {
        parts.push("--verbose".to_string());
    }

    parts.join(" ")
}

/// Print pre-publish summary before execution.
fn print_pre_publish_summary(plan: &PublishPlan, args: &PublishArgs, start_idx: usize) {
    let crates_to_publish = plan.publish_order.get(start_idx..).unwrap_or_default();
    let mode = if args.dry_run { "[DRY RUN] " } else { "" };

    println!("\n{}Publishing {} crate(s):", mode, crates_to_publish.len());
    for name in crates_to_publish {
        println!("  - {}", name);
    }
    println!();
}

/// Ask for confirmation before publishing.
fn confirm_publish() -> Result<bool> {
    print!("Proceed with publishing? [y/N] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes"))
}

fn validate_publish_authorization(args: &PublishArgs, stdin_is_terminal: bool) -> Result<()> {
    if !args.dry_run && !args.yes && !stdin_is_terminal {
        bail!(
            "non-interactive publication requires explicit --yes; CI environment variables do not authorize mutation"
        );
    }
    Ok(())
}

trait PublishBackend {
    fn publish(
        &mut self,
        crate_name: &str,
        args: &PublishArgs,
        bootstrap: bool,
    ) -> Result<PublishResult>;

    fn wait_for_visibility(
        &mut self,
        crate_name: &str,
        version: &str,
        interval: u64,
    ) -> RegistryVersionLookup;
}

struct LivePublishBackend;

impl PublishBackend for LivePublishBackend {
    fn publish(
        &mut self,
        crate_name: &str,
        args: &PublishArgs,
        bootstrap: bool,
    ) -> Result<PublishResult> {
        publish_crate_with_retry(crate_name, args, bootstrap)
    }

    fn wait_for_visibility(
        &mut self,
        crate_name: &str,
        version: &str,
        interval: u64,
    ) -> RegistryVersionLookup {
        wait_for_registry_visibility(crate_name, version, interval)
    }
}

/// Execute the publish for a list of crates using the live Cargo/registry path.
fn resume_instruction(receipt_path: Option<&Path>) -> String {
    receipt_path.map_or_else(
        || "rerun after inspecting the failure".to_string(),
        |path| {
            format!(
                "resume with: cargo xtask publish --resume --receipt \"{}\" --yes",
                path.display()
            )
        },
    )
}

fn execute_publish(
    crates: &[String],
    args: &PublishArgs,
    version: &str,
    bootstrap: &BTreeSet<String>,
    predecessors: &BTreeMap<String, BTreeSet<String>>,
    receipt_path: Option<&Path>,
    receipt: Option<&mut PublishReceipt>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut backend = LivePublishBackend;
    execute_publish_with_backend(
        crates,
        args,
        version,
        bootstrap,
        predecessors,
        receipt_path,
        receipt,
        &mut backend,
    )
}

#[allow(clippy::too_many_arguments)] // The test seam mirrors the publish inputs deliberately.
fn execute_publish_with_backend<B: PublishBackend>(
    crates: &[String],
    args: &PublishArgs,
    version: &str,
    bootstrap: &BTreeSet<String>,
    predecessors: &BTreeMap<String, BTreeSet<String>>,
    receipt_path: Option<&Path>,
    mut receipt: Option<&mut PublishReceipt>,
    backend: &mut B,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    let mut unavailable = BTreeSet::new();

    for (idx, crate_name) in crates.iter().enumerate() {
        let position = format!("[{}/{}]", idx + 1, crates.len());
        println!("{} Publishing {}...", position, crate_name);

        let blocked_by: Vec<_> = predecessors
            .get(crate_name)
            .into_iter()
            .flatten()
            .filter(|name| unavailable.contains(*name))
            .cloned()
            .collect();
        if !blocked_by.is_empty() {
            let reason = format!(
                "blocked by unavailable prerequisite(s): {}",
                blocked_by.join(", ")
            );
            failed.push(crate_name.clone());
            unavailable.insert(crate_name.clone());
            if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                update_publish_receipt(
                    path,
                    receipt,
                    crate_name,
                    PublishReceiptState::Blocked,
                    Some(reason.clone()),
                    false,
                )?;
            }
            if !args.continue_on_error {
                bail!(
                    "{crate_name} {reason}. {}",
                    resume_instruction(receipt_path)
                );
            }
            continue;
        }

        if let Some(previous_state) = pending_visibility_state(receipt.as_deref(), crate_name) {
            println!("  Checking registry visibility for {}...", crate_name);
            let lookup = backend.wait_for_visibility(crate_name, version, args.interval);
            let visible = lookup.state == "present";
            if visible {
                println!("  ✓ Registry visibility confirmed for {}", crate_name);
                succeeded.push(crate_name.clone());
            } else {
                println!(
                    "  ✗ Registry visibility remains unproven for {} ({})",
                    crate_name, lookup.state
                );
                failed.push(crate_name.clone());
                unavailable.insert(crate_name.clone());
            }
            if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                let state = if lookup.state == "yanked" {
                    PublishReceiptState::Yanked
                } else {
                    previous_state
                };
                let reason = (!visible).then(|| {
                    lookup.error.clone().unwrap_or_else(|| {
                        format!("registry visibility check ended in `{}`", lookup.state)
                    })
                });
                update_publish_receipt_with_visibility(
                    path, receipt, crate_name, state, reason, false, &lookup,
                )?;
            }
            if !visible && !args.continue_on_error {
                bail!(
                    "Registry visibility for {crate_name} remains unproven. {}",
                    resume_instruction(receipt_path)
                );
            }
            continue;
        }

        let selected_bootstrap = bootstrap.contains(crate_name);
        let effective_bootstrap = publish_bootstrap_decision(
            receipt.as_deref_mut(),
            crate_name,
            selected_bootstrap,
            args.dry_run,
        )?;
        if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
            update_publish_receipt(
                path,
                receipt,
                crate_name,
                PublishReceiptState::InProgress,
                None,
                true,
            )?;
        }

        let result = backend.publish(crate_name, args, effective_bootstrap)?;

        match result {
            PublishResult::Success => {
                let visibility = (!args.dry_run)
                    .then(|| backend.wait_for_visibility(crate_name, version, args.interval));
                if let Some(lookup) = visibility
                    .as_ref()
                    .filter(|lookup| lookup.state != "present")
                {
                    let reason = lookup.error.clone().unwrap_or_else(|| {
                        format!("registry visibility check ended in `{}`", lookup.state)
                    });
                    println!(
                        "  ✗ Published {} but registry visibility was not proven",
                        crate_name
                    );
                    failed.push(crate_name.clone());
                    unavailable.insert(crate_name.clone());
                    if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                        let state = if lookup.state == "yanked" {
                            PublishReceiptState::Yanked
                        } else {
                            PublishReceiptState::Published
                        };
                        update_publish_receipt_with_visibility(
                            path,
                            receipt,
                            crate_name,
                            state,
                            Some(reason),
                            false,
                            lookup,
                        )?;
                    }
                    if !args.continue_on_error {
                        bail!(
                            "Published {crate_name} but registry visibility was not proven. {}",
                            resume_instruction(receipt_path)
                        );
                    }
                    continue;
                }
                println!("  ✓ Published {}", crate_name);
                succeeded.push(crate_name.clone());
                if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                    if let Some(lookup) = visibility.as_ref() {
                        update_publish_receipt_with_visibility(
                            path,
                            receipt,
                            crate_name,
                            PublishReceiptState::Published,
                            None,
                            false,
                            lookup,
                        )?;
                    } else {
                        update_publish_receipt(
                            path,
                            receipt,
                            crate_name,
                            PublishReceiptState::Published,
                            None,
                            false,
                        )?;
                    }
                }
            }
            PublishResult::AlreadyPublished => {
                let visibility = (!args.dry_run)
                    .then(|| backend.wait_for_visibility(crate_name, version, args.interval));
                if let Some(lookup) = visibility
                    .as_ref()
                    .filter(|lookup| lookup.state != "present")
                {
                    let reason = lookup.error.clone().unwrap_or_else(|| {
                        format!("registry visibility check ended in `{}`", lookup.state)
                    });
                    println!(
                        "  ✗ {} was reported already published but registry visibility was not proven",
                        crate_name
                    );
                    failed.push(crate_name.clone());
                    unavailable.insert(crate_name.clone());
                    if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                        let state = if lookup.state == "yanked" {
                            PublishReceiptState::Yanked
                        } else {
                            PublishReceiptState::AlreadyPresent
                        };
                        update_publish_receipt_with_visibility(
                            path,
                            receipt,
                            crate_name,
                            state,
                            Some(reason),
                            false,
                            lookup,
                        )?;
                    }
                    if !args.continue_on_error {
                        bail!(
                            "{crate_name} was reported already published but registry visibility was not proven. {}",
                            resume_instruction(receipt_path)
                        );
                    }
                    continue;
                }
                println!("  ✓ {} already published", crate_name);
                succeeded.push(crate_name.clone());
                if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                    if let Some(lookup) = visibility.as_ref() {
                        update_publish_receipt_with_visibility(
                            path,
                            receipt,
                            crate_name,
                            PublishReceiptState::AlreadyPresent,
                            None,
                            false,
                            lookup,
                        )?;
                    } else {
                        update_publish_receipt(
                            path,
                            receipt,
                            crate_name,
                            PublishReceiptState::AlreadyPresent,
                            None,
                            false,
                        )?;
                    }
                }
            }
            PublishResult::Failed(e) => {
                println!("  ✗ Failed to publish {}: {}", crate_name, e);
                failed.push(crate_name.clone());
                unavailable.insert(crate_name.clone());
                if let (Some(path), Some(receipt)) = (receipt_path, receipt.as_deref_mut()) {
                    update_publish_receipt(
                        path,
                        receipt,
                        crate_name,
                        PublishReceiptState::Failed,
                        Some(e.to_string()),
                        false,
                    )?;
                }

                if !args.continue_on_error {
                    bail!(
                        "Publishing {crate_name} failed. {}",
                        resume_instruction(receipt_path)
                    );
                }
            }
        }
    }

    Ok((succeeded, failed))
}

/// Check if a dependency kind should be considered for publish ordering.
fn is_publish_dependency(kind: &DependencyKind) -> bool {
    matches!(
        kind,
        DependencyKind::Normal | DependencyKind::Build | DependencyKind::Unknown
    )
}

/// Validate the explicitly opted-in development-cycle bootstrap crates.
fn validate_bootstrap_crates(
    crates_to_publish: &[String],
    bootstrap: Option<&[String]>,
) -> Result<BTreeSet<String>> {
    let selected: BTreeSet<String> = bootstrap.unwrap_or_default().iter().cloned().collect();
    let executable: BTreeSet<&str> = crates_to_publish.iter().map(String::as_str).collect();
    let unknown: Vec<_> = selected
        .iter()
        .filter(|name| !executable.contains(name.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        bail!(
            "bootstrap crate(s) are not in the execution set: {}",
            unknown.join(", ")
        );
    }
    Ok(selected)
}
/// Mark every planned crate as having passed the pre-upload dependency check.
fn mark_dependency_closure_verified(receipt: &mut PublishReceipt) {
    for entry in &mut receipt.crates {
        entry.dependency_closure = Some(true);
    }
}

/// Prove that every non-development workspace dependency has a publish target
/// and that its manifest requirement accepts the version in the same plan.
///
/// This is deliberately a metadata check, not a crates.io lookup: registry
/// visibility is recorded separately after each upload and remains a distinct
/// receipt fact.
fn validate_publish_dependency_closure(metadata: &Metadata, plan: &PublishPlan) -> Result<()> {
    let publishable: BTreeSet<&str> = plan.publish_order.iter().map(String::as_str).collect();
    let workspace_member_ids: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let packages: BTreeMap<&str, &Package> = metadata
        .packages
        .iter()
        .filter(|package| workspace_member_ids.contains(&package.id))
        .map(|package| (package.name.as_str(), package))
        .collect();
    let mut violations = Vec::new();

    for name in &plan.publish_order {
        let Some(package) = packages.get(name.as_str()) else {
            violations.push(format!(
                "publish-plan package metadata is missing for '{name}'"
            ));
            continue;
        };

        for dependency in &package.dependencies {
            if !is_publish_dependency(&dependency.kind) {
                continue;
            }
            let Some(target) = packages.get(dependency.name.as_str()) else {
                continue;
            };
            if !publishable.contains(dependency.name.as_str()) {
                violations.push(format!(
                    "'{}' depends on workspace crate '{}' outside the publish plan",
                    package.name, dependency.name
                ));
                continue;
            }
            if !dependency.req.matches(&target.version) {
                violations.push(format!(
                    "'{}' requires '{}' as {}, but the publish plan contains {}",
                    package.name, dependency.name, dependency.req, target.version
                ));
            }
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        bail!(
            "Cannot publish: dependency closure verification failed:\n  - {}",
            violations.join("\n  - ")
        )
    }
}

/// Validate the package file list for every crate before the first upload.
fn validate_publish_packages(plan: &PublishPlan) -> Result<()> {
    let mut failures = Vec::new();
    for crate_name in &plan.publish_order {
        let output = Command::new("cargo")
            .args(["package", "-p", crate_name, "--list", "--locked"])
            .output()
            .with_context(|| format!("spawn cargo package for {crate_name}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let details = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            let details = if details.is_empty() {
                format!("cargo package exited with status {}", output.status)
            } else {
                details.to_string()
            };
            failures.push(format!("{crate_name}: {details}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(
            "Cannot publish: package preflight failed:\n  - {}",
            failures.join("\n  - ")
        )
    }
}

/// Validate that publishable crates don't depend on non-publishable workspace crates.
///
/// This catches the "silent broken publish" case where:
/// - Crate A is publishable
/// - Crate A depends on workspace crate B
/// - Crate B has publish = false (or is otherwise excluded)
fn validate_no_unpublishable_deps(
    packages: &[&Package],
    publishable: &BTreeSet<String>,
    exclusion_reasons: &BTreeMap<String, ExclusionReason>,
) -> Result<()> {
    let workspace_names: HashSet<_> = packages.iter().map(|p| p.name.as_str()).collect();
    let mut errors = Vec::new();

    for pkg in packages {
        if !publishable.contains(pkg.name.as_str()) {
            continue;
        }

        for dep in &pkg.dependencies {
            if !is_publish_dependency(&dep.kind) {
                continue;
            }

            // Only check workspace dependencies
            if !workspace_names.contains(dep.name.as_str()) {
                continue;
            }

            // If the dependency is a workspace crate but not publishable, that's an error
            if !publishable.contains(&dep.name) {
                let reason = exclusion_reasons
                    .get(&dep.name)
                    .map(|r| format!("{:?}", r))
                    .unwrap_or_else(|| "unknown".to_string());
                errors.push(format!(
                    "'{}' depends on non-publishable workspace crate '{}' ({})",
                    pkg.name, dep.name, reason
                ));
            }
        }
    }

    if !errors.is_empty() {
        bail!(
            "Cannot publish: workspace dependency violation(s):\n  - {}",
            errors.join("\n  - ")
        );
    }

    Ok(())
}

/// Compute topological publish order from workspace dependencies.
fn compute_publish_order(
    packages: &[&Package],
    publishable: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut graph = DiGraph::<&str, ()>::new();
    let mut indices = BTreeMap::new();

    // Add publishable crates as nodes
    for pkg in packages {
        if publishable.contains(pkg.name.as_str()) {
            let idx = graph.add_node(pkg.name.as_str());
            indices.insert(pkg.name.as_str(), idx);
        }
    }

    // Add edges: dependency -> dependent (dependency must be published first)
    for pkg in packages {
        if !publishable.contains(pkg.name.as_str()) {
            continue;
        }
        let from_idx = indices[pkg.name.as_str()];

        for dep in &pkg.dependencies {
            if !is_publish_dependency(&dep.kind) {
                continue;
            }

            // Only add edges for publishable workspace crates
            if let Some(&to_idx) = indices.get(dep.name.as_str()) {
                graph.add_edge(to_idx, from_idx, ());
            }
        }
    }

    // Topological sort
    let sorted = toposort(&graph, None).map_err(|cycle| {
        let node = graph[cycle.node_id()];
        anyhow!("Dependency cycle detected involving: {}", node)
    })?;

    Ok(sorted
        .into_iter()
        .map(|idx| graph[idx].to_string())
        .collect())
}

/// Add transitive workspace dependencies to the set.
fn add_transitive_deps(
    crate_name: &str,
    packages: &[&Package],
    publishable: &BTreeSet<String>,
    result: &mut BTreeSet<String>,
    inclusion_reasons: &mut BTreeMap<String, InclusionReason>,
) {
    if let Some(pkg) = packages.iter().find(|p| p.name == crate_name) {
        for dep in &pkg.dependencies {
            if !is_publish_dependency(&dep.kind) {
                continue;
            }

            if publishable.contains(&dep.name) && !result.contains(&dep.name) {
                result.insert(dep.name.clone());
                // Only set reason if not already set (preserve explicit over transitive)
                inclusion_reasons
                    .entry(dep.name.clone())
                    .or_insert_with(|| InclusionReason::TransitiveDep(crate_name.to_string()));
                add_transitive_deps(&dep.name, packages, publishable, result, inclusion_reasons);
            }
        }
    }
}

/// Classify publish errors for retry logic.
#[derive(Debug)]
enum PublishErrorKind {
    /// Dependency not yet visible on crates.io - retryable
    PropagationDelay,
    /// Crate version already exists - treat as success
    AlreadyPublished,
    /// Authentication error - fail fast
    AuthError,
    /// Invalid manifest or missing files - fail fast
    ManifestError,
    /// Network error - potentially retryable
    NetworkError,
    /// Rate limited by crates.io (HTTP 429) - retryable after cooldown
    RateLimited,
    /// Unknown error - fail
    Unknown,
}

/// Classify the stderr output from cargo publish.
fn classify_publish_error(stderr: &str) -> PublishErrorKind {
    let lower = stderr.to_lowercase();

    // Already published - not an error
    if lower.contains("is already uploaded")
        || (lower.contains("crate version") && lower.contains("already exists"))
    {
        return PublishErrorKind::AlreadyPublished;
    }

    // Rate limit (429) - retryable after cooldown
    //
    // Be strict to avoid false positives (e.g. "too many open files").
    let has_status_429 = lower.contains("status 429");
    let has_429_tmr = lower.contains("429 too many requests");
    let has_429 = lower.contains("429");
    let has_rate_limit = lower.contains("rate limit");
    let has_tmr = lower.contains("too many requests");
    let has_crates_io_ctx =
        lower.contains("crates.io") || lower.contains("registry at https://crates.io");
    let has_publish_limit_phrase = lower.contains("you have published too many new crates");
    let has_try_again = lower.contains("try again after");
    let has_help = lower.contains("help@crates.io");

    if has_status_429
        || has_429_tmr
        || (has_429 && has_rate_limit)
        || (has_tmr && has_crates_io_ctx)
        || (has_publish_limit_phrase && (has_try_again || has_help))
    {
        return PublishErrorKind::RateLimited;
    }

    // Auth errors - fail fast, no retry
    if lower.contains("token") && (lower.contains("invalid") || lower.contains("expired"))
        || lower.contains("not logged in")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("403")
    {
        return PublishErrorKind::AuthError;
    }

    // Manifest/packaging errors - fail fast, no retry
    if lower.contains("invalid manifest")
        || lower.contains("missing") && lower.contains("field")
        || lower.contains("could not find")
        || lower.contains("failed to package")
        || lower.contains("license")
        || lower.contains("readme")
    {
        return PublishErrorKind::ManifestError;
    }

    // Propagation errors - retryable
    if lower.contains("failed to select a version for the requirement")
        || lower.contains("no matching package named")
        || lower.contains("failed to get")
        || lower.contains("no matching version")
        || (lower.contains("dependency") && lower.contains("not found"))
    {
        return PublishErrorKind::PropagationDelay;
    }

    // Network errors - potentially retryable
    if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("timeout")
        || lower.contains("timed out")
    {
        return PublishErrorKind::NetworkError;
    }

    PublishErrorKind::Unknown
}

/// Parse the rate limit retry-after timestamp from crates.io error output.
///
/// Looks for "try again after <RFC2822 timestamp>" in the stderr text.
/// Returns the parsed timestamp, or `None` if not found/parseable.
fn parse_rate_limit_timestamp(stderr: &str) -> Option<DateTime<FixedOffset>> {
    // Look for "try again after " (case-insensitive) followed by an RFC2822 timestamp.
    // Example: "Please try again after Tue, 24 Feb 2026 16:57:08 GMT"
    let lower = stderr.to_lowercase();
    let marker = "try again after ";
    let pos = lower.find(marker)?;
    let after = &stderr[pos + marker.len()..];

    // The timestamp ends at " or ", a quote, or a newline
    let end = after
        .find(" or ")
        .or_else(|| after.find(['"', '\n', '\r']))
        .unwrap_or(after.len());
    let timestamp_str = after[..end].trim();

    DateTime::parse_from_rfc2822(timestamp_str).ok()
}

/// Publish a single crate with retry logic for propagation delays.
fn publish_crate_with_retry(
    crate_name: &str,
    args: &PublishArgs,
    bootstrap: bool,
) -> Result<PublishResult> {
    const MAX_RETRIES: u32 = 5;
    const MAX_RATE_LIMIT_WAITS: u32 = 6;
    const RATE_LIMIT_FALLBACK_SECS: u64 = 300;

    let retry_delay = Duration::from_secs(args.retry_delay);
    let rate_limit_timeout = Duration::from_secs(args.rate_limit_timeout);

    // Dry-run mode: validate packaging locally.
    //
    // We use `cargo package --list` instead of `cargo publish --dry-run`
    // because lockstep workspace releases reference versions that may not yet
    // exist on crates.io during preparation.
    if args.dry_run {
        println!("  [DRY RUN] Validating {}...", crate_name);
        let output = Command::new("cargo")
            .args(["package", "-p", crate_name, "--list", "--locked"])
            .output()
            .context("Failed to spawn cargo package")?;

        if output.status.success() {
            return Ok(PublishResult::Success);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if stderr.trim().is_empty() {
            stdout.to_string()
        } else {
            stderr.to_string()
        };

        return Ok(PublishResult::Failed(anyhow!(
            "Dry-run packaging validation failed:\n{}",
            details
        )));
    }

    // Actual publish with retries.
    // Rate limit waits are tracked separately and don't count against MAX_RETRIES.
    let mut attempt: u32 = 0;
    let mut rate_limit_waits: u32 = 0;
    let mut total_rate_limit_wait = Duration::ZERO;

    loop {
        attempt += 1;

        let cargo_args = cargo_publish_args(crate_name, bootstrap);
        let output = Command::new("cargo")
            .args(cargo_args)
            .output()
            .context("Failed to spawn cargo publish")?;

        if output.status.success() {
            return Ok(PublishResult::Success);
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let error_kind = classify_publish_error(&stderr);

        match error_kind {
            PublishErrorKind::AlreadyPublished => {
                return Ok(PublishResult::AlreadyPublished);
            }
            PublishErrorKind::AuthError => {
                return Ok(PublishResult::Failed(anyhow!(
                    "Authentication error (check `cargo login`):\n{}",
                    stderr
                )));
            }
            PublishErrorKind::ManifestError => {
                return Ok(PublishResult::Failed(anyhow!(
                    "Manifest or packaging error:\n{}",
                    stderr
                )));
            }
            PublishErrorKind::RateLimited => {
                rate_limit_waits += 1;
                if rate_limit_waits > MAX_RATE_LIMIT_WAITS {
                    return Ok(PublishResult::Failed(anyhow!(
                        "Rate limited {} times for {}, giving up:\n{}",
                        rate_limit_waits,
                        crate_name,
                        stderr
                    )));
                }

                // Parse the retry-after timestamp, fall back to default
                let wait_secs = if let Some(retry_after) = parse_rate_limit_timestamp(&stderr) {
                    let now = Utc::now();
                    let until = retry_after.with_timezone(&Utc);
                    let delta = until.signed_duration_since(now);
                    if delta.num_seconds() > 0 {
                        println!(
                            "  Rate limited by crates.io. Waiting until {} ({}s)...",
                            retry_after,
                            delta.num_seconds()
                        );
                        delta.num_seconds() as u64
                    } else {
                        // Timestamp is in the past, wait a small amount
                        println!(
                            "  Rate limited by crates.io (retry-after already passed). Waiting 10s..."
                        );
                        10
                    }
                } else {
                    println!(
                        "  Rate limited by crates.io (could not parse retry-after). Waiting {}s...",
                        RATE_LIMIT_FALLBACK_SECS
                    );
                    RATE_LIMIT_FALLBACK_SECS
                };

                let wait_duration = Duration::from_secs(wait_secs);

                // Check against total rate limit timeout budget
                if total_rate_limit_wait + wait_duration > rate_limit_timeout {
                    return Ok(PublishResult::Failed(anyhow!(
                        "Rate limit wait would exceed --rate-limit-timeout ({}s) for {}:\n{}",
                        rate_limit_timeout.as_secs(),
                        crate_name,
                        stderr
                    )));
                }

                total_rate_limit_wait += wait_duration;
                sleep(wait_duration);
                // Don't increment attempt - rate limit waits are tracked separately
                attempt -= 1;
                continue;
            }
            PublishErrorKind::PropagationDelay | PublishErrorKind::NetworkError => {
                if attempt < MAX_RETRIES {
                    let kind_desc = match error_kind {
                        PublishErrorKind::PropagationDelay => "dependency not yet propagated",
                        PublishErrorKind::NetworkError => "network error",
                        _ => "transient error",
                    };
                    println!(
                        "  Attempt {}/{}: {}, retrying in {}s...",
                        attempt,
                        MAX_RETRIES,
                        kind_desc,
                        retry_delay.as_secs()
                    );
                    if args.verbose {
                        println!("  stderr: {}", stderr.lines().next().unwrap_or(""));
                    }
                    sleep(retry_delay);
                    continue;
                }
            }
            PublishErrorKind::Unknown => {
                // Unknown errors don't get retried
            }
        }

        // Max retries exceeded or non-retryable error
        return Ok(PublishResult::Failed(anyhow!(
            "cargo publish failed for {}:\n{}",
            crate_name,
            stderr
        )));
    }
}

fn cargo_publish_args(crate_name: &str, bootstrap: bool) -> Vec<&str> {
    let mut args = vec![
        "publish",
        "-p",
        crate_name,
        "--locked",
        "--registry",
        "crates-io",
    ];
    if bootstrap {
        args.push("--no-verify");
    }
    args
}

/// Run pre-publish checks.
fn run_pre_publish_checks(
    args: &PublishArgs,
    metadata: &Metadata,
    plan: &PublishPlan,
) -> Result<()> {
    println!("Running pre-publish checks...\n");

    // Git status check
    if !args.skip_git_check {
        print!("  Checking git status... ");
        io::stdout().flush()?;

        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .context("Failed to run git status")?;

        if !status.stdout.is_empty() {
            println!("✗");
            bail!("Working directory is not clean. Commit or stash changes first.");
        }
        println!("✓");
    }

    // Version consistency check
    if !args.skip_version_check {
        print!("  Checking version consistency... ");
        io::stdout().flush()?;

        let workspace_member_ids: HashSet<_> = metadata.workspace_members.iter().collect();

        let mut inconsistent = Vec::new();
        for pkg in &metadata.packages {
            if !workspace_member_ids.contains(&pkg.id) {
                continue;
            }
            // Skip non-publishable crates
            if pkg.publish.as_ref().is_some_and(|p| p.is_empty()) {
                continue;
            }
            if pkg.name == "xtask" || pkg.name == "tokmd-fuzz" || pkg.name == "fuzz" {
                continue;
            }

            let pkg_version = pkg.version.to_string();
            if pkg_version != plan.workspace_version {
                inconsistent.push(format!("{} ({})", pkg.name, pkg_version));
            }
        }

        if !inconsistent.is_empty() {
            println!("✗");
            bail!(
                "Version mismatch! Expected {}, but found:\n  {}",
                plan.workspace_version,
                inconsistent.join("\n  ")
            );
        }
        println!("✓ (all crates at {})", plan.workspace_version);
    }

    // Changelog check
    if !args.skip_changelog_check {
        print!(
            "  Checking CHANGELOG.md contains {}... ",
            plan.workspace_version
        );
        io::stdout().flush()?;

        let changelog_path = Path::new("CHANGELOG.md");
        if !changelog_path.exists() {
            println!("✗");
            bail!("CHANGELOG.md not found");
        }

        let changelog =
            std::fs::read_to_string(changelog_path).context("Failed to read CHANGELOG.md")?;

        // Look for version header like [1.3.0] or ## 1.3.0
        let version_patterns = [
            format!("[{}]", plan.workspace_version),
            format!("## {}", plan.workspace_version),
        ];

        let has_version = version_patterns
            .iter()
            .any(|pattern| changelog.contains(pattern));

        if !has_version {
            println!("✗");
            bail!(
                "CHANGELOG.md does not contain version {}. Add a changelog entry first.",
                plan.workspace_version
            );
        }
        println!("✓");
    }

    print!("  Checking dependency closure... ");
    io::stdout().flush()?;
    validate_publish_dependency_closure(metadata, plan)?;
    println!("✓");

    print!("  Checking package contents... ");
    io::stdout().flush()?;
    validate_publish_packages(plan)?;
    println!("✓ ({} crates)", plan.publish_order.len());

    // Tests
    if !args.skip_tests {
        println!("  Running tests...");
        let mut test_command = Command::new("cargo");
        test_command.args([
            "test",
            "--workspace",
            "--all-features",
            "--exclude",
            "tokmd-fuzz",
            "--locked",
        ]);
        if cfg!(windows) {
            // Windows keeps the running xtask binary locked, so exclude it
            // from the publish preflight workspace test pass and let the
            // dedicated xtask test suite cover the binary crate separately.
            test_command.args(["--exclude", "xtask"]);
        }

        let test_status = test_command.status().context("Failed to run tests")?;

        if !test_status.success() {
            bail!("Tests failed");
        }
        println!("  ✓ Tests passed");
    }

    println!();
    Ok(())
}

/// Create and push a git tag.
fn create_git_tag(args: &PublishArgs, version: &str) -> Result<()> {
    let tag = args.tag_format.replace("{version}", version);

    // Check if tag already exists
    let tag_check = Command::new("git")
        .args(["tag", "-l", &tag])
        .output()
        .context("Failed to check existing tags")?;

    if !tag_check.stdout.is_empty() {
        println!("Tag {} already exists, skipping tag creation.", tag);
        return Ok(());
    }

    println!("Creating git tag: {}", tag);

    let status = Command::new("git")
        .args(["tag", "-a", &tag, "-m", &format!("Release {}", tag)])
        .status()
        .context("Failed to create git tag")?;

    if !status.success() {
        bail!("Failed to create git tag");
    }

    println!("Pushing tag to origin...");
    let push_status = Command::new("git")
        .args(["push", "origin", &tag])
        .status()
        .context("Failed to push git tag")?;

    if !push_status.success() {
        bail!("Failed to push git tag");
    }

    println!("  ✓ Tag {} created and pushed", tag);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::collections::VecDeque;
    use tempfile::tempdir;

    enum FixturePublish {
        Success,
        AlreadyPublished,
        Failed(&'static str),
    }

    struct FixtureAttempt {
        publish: FixturePublish,
        visibility: VecDeque<RegistryVersionLookup>,
    }

    struct FixtureBackend {
        attempts: VecDeque<FixtureAttempt>,
        pending_visibility: VecDeque<RegistryVersionLookup>,
        publish_calls: Vec<(String, bool)>,
        visibility_calls: Vec<String>,
    }

    impl FixtureBackend {
        fn new(attempts: impl IntoIterator<Item = FixtureAttempt>) -> Self {
            Self {
                attempts: attempts.into_iter().collect(),
                pending_visibility: VecDeque::new(),
                publish_calls: Vec::new(),
                visibility_calls: Vec::new(),
            }
        }
    }

    impl PublishBackend for FixtureBackend {
        fn publish(
            &mut self,
            crate_name: &str,
            _args: &PublishArgs,
            bootstrap: bool,
        ) -> Result<PublishResult> {
            let attempt = self
                .attempts
                .pop_front()
                .ok_or_else(|| anyhow!("fixture has no attempt for {crate_name}"))?;
            self.publish_calls.push((crate_name.to_string(), bootstrap));
            self.pending_visibility = attempt.visibility;
            Ok(match attempt.publish {
                FixturePublish::Success => PublishResult::Success,
                FixturePublish::AlreadyPublished => PublishResult::AlreadyPublished,
                FixturePublish::Failed(message) => PublishResult::Failed(anyhow!(message)),
            })
        }

        fn wait_for_visibility(
            &mut self,
            crate_name: &str,
            _version: &str,
            _interval: u64,
        ) -> RegistryVersionLookup {
            self.visibility_calls.push(crate_name.to_string());
            let mut last = RegistryVersionLookup {
                state: "unavailable",
                published_at: None,
                error: Some("fixture omitted a visibility response".to_string()),
            };
            for _ in 0..REGISTRY_VISIBILITY_ATTEMPTS {
                let Some(lookup) = self.pending_visibility.pop_front() else {
                    break;
                };
                last = lookup;
                if matches!(last.state, "present" | "yanked") {
                    break;
                }
            }
            last
        }
    }

    fn fixture_lookup(state: &'static str) -> RegistryVersionLookup {
        RegistryVersionLookup {
            state,
            published_at: None,
            error: (state != "present").then(|| format!("fixture state: {state}")),
        }
    }

    fn fixture_attempt(
        publish: FixturePublish,
        visibility: impl IntoIterator<Item = &'static str>,
    ) -> FixtureAttempt {
        FixtureAttempt {
            publish,
            visibility: visibility.into_iter().map(fixture_lookup).collect(),
        }
    }

    fn fixture_plan() -> PublishPlan {
        PublishPlan {
            publish_order: vec![
                "tokmd-types".to_string(),
                "tokmd-envelope".to_string(),
                "tokmd-core".to_string(),
                "tokmd".to_string(),
            ],
            inclusion_reasons: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
            workspace_version: "1.15.1".to_string(),
            predecessors: BTreeMap::new(),
        }
    }

    #[test]
    fn test_classify_already_published() {
        assert!(matches!(
            classify_publish_error("crate version `1.0.0` is already uploaded"),
            PublishErrorKind::AlreadyPublished
        ));
        assert!(matches!(
            classify_publish_error("the crate version 1.0.0 already exists"),
            PublishErrorKind::AlreadyPublished
        ));
    }

    #[test]
    fn test_classify_auth_error() {
        assert!(matches!(
            classify_publish_error("token is invalid"),
            PublishErrorKind::AuthError
        ));
        assert!(matches!(
            classify_publish_error("error: not logged in"),
            PublishErrorKind::AuthError
        ));
    }

    #[test]
    fn test_classify_propagation_error() {
        assert!(matches!(
            classify_publish_error("failed to select a version for the requirement `foo`"),
            PublishErrorKind::PropagationDelay
        ));
        assert!(matches!(
            classify_publish_error("no matching package named `bar`"),
            PublishErrorKind::PropagationDelay
        ));
    }

    #[test]
    fn test_classify_manifest_error() {
        assert!(matches!(
            classify_publish_error("invalid manifest: missing field `description`"),
            PublishErrorKind::ManifestError
        ));
    }

    #[test]
    fn test_is_publish_dependency() {
        assert!(is_publish_dependency(&DependencyKind::Normal));
        assert!(is_publish_dependency(&DependencyKind::Build));
        assert!(!is_publish_dependency(&DependencyKind::Development));
    }

    #[test]
    fn test_classify_rate_limit() {
        // HTTP 429 status code in error message
        assert!(matches!(
            classify_publish_error(
                "the remote server responded with an error (status 429 Too Many Requests): \
                 You have published too many new crates"
            ),
            PublishErrorKind::RateLimited
        ));

        // crates.io publish-limit phrasing (should match only when it looks like the real message)
        assert!(matches!(
            classify_publish_error(
                "You have published too many new crates in a short period of time. \
                 Please try again after Tue, 24 Feb 2026 16:57:08 GMT or email help@crates.io"
            ),
            PublishErrorKind::RateLimited
        ));

        // 429 + Too Many Requests without extra context
        assert!(matches!(
            classify_publish_error("error: 429 Too Many Requests"),
            PublishErrorKind::RateLimited
        ));

        assert!(matches!(
            classify_publish_error("error: 429 rate limit exceeded"),
            PublishErrorKind::RateLimited
        ));

        // unrelated "too many" should not match rate limiting
        assert!(matches!(
            classify_publish_error("open files: too many open files"),
            PublishErrorKind::Unknown
        ));
    }

    #[test]
    fn test_parse_rate_limit_timestamp() {
        // Full crates.io error message
        let stderr = "the remote server responded with an error (status 429 Too Many Requests): \
                       You have published too many new crates in a short period of time. \
                       Please try again after Tue, 24 Feb 2026 16:57:08 GMT or email help@crates.io";
        let ts = parse_rate_limit_timestamp(stderr);
        assert!(ts.is_some(), "should parse RFC2822 timestamp");
        let ts = ts.expect("ts should be Some as checked by assert");
        assert_eq!(ts.year(), 2026);
        assert_eq!(ts.month(), 2);
        assert_eq!(ts.day(), 24);

        // No timestamp present
        assert!(parse_rate_limit_timestamp("some random error").is_none());

        // Marker present but invalid timestamp
        assert!(parse_rate_limit_timestamp("try again after not-a-real-timestamp").is_none());
    }

    #[test]
    fn registry_response_distinguishes_present_yanked_and_missing() {
        let body = r#"{
            "versions": [
                {"num":"1.15.0","yanked":false,"created_at":"2026-08-01T00:00:00Z"},
                {"num":"1.14.0","yanked":true,"created_at":"2026-07-01T00:00:00Z"}
            ]
        }"#;

        let present = parse_registry_version_response("tokmd", "1.15.0", 200, body);
        assert_eq!(present.state, "present");
        assert_eq!(
            present.published_at.as_deref(),
            Some("2026-08-01T00:00:00Z")
        );

        let yanked = parse_registry_version_response("tokmd", "1.14.0", 200, body);
        assert_eq!(yanked.state, "yanked");

        let missing = parse_registry_version_response("tokmd", "1.13.0", 200, body);
        assert_eq!(missing.state, "missing");
    }

    #[test]
    fn registry_response_fails_closed_for_transport_and_shape_errors() {
        let not_found = parse_registry_version_response("tokmd", "1.15.0", 404, "");
        assert_eq!(not_found.state, "missing");

        let server_error = parse_registry_version_response("tokmd", "1.15.0", 503, "");
        assert_eq!(server_error.state, "unavailable");

        let malformed = parse_registry_version_response("tokmd", "1.15.0", 200, "{}");
        assert_eq!(malformed.state, "unavailable");

        // An HTML error page served with HTTP 200 must not be read as success.
        let html = parse_registry_version_response(
            "tokmd",
            "1.15.0",
            200,
            "<html><body>502 Bad Gateway</body></html>",
        );
        assert_eq!(html.state, "unavailable");

        // A version entry without a boolean `yanked` field proves nothing.
        let body = r#"{"versions":[{"num":"1.15.0","created_at":"2026-08-01T00:00:00Z"}]}"#;
        let no_yanked = parse_registry_version_response("tokmd", "1.15.0", 200, body);
        assert_eq!(no_yanked.state, "unavailable");
    }

    #[test]
    fn registry_query_rejects_unsafe_crate_names_without_network() {
        for name in ["tokmd/../admin", "tokmd:1", "tokmd?x=1", "tok md"] {
            let lookup = query_registry_version(name, "1.15.0");
            assert_eq!(lookup.state, "unavailable", "name {name} must be rejected");
            assert!(lookup.published_at.is_none());
        }
    }

    fn receipt(
        name: &str,
        state: &'static str,
        dependencies_resolvable: bool,
    ) -> RegistryCrateReceipt {
        RegistryCrateReceipt {
            name: name.to_string(),
            version: "1.15.0".to_string(),
            state,
            dependencies_resolvable,
            published_at: None,
            error: None,
        }
    }

    #[test]
    fn registry_inventory_completeness_is_fail_closed() {
        // An empty plan proves nothing and must not report success.
        assert!(!registry_inventory_is_complete(&[]));

        assert!(registry_inventory_is_complete(&[
            receipt("tokmd-core", "present", true),
            receipt("tokmd", "present", true),
        ]));

        assert!(!registry_inventory_is_complete(&[
            receipt("tokmd-core", "present", true),
            receipt("tokmd", "missing", false),
        ]));

        assert!(!registry_inventory_is_complete(&[receipt(
            "tokmd", "present", false
        )]));

        assert!(!registry_inventory_is_complete(&[receipt(
            "tokmd", "yanked", true
        )]));
    }

    #[test]
    fn retry_after_is_parsed_and_capped() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  7 "), Some(Duration::from_secs(7)));
        // Hostile or misconfigured values must not stall the inventory.
        assert_eq!(parse_retry_after("100000"), Some(Duration::from_secs(60)));
        // The HTTP-date form falls back to the caller's bounded backoff.
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn registry_headers_split_from_body_across_redirects() {
        let response = "HTTP/2 301\r\nlocation: https://crates.io/x\r\n\r\nHTTP/2 429\r\nretry-after: 12\r\n\r\n{\"versions\":[]}";
        let (headers, body) = split_registry_headers(response);
        assert_eq!(body, "{\"versions\":[]}");
        assert_eq!(header_value(headers, "Retry-After").as_deref(), Some("12"));
        assert!(header_value(headers, "x-absent").is_none());
    }

    #[test]
    fn rate_limit_statuses_are_retryable() {
        assert!(is_registry_rate_limited(429));
        assert!(is_registry_rate_limited(503));
        assert!(!is_registry_rate_limited(200));
        assert!(!is_registry_rate_limited(404));
    }

    fn receipt_test_plan() -> PublishPlan {
        PublishPlan {
            publish_order: vec!["tokmd-core".to_string(), "tokmd".to_string()],
            inclusion_reasons: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
            workspace_version: "1.15.0".to_string(),
            predecessors: BTreeMap::new(),
        }
    }

    fn write_recovery_candidate(path: &Path, receipt: &PublishReceipt) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec_pretty(receipt)?;
        fs::write(path, &bytes)?;
        Ok(bytes)
    }

    #[test]
    fn publication_receipt_path_accepts_portable_external_location() -> Result<()> {
        let workspace = tempdir()?;
        let outside = tempdir()?;
        validated_publish_receipt_path(&outside.path().join("publish.json"), workspace.path())?;
        Ok(())
    }

    #[test]
    fn publication_receipt_path_rejects_parent_escape() -> Result<()> {
        let workspace = tempdir()?;
        let escaped = workspace
            .path()
            .join("target")
            .join("..")
            .join("tracked")
            .join("publish.json");
        if validated_publish_receipt_path(&escaped, workspace.path()).is_ok() {
            bail!("target/../tracked must not pass receipt containment");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn publication_receipt_path_rejects_symlink_alias_into_tracked_worktree() -> Result<()> {
        use std::os::unix::fs::symlink;

        let workspace = tempdir()?;
        let tracked = workspace.path().join("tracked");
        fs::create_dir_all(&tracked)?;
        let outside = tempdir()?;
        let alias = outside.path().join("alias");
        symlink(&tracked, &alias)?;
        if validated_publish_receipt_path(&alias.join("publish.json"), workspace.path()).is_ok() {
            bail!("a symlink alias must not bypass worktree containment");
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn publication_receipt_path_rejects_directory_alias_into_tracked_worktree() -> Result<()> {
        use std::os::windows::fs::symlink_dir;

        let workspace = tempdir()?;
        let tracked = workspace.path().join("tracked");
        fs::create_dir_all(&tracked)?;
        let outside = tempdir()?;
        let alias = outside.path().join("alias");
        if symlink_dir(&tracked, &alias).is_err() {
            return Ok(());
        }
        if validated_publish_receipt_path(&alias.join("publish.json"), workspace.path()).is_ok() {
            bail!("a directory alias must not bypass worktree containment");
        }
        Ok(())
    }

    #[test]
    fn relative_receipt_is_normalized_to_workspace_target_from_nested_context() -> Result<()> {
        let workspace = tempdir()?;
        let nested_context = workspace.path().join("docs").join("nested");
        fs::create_dir_all(&nested_context)?;
        let resolved = validated_publish_receipt_path(
            Path::new("target/publishing/publish.json"),
            workspace.path(),
        )?;
        let expected = workspace
            .path()
            .canonicalize()?
            .join("target/publishing/publish.json");
        if resolved != expected || resolved.starts_with(&nested_context) {
            bail!("relative receipt I/O must remain rooted at workspace target/");
        }
        Ok(())
    }

    #[test]
    fn recovery_selects_and_restores_higher_valid_temp() -> Result<()> {
        let plan = receipt_test_plan();
        let identity = fixture_publish_identity(&plan);
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut canonical = new_publish_receipt(&plan);
        canonical.generation = 1;
        write_recovery_candidate(&path, &canonical)?;
        let mut newer = canonical.clone();
        newer.generation = 2;
        let temp = directory.path().join(".publish.json.tmp-fixture");
        let expected = write_recovery_candidate(&temp, &newer)?;

        let recovered = recover_publish_receipt(&path, &plan, &identity)?;
        if recovered.generation != 2 || fs::read(&path)? != expected {
            bail!("the highest valid generation must be restored canonically");
        }
        Ok(())
    }

    #[test]
    fn recovery_restores_backup_when_canonical_is_absent_or_corrupt() -> Result<()> {
        let plan = receipt_test_plan();
        let identity = fixture_publish_identity(&plan);
        for corrupt_canonical in [false, true] {
            let directory = tempdir()?;
            let path = directory.path().join("publish.json");
            if corrupt_canonical {
                fs::write(&path, b"not json")?;
            }
            let mut backup_receipt = new_publish_receipt(&plan);
            backup_receipt.generation = 4;
            let backup = directory.path().join(".publish.json.backup-fixture");
            let expected = write_recovery_candidate(&backup, &backup_receipt)?;
            let recovered = recover_publish_receipt(&path, &plan, &identity)?;
            if recovered.generation != 4 || fs::read(&path)? != expected {
                bail!("a valid backup must replace an absent or corrupt canonical receipt");
            }
        }
        Ok(())
    }

    #[test]
    fn recovery_rejects_equal_generation_divergence() -> Result<()> {
        let plan = receipt_test_plan();
        let identity = fixture_publish_identity(&plan);
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut first = new_publish_receipt(&plan);
        first.generation = 3;
        write_recovery_candidate(&path, &first)?;
        let mut divergent = first.clone();
        divergent.state = PublishRunState::InProgress;
        write_recovery_candidate(
            &directory.path().join(".publish.json.tmp-divergent"),
            &divergent,
        )?;
        if recover_publish_receipt(&path, &plan, &identity).is_ok() {
            bail!("equal-generation divergent candidates must fail closed");
        }
        Ok(())
    }

    #[test]
    fn recovery_ignores_invalid_higher_candidate_and_fails_without_valid_candidate() -> Result<()> {
        let plan = receipt_test_plan();
        let identity = fixture_publish_identity(&plan);
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut valid = new_publish_receipt(&plan);
        valid.generation = 2;
        write_recovery_candidate(&path, &valid)?;
        let mut invalid = valid.clone();
        invalid.generation = 9;
        invalid.identity.source_commit = "different".to_string();
        let temp = directory.path().join(".publish.json.tmp-invalid");
        write_recovery_candidate(&temp, &invalid)?;
        if recover_publish_receipt(&path, &plan, &identity)?.generation != 2 {
            bail!("an invalid higher generation must not supersede valid state");
        }
        fs::remove_file(&path)?;
        if recover_publish_receipt(&path, &plan, &identity).is_ok() {
            bail!("recovery must fail when only invalid candidates remain");
        }
        Ok(())
    }

    #[test]
    fn non_resume_detects_orphaned_recovery_candidate() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        fs::write(directory.path().join(".publish.json.tmp-orphan"), b"orphan")?;
        if !has_publish_recovery_candidates(&path)? {
            bail!("a non-resume run must detect orphaned recovery state");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_replacement_is_atomic_and_parseable() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        write_publish_receipt(&path, &receipt)?;
        receipt.state = PublishRunState::InProgress;
        write_publish_receipt(&path, &receipt)?;
        let restored: PublishReceipt = serde_json::from_slice(&fs::read(&path)?)?;
        if restored.generation != 2 || restored.state != PublishRunState::InProgress {
            bail!("atomic replacement must install the complete newer snapshot");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_transition_guard_rejects_terminal_regression() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let entry = receipt
            .crates
            .first_mut()
            .ok_or_else(|| anyhow!("receipt test plan should contain a crate"))?;
        entry.state = PublishReceiptState::Published;
        entry.attempts = 1;
        entry.registry_visible = Some(true);
        let name = entry.name.clone();
        if update_publish_receipt_entry(
            &mut receipt,
            &name,
            PublishReceiptState::Planned,
            None,
            false,
        )
        .is_ok()
        {
            bail!("terminal publication evidence must not regress to planned");
        }
        Ok(())
    }

    #[test]
    fn current_inventory_can_complete_an_already_present_entry() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        let inventory = plan
            .publish_order
            .iter()
            .map(|name| (name.clone(), fixture_lookup("present")))
            .collect();
        reconcile_inventory(&path, &mut receipt, &inventory)?;
        if receipt.state != PublishRunState::Complete
            || receipt.crates.iter().any(|entry| {
                entry.state != PublishReceiptState::AlreadyPresent
                    || entry.registry_visible != Some(true)
            })
        {
            bail!("live current-run inventory is valid completion evidence");
        }
        Ok(())
    }

    #[test]
    fn inventory_reconciliation_uses_explicit_authoritative_transitions() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        let first = receipt
            .crates
            .first_mut()
            .ok_or_else(|| anyhow!("receipt test plan should contain a crate"))?;
        first.state = PublishReceiptState::Failed;
        first.attempts = 1;
        first.reason = Some("upload result was uncertain".to_string());
        let inventory = plan
            .publish_order
            .iter()
            .map(|name| (name.clone(), fixture_lookup("present")))
            .collect();
        reconcile_inventory(&path, &mut receipt, &inventory)?;
        if receipt.crates.first().is_none_or(|entry| {
            entry.state != PublishReceiptState::AlreadyPresent
                || entry.registry_visible != Some(true)
        }) {
            bail!("authoritative live inventory must reconcile an uncertain upload");
        }
        Ok(())
    }

    #[test]
    fn resume_rejects_bootstrap_decision_drift() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let entry = receipt
            .crates
            .first_mut()
            .ok_or_else(|| anyhow!("receipt test plan should contain a crate"))?;
        entry.state = PublishReceiptState::Failed;
        entry.attempts = 1;
        entry.bootstrap = true;
        entry.reason = Some("retryable".to_string());
        let name = entry.name.clone();
        if publish_bootstrap_decision(Some(&mut receipt), &name, false, false).is_ok() {
            bail!("resume must not overwrite a persisted bootstrap decision");
        }
        if !publish_bootstrap_decision(Some(&mut receipt), &name, true, false)? {
            bail!("resume must preserve the matching persisted bootstrap decision");
        }
        Ok(())
    }

    #[test]
    fn cargo_publish_is_bound_to_crates_io_registry() -> Result<()> {
        let args = cargo_publish_args("tokmd", true);
        if !args
            .windows(2)
            .any(|pair| pair == ["--registry", "crates-io"])
            || !args.contains(&"--no-verify")
        {
            bail!("cargo publish must bind mutation to crates-io and preserve bootstrap");
        }
        Ok(())
    }

    #[test]
    fn publisher_fixture_covers_resume_propagation_existing_and_yanked() -> Result<()> {
        let plan = fixture_plan();
        let directory = tempdir().context("create publisher fixture directory")?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        write_publish_receipt(&path, &receipt)?;

        let mut args = PublishArgs {
            continue_on_error: true,
            interval: 0,
            ..PublishArgs::default()
        };
        let bootstrap = BTreeSet::from(["tokmd-types".to_string(), "tokmd-envelope".to_string()]);
        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        let mut first = FixtureBackend::new([
            fixture_attempt(FixturePublish::Success, ["missing", "present"]),
            fixture_attempt(FixturePublish::AlreadyPublished, ["present"]),
            fixture_attempt(FixturePublish::Success, ["yanked"]),
            fixture_attempt(FixturePublish::Failed("simulated partial failure"), []),
        ]);
        let (succeeded, failed) = execute_publish_with_backend(
            &crates,
            &args,
            &plan.workspace_version,
            &bootstrap,
            &plan.predecessors,
            Some(&path),
            Some(&mut receipt),
            &mut first,
        )?;
        if succeeded != ["tokmd-types", "tokmd-envelope"] || failed != ["tokmd-core", "tokmd"] {
            bail!("fixture should record clean, existing, yanked, and failed outcomes");
        }
        if first.publish_calls
            != [
                ("tokmd-types".to_string(), true),
                ("tokmd-envelope".to_string(), true),
                ("tokmd-core".to_string(), false),
                ("tokmd".to_string(), false),
            ]
        {
            bail!("fixture should record the per-crate bootstrap invocation decision");
        }
        if first.visibility_calls
            != [
                "tokmd-types".to_string(),
                "tokmd-envelope".to_string(),
                "tokmd-core".to_string(),
            ]
        {
            bail!("fixture should observe visibility after each non-failed publish");
        }

        receipt = load_publish_receipt(&path, &plan)?;
        let persisted_bootstrap: BTreeMap<_, _> = receipt
            .crates
            .iter()
            .map(|entry| (entry.name.as_str(), entry.bootstrap))
            .collect();
        if persisted_bootstrap
            != BTreeMap::from([
                ("tokmd-types", true),
                ("tokmd-envelope", true),
                ("tokmd-core", false),
                ("tokmd", false),
            ])
        {
            bail!("on-disk receipt must preserve each bootstrap invocation decision");
        }

        let resume = crates_to_publish(&plan, 0, Some(&receipt));
        if resume != ["tokmd"].map(str::to_string) {
            bail!("resume should skip visible published and yanked entries");
        }

        args.continue_on_error = false;
        let mut second = FixtureBackend::new([fixture_attempt(
            FixturePublish::AlreadyPublished,
            ["present"],
        )]);
        let (succeeded, failed) = execute_publish_with_backend(
            &resume,
            &args,
            &plan.workspace_version,
            &BTreeSet::new(),
            &plan.predecessors,
            Some(&path),
            Some(&mut receipt),
            &mut second,
        )?;
        if succeeded != ["tokmd"] || !failed.is_empty() {
            bail!("resume fixture should complete the previously failed suffix");
        }
        if second.publish_calls != [("tokmd".to_string(), false)] {
            bail!("resume must not republish terminal entries or retain bootstrap intent");
        }

        receipt.state = PublishRunState::Complete;
        write_publish_receipt(&path, &receipt)?;
        if load_publish_receipt(&path, &plan).is_ok() {
            bail!("a yanked crate must prevent a complete publication receipt");
        }
        Ok(())
    }

    #[test]
    fn resume_rechecks_visibility_without_republishing_uploaded_crate() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir().context("create visibility resume fixture directory")?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Published;
        entry.attempts = 1;
        write_publish_receipt(&path, &receipt)?;

        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        let mut backend =
            FixtureBackend::new([fixture_attempt(FixturePublish::Success, ["present"])]);
        backend
            .pending_visibility
            .push_back(fixture_lookup("present"));
        let args = PublishArgs {
            interval: 0,
            ..PublishArgs::default()
        };
        let (succeeded, failed) = execute_publish_with_backend(
            &crates,
            &args,
            &plan.workspace_version,
            &BTreeSet::new(),
            &plan.predecessors,
            Some(&path),
            Some(&mut receipt),
            &mut backend,
        )?;

        if succeeded != ["tokmd-core", "tokmd"] || !failed.is_empty() {
            bail!("resume should verify the uploaded crate and publish only the pending crate");
        }
        if backend.publish_calls != [("tokmd".to_string(), false)] {
            bail!("resume must not republish a crate whose upload already succeeded");
        }
        let loaded = load_publish_receipt(&path, &plan)?;
        if loaded
            .crates
            .first()
            .and_then(|entry| entry.registry_visible)
            != Some(true)
        {
            bail!("visibility-only resume should persist the successful observation");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_round_trips_terminal_state_and_attempt_count() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir().context("create receipt test directory")?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        write_publish_receipt(&path, &receipt)?;
        update_publish_receipt(
            &path,
            &mut receipt,
            "tokmd-core",
            PublishReceiptState::InProgress,
            None,
            true,
        )?;
        update_publish_receipt(
            &path,
            &mut receipt,
            "tokmd-core",
            PublishReceiptState::Published,
            None,
            false,
        )?;

        let loaded = load_publish_receipt(&path, &plan)?;
        let Some(entry) = loaded.crates.first() else {
            bail!("receipt should contain the first planned crate");
        };
        if entry.state != PublishReceiptState::Published || entry.attempts != 1 {
            bail!("terminal receipt state and attempt count were not persisted");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_rejects_unbound_legacy_schema() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir().context("create legacy receipt test directory")?;
        let path = directory.path().join("publish.json");
        let mut legacy = serde_json::to_value(new_publish_receipt(&plan))?;
        let object = legacy
            .as_object_mut()
            .ok_or_else(|| anyhow!("receipt should serialize as an object"))?;
        object.insert(
            "schema".to_string(),
            Value::String("tokmd.publish_receipt.v2".to_string()),
        );
        object.insert("schema_version".to_string(), Value::Number(2.into()));
        fs::write(&path, serde_json::to_string_pretty(&legacy)?)?;

        if load_publish_receipt(&path, &plan).is_ok() {
            bail!("legacy receipts without immutable identity must fail closed");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_records_dependency_closure_preflight() -> Result<()> {
        let mut receipt = new_publish_receipt(&receipt_test_plan());
        if receipt
            .crates
            .iter()
            .any(|entry| entry.dependency_closure.is_some())
        {
            bail!("new receipt must not claim dependency closure before preflight");
        }

        mark_dependency_closure_verified(&mut receipt);
        if receipt
            .crates
            .iter()
            .any(|entry| entry.dependency_closure != Some(true))
        {
            bail!("preflight must mark every planned crate as verified");
        }
        Ok(())
    }

    #[test]
    fn bootstrap_selection_is_explicit_and_plan_bound() -> Result<()> {
        let plan = receipt_test_plan();
        let selected =
            validate_bootstrap_crates(&plan.publish_order, Some(&["tokmd-core".to_string()]))?;
        if selected != BTreeSet::from(["tokmd-core".to_string()]) {
            bail!("bootstrap selection should preserve the requested planned crate");
        }
        let filtered_execution = crates_to_publish(&plan, 1, None);
        if filtered_execution != vec!["tokmd".to_string()] {
            bail!("filtered execution should begin at the requested --from crate");
        }
        let selected_from_filtered_execution =
            validate_bootstrap_crates(&filtered_execution, Some(&["tokmd".to_string()]))?;
        if selected_from_filtered_execution != BTreeSet::from(["tokmd".to_string()]) {
            bail!("bootstrap selection should accept an in-window filtered crate");
        }
        if validate_bootstrap_crates(&plan.publish_order, Some(&["not-in-plan".to_string()]))
            .is_ok()
        {
            bail!("bootstrap selection must reject crates outside the publish plan");
        }

        if validate_bootstrap_crates(&["tokmd".to_string()], Some(&["tokmd-core".to_string()]))
            .is_ok()
        {
            bail!("bootstrap selection must reject a terminal skipped crate");
        }
        Ok(())
    }

    #[test]
    fn bootstrap_receipt_audit_records_true_and_false_decisions() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        publish_bootstrap_decision(Some(&mut receipt), "tokmd-core", true, false)?;
        publish_bootstrap_decision(Some(&mut receipt), "tokmd", false, false)?;
        let core = receipt
            .crates
            .iter()
            .find(|entry| entry.name == "tokmd-core")
            .ok_or_else(|| anyhow!("bootstrap receipt should contain tokmd-core"))?;
        let tokmd = receipt
            .crates
            .iter()
            .find(|entry| entry.name == "tokmd")
            .ok_or_else(|| anyhow!("bootstrap receipt should contain tokmd"))?;
        if !core.bootstrap || tokmd.bootstrap {
            bail!("receipt must preserve both opted-in and ordinary invocation decisions");
        }
        Ok(())
    }

    #[test]
    fn dependency_closure_preflight_accepts_the_current_workspace() -> Result<()> {
        let metadata = MetadataCommand::new()
            .no_deps()
            .exec()
            .context("load workspace metadata for closure fixture")?;
        let workspace_member_ids: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
        let publish_order = metadata
            .packages
            .iter()
            .filter(|package| workspace_member_ids.contains(&package.id))
            .filter(|package| {
                package
                    .publish
                    .as_ref()
                    .is_none_or(|targets| !targets.is_empty())
            })
            .map(|package| package.name.to_string())
            .collect();
        let plan = PublishPlan {
            publish_order,
            inclusion_reasons: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
            workspace_version: "fixture".to_string(),
            predecessors: BTreeMap::new(),
        };

        validate_publish_dependency_closure(&metadata, &plan)?;
        let mut receipt = new_publish_receipt(&plan);
        mark_dependency_closure_verified(&mut receipt);
        if receipt
            .crates
            .iter()
            .any(|entry| entry.dependency_closure != Some(true))
        {
            bail!("validated closure must be recorded for every planned crate");
        }
        Ok(())
    }

    #[test]
    fn dependency_closure_preflight_rejects_an_omitted_workspace_dependency() -> Result<()> {
        let metadata = MetadataCommand::new()
            .no_deps()
            .exec()
            .context("load workspace metadata for negative closure fixture")?;
        let workspace_member_ids: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
        let packages: Vec<&Package> = metadata
            .packages
            .iter()
            .filter(|package| workspace_member_ids.contains(&package.id))
            .filter(|package| {
                package
                    .publish
                    .as_ref()
                    .is_none_or(|targets| !targets.is_empty())
            })
            .collect();
        let dependent = packages
            .iter()
            .find(|package| package.name == "tokmd")
            .ok_or_else(|| anyhow!("workspace fixture must contain the tokmd package"))?;
        let dependency = dependent
            .dependencies
            .iter()
            .find(|candidate| {
                candidate.name == "tokmd-core" && is_publish_dependency(&candidate.kind)
            })
            .map(|candidate| candidate.name.as_str())
            .ok_or_else(|| anyhow!("workspace fixture must contain tokmd -> tokmd-core"))?;
        let dependent = dependent.name.as_str();
        let publish_order = packages
            .iter()
            .filter(|package| package.name != dependency || package.name == dependent)
            .map(|package| package.name.to_string())
            .collect();
        let plan = PublishPlan {
            publish_order,
            inclusion_reasons: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
            workspace_version: "fixture".to_string(),
            predecessors: BTreeMap::new(),
        };

        if validate_publish_dependency_closure(&metadata, &plan).is_ok() {
            bail!("closure preflight must reject an omitted workspace dependency");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_rejects_a_different_publish_plan() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir().context("create receipt test directory")?;
        let path = directory.path().join("publish.json");
        write_publish_receipt(&path, &new_publish_receipt(&plan))?;
        let mut changed = plan;
        changed.publish_order.reverse();
        if load_publish_receipt(&path, &changed).is_ok() {
            bail!("resume must reject a receipt created for a different publish order");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_resume_skips_terminal_entries_and_retries_failed_entries() -> Result<()>
    {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Published;
        entry.attempts = 1;
        entry.registry_visible = Some(true);

        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        if crates != vec!["tokmd".to_string()] {
            bail!("resume should skip only the terminal publication entry");
        }

        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Failed;
        entry.attempts = 1;
        entry.reason = Some("registry unavailable".to_string());
        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        if crates != plan.publish_order {
            bail!("resume should retry a failed publication entry");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_retries_published_entries_without_visibility_proof() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Published;
        entry.attempts = 1;
        entry.registry_visible = Some(false);

        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        if crates != plan.publish_order {
            bail!("resume must retry a published entry without visibility proof");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_does_not_retry_yanked_entries() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Yanked;
        entry.attempts = 1;
        entry.registry_visible = Some(false);
        entry.reason = Some("registry version is yanked".to_string());

        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        if crates != ["tokmd"].map(str::to_string) {
            bail!("resume must not retry a terminal yanked entry");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_retries_unobserved_registry_visibility() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Published;
        entry.attempts = 1;

        let crates = crates_to_publish(&plan, 0, Some(&receipt));
        if crates != plan.publish_order {
            bail!("resume must retry an unobserved registry entry");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_with_all_terminal_entries_has_no_work() -> Result<()> {
        let plan = receipt_test_plan();
        let mut receipt = new_publish_receipt(&plan);
        for entry in &mut receipt.crates {
            entry.state = PublishReceiptState::Published;
            entry.attempts = 1;
            entry.registry_visible = Some(true);
        }

        if !crates_to_publish(&plan, 0, Some(&receipt)).is_empty() {
            bail!("resume should not offer work after every crate is terminal");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_rejects_inconsistent_entry_state() -> Result<()> {
        let mut receipt = new_publish_receipt(&receipt_test_plan());
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.state = PublishReceiptState::Published;
        if validate_publish_receipt_entry(entry).is_ok() {
            bail!("inconsistent published entry should be rejected");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_rejects_impossible_evidence_fields() -> Result<()> {
        let mut receipt = new_publish_receipt(&receipt_test_plan());
        let Some(entry) = receipt.crates.get_mut(0) else {
            bail!("receipt test plan should contain a first crate");
        };
        entry.registry_visible = Some(true);
        if validate_publish_receipt_entry(entry).is_ok() {
            bail!("unattempted receipt entry must not claim registry visibility");
        }
        entry.registry_visible = None;
        entry.dependency_closure = Some(false);
        if validate_publish_receipt_entry(entry).is_ok() {
            bail!("receipt must not accept a failed dependency-closure proof");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_state_types_reject_cross_level_values() -> Result<()> {
        let receipt = new_publish_receipt(&receipt_test_plan());
        let mut crate_value = serde_json::to_value(
            receipt
                .crates
                .first()
                .ok_or_else(|| anyhow!("receipt test plan should contain a crate"))?,
        )?;
        *crate_value
            .get_mut("state")
            .ok_or_else(|| anyhow!("crate receipt should serialize a state field"))? =
            Value::String("complete".to_string());
        if serde_json::from_value::<PublishCrateReceipt>(crate_value).is_ok() {
            bail!("crate receipt must reject a run-level state");
        }

        let mut run_value = serde_json::to_value(&receipt)?;
        *run_value
            .get_mut("state")
            .ok_or_else(|| anyhow!("run receipt should serialize a state field"))? =
            Value::String("published".to_string());
        if serde_json::from_value::<PublishReceipt>(run_value).is_ok() {
            bail!("run receipt must reject a crate-level state");
        }
        Ok(())
    }

    #[test]
    fn verify_alias_normalizes_to_dry_run() -> Result<()> {
        let args = PublishArgs {
            verify: true,
            ..PublishArgs::default()
        };
        let normalized = normalize_publish_args(args);
        if !normalized.dry_run || normalized.verify {
            bail!("--verify should behave as a hidden --dry-run alias");
        }
        Ok(())
    }

    #[test]
    fn dry_run_rejects_publication_receipt() -> Result<()> {
        let args = PublishArgs {
            dry_run: true,
            receipt: Some(std::path::PathBuf::from("publish.json")),
            ..PublishArgs::default()
        };
        if validate_publish_mode(&args).is_ok() {
            bail!("dry-run must not create a publication receipt");
        }
        Ok(())
    }

    #[test]
    fn incomplete_resume_cannot_report_success_without_work() -> Result<()> {
        let args = PublishArgs {
            resume: true,
            ..PublishArgs::default()
        };
        let mut receipt = new_publish_receipt(&receipt_test_plan());
        receipt.state = PublishRunState::Incomplete;
        if validate_no_work_resume(&args, Some(&receipt)).is_ok() {
            bail!("an incomplete no-work resume must fail closed");
        }
        for entry in &mut receipt.crates {
            entry.state = PublishReceiptState::Published;
            entry.attempts = 1;
            entry.registry_visible = Some(true);
        }
        validate_no_work_resume(&args, Some(&receipt))?;
        receipt.state = PublishRunState::Complete;
        validate_no_work_resume(&args, Some(&receipt))?;
        Ok(())
    }

    #[test]
    fn missing_inventory_round_trips_and_does_not_claim_completion() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let mut receipt = new_publish_receipt(&plan);
        let inventory = plan
            .publish_order
            .iter()
            .map(|name| (name.clone(), fixture_lookup("missing")))
            .collect();
        reconcile_inventory(&path, &mut receipt, &inventory)?;
        let loaded = load_publish_receipt(&path, &plan)?;
        if loaded.state == PublishRunState::Complete
            || loaded.crates.iter().any(|entry| {
                entry.state != PublishReceiptState::Planned || entry.registry_visible != Some(false)
            })
        {
            bail!("missing inventory must remain a valid, resumable planned receipt");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_identity_mismatch_fails_closed() -> Result<()> {
        let plan = receipt_test_plan();
        let directory = tempdir()?;
        let path = directory.path().join("publish.json");
        let receipt = new_publish_receipt(&plan);
        write_publish_receipt(&path, &receipt)?;
        let mut changed = fixture_publish_identity(&plan);
        changed.source_tree = "changed-tree".to_string();
        if load_publish_receipt_for_identity(&path, &plan, &changed).is_ok() {
            bail!("changed immutable source identity must reject resume");
        }
        Ok(())
    }

    #[test]
    fn publication_receipt_lock_is_exclusive() -> Result<()> {
        let directory = tempdir()?;
        let path = directory.path().join("nested").join("publish.json");
        let first = acquire_publish_receipt_lock(&path)?;
        if acquire_publish_receipt_lock(&path).is_ok() {
            bail!("a concurrent publisher must not acquire the same receipt lock");
        }
        drop(first);
        acquire_publish_receipt_lock(&path)?;
        Ok(())
    }

    #[test]
    fn publication_receipt_path_rejects_unignored_worktree_locations() -> Result<()> {
        let parent = tempdir()?;
        let root = parent.path().join("repo");
        fs::create_dir_all(&root)?;
        let outside = parent.path().join("outside").join("receipt.json");
        let relative =
            validated_publish_receipt_path(Path::new("target/publishing/receipt.json"), &root)?;
        if relative != root.canonicalize()?.join("target/publishing/receipt.json") {
            bail!("relative target receipt must normalize against the workspace root");
        }
        validated_publish_receipt_path(&outside, &root)?;
        if validated_publish_receipt_path(Path::new("receipt.json"), &root).is_ok()
            || validated_publish_receipt_path(Path::new("docs/receipt.json"), &root).is_ok()
            || validated_publish_receipt_path(&root.join("receipt.json"), &root).is_ok()
        {
            bail!("receipt paths inside the worktree must stay under ignored target/");
        }
        Ok(())
    }

    #[test]
    fn noninteractive_publication_requires_explicit_yes() -> Result<()> {
        let args = PublishArgs::default();
        if validate_publish_authorization(&args, false).is_ok() {
            bail!("CI or redirected stdin must not authorize publication");
        }
        let authorized = PublishArgs {
            yes: true,
            ..PublishArgs::default()
        };
        validate_publish_authorization(&authorized, false)?;
        Ok(())
    }

    #[test]
    fn continue_on_error_blocks_descendants_but_runs_independent_branch() -> Result<()> {
        let crates = vec![
            "base".to_string(),
            "dependent".to_string(),
            "independent".to_string(),
        ];
        let predecessors = BTreeMap::from([
            ("base".to_string(), BTreeSet::new()),
            (
                "dependent".to_string(),
                BTreeSet::from(["base".to_string()]),
            ),
            ("independent".to_string(), BTreeSet::new()),
        ]);
        let args = PublishArgs {
            continue_on_error: true,
            ..PublishArgs::default()
        };
        let mut backend = FixtureBackend::new([
            fixture_attempt(FixturePublish::Failed("base failed"), []),
            fixture_attempt(FixturePublish::Success, ["present"]),
        ]);
        let (succeeded, failed) = execute_publish_with_backend(
            &crates,
            &args,
            "1.15.0",
            &BTreeSet::new(),
            &predecessors,
            None,
            None,
            &mut backend,
        )?;
        if succeeded != ["independent"] || failed != ["base", "dependent"] {
            bail!("only the failed prerequisite and its descendants should be unavailable");
        }
        if backend.publish_calls
            != [
                ("base".to_string(), false),
                ("independent".to_string(), false),
            ]
        {
            bail!("dependent must not upload, while the independent branch may continue");
        }
        Ok(())
    }
}
