//! Real Git/NUL-delimited fixtures for the UTF-8 discovery boundary.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use tokmd_scan::walk::list_files;

fn git(root: &Path, args: &[&OsStr]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .current_dir(root)
        .args(args)
        .output()
        .context("launch Git fixture")?;
    ensure!(
        output.status.success(),
        "Git fixture command failed: {output:?}"
    );
    Ok(output.stdout)
}

fn tracked(root: &Path, names: &[PathBuf]) -> Result<()> {
    git(root, &[OsStr::new("init"), OsStr::new("-q")])?;
    for name in names {
        fs::write(root.join(name), b"fixture\n")?;
    }
    git(root, &[OsStr::new("add"), OsStr::new("--all")])?;
    Ok(())
}

#[test]
fn git_discovery_preserves_supported_names_and_stable_limits() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut expected: Vec<PathBuf> = ["plain.txt", "caf\u{e9}.txt", "comma,name.txt"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    #[cfg(unix)]
    expected.extend(
        [
            "quote\"name.txt",
            "tab\tname.txt",
            "line\nname.txt",
            "literal\\name.txt",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    tracked(root.path(), &expected)?;
    fs::write(root.path().join("untracked.txt"), b"not selected\n")?;
    expected.sort();

    let raw = git(root.path(), &[OsStr::new("ls-files"), OsStr::new("-z")])?;
    ensure!(raw.ends_with(&[0]), "fixture must exercise NUL framing");
    assert_eq!(list_files(root.path(), None)?, expected);
    assert_eq!(list_files(root.path(), None)?, expected);
    assert_eq!(
        list_files(root.path(), Some(2))?,
        expected.iter().take(2).cloned().collect::<Vec<_>>()
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn git_discovery_rejects_non_utf8_without_aliasing_and_recovers_after_rename() -> Result<()> {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let root = tempfile::tempdir()?;
    let invalid = PathBuf::from(OsString::from_vec(b"raw-\xff.png".to_vec()));
    let replacement = PathBuf::from("raw-\u{fffd}.png");
    tracked(root.path(), &[invalid.clone(), replacement.clone()])?;
    let raw = git(root.path(), &[OsStr::new("ls-files"), OsStr::new("-z")])?;
    ensure!(
        raw.split(|b| *b == 0)
            .any(|name| name == invalid.as_os_str().as_bytes())
    );

    let error = list_files(root.path(), None)
        .err()
        .context("unsupported tracked path aliased a valid replacement-character filename")?;
    let message = format!("{error:#}");
    ensure!(
        message.contains("not valid UTF-8"),
        "missing encoding cause: {message}"
    );
    ensure!(
        message.contains("ff"),
        "missing identifying raw path bytes: {message}"
    );
    ensure!(
        message.contains("rename"),
        "missing local recovery instruction: {message}"
    );
    ensure!(
        message.contains("git ls-files"),
        "missing producer context: {message}"
    );

    fs::rename(root.path().join(&invalid), root.path().join("repaired.png"))?;
    git(root.path(), &[OsStr::new("add"), OsStr::new("--all")])?;
    let mut expected = vec![replacement, PathBuf::from("repaired.png")];
    expected.sort();
    assert_eq!(
        list_files(root.path(), None).context("retry after renaming")?,
        expected
    );
    Ok(())
}
