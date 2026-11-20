use anyhow::{Context, Result, bail};
use cargo_metadata::{MetadataCommand, PackageId};
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the nearest ancestor with a top-level workspace table in Cargo.toml.
pub(crate) fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        let path = dir.join("Cargo.toml");
        let content = match fs::read_to_string(&path) {
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            result => result.with_context(|| format!("Failed to read {}", path.display()))?,
        };
        let manifest: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML in {}", path.display()))?;
        if manifest.get("workspace").is_some_and(toml::Value::is_table) {
            return Ok(dir.to_path_buf());
        }
    }
    bail!(
        "Could not find workspace root (Cargo.toml with [workspace]) from {}",
        start.display()
    );
}

fn workspace_package_names() -> Result<Vec<String>> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to load cargo metadata")?;

    let workspace_members: HashSet<PackageId> = metadata.workspace_members.into_iter().collect();
    let mut packages: Vec<String> = metadata
        .packages
        .into_iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| package.name.to_string())
        .collect();

    packages.sort();
    packages.dedup();
    Ok(packages)
}

pub fn run_workspace_fmt(check: bool) -> Result<()> {
    if !cfg!(windows) {
        let mut command = Command::new("cargo");
        command.arg("fmt").arg("--all");
        if check {
            command.arg("--").arg("--check");
        }

        let status = command.status().context("failed to run cargo fmt")?;
        if !status.success() {
            bail!("fmt failed");
        }

        return Ok(());
    }

    for package in workspace_package_names()? {
        let mut command = Command::new("cargo");
        command.arg("fmt").arg("-p").arg(&package);
        if check {
            command.arg("--").arg("--check");
        }

        let status = command
            .status()
            .with_context(|| format!("failed to run cargo fmt for package {package}"))?;
        if !status.success() {
            bail!("fmt failed for package {package}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_workspace_above_nested_manifest_impostors() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let member = root.join("member");
        let nested = member.join("nested");
        let start = nested.join("src");
        fs::create_dir_all(&start)?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = '1.2.3'\n",
        )?;
        fs::write(
            member.join("Cargo.toml"),
            "# [workspace]\n[package]\nname = 'member'\n",
        )?;
        fs::write(
            nested.join("Cargo.toml"),
            "[package]\nname = 'nested'\ndescription = '[workspace]'\n",
        )?;

        assert_eq!(find_workspace_root(&start)?, root);
        assert_eq!(find_workspace_root(&member)?, root);
        assert_eq!(find_workspace_root(root)?, root);
        Ok(())
    }

    #[test]
    fn finds_repository_workspace_from_xtask() -> Result<()> {
        let xtask = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            find_workspace_root(xtask)?,
            xtask.parent().context("xtask needs a parent directory")?
        );
        Ok(())
    }

    #[test]
    fn reports_unreadable_and_malformed_manifest_paths() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("Cargo.toml");
        fs::create_dir(&path)?;
        let read_error = find_workspace_root(temp.path())
            .err()
            .context("expected manifest read error")?;
        assert_eq!(
            read_error.to_string(),
            format!("Failed to read {}", path.display())
        );

        fs::remove_dir(&path)?;
        fs::write(&path, "[workspace\n")?;
        let parse_error = find_workspace_root(temp.path())
            .err()
            .context("expected manifest parse error")?;
        assert_eq!(
            parse_error.to_string(),
            format!("Failed to parse TOML in {}", path.display())
        );
        Ok(())
    }

    #[test]
    fn workspace_package_names_are_sorted_and_include_known_members() {
        let packages = workspace_package_names().expect("workspace metadata should load");
        let mut sorted = packages.clone();
        sorted.sort();
        sorted.dedup();

        assert_eq!(packages, sorted);
        assert!(packages.iter().any(|name| name == "tokmd"));
        assert!(packages.iter().any(|name| name == "xtask"));
    }
}
