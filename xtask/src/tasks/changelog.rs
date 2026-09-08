//! Fast, staged-diff checks for the repository's Changie release-note workflow.

use crate::cli::{ChangeArgs, HooksArgs, HooksCommand, PrecommitArgs};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::to_string;
use std::collections::BTreeMap;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// Pinned with .changie.yaml; the contract fixture below detects layout drift.
const FRAGMENT_DIRECTORY: &str = ".changes/unreleased";

const KINDS: &[&str] = &[
    "added",
    "changed",
    "fixed",
    "security",
    "documentation",
    "internal",
];
const COMPONENTS: &[&str] = &[
    "CLI",
    "Action",
    "Packets",
    "Browser/WASM",
    "Release",
    "Security",
    "Documentation",
    "Internal",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChangeClass {
    Required(String),
    Exempt(String),
}

pub fn run_change(args: ChangeArgs) -> Result<()> {
    let root = repository_root()?;
    let kind = args.kind.trim().to_ascii_lowercase();
    let component = canonical_component(args.component.trim())?;
    let body = args.body.trim();
    validate_kind(&kind)?;
    validate_component(&component)?;
    if body.is_empty() {
        bail!("--body must not be empty");
    }

    let encoded_body = to_string(body).context("encode fragment body")?;
    let content = format!("component: {component}\nkind: {kind}\nbody: {encoded_body}\n");
    let relative = match args.output {
        Some(output) => write_fragment(&root, &output, &content)?,
        None => {
            write_generated_fragment(&root, &default_fragment_path(&component, &kind), &content)?
        }
    };
    println!("created {relative}");
    println!("stage it with: git add -- {relative}");
    Ok(())
}

pub fn run_precommit(args: PrecommitArgs) -> Result<()> {
    if !args.staged {
        bail!("precommit requires --staged so unstaged working-tree noise is ignored");
    }
    let root = repository_root()?;
    let paths = staged_paths(&root)?;
    validate_staged(&root, &paths)
}

pub fn run_hooks(args: HooksArgs) -> Result<()> {
    match args.command {
        HooksCommand::Install => install_hooks(&repository_root()?),
    }
}

fn install_hooks(root: &Path) -> Result<()> {
    let configured = git_config(root, "--get", "core.hooksPath")?;
    prepare_and_enable_hooks(root, configured.as_deref())
}

fn prepare_and_enable_hooks(root: &Path, configured: Option<&str>) -> Result<()> {
    if let Some(path) = configured {
        let normalized = path.trim().trim_end_matches('/').trim_end_matches('\\');
        if normalized != ".githooks" && normalized != "./.githooks" {
            bail!(
                "refusing to replace unrelated core.hooksPath `{}`; configure .githooks explicitly if desired",
                path.trim()
            );
        }
    }

    #[cfg(unix)]
    let mut permission_updates = Vec::new();
    for (name, command) in [
        ("pre-commit", "cargo --locked xtask precommit --staged"),
        ("pre-push", "cargo --locked xtask gate --check"),
    ] {
        let hook = root.join(".githooks").join(name);
        let metadata = std::fs::symlink_metadata(&hook)
            .with_context(|| format!("inspect repository {name} hook {}", hook.display()))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            bail!("repository {name} hook must be a regular file");
        }
        let content = std::fs::read_to_string(&hook)
            .with_context(|| format!("read repository {name} hook {}", hook.display()))?;
        if !content.lines().any(|line| line.trim() == command) {
            bail!("repository {name} hook does not invoke `{command}`; refusing to overwrite it");
        }
        #[cfg(unix)]
        permission_updates.push((hook, metadata.permissions()));
    }
    #[cfg(unix)]
    for (hook, mut permissions) in permission_updates {
        permissions.set_mode(permissions.mode() | 0o111);
        std::fs::set_permissions(&hook, permissions)
            .with_context(|| format!("make repository hook executable {}", hook.display()))?;
    }
    // Validate both hooks and prepare their modes before redirecting Git.
    if configured.is_none() {
        run_git_checked(root, ["config", "--local", "core.hooksPath", ".githooks"])?;
    }
    println!("Git hooks are configured at .githooks (idempotent)");
    Ok(())
}

fn validate_staged(root: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        println!("precommit: pass (no staged changes)");
        return Ok(());
    }
    let mut fragments = Vec::new();
    let mut deleted_fragment = false;
    for path in paths.iter().filter(|path| is_fragment(path)) {
        match read_index_file(root, path)? {
            Some(content) => {
                validate_fragment(path, &content)?;
                fragments.push(path);
            }
            None => deleted_fragment = true,
        }
    }
    if deleted_fragment && fragments.is_empty() {
        bail!(
            "precommit: a release-note fragment was deleted without a replacement\n\nCreate one explicitly, then stage it:\n  cargo change --kind changed --component Release --body \"Describe the release-note correction\""
        );
    }

    let class = classify_paths(paths);
    match class {
        ChangeClass::Required(reason) if fragments.is_empty() => bail!(
            "precommit: release-note fragment required ({reason})\n\nCreate one explicitly, then stage it:\n  cargo change --kind fixed --component CLI --body \"Describe the user-visible change\""
        ),
        ChangeClass::Required(reason) => {
            println!("precommit: pass (fragment validated; {reason})");
        }
        ChangeClass::Exempt(reason) if fragments.is_empty() => {
            println!("precommit: pass (explicit exemption: {reason})");
        }
        ChangeClass::Exempt(reason) => {
            println!("precommit: pass (fragment validated; exemption: {reason})");
        }
    }
    Ok(())
}

fn classify_paths(paths: &[String]) -> ChangeClass {
    let mut reasons = Vec::new();
    for path in paths {
        if is_fragment(path) {
            continue;
        }
        if is_explicitly_exempt(path) {
            reasons.push(format!("{path} is test/generated-only"));
        } else {
            return ChangeClass::Required(format!(
                "staged path `{path}` is user-visible or unknown"
            ));
        }
    }
    if reasons.is_empty() {
        ChangeClass::Exempt("only release-note fragments changed".to_string())
    } else {
        ChangeClass::Exempt(reasons.join(", "))
    }
}

fn is_explicitly_exempt(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let path = normalized.as_str();
    path == "Cargo.lock"
        || path == ".changes/unreleased/.gitkeep"
        || path.starts_with("tests/")
        || path.starts_with("xtask/tests/")
        || path.split('/').any(|component| component == "tests")
        || path.starts_with(".jules/")
        || path.starts_with("target/")
        || path.ends_with("_test.rs")
        || path.ends_with("/tests.rs")
}

fn is_fragment(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    fragment_name(&normalized).is_some()
        && (normalized.ends_with(".yaml") || normalized.ends_with(".yml"))
}

fn fragment_name(path: &str) -> Option<&str> {
    path.strip_prefix(FRAGMENT_DIRECTORY)?.strip_prefix('/')
}

fn validate_fragment(path: &str, content: &str) -> Result<()> {
    let normalized_path = path.replace('\\', "/");
    let fragment_name = fragment_name(&normalized_path)
        .ok_or_else(|| anyhow::anyhow!("fragment is outside .changes/unreleased/: {path}"))?;
    if fragment_name.contains('/') {
        bail!("fragment must be directly in .changes/unreleased/: {path}");
    }
    let file_name = Path::new(fragment_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("fragment path has no valid filename: {path}"))?;
    if file_name == ".gitkeep" {
        bail!("invalid unreleased fragment filename: {path}");
    }
    let fields = fragment_fields(content).with_context(|| {
        format!("invalid fragment {path}; use `cargo change` to create a fragment")
    })?;
    let required = |name| {
        fields
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("fragment {path} is missing `{name}:`"))
    };
    let component = required("component")?;
    let kind = required("kind")?;
    let body = required("body")?;
    validate_component(component)?;
    validate_kind(kind)?;
    if body.trim().is_empty() {
        bail!("fragment {path} has an empty body");
    }
    if let Some(time) = fields.get("time") {
        validate_fragment_time(time).with_context(|| format!("invalid time in fragment {path}"))?;
    }
    Ok(())
}

fn validate_fragment_time(value: &str) -> Result<()> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value)
        .context("time must be an RFC3339 timestamp, for example 2026-09-07T00:00:00Z")?;
    // Go's time.Time consumer requires a four-digit year and uppercase T/Z;
    // unlike Chrono it does not represent leap seconds.
    if !value.is_ascii()
        || value.as_bytes().get(10) != Some(&b'T')
        || !value
            .as_bytes()
            .get(..4)
            .is_some_and(|year| year.iter().all(u8::is_ascii_digit))
        || value.ends_with('z')
        || value.contains(',')
        || value.chars().any(char::is_whitespace)
        || parsed.timestamp_subsec_nanos() >= 1_000_000_000
    {
        bail!("time must use Changie-compatible RFC3339 syntax without leap seconds");
    }
    Ok(())
}

fn fragment_fields(content: &str) -> Result<BTreeMap<&str, String>> {
    let mut fields = BTreeMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.trim_start() != line {
            bail!("fragment fields must be top-level, one per line");
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("fragment line must have `field: value` form"))?;
        if !matches!(name, "component" | "kind" | "body" | "time") {
            bail!("unsupported fragment field `{name}`");
        }
        if !value.starts_with(char::is_whitespace) {
            bail!("fragment field `{name}` needs whitespace after its colon");
        }
        let value = fragment_string(value.trim())
            .with_context(|| format!("decode fragment field `{name}`"))?;
        if fields.insert(name, value).is_some() {
            bail!("duplicate fragment field `{name}`");
        }
    }
    Ok(fields)
}

fn fragment_string(value: &str) -> Result<String> {
    if value.starts_with('"') {
        return serde_json::from_str(value).context("expected a JSON-quoted string");
    }
    // Accept the flat plain scalars emitted by our pinned fragments. Rich YAML
    // syntax must be quoted instead of being silently mistaken for a string.
    if value.is_empty()
        || value.starts_with([
            '[', ']', '{', '}', '&', '*', '!', '|', '>', '\'', '%', '@', '`', ',', '-', '?', ':',
            '#',
        ])
        || value.contains(": ")
        || value.contains(":\t")
        || value.ends_with(':')
        || value.contains(" #")
        || value.contains("\t#")
        || value.chars().any(char::is_control)
        || value == "~"
        || value.eq_ignore_ascii_case("null")
    {
        bail!("unsupported plain YAML value; use a JSON-quoted string");
    }
    Ok(value.to_owned())
}

#[cfg(test)]
fn yaml_field(content: &str, field: &str) -> Result<String> {
    let prefix = format!("{field}:");
    let line = content
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::trim))
        .ok_or_else(|| anyhow::anyhow!("fragment is missing `{field}:`"))?;
    if line.is_empty() {
        bail!("fragment field `{field}` is empty");
    }
    if line.starts_with('"') {
        serde_json::from_str(line).with_context(|| format!("decode quoted `{field}` field"))
    } else {
        Ok(line.to_string())
    }
}

fn validate_kind(kind: &str) -> Result<()> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        bail!(
            "unsupported fragment kind `{kind}`; expected one of {}",
            KINDS.join(", ")
        )
    }
}

fn validate_component(component: &str) -> Result<()> {
    let canonical = canonical_component(component)?;
    if component != canonical {
        bail!("fragment component `{component}` must use canonical spelling `{canonical}`");
    }
    Ok(())
}

fn canonical_component(component: &str) -> Result<String> {
    COMPONENTS
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(component))
        .map(|candidate| (*candidate).to_string())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported fragment component `{component}`; expected one of {}",
                COMPONENTS.join(", ")
            )
        })
}

fn default_fragment_path(component: &str, kind: &str) -> PathBuf {
    let safe_component = component
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    PathBuf::from(format!(
        "{FRAGMENT_DIRECTORY}/{safe_component}-{kind}-{timestamp}.yaml"
    ))
}

fn fragment_output_path(root: &Path, output: &Path) -> Result<(String, PathBuf)> {
    let relative = normalize_relative(output);
    let output_path = Path::new(&relative);
    if output_path.is_absolute()
        || output_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("fragment output must not be absolute or contain parent traversal: {relative}");
    }
    let fragment_name = fragment_name(&relative).ok_or_else(|| {
        anyhow::anyhow!("fragment output must be under {FRAGMENT_DIRECTORY}/: {relative}")
    })?;
    if fragment_name.is_empty()
        || fragment_name.contains('/')
        || fragment_name.contains(':')
        || fragment_name.chars().any(char::is_control)
    {
        bail!("fragment output must be directly in .changes/unreleased/: {relative}");
    }
    if !fragment_name.ends_with(".yaml") && !fragment_name.ends_with(".yml") {
        bail!("fragment output must have a .yaml or .yml extension: {relative}");
    }
    let path = root.join(output_path);
    Ok((relative, path))
}

fn write_fragment(root: &Path, output: &Path, content: &str) -> Result<String> {
    write_fragment_with(root, output, content, |file, bytes| file.write_all(bytes))
}

fn write_generated_fragment(root: &Path, output: &Path, content: &str) -> Result<String> {
    let stem = output
        .file_stem()
        .and_then(|name| name.to_str())
        .context("generated fragment has no UTF-8 filename stem")?;
    for attempt in 0..100 {
        let candidate = if attempt == 0 {
            output.to_path_buf()
        } else {
            output.with_file_name(format!("{stem}-{attempt}.yaml"))
        };
        match write_fragment(root, &candidate, content) {
            Ok(relative) => return Ok(relative),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) => {}
            Err(error) => return Err(error),
        }
    }
    bail!("could not allocate a fragment filename; retry or select a new --output path")
}

fn write_fragment_with(
    root: &Path,
    output: &Path,
    content: &str,
    write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<String> {
    let root = root
        .canonicalize()
        .with_context(|| format!("canonicalize repository root {}", root.display()))?;
    let (relative, path) = fragment_output_path(&root, output)?;
    prepare_fragment_directory(&root)?;

    // Stage the complete contents in the same directory. Ordinary write
    // failures drop only our temporary file; the destination remains absent.
    let mut temporary = tempfile::Builder::new()
        .prefix(".tokmd-change-")
        .suffix(".tmp")
        .tempfile_in(root.join(FRAGMENT_DIRECTORY))
        .with_context(|| format!("stage fragment {relative}"))?;
    write(temporary.as_file_mut(), content.as_bytes())
        .with_context(|| format!("write fragment {relative}"))?;
    temporary
        .as_file()
        .sync_all()
        .with_context(|| format!("sync staged fragment {relative}"))?;
    // No-clobber installation refuses existing files and leaf symlinks. Drop
    // the returned temporary file immediately on failure, retaining the IO
    // error kind so generated-name collisions can retry safely.
    temporary
        .persist_noclobber(&path)
        .map_err(|error| error.error)
        .with_context(|| {
            format!("install new fragment {relative}; existing outputs are never overwritten")
        })?;
    Ok(relative)
}

fn prepare_fragment_directory(root: &Path) -> Result<()> {
    let mut directory = root.to_path_buf();
    for component in Path::new(FRAGMENT_DIRECTORY).components() {
        directory.push(component);
        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create fragment directory {}", directory.display()));
            }
        }
        let metadata = std::fs::symlink_metadata(&directory)
            .with_context(|| format!("inspect fragment directory {}", directory.display()))?;
        let canonical = directory
            .canonicalize()
            .with_context(|| format!("canonicalize fragment directory {}", directory.display()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || canonical != directory {
            bail!(
                "fragment directory must be a real directory at the pinned path, not a symlink or alias: {}",
                directory.display()
            );
        }
    }
    Ok(())
}

fn normalize_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn repository_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("run git rev-parse --show-toplevel")?;
    if !output.status.success() {
        bail!("not inside a Git repository");
    }
    let root = String::from_utf8(output.stdout).context("Git repository root is not UTF-8")?;
    Ok(PathBuf::from(root.trim()))
}

fn staged_paths(root: &Path) -> Result<Vec<String>> {
    let output = run_git(root, ["diff", "--cached", "--name-status", "-z", "--"])?;
    if !output.status.success() {
        bail!(
            "git staged-diff inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    parse_name_status(&output.stdout)
}

fn parse_name_status(bytes: &[u8]) -> Result<Vec<String>> {
    let tokens = bytes
        .split(|byte| *byte == 0)
        .filter(|token| !token.is_empty())
        .map(|token| String::from_utf8(token.to_vec()).context("staged path is not UTF-8"))
        .collect::<Result<Vec<_>>>()?;
    let mut paths = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let status = tokens
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("missing staged diff status record"))?;
        index = index
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("staged diff status index overflowed"))?;
        let path_count = usize::from(status.starts_with('R') || status.starts_with('C')) + 1;
        let end = index
            .checked_add(path_count)
            .ok_or_else(|| anyhow::anyhow!("staged diff path index overflowed"))?;
        if end > tokens.len() {
            bail!("malformed staged diff status record `{status}`");
        }
        for offset in 0..path_count {
            let path = tokens
                .get(index + offset)
                .ok_or_else(|| anyhow::anyhow!("missing staged diff path"))?;
            paths.push(path.clone());
        }
        index = end;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn read_index_file(root: &Path, path: &str) -> Result<Option<String>> {
    let spec = format!(":{path}");
    let output = run_git(root, ["show", &spec])?;
    if !output.status.success() {
        let deletion = run_git(
            root,
            [
                "diff",
                "--cached",
                "--no-renames",
                "--diff-filter=D",
                "--name-only",
                "--",
                path,
            ],
        )?;
        if !deletion.status.success() {
            bail!(
                "staged fragment `{path}` could not be inspected: {}",
                String::from_utf8_lossy(&deletion.stderr).trim()
            );
        }
        let deleted = String::from_utf8(deletion.stdout)
            .context("staged deletion path is not UTF-8")?
            .lines()
            .any(|candidate| candidate == path);
        if deleted {
            return Ok(None);
        }
        bail!(
            "staged fragment `{path}` could not be read: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(Some(String::from_utf8(output.stdout).with_context(
        || format!("fragment `{path}` is not UTF-8"),
    )?))
}

fn git_config(root: &Path, first: &str, second: &str) -> Result<Option<String>> {
    // Respect inherited hook configuration as well as repository-local values.
    let output = run_git(root, ["config", first, second])?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout).context("Git config value is not UTF-8")?;
        Ok(Some(value.trim().to_string()))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        bail!(
            "read Git configuration failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
}

fn run_git<const N: usize>(root: &Path, args: [&str; N]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("run Git command")
}

fn run_git_checked<const N: usize>(root: &Path, args: [&str; N]) -> Result<()> {
    let output = run_git(root, args)?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn installed_launcher_preserves_partial_staging_and_propagates_verdict() -> Result<()> {
        let root = tempfile::tempdir()?;
        run_git_checked(root.path(), ["init", "--quiet"])?;
        run_git_checked(root.path(), ["config", "core.autocrlf", "false"])?;
        let source = root.path().join("sample.rs");
        std::fs::write(&source, "fn staged() {}\n")?;
        run_git_checked(root.path(), ["add", "--", "sample.rs"])?;
        let staged = run_git(root.path(), ["show", ":sample.rs"])?;
        if !staged.status.success() {
            bail!("could not capture fixture index contents");
        }
        let working = "fn unstaged() {} // preserve partial staging\n";
        std::fs::write(&source, working)?;
        let hook = root.path().join("pre-commit");
        std::fs::write(&hook, include_str!("../../../.githooks/pre-commit"))?;
        let log = root.path().join("cargo.log");
        // Record the command boundary without running Cargo or changing the index.
        let driver = r#"cargo() { printf '%s\n' "$*" >> "$TOKMD_TEST_CARGO_LOG"; return "$TOKMD_TEST_CARGO_STATUS"; }
export -f cargo
bash "$1"
"#;
        for status in [0, 7] {
            std::fs::write(&log, "")?;
            let output = Command::new("bash")
                .current_dir(root.path())
                .args(["-c", driver, "hook-fixture"])
                .arg(&hook)
                .env("TOKMD_TEST_CARGO_LOG", &log)
                .env("TOKMD_TEST_CARGO_STATUS", status.to_string())
                .output()
                .context("execute the checked-in pre-commit launcher")?;
            if output.status.code() != Some(status) {
                bail!("hook did not propagate validator exit {status}: {output:?}");
            }
            if std::fs::read_to_string(&log)? != "--locked xtask precommit --staged\n" {
                bail!("pre-commit ran additional or unexpected Cargo work");
            }
            let after = run_git(root.path(), ["show", ":sample.rs"])?;
            if !after.status.success()
                || after.stdout != staged.stdout
                || std::fs::read_to_string(&source)? != working
            {
                bail!("pre-commit changed the index or unstaged working-tree contents");
            }
        }
        Ok(())
    }

    #[test]
    fn hook_preparation_precedes_configuration_and_install_is_idempotent() -> Result<()> {
        let root = tempfile::tempdir()?;
        run_git_checked(root.path(), ["init", "--quiet"])?;
        let config = root.path().join(".git/config");
        let original = std::fs::read(&config)?;
        std::fs::create_dir(root.path().join(".githooks"))?;
        std::fs::write(
            root.path().join(".githooks/pre-commit"),
            include_str!("../../../.githooks/pre-commit"),
        )?;
        if prepare_and_enable_hooks(root.path(), None).is_ok()
            || std::fs::read(&config)? != original
        {
            bail!("missing pre-push hook changed Git configuration");
        }
        std::fs::write(
            root.path().join(".githooks/pre-push"),
            include_str!("../../../.githooks/pre-push"),
        )?;
        prepare_and_enable_hooks(root.path(), None)?;
        let installed = std::fs::read(&config)?;
        install_hooks(root.path())?;
        if std::fs::read(&config)? != installed {
            bail!("repeated hook installation changed Git configuration");
        }
        run_git_checked(
            root.path(),
            ["config", "--local", "core.hooksPath", "unrelated-hooks"],
        )?;
        let unrelated = std::fs::read(&config)?;
        if install_hooks(root.path()).is_ok() || std::fs::read(&config)? != unrelated {
            bail!("hook installation replaced unrelated configuration");
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn hook_installation_makes_both_managed_hooks_executable() -> Result<()> {
        let root = tempfile::tempdir()?;
        run_git_checked(root.path(), ["init", "--quiet"])?;
        std::fs::create_dir(root.path().join(".githooks"))?;
        for (name, content) in [
            ("pre-commit", include_str!("../../../.githooks/pre-commit")),
            ("pre-push", include_str!("../../../.githooks/pre-push")),
        ] {
            let hook = root.path().join(".githooks").join(name);
            std::fs::write(&hook, content)?;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o640))?;
        }
        prepare_and_enable_hooks(root.path(), None)?;
        for name in ["pre-commit", "pre-push"] {
            let mode = std::fs::metadata(root.path().join(".githooks").join(name))?
                .permissions()
                .mode();
            if mode & 0o777 != 0o751 {
                bail!("{name} execute bits or prior permissions were not preserved");
            }
        }
        Ok(())
    }

    #[test]
    fn parses_added_modified_deleted_and_rename_records() -> Result<()> {
        let bytes = b"A\0README.md\0M\0src/lib.rs\0D\0tests/old.rs\0R100\0old.md\0new.md\0";
        let paths = parse_name_status(bytes)?;
        let expected = vec![
            "README.md".to_string(),
            "new.md".to_string(),
            "old.md".to_string(),
            "src/lib.rs".to_string(),
            "tests/old.rs".to_string(),
        ];
        if paths != expected {
            bail!("parsed paths did not match expected Git name-status records");
        }
        Ok(())
    }

    #[test]
    fn classifies_user_visible_and_unknown_paths_conservatively() -> Result<()> {
        let cases = [
            (
                vec!["README.md".to_string()],
                true,
                "README should require a release note",
            ),
            (
                vec!["tests/unit.rs".to_string(), "Cargo.lock".to_string()],
                false,
                "test and lockfile changes should be exempt",
            ),
            (
                vec!["crates/example/tests/fixture.rs".to_string()],
                false,
                "fixture-only changes should be exempt",
            ),
            (
                vec!["scripts/new-tool.ps1".to_string()],
                true,
                "unknown scripts should require a release note",
            ),
        ];
        for (paths, required, message) in cases {
            let is_required = matches!(classify_paths(&paths), ChangeClass::Required(_));
            if is_required != required {
                bail!("{message}");
            }
        }
        Ok(())
    }

    #[test]
    fn fragment_output_rejects_absolute_and_parent_traversal_paths() -> Result<()> {
        let root = Path::new("C:/repo");
        for output in [
            Path::new(".changes/unreleased/../escaped.yaml"),
            Path::new("C:/repo/.changes/unreleased/escaped.yaml"),
            Path::new(".changes/unreleased/nested/escaped.yaml"),
        ] {
            if fragment_output_path(root, output).is_ok() {
                bail!("unsafe fragment output was accepted: {}", output.display());
            }
        }
        let (relative, path) = fragment_output_path(
            root,
            Path::new(".changes/unreleased/Release-fixed-20260808.yaml"),
        )?;
        if relative != ".changes/unreleased/Release-fixed-20260808.yaml"
            || path != root.join(&relative)
        {
            bail!("safe fragment output was normalized incorrectly");
        }
        if !is_explicitly_exempt(".changes/unreleased/.gitkeep") {
            bail!("the unreleased directory sentinel must be exempt");
        }
        Ok(())
    }

    #[test]
    fn validates_valid_and_invalid_fragments() -> Result<()> {
        validate_fragment(
            ".changes/unreleased/CLI-fixed-20260808.yaml",
            "component: CLI\nkind: fixed\nbody: \"Make the error actionable\"\n",
        )?;
        if validate_fragment(
            ".changes/unreleased/CLI-fixed-20260808.yaml",
            "component: Nope\nkind: fixed\nbody: broken\n",
        )
        .is_ok()
        {
            bail!("invalid component should be rejected");
        }
        if validate_fragment(
            ".changes/unreleased/CLI-fixed-20260808.yaml",
            "component: CLI\nkind: fixed\nbody:\n",
        )
        .is_ok()
        {
            bail!("empty body should be rejected");
        }
        if canonical_component("release")? != "Release" {
            bail!("component aliases should normalize to their canonical spelling");
        }
        if validate_fragment(
            ".changes/unreleased/nested/CLI-fixed-20260808.yaml",
            "component: CLI\nkind: fixed\nbody: \"Make the error actionable\"\n",
        )
        .is_ok()
        {
            bail!("nested fragment paths should be rejected");
        }
        Ok(())
    }

    #[test]
    fn changie_config_matches_the_pinned_validator_contract() -> Result<()> {
        let config = include_str!("../../../.changie.yaml").replace("\r\n", "\n");
        let directory = format!(
            "{}/{}",
            yaml_field(&config, "changesDir")?,
            yaml_field(&config, "unreleasedDir")?
        );
        if directory != FRAGMENT_DIRECTORY {
            bail!("Changie directory differs from the staged-fragment contract");
        }
        let (_, components) = config
            .split_once("components:\n")
            .ok_or_else(|| anyhow::anyhow!("Changie components are missing"))?;
        let (components, kinds) = components
            .split_once("kinds:\n")
            .ok_or_else(|| anyhow::anyhow!("Changie kinds are missing"))?;
        let components = components
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- "))
            .collect::<Vec<_>>();
        let kinds = kinds
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- key: "))
            .collect::<Vec<_>>();
        if components != COMPONENTS || kinds != KINDS {
            bail!("Changie vocabulary differs from the fragment validator");
        }
        Ok(())
    }

    #[test]
    fn staged_fragments_reject_invalid_optional_times() -> Result<()> {
        for time in [
            "yesterday",
            "2026-99-07T00:00:00Z",
            "2026-09-07T00:00:60Z",
            "2026-09-07t00:00:00z",
        ] {
            let content = format!("component: CLI\nkind: fixed\nbody: valid\ntime: {time}\n");
            if validate_fragment(".changes/unreleased/invalid-time.yaml", &content).is_ok() {
                bail!("invalid timestamp accepted: {time}");
            }
        }
        Ok(())
    }

    #[test]
    fn staged_fragments_accept_quoted_and_generated_times() -> Result<()> {
        for time in ["2026-09-07T00:00:00Z", "2026-08-08T06:11:22.8530286-04:00"] {
            for encoded in [time.to_owned(), to_string(time)?] {
                let content =
                    format!("component: CLI\nkind: fixed\nbody: valid\ntime: {encoded}\n");
                validate_fragment(".changes/unreleased/time.yaml", &content)?;
            }
        }
        Ok(())
    }

    #[test]
    fn generated_fragment_collisions_preserve_both_contents() -> Result<()> {
        let root = tempfile::tempdir()?;
        let output = default_fragment_path("CLI", "fixed");
        // Reuse the identical timestamp to force the collision independently
        // of clock resolution or test scheduling.
        let first = write_generated_fragment(root.path(), &output, "first")?;
        let second = write_generated_fragment(root.path(), &output, "second")?;
        if first == second
            || std::fs::read_to_string(root.path().join(first))? != "first"
            || std::fs::read_to_string(root.path().join(second))? != "second"
        {
            bail!("generated fragment collision lost or overwrote contents");
        }
        Ok(())
    }

    #[test]
    fn failed_fragment_write_leaves_no_destination_or_temporary_file() -> Result<()> {
        let root = tempfile::tempdir()?;
        let output = Path::new(".changes/unreleased/failed.yaml");
        let result = write_fragment_with(root.path(), output, "complete", |file, _| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write failure"))
        });
        if result.is_ok()
            || root.path().join(output).try_exists()?
            || root
                .path()
                .join(FRAGMENT_DIRECTORY)
                .read_dir()?
                .next()
                .transpose()?
                .is_some()
        {
            bail!("failed fragment write retained partial output");
        }
        write_fragment(root.path(), output, "complete")?;
        if std::fs::read_to_string(root.path().join(output))? != "complete" {
            bail!("retry after failed fragment write did not install complete contents");
        }
        Ok(())
    }

    #[test]
    fn staged_fragments_reject_malformed_yaml_and_non_string_bodies() -> Result<()> {
        for content in [
            "component: CLI\nkind: fixed\nbody: []\n",
            "component: CLI\nkind: fixed\nbody: # only a comment\n",
            "component: CLI\nkind: fixed\nbody: first\nbody: second\n",
            "component: CLI\nkind: fixed\nbody: broken: mapping\n",
            "component: CLI\nkind: fixed\nbody: first\nthis is not a mapping\n",
            "component: CLI\nkind: fixed\nbody: null\n",
            "component: CLI\nkind: fixed\nbody: >-\n  hidden body\n",
            "component: CLI\nkind: fixed\nbody: fine\nextra: unsupported\n",
        ] {
            if validate_fragment(".changes/unreleased/invalid.yaml", content).is_ok() {
                bail!("malformed fragment was accepted: {content}");
            }
        }
        Ok(())
    }

    #[test]
    fn staged_fragments_accept_generated_strings_and_the_pinned_time_field() -> Result<()> {
        let path = ".changes/unreleased/valid.yaml";
        validate_fragment(
            path,
            include_str!("../../../.changes/unreleased/Release-internal-20260808-061122.yaml"),
        )?;
        for body in [
            "plain words",
            "nested: value # text",
            "line one\nline two",
            "[]",
            "null",
            "Unicode: λ 🦀",
        ] {
            let content = format!("component: CLI\nkind: fixed\nbody: {}\n", to_string(body)?);
            validate_fragment(path, &content)?;
            if fragment_fields(&content)?.get("body").map(String::as_str) != Some(body) {
                bail!("quoted fragment body did not round-trip");
            }
        }
        Ok(())
    }

    #[test]
    fn staged_fragments_require_canonical_keys_on_both_line_endings() -> Result<()> {
        for component in COMPONENTS {
            for kind in KINDS {
                for newline in ["\n", "\r\n"] {
                    let content = format!(
                        "component: {component}{newline}kind: {kind}{newline}body: \"A correction\"{newline}"
                    );
                    validate_fragment(".changes/unreleased/valid.yaml", &content)?;
                }
            }
        }
        for content in [
            "component: cli\nkind: fixed\nbody: correction\n",
            "component: CLI\nkind: Fixed\nbody: correction\n",
            "component: CLI\nkind: UNKNOWN\nbody: correction\n",
        ] {
            if validate_fragment(".changes/unreleased/invalid.yaml", content).is_ok() {
                bail!("a non-canonical staged fragment was accepted: {content}");
            }
        }
        Ok(())
    }

    #[test]
    fn fragment_classification_normalizes_path_separators() -> Result<()> {
        for path in [
            "tests/unit.rs",
            "xtask/tests/fixture.rs",
            "crates/example/tests/unit.rs",
            ".changes/unreleased/.gitkeep",
            ".jules/provenance.md",
        ] {
            if !is_explicitly_exempt(path) || !is_explicitly_exempt(&path.replace('/', "\\")) {
                bail!("path separator changed the exemption for {path}");
            }
        }
        let path = ".changes/unreleased/Release-fixed.yaml";
        if !is_fragment(path) || !is_fragment(&path.replace('/', "\\")) {
            bail!("path separator changed fragment recognition");
        }
        if is_fragment(".changes/unreleased-other/fake.yaml") {
            bail!("a sibling directory was accepted as the fragment directory");
        }
        Ok(())
    }

    #[test]
    fn fragment_creation_preserves_existing_output() -> Result<()> {
        let root = tempfile::tempdir()?;
        let output = Path::new(".changes/unreleased/Release-fixed.yaml");
        let content = "component: Release\nkind: fixed\nbody: original\n";
        let relative = write_fragment(root.path(), output, content)?;
        validate_fragment(
            &relative,
            &std::fs::read_to_string(root.path().join(output))?,
        )?;
        if write_fragment(root.path(), output, "replacement").is_ok() {
            bail!("an existing fragment was overwritten");
        }
        if std::fs::read_to_string(root.path().join(output))? != content {
            bail!("refused overwrite changed the original fragment");
        }
        Ok(())
    }

    #[test]
    fn fragment_creation_rejects_invalid_names_before_creating_directories() -> Result<()> {
        let root = tempfile::tempdir()?;
        for output in [
            ".changes/unreleased/../escape.yaml",
            ".changes/unreleased/nested/escape.yaml",
            ".changes/unreleased/file.yaml:stream",
            ".changes/unreleased/file:stream.yaml",
            ".changes/unreleased/control\n.yaml",
            ".changes/unreleased/not-a-fragment.txt",
        ] {
            if write_fragment(root.path(), Path::new(output), "invalid").is_ok() {
                bail!("invalid fragment output was accepted: {output:?}");
            }
        }
        if root.path().join(".changes").try_exists()? {
            bail!("invalid output created the fragment directory");
        }
        Ok(())
    }

    #[cfg(any(unix, windows))]
    fn create_directory_alias(target: &Path, link: &Path) -> Result<()> {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link)?;
        #[cfg(windows)]
        {
            // Junctions need no symlink privilege on Windows. Paths arrive as
            // environment values, never interpolated PowerShell source.
            let output = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "$ErrorActionPreference = 'Stop'; New-Item -ItemType Junction -Path $env:TOKMD_TEST_JUNCTION -Target $env:TOKMD_TEST_TARGET | Out-Null",
                ])
                .env("TOKMD_TEST_JUNCTION", link)
                .env("TOKMD_TEST_TARGET", target)
                .output()
                .context("create test junction")?;
            if !output.status.success() {
                bail!(
                    "create test junction failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Ok(())
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn fragment_creation_rejects_directory_aliases() -> Result<()> {
        for directory in [".changes", FRAGMENT_DIRECTORY] {
            let root = tempfile::tempdir()?;
            let outside = tempfile::tempdir()?;
            let link = root.path().join(directory);
            let parent = link
                .parent()
                .ok_or_else(|| anyhow::anyhow!("test link has no parent"))?;
            std::fs::create_dir_all(parent)?;
            create_directory_alias(outside.path(), &link)?;
            let result = write_fragment(
                root.path(),
                Path::new(".changes/unreleased/escaped.yaml"),
                "must stay inside the repository",
            );
            let outside_changed = outside.path().read_dir()?.next().transpose()?.is_some();
            #[cfg(unix)]
            std::fs::remove_file(&link)?;
            #[cfg(windows)]
            std::fs::remove_dir(&link)?;
            if result.is_ok() || outside_changed {
                bail!("fragment creation followed directory alias {directory}");
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fragment_creation_rejects_existing_and_dangling_leaf_symlinks() -> Result<()> {
        for target_exists in [true, false] {
            let root = tempfile::tempdir()?;
            let outside = tempfile::tempdir()?;
            let target = outside.path().join("outside.yaml");
            if target_exists {
                std::fs::write(&target, "original")?;
            }
            let output = Path::new(".changes/unreleased/linked.yaml");
            let link = root.path().join(output);
            std::fs::create_dir_all(root.path().join(FRAGMENT_DIRECTORY))?;
            std::os::unix::fs::symlink(&target, &link)?;
            if write_fragment(root.path(), output, "replacement").is_ok() {
                bail!("fragment creation followed a leaf symlink");
            }
            if target_exists {
                if std::fs::read_to_string(&target)? != "original" {
                    bail!("fragment creation overwrote the symlink target");
                }
            } else if target.try_exists()? {
                bail!("fragment creation wrote through a dangling symlink");
            }
        }
        Ok(())
    }

    #[test]
    fn staged_validation_uses_index_content_and_rejects_noncanonical_keys() -> Result<()> {
        let root = tempfile::tempdir()?;
        run_git_checked(root.path(), ["init", "--quiet"])?;
        let output = Path::new(".changes/unreleased/CLI-fixed.yaml");
        let content = "component: CLI\nkind: fixed\nbody: correct\n";
        write_fragment(root.path(), output, content)?;
        std::fs::write(root.path().join("README.md"), "user-visible change")?;
        run_git_checked(root.path(), ["add", "--", "README.md", ".changes"])?;
        std::fs::write(root.path().join(output), content.replace("CLI", "cli"))?;
        let paths = staged_paths(root.path())?;
        validate_staged(root.path(), &paths)?;
        run_git_checked(root.path(), ["add", "--", ".changes"])?;
        if validate_staged(root.path(), &paths).is_ok() {
            bail!("staged noncanonical component was accepted");
        }
        Ok(())
    }
}
