//! Flat bundle replacement. The manifest is withdrawn first and installed last.
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use tokmd_types::ArtifactEntry;

const MANIFEST: &str = "manifest.json";
const STAGE: &str = ".tokmd-bundle-stage";
// Only these writer-owned names may be moved or removed.
const MEMBERS: &[&str] = &[
    MANIFEST,
    "receipt.json",
    "bundle.txt",
    "map.jsonl",
    "intelligence.json",
    "code.txt",
    "review-links.json",
    "proof-links.json",
    "evidence-packet-links.json",
    "work-order.md",
];

pub(crate) struct Generation {
    dest: PathBuf,
    stage: PathBuf,
    keep: bool,
}

impl Generation {
    pub(crate) fn begin(dest: &Path, force: bool) -> Result<Self> {
        fs::create_dir_all(dest)?;
        let stage = dest.join(STAGE);
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            let mut private = builder;
            std::os::unix::fs::DirBuilderExt::mode(&mut private, 0o700);
            private
        };
        builder.create(&stage).with_context(|| {
            format!(
                "Cannot reserve bundle staging directory {}",
                stage.display()
            )
        })?;
        let generation = Self {
            dest: dest.into(),
            stage,
            keep: false,
        };
        if !force {
            for entry in fs::read_dir(dest)? {
                ensure!(
                    entry?.file_name() == STAGE,
                    "Bundle directory is not empty: {}. Use --force to overwrite.",
                    dest.display()
                );
            }
        }
        Ok(generation)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.stage
    }

    pub(crate) fn publish(self, manifest_json: &str, entries: &[ArtifactEntry]) -> Result<()> {
        write_file(&self.stage.join(MANIFEST), manifest_json)?;
        self.commit(manifest_json, entries)
    }

    fn commit(mut self, manifest_json: &str, entries: &[ArtifactEntry]) -> Result<()> {
        ensure!(
            fs::read(self.stage.join(MANIFEST))? == manifest_json.as_bytes(),
            "Staged manifest changed"
        );
        let mut names = Vec::new();
        for entry in entries {
            let name = entry.path.as_str();
            ensure!(
                MEMBERS.contains(&name) && !names.contains(&name),
                "Unexpected bundle member {name}"
            );
            names.push(name);
            let path = self.stage.join(name);
            if name != MANIFEST {
                let expected = entry.hash.as_ref().context("Missing artifact hash")?;
                let mut hasher = blake3::Hasher::new();
                let len = io::copy(&mut File::open(&path)?, &mut hasher)?;
                ensure!(
                    len == entry.bytes
                        && expected.algo == "blake3"
                        && expected.hash == hasher.finalize().to_hex().as_str(),
                    "Staged artifact mismatch: {name}"
                );
                boundary("verify hash", &path)?;
            }
        }
        ensure!(names.contains(&MANIFEST), "Missing manifest");
        names.retain(|name| *name != MANIFEST);
        names.push(MANIFEST);
        let mut saved = Vec::new();
        let mut installed = Vec::new();
        let backup = |name: &str| self.stage.join(format!("previous-{name}"));
        let switch = (|| -> Result<()> {
            for &name in names.iter().rev() {
                let path = self.dest.join(name);
                match fs::symlink_metadata(&path) {
                    Ok(meta) => {
                        ensure!(
                            meta.is_file(),
                            "Refusing to replace non-file {}",
                            path.display()
                        );
                        ensure!(
                            !meta.permissions().readonly(),
                            "Read-only bundle artifact {}",
                            path.display()
                        );
                        File::options().write(true).open(&path)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let mode = meta.permissions().mode() & 0o777;
                            fs::set_permissions(
                                self.stage.join(name),
                                fs::Permissions::from_mode(mode),
                            )?;
                        }
                        fs::rename(path, backup(name))?;
                        saved.push(name);
                        boundary("withdraw", &self.dest.join(name))?;
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => (),
                    Err(err) => return Err(err.into()),
                }
            }
            for &name in &names {
                fs::rename(self.stage.join(name), self.dest.join(name))?;
                installed.push(name);
                boundary("install", &self.dest.join(name))?;
            }
            Ok(())
        })();
        if let Err(error) = switch {
            // Stop at the first rollback error, keeping backups and the writer reservation.
            self.keep = true;
            let rollback = (|| -> io::Result<()> {
                for name in installed.into_iter().rev() {
                    fs::remove_file(self.dest.join(name))?;
                }
                for name in saved.into_iter().rev() {
                    fs::rename(backup(name), self.dest.join(name))?;
                }
                Ok(())
            })();
            if let Err(rollback) = rollback {
                bail!(
                    "{error:#}; rollback failed: {rollback}; recover files from {}",
                    self.stage.display()
                );
            }
            self.keep = false;
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        for name in MEMBERS {
            let _ = fs::remove_file(self.stage.join(name));
            let _ = fs::remove_file(self.stage.join(format!("previous-{name}")));
        }
        if let Err(error) = fs::remove_dir(&self.stage) {
            eprintln!(
                "Bundle staging cleanup incomplete at {}: {error}",
                self.stage.display()
            );
        }
    }
}

/// Keep the fault seam at the actual writer, including streamed output.
pub(crate) struct StagedWriter<'a> {
    file: File,
    path: &'a Path,
}

impl<'a> StagedWriter<'a> {
    pub(crate) fn create(path: &'a Path) -> io::Result<Self> {
        Ok(Self {
            file: File::create_new(path)?,
            path,
        })
    }
}

impl Write for StagedWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let count = self.file.write(buf)?;
        boundary("write", self.path)?;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        boundary("flush", self.path)
    }
}

pub(crate) fn write_file(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let mut writer = StagedWriter::create(path)?;
    writer.write_all(bytes.as_ref())?;
    writer.flush()
}

#[cfg(not(test))]
#[inline]
pub(crate) fn boundary(_: &str, _: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAULT: std::cell::RefCell<Option<(usize, usize)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn boundary(op: &str, path: &Path) -> io::Result<()> {
    if matches!(op, "withdraw" | "install") && FAULT.with_borrow(Option::is_some) {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("missing destination"))?;
        if parent.join(MANIFEST).exists() {
            let result = tests::verify_visible(parent);
            assert!(
                result.is_ok(),
                "mixed visible generation after {op}: {result:?}"
            );
        }
    }
    FAULT.with_borrow_mut(|fault| {
        if let Some((fail_at, count)) = fault {
            *count += 1;
            if *count == *fail_at {
                return Err(io::Error::other(format!(
                    "injected {op} after {}",
                    path.display()
                )));
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::collections::BTreeMap;

    fn snapshot(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut files = BTreeMap::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            files.insert(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("invalid test name"))?,
                fs::read(entry.path())?,
            );
        }
        Ok(files)
    }

    pub(super) fn verify_visible(dir: &Path) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Manifest {
            artifacts: Vec<ArtifactEntry>,
        }
        let manifest: Manifest = serde_json::from_slice(&fs::read(dir.join(MANIFEST))?)?;
        for entry in manifest.artifacts {
            if entry.path == MANIFEST {
                continue;
            }
            let bytes = fs::read(dir.join(entry.path))?;
            let hash = entry.hash.context("missing hash")?;
            ensure!(
                entry.bytes == bytes.len() as u64
                    && hash.algo == "blake3"
                    && hash.hash == blake3::hash(&bytes).to_hex().as_str(),
                "digest mismatch"
            );
        }
        Ok(())
    }

    #[test]
    fn failed_replacement_preserves_every_byte() -> Result<()> {
        for command in ["context", "handoff"] {
            let tmp = tempfile::tempdir()?;
            let input = tmp.path().join("input");
            let out = tmp.path().join("output");
            fs::create_dir(&input)?;
            fs::create_dir(&out)?;
            fs::write(out.join("notes"), "user data")?;
            let source = input.join("demo.rs");
            let run = || -> Result<()> {
                let mut argv = vec![
                    "tokmd",
                    "--no-progress",
                    command,
                    input.to_str().context("fixture path")?,
                    "--no-git",
                    "--force",
                    "--output-dir",
                    out.to_str().context("fixture path")?,
                ];
                if command == "handoff" {
                    let link = input.to_str().context("fixture path")?;
                    argv.extend([
                        "--review-packet-dir",
                        link,
                        "--proof-plan",
                        link,
                        "--evidence-packet",
                        link,
                    ]);
                }
                let cli = crate::cli::Cli::try_parse_from(argv)?;
                crate::commands::dispatch(cli, &crate::ResolvedConfig::default())
            };
            fs::write(&source, "fn old() {}")?;
            run()?;
            let member = out.join(if command == "context" {
                "receipt.json"
            } else {
                "map.jsonl"
            });
            let member_bytes = fs::read(&member)?;
            fs::write(&member, vec![b'x'; member_bytes.len()])?;
            ensure!(verify_visible(&out).is_err());
            fs::remove_file(&member)?;
            ensure!(verify_visible(&out).is_err());
            fs::write(&member, member_bytes)?;
            ensure!(Generation::begin(&out, false).is_err());
            fs::create_dir(out.join(STAGE))?;
            fs::write(out.join(STAGE).join("notes"), "keep")?;
            ensure!(run().is_err());
            ensure!(fs::read(out.join(STAGE).join("notes"))? == b"keep");
            fs::remove_file(out.join(STAGE).join("notes"))?;
            fs::remove_dir(out.join(STAGE))?;
            let permissions = member.metadata()?.permissions();
            let mut readonly = permissions.clone();
            readonly.set_readonly(true);
            fs::set_permissions(&member, readonly)?;
            ensure!(run().is_err());
            verify_visible(&out)?;
            fs::set_permissions(&member, permissions)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&member, fs::Permissions::from_mode(0o600))?;
            }
            let previous = snapshot(&out)?;
            fs::write(&source, "fn replacement() {}")?;
            for fail_at in 1.. {
                FAULT.set(Some((fail_at, 0)));
                let result = run();
                let (_, count) = FAULT.take().context("missing test counter")?;
                if count < fail_at {
                    result?;
                    break;
                }
                ensure!(result.is_err(), "fault {fail_at} ignored");
                ensure!(
                    snapshot(&out)? == previous,
                    "{command} fault {fail_at} damaged previous generation"
                );
            }
            let payload = if command == "context" {
                "bundle.txt"
            } else {
                "code.txt"
            };
            ensure!(fs::read_to_string(out.join(payload))?.contains("replacement"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                ensure!(member.metadata()?.permissions().mode() & 0o777 == 0o600);
            }
            let old_manifest = previous.get(MANIFEST).context("old manifest")?;
            fs::write(out.join(MANIFEST), old_manifest)?;
            ensure!(verify_visible(&out).is_err(), "accepted stale manifest");
            ensure!(fs::read(out.join("notes"))? == b"user data");
        }
        Ok(())
    }
}
