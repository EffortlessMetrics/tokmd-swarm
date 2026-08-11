//! Fast, staged-diff checks for the repository's Changie release-note workflow.

use crate::cli::{ChangeArgs, HooksArgs, HooksCommand, PrecommitArgs};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::to_string;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

    let output = match args.output {
        Some(output) => output,
        None => default_fragment_path(&component, &kind),
    };
    let (relative, path) = fragment_output_path(&root, &output)?;
    if path.exists() {
        bail!("refusing to overwrite existing fragment: {relative}");
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create fragment directory {}", parent.display()))?;
    }
    let encoded_body = to_string(body).context("encode fragment body")?;
    let content = format!("component: {component}\nkind: {kind}\nbody: {encoded_body}\n");
    std::fs::write(&path, content).with_context(|| format!("write fragment {}", path.display()))?;
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
    if let Some(path) = configured {
        let normalized = path.trim().trim_end_matches('/').trim_end_matches('\\');
        if normalized != ".githooks" && normalized != "./.githooks" {
            bail!(
                "refusing to replace unrelated core.hooksPath `{}`; configure .githooks explicitly if desired",
                path.trim()
            );
        }
    } else {
        run_git_checked(root, ["config", "--local", "core.hooksPath", ".githooks"])?;
    }

    let hook = root.join(".githooks/pre-commit");
    let content = std::fs::read_to_string(&hook)
        .with_context(|| format!("read repository pre-commit hook {}", hook.display()))?;
    if !content.contains("cargo xtask precommit --staged") {
        bail!(
            "repository pre-commit hook does not invoke `cargo xtask precommit --staged`; refusing to overwrite it"
        );
    }
    #[cfg(unix)]
    {
        let metadata = std::fs::metadata(&hook).with_context(|| {
            format!(
                "read repository pre-commit hook metadata {}",
                hook.display()
            )
        })?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        std::fs::set_permissions(&hook, permissions).with_context(|| {
            format!(
                "make repository pre-commit hook executable {}",
                hook.display()
            )
        })?;
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
    path.starts_with(".changes/unreleased/") && (path.ends_with(".yaml") || path.ends_with(".yml"))
}

fn validate_fragment(path: &str, content: &str) -> Result<()> {
    let normalized_path = path.replace('\\', "/");
    let fragment_name = normalized_path
        .strip_prefix(".changes/unreleased/")
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
    let component = yaml_field(content, "component")?;
    let kind = yaml_field(content, "kind")?.to_ascii_lowercase();
    let body = yaml_field(content, "body")?;
    validate_component(&component)?;
    validate_kind(&kind)?;
    if body.trim().is_empty() {
        bail!("fragment {path} has an empty body");
    }
    Ok(())
}

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
    canonical_component(component).map(|_| ())
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
        ".changes/unreleased/{safe_component}-{kind}-{timestamp}.yaml"
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
    if !relative.starts_with(".changes/unreleased/") {
        bail!("fragment output must be under .changes/unreleased/: {relative}");
    }
    let fragment_name = relative
        .strip_prefix(".changes/unreleased/")
        .ok_or_else(|| anyhow::anyhow!("fragment output has no filename: {relative}"))?;
    if fragment_name.is_empty() || fragment_name.contains('/') {
        bail!("fragment output must be directly in .changes/unreleased/: {relative}");
    }
    let path = root.join(output_path);
    let unreleased_root = root.join(".changes/unreleased");
    if !path.starts_with(&unreleased_root) {
        bail!("fragment output escapes .changes/unreleased/: {relative}");
    }
    Ok((relative, path))
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
    let output = run_git(root, ["config", "--local", first, second])?;
    if output.status.success() {
        let value = String::from_utf8(output.stdout).context("Git config value is not UTF-8")?;
        Ok(Some(value.trim().to_string()))
    } else {
        Ok(None)
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
}
