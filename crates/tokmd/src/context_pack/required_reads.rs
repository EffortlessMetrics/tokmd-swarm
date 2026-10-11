//! Deterministic selection-to-render witnesses shared by both command outputs.

use std::path::Path;

use anyhow::{Result, bail, ensure};
use tokmd_types::{ContextFileRow, FileKind, FileRow, InclusionPolicy};

use super::{SelectOptions, SelectResult, select_files_with_options};
use crate::cli::{ContextStrategy, ValueMetric};

pub(crate) const CONTENT: &str = "one\ntwo\n\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

pub(crate) fn select_fixture(path: &Path, policy: InclusionPolicy) -> Result<SelectResult> {
    std::fs::write(path, CONTENT)?;
    let row = FileRow {
        path: path.display().to_string().replace('\\', "/"),
        module: "src".into(),
        lang: "Rust".into(),
        kind: FileKind::Parent,
        code: 9,
        comments: 0,
        blanks: 1,
        lines: 10,
        bytes: CONTENT.len(),
        tokens: 10,
    };
    let selection = select_files_with_options(
        &[row],
        100,
        ContextStrategy::Greedy,
        ValueMetric::Code,
        None,
        &SelectOptions {
            max_file_tokens: Some(if policy == InclusionPolicy::HeadTail {
                4
            } else {
                100
            }),
            ..Default::default()
        },
    );
    ensure!(selection.selected.len() == 1, "fixture was not selected");
    ensure!(
        selection
            .selected
            .first()
            .is_some_and(|row| row.policy == policy),
        "wrong selection policy"
    );
    Ok(selection)
}

pub(crate) fn expect_io(error: &anyhow::Error, path: &Path, missing: bool) -> Result<()> {
    let io = error
        .downcast_ref::<std::io::Error>()
        .ok_or_else(|| anyhow::anyhow!("lost IO cause: {error:#}"))?;
    #[cfg(unix)]
    if !missing {
        ensure!(
            io.kind() == std::io::ErrorKind::IsADirectory,
            "replacement did not fail during read: {io}"
        );
    }
    if missing {
        ensure!(
            io.kind() == std::io::ErrorKind::NotFound,
            "wrong IO cause: {io}"
        );
    }
    ensure!(
        format!("{error:#}").contains(&path.display().to_string()),
        "lost selected path: {error:#}"
    );
    ensure!(
        crate::exit_code(error) == 1,
        "required read failure must exit 1"
    );
    Ok(())
}

pub(crate) fn required_read_matrix(
    mut render: impl FnMut(&Path, &[ContextFileRow], bool) -> Result<Vec<u8>>,
) -> Result<()> {
    for nested in [false, true] {
        for policy in [InclusionPolicy::Full, InclusionPolicy::HeadTail] {
            for compress in [false, true] {
                let temp = tempfile::tempdir()?;
                let parent = if nested {
                    temp.path().join("nested/src")
                } else {
                    temp.path().to_path_buf()
                };
                std::fs::create_dir_all(&parent)?;
                let path = parent.join("input.rs");
                let selected = select_fixture(&path, policy)?.selected;
                let stable = render(temp.path(), &selected, compress)?;
                let body = match (policy, compress) {
                    (InclusionPolicy::Full, false) => CONTENT.to_string(),
                    (InclusionPolicy::Full, true) => {
                        "one\ntwo\nfour\nfive\nsix\nseven\neight\nnine\nten\n".into()
                    }
                    (InclusionPolicy::HeadTail, false) => {
                        "one\ntwo\n\n// ... [6 lines omitted] ...\nten\n".into()
                    }
                    (InclusionPolicy::HeadTail, true) => {
                        "one\ntwo\n// ... [6 lines omitted] ...\nten\n".into()
                    }
                    _ => bail!("unexpected required policy"),
                };
                let row = selected
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("missing selected fixture"))?;
                ensure!(
                    stable == format!("// === {} ===\n{body}\n", row.path).as_bytes(),
                    "stable output format changed"
                );
                // Mutation is sequenced after real selection and before render, with no sleep.
                for _ in 0..2 {
                    std::fs::remove_file(&path)?;
                    let error = match render(temp.path(), &selected, compress) {
                        Ok(_) => bail!(
                            "missing selected input silently succeeded: {policy:?}, compress={compress}"
                        ),
                        Err(error) => error,
                    };
                    expect_io(&error, &path, true)?;
                    std::fs::write(&path, CONTENT)?;
                    ensure!(
                        render(temp.path(), &selected, compress)? == stable,
                        "repair/retry changed stable bytes"
                    );
                }
                // A directory opens on Unix but fails the actual read; no privilege changes.
                std::fs::remove_file(&path)?;
                std::fs::create_dir(&path)?;
                #[cfg(unix)]
                let _opened_directory = std::fs::File::open(&path)?;
                let error = match render(temp.path(), &selected, compress) {
                    Ok(_) => bail!("directory replacement silently succeeded"),
                    Err(error) => error,
                };
                expect_io(&error, &path, false)?;
                std::fs::remove_dir(&path)?;
                std::fs::write(&path, CONTENT.replace("one", "new"))?;
                let changed = render(temp.path(), &selected, compress)?;
                ensure!(
                    changed != stable,
                    "renderer did not read replacement contents"
                );
                ensure!(
                    std::str::from_utf8(&changed)?.contains("new\n"),
                    "missing replacement content"
                );
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn dangling_symlink_matrix(
    mut render: impl FnMut(&Path, &[ContextFileRow], bool) -> Result<Vec<u8>>,
) -> Result<()> {
    for policy in [InclusionPolicy::Full, InclusionPolicy::HeadTail] {
        for compress in [false, true] {
            let temp = tempfile::tempdir()?;
            let target = temp.path().join("target.rs");
            std::fs::write(&target, CONTENT)?;
            let path = temp.path().join("input.rs");
            let selected = select_fixture(&path, policy)?.selected;
            std::fs::remove_file(&path)?;
            std::os::unix::fs::symlink(&target, &path)?;
            let stable = render(temp.path(), &selected, compress)?;
            std::fs::remove_file(&target)?;
            let error = match render(temp.path(), &selected, compress) {
                Ok(_) => bail!("dangling selected symlink silently succeeded"),
                Err(error) => error,
            };
            expect_io(&error, &path, true)?;
            std::fs::write(&target, CONTENT)?;
            ensure!(
                render(temp.path(), &selected, compress)? == stable,
                "symlink repair changed bytes"
            );
        }
    }
    Ok(())
}

pub(crate) fn policy_exclusions(
    mut render: impl FnMut(&Path, &[ContextFileRow], bool) -> Result<Vec<u8>>,
) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("input.rs");
    let selected = select_fixture(&path, InclusionPolicy::Full)?.selected;
    let mut row = selected
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no fixture"))?;
    std::fs::remove_file(&path)?;
    for policy in [InclusionPolicy::Summary, InclusionPolicy::Skip] {
        row.policy = policy;
        row.policy_reason = Some("fixture policy".into());
        let expected = format!("// === {} [skipped: fixture policy] ===\n\n", row.path);
        for compress in [false, true] {
            ensure!(
                render(temp.path(), std::slice::from_ref(&row), compress)? == expected.as_bytes(),
                "policy omission changed or tried required read"
            );
        }
    }
    Ok(())
}

#[test]
fn context_pack_required_read_matrix() -> Result<()> {
    required_read_matrix(|_, selected, compress| {
        let mut out = Vec::new();
        super::write_bundle_output(&mut out, selected, compress, &[])?;
        Ok(out)
    })
}

#[cfg(unix)]
#[test]
fn context_pack_dangling_selected_symlink() -> Result<()> {
    dangling_symlink_matrix(|_, selected, compress| {
        let mut out = Vec::new();
        super::write_bundle_output(&mut out, selected, compress, &[])?;
        Ok(out)
    })
}

#[test]
fn context_pack_missing_policy_exclusions() -> Result<()> {
    policy_exclusions(|_, selected, compress| {
        let mut out = Vec::new();
        super::write_bundle_output(&mut out, selected, compress, &[])?;
        Ok(out)
    })
}

// The callback is inside the real command immediately after selection. It is
// per invocation, with no environment knob, global state or timing oracle.
type SelectionMutation<'a> = &'a mut dyn FnMut(&[ContextFileRow]) -> Result<()>;

pub(crate) fn command_matrix(
    command: &str,
    mut run: impl FnMut(crate::cli::Cli, SelectionMutation<'_>) -> Result<()>,
) -> Result<()> {
    use clap::Parser;
    for cap in [1000, 4] {
        for compress in [false, true] {
            let cwd = std::env::current_dir()?;
            let temp = tempfile::tempdir_in(&cwd)?;
            let root = temp.path().strip_prefix(&cwd)?.join("repo/nested");
            std::fs::create_dir_all(&root)?;
            let input = root.join("input.rs");
            let content = "fn f() {}\n\n".repeat(20);
            std::fs::write(&input, &content)?;
            let out = temp.path().join("out");
            let parse = || -> Result<crate::cli::Cli> {
                let mut argv = vec![
                    "tokmd".to_string(),
                    "--no-progress".into(),
                    command.into(),
                    root.display().to_string(),
                    "--budget".into(),
                    "1000".into(),
                    "--max-file-pct".into(),
                    "1.0".into(),
                    "--max-file-tokens".into(),
                    cap.to_string(),
                    "--no-git".into(),
                    "--output-dir".into(),
                    out.display().to_string(),
                    "--force".into(),
                ];
                if compress {
                    argv.push("--compress".into());
                }
                Ok(crate::cli::Cli::try_parse_from(argv)?)
            };
            let policy = if cap == 4 {
                InclusionPolicy::HeadTail
            } else {
                InclusionPolicy::Full
            };
            let mut check_selection = |selected: &[ContextFileRow]| -> Result<()> {
                ensure!(
                    selected.len() == 1,
                    "command did not select exactly one input"
                );
                ensure!(
                    selected.first().is_some_and(|row| row.policy == policy),
                    "wrong command policy: expected {policy:?}, selected={selected:?}"
                );
                Ok(())
            };
            let mut fresh_remove = |selected: &[ContextFileRow]| -> Result<()> {
                check_selection(selected)?;
                std::fs::remove_file(&input)?;
                Ok(())
            };
            let error = match run(parse()?, &mut fresh_remove) {
                Ok(()) => bail!("{command} created success on fresh missing input"),
                Err(error) => error,
            };
            expect_io(&error, &input, true)?;
            ensure!(
                !out.join("manifest.json").exists(),
                "fresh failure published manifest"
            );
            ensure!(
                !out.join("receipt.json").exists(),
                "fresh failure published receipt"
            );
            std::fs::write(&input, &content)?;
            run(parse()?, &mut check_selection)?;
            let payload_name = if command == "context" {
                "bundle.txt"
            } else {
                "code.txt"
            };
            let stable = std::fs::read(out.join(payload_name))?;
            check_completion(&out, command, &stable)?;
            for _ in 0..2 {
                let mut remove = |selected: &[ContextFileRow]| -> Result<()> {
                    check_selection(selected)?;
                    std::fs::remove_file(&input)?;
                    Ok(())
                };
                let error = match run(parse()?, &mut remove) {
                    Ok(()) => bail!("{command} silently completed after selection lost input"),
                    Err(error) => error,
                };
                expect_io(&error, &input, true)?;
                ensure!(
                    !out.join("manifest.json").exists(),
                    "{command} retained a complete-looking manifest on failure"
                );
                if command == "context" {
                    ensure!(
                        !out.join("receipt.json").exists(),
                        "context retained completed receipt on failure"
                    );
                }
                std::fs::write(&input, &content)?;
                run(parse()?, &mut check_selection)?;
                let retry = std::fs::read(out.join(payload_name))?;
                ensure!(retry == stable, "command repair/retry changed payload");
                check_completion(&out, command, &retry)?;
            }
            let mut replace = |selected: &[ContextFileRow]| -> Result<()> {
                check_selection(selected)?;
                std::fs::remove_file(&input)?;
                std::fs::create_dir(&input)?;
                Ok(())
            };
            let error = match run(parse()?, &mut replace) {
                Ok(()) => bail!("{command} completed after input was replaced with directory"),
                Err(error) => error,
            };
            expect_io(&error, &input, false)?;
            ensure!(
                !out.join("manifest.json").exists(),
                "{command} retained manifest after read failure"
            );
        }
    }
    Ok(())
}

fn check_completion(out: &Path, command: &str, payload: &[u8]) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json"))?)?;
    let files = value
        .get("included_files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("missing included_files"))?;
    ensure!(files.len() == 1, "wrong inventory count");
    let count_key = if command == "context" {
        "file_count"
    } else {
        "bundled_files"
    };
    ensure!(
        value.get(count_key).and_then(serde_json::Value::as_u64) == Some(1),
        "wrong selected count"
    );
    let selected_path = files
        .first()
        .and_then(|row| row.get("path"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing inventory path"))?;
    ensure!(
        std::str::from_utf8(payload)?.contains(&format!("// === {selected_path} ===\n")),
        "inventory disagrees with emitted path"
    );
    let tokens = files.iter().try_fold(0_u64, |sum, row| -> Result<u64> {
        let charge = row
            .get("effective_tokens")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| row.get("tokens").and_then(serde_json::Value::as_u64))
            .ok_or_else(|| anyhow::anyhow!("missing selected charge"))?;
        Ok(sum + charge)
    })?;
    ensure!(
        value.get("used_tokens").and_then(serde_json::Value::as_u64) == Some(tokens),
        "selected charge disagrees with manifest"
    );
    if command == "context" {
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("receipt.json"))?)?;
        ensure!(
            receipt.get("files") == value.get("included_files"),
            "receipt inventory disagrees with manifest"
        );
        ensure!(
            receipt.get("file_count") == value.get("file_count"),
            "receipt count disagrees"
        );
        ensure!(
            receipt.get("bundle_audit") == value.get("bundle_audit"),
            "receipt audit disagrees"
        );
    }
    let artifact_name = if command == "context" {
        "bundle"
    } else {
        "code"
    };
    let artifact = value
        .get("artifacts")
        .and_then(serde_json::Value::as_array)
        .and_then(|artifacts| {
            artifacts.iter().find(|artifact| {
                artifact.get("name").and_then(serde_json::Value::as_str) == Some(artifact_name)
            })
        })
        .ok_or_else(|| anyhow::anyhow!("missing payload artifact"))?;
    ensure!(
        artifact.get("bytes").and_then(serde_json::Value::as_u64) == Some(payload.len() as u64),
        "manifest bytes disagree"
    );
    let hash = blake3::hash(payload).to_hex().to_string();
    ensure!(
        artifact
            .get("hash")
            .and_then(|value| value.get("hash"))
            .and_then(serde_json::Value::as_str)
            == Some(hash.as_str()),
        "manifest hash disagrees"
    );
    let audit_key = if command == "context" {
        "bundle_audit"
    } else {
        "code_audit"
    };
    ensure!(
        value
            .get(audit_key)
            .and_then(|audit| audit.get("output_bytes"))
            .and_then(serde_json::Value::as_u64)
            == Some(payload.len() as u64),
        "audit bytes disagree"
    );
    Ok(())
}
