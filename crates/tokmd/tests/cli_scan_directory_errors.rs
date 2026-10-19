//! A selected directory that cannot be enumerated must not become a complete receipt.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use tempfile::TempDir;

const FORMATS: &[(&str, &str)] = &[
    ("lang", "json"),
    ("module", "json"),
    ("export", "json"),
    ("export", "jsonl"),
];

struct Fixture {
    dir: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("source");
        fs::create_dir(&root)?;
        fs::write(dir.path().join("empty-config"), "")?;
        Ok(Self { dir, root })
    }

    fn write(&self, name: &str, content: &str) -> Result<()> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tokmd"));
        command.current_dir(&self.root);
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy();
            if key.starts_with("TOKMD_") || key.starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        let config = self.dir.path().join("empty-config");
        command
            .env("TOKMD_CONFIG", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", &config)
            .env("GIT_CONFIG_GLOBAL", &config)
            .args(["--no-progress", "--config", "none"]);
        command
    }

    fn run(&self, command: &str, format: &str, extra: &[&str]) -> Result<Output> {
        Ok(self
            .command()
            .args([command, ".", "--format", format])
            .args(extra)
            .output()?)
    }
}

fn assert_complete(output: Output, command: &str, format: &str, paths: &[&str]) -> Result<()> {
    ensure!(output.status.code() == Some(0), "{output:?}");
    ensure!(output.stderr.is_empty(), "{output:?}");
    let documents: Vec<Value> = if format == "jsonl" {
        String::from_utf8(output.stdout)?
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?
    } else {
        vec![serde_json::from_slice(&output.stdout)?]
    };
    let receipt = documents
        .first()
        .ok_or_else(|| anyhow::anyhow!("successful process emitted no receipt"))?;
    ensure!(receipt["schema_version"] == json!(2), "{receipt}");
    ensure!(receipt["mode"] == json!(command), "{receipt}");
    ensure!(receipt["status"] == json!("complete"), "{receipt}");
    ensure!(receipt["warnings"] == json!([]), "{receipt}");
    ensure!(receipt["scan"]["paths"] == json!(["."]), "{receipt}");
    ensure!(receipt["scan"]["config"] == json!("none"), "{receipt}");
    if command == "export" {
        let rows: Vec<&Value> = if format == "jsonl" {
            ensure!(receipt["type"] == json!("meta"), "{receipt}");
            let rows: Vec<_> = documents.iter().skip(1).collect();
            ensure!(rows.iter().all(|row| row["type"] == json!("row")));
            rows
        } else {
            receipt["rows"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing export rows: {receipt}"))?
                .iter()
                .collect()
        };
        let mut actual: Vec<_> = rows
            .iter()
            .map(|row| {
                ensure!(row["code"] == json!(1), "{row}");
                ensure!(row["lines"] == json!(1), "{row}");
                ensure!(row["lang"] == json!("Rust"), "{row}");
                ensure!(row["kind"] == json!("parent"), "{row}");
                row["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing inventory path: {row}"))
            })
            .collect::<Result<_>>()?;
        actual.sort_unstable();
        let mut expected = paths.to_vec();
        expected.sort_unstable();
        ensure!(actual == expected, "inventory {actual:?} != {expected:?}");
    } else {
        for field in ["files", "code", "lines"] {
            ensure!(receipt["total"][field] == json!(paths.len()), "{receipt}");
        }
    }
    Ok(())
}

#[test]
fn empty_and_healthy_directories_keep_exact_complete_inventory() -> Result<()> {
    let fixture = Fixture::new()?;
    for &(command, format) in FORMATS {
        assert_complete(fixture.run(command, format, &[])?, command, format, &[])?;
    }
    let no_meta = fixture.run("export", "jsonl", &["--meta", "false"])?;
    ensure!(no_meta.status.code() == Some(0), "{no_meta:?}");
    ensure!(no_meta.stdout.is_empty() && no_meta.stderr.is_empty());

    fixture.write("good.rs", "fn good() {}\n")?;
    fixture.write("blocked/omitted.rs", "fn omitted() {}\n")?;
    for &(command, format) in FORMATS {
        assert_complete(
            fixture.run(command, format, &[])?,
            command,
            format,
            &["good.rs", "blocked/omitted.rs"],
        )?;
    }
    Ok(())
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::ErrorKind;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    struct UnreadableDirectory {
        path: PathBuf,
        original: fs::Permissions,
        restored: bool,
    }

    impl UnreadableDirectory {
        fn new(path: &Path) -> Result<Self> {
            let guard = Self {
                path: path.to_path_buf(),
                original: fs::metadata(path)?.permissions(),
                restored: false,
            };
            fs::set_permissions(path, fs::Permissions::from_mode(0))?;
            match fs::read_dir(path) {
                Ok(_) => anyhow::bail!(
                    "permission fault unavailable: current user can enumerate mode-000 directory {}; this is not proof of the failure contract",
                    path.display()
                ),
                Err(error) => ensure!(
                    error.kind() == ErrorKind::PermissionDenied,
                    "fixture needs PermissionDenied, received {error:?}"
                ),
            }
            Ok(guard)
        }

        fn restore(&mut self) -> Result<()> {
            fs::set_permissions(&self.path, self.original.clone())?;
            self.restored = true;
            Ok(())
        }
    }

    impl Drop for UnreadableDirectory {
        fn drop(&mut self) {
            if !self.restored {
                let _ = fs::set_permissions(&self.path, self.original.clone());
            }
        }
    }

    fn assert_directory_error(output: &Output, directory: &str) -> Result<()> {
        ensure!(output.status.code() == Some(1), "{output:?}");
        ensure!(
            output.stdout.is_empty(),
            "incomplete scan emitted a success receipt: {output:?}"
        );
        let stderr = std::str::from_utf8(&output.stderr)?;
        ensure!(stderr.contains("Incomplete scan"), "{stderr}");
        ensure!(
            stderr.contains(directory),
            "missing failed directory: {stderr}"
        );
        let lower = stderr.to_ascii_lowercase();
        ensure!(
            lower.contains("permission"),
            "missing local cause/repair: {stderr}"
        );
        ensure!(lower.contains("retry"), "missing recovery advice: {stderr}");
        Ok(())
    }

    fn directory_failure_case(command: &str, format: &str, mixed: bool) -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.write("blocked/omitted.rs", "fn omitted() {}\n")?;
        if mixed {
            fixture.write("good.rs", "fn good() {}\n")?;
        }
        let _guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
        assert_directory_error(&fixture.run(command, format, &[])?, "blocked")
    }

    // Separate tests retain all eight baseline failures instead of stopping a
    // command/format loop after the first false-complete receipt.
    macro_rules! directory_failure {
        ($name:ident, $command:literal, $format:literal, $mixed:literal) => {
            #[test]
            fn $name() -> Result<()> {
                directory_failure_case($command, $format, $mixed)
            }
        };
    }

    directory_failure!(failed_only_lang_json, "lang", "json", false);
    directory_failure!(failed_only_module_json, "module", "json", false);
    directory_failure!(failed_only_export_json, "export", "json", false);
    directory_failure!(failed_only_export_jsonl, "export", "jsonl", false);
    directory_failure!(mixed_lang_json, "lang", "json", true);
    directory_failure!(mixed_module_json, "module", "json", true);
    directory_failure!(mixed_export_json, "export", "json", true);
    directory_failure!(mixed_export_jsonl, "export", "jsonl", true);

    #[test]
    fn explicit_exclusion_allows_unreadable_directory_in_all_machine_modes() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.write("good.rs", "fn good() {}\n")?;
        fixture.write("blocked/omitted.rs", "fn omitted() {}\n")?;
        let _guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
        for &(command, format) in FORMATS {
            let output = fixture.run(command, format, &["--exclude", "blocked/**"])?;
            let receipt: Value = if format == "jsonl" {
                let text = std::str::from_utf8(&output.stdout)?;
                serde_json::from_str(
                    text.lines()
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("missing JSONL meta"))?,
                )?
            } else {
                serde_json::from_slice(&output.stdout)?
            };
            ensure!(receipt["scan"]["excluded"] == json!(["blocked/**"]));
            assert_complete(output, command, format, &["good.rs"])?;
        }
        Ok(())
    }

    fn ignored_directory(ignore_file: &str) -> Result<Fixture> {
        let fixture = Fixture::new()?;
        // The ignore walker applies .gitignore only inside a Git repository.
        fs::create_dir(fixture.root.join(".git"))?;
        fixture.write(ignore_file, "blocked/\n")?;
        fixture.write("good.rs", "fn good() {}\n")?;
        fixture.write("blocked/omitted.rs", "fn omitted() {}\n")?;
        Ok(fixture)
    }

    #[test]
    fn dot_ignore_files_preserve_unselected_directories_and_obey_overrides() -> Result<()> {
        for ignore_file in [".tokeignore", ".ignore"] {
            let fixture = ignored_directory(ignore_file)?;
            let _guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
            for extra in [&[][..], &["--no-ignore-vcs"][..]] {
                assert_complete(
                    fixture.run("export", "json", extra)?,
                    "export",
                    "json",
                    &["good.rs"],
                )?;
            }
            for flag in ["--no-ignore-dot", "--no-ignore"] {
                assert_directory_error(&fixture.run("export", "json", &[flag])?, "blocked")?;
            }
        }
        Ok(())
    }

    #[test]
    fn gitignore_preserves_unselected_directory_and_obeys_vcs_override() -> Result<()> {
        let fixture = ignored_directory(".gitignore")?;
        let _guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
        for extra in [&[][..], &["--no-ignore-dot"][..]] {
            assert_complete(
                fixture.run("export", "json", extra)?,
                "export",
                "json",
                &["good.rs"],
            )?;
        }
        for flag in ["--no-ignore-vcs", "--no-ignore"] {
            assert_directory_error(&fixture.run("export", "json", &[flag])?, "blocked")?;
        }
        Ok(())
    }

    #[test]
    fn hidden_directory_is_checked_only_when_selected() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.write("good.rs", "fn good() {}\n")?;
        fixture.write(".blocked/omitted.rs", "fn omitted() {}\n")?;
        let _guard = UnreadableDirectory::new(&fixture.root.join(".blocked"))?;
        for extra in [&[][..], &["--no-ignore"][..]] {
            assert_complete(
                fixture.run("export", "json", extra)?,
                "export",
                "json",
                &["good.rs"],
            )?;
        }
        assert_directory_error(&fixture.run("export", "json", &["--hidden"])?, ".blocked")
    }

    #[test]
    fn failed_export_retains_destination_and_access_repair_reuses_identical_argv() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.write("good.rs", "fn good() {}\n")?;
        fixture.write("blocked/omitted.rs", "fn omitted() {}\n")?;
        let mut guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
        for format in ["json", "jsonl"] {
            let destination = fixture.dir.path().join(format!("retained-{format}"));
            fs::write(&destination, b"retained artifact\n")?;
            let mut command = fixture.command();
            command
                .args(["export", ".", "--format", format, "--output"])
                .arg(&destination);
            let original_args: Vec<_> = command.get_args().map(|arg| arg.to_os_string()).collect();
            assert_directory_error(&command.output()?, "blocked")?;
            ensure!(fs::read(&destination)? == b"retained artifact\n");

            guard.restore()?;
            let repaired = command.output()?;
            ensure!(repaired.status.code() == Some(0), "{repaired:?}");
            ensure!(repaired.stdout.is_empty() && repaired.stderr.is_empty());
            ensure!(
                command
                    .get_args()
                    .map(|arg| arg.to_os_string())
                    .collect::<Vec<_>>()
                    == original_args,
                "repair must repeat identical CLI arguments"
            );
            assert_complete(
                Output {
                    stdout: fs::read(&destination)?,
                    ..repaired
                },
                "export",
                format,
                &["good.rs", "blocked/omitted.rs"],
            )?;
            guard = UnreadableDirectory::new(&fixture.root.join("blocked"))?;
        }
        Ok(())
    }

    #[test]
    fn keyword_directory_names_keep_permission_recovery_local() -> Result<()> {
        for name in ["timeout", "rate_limit"] {
            let fixture = Fixture::new()?;
            fixture.write("good.rs", "fn good() {}\n")?;
            fixture.write(&format!("{name}/omitted.rs"), "fn omitted() {}\n")?;
            let _guard = UnreadableDirectory::new(&fixture.root.join(name))?;
            let output = fixture.run("lang", "json", &[])?;
            assert_directory_error(&output, name)?;
            let stderr = std::str::from_utf8(&output.stderr)?;
            for remote_hint in [
                "This looks transient. Retry with backoff after network or service health recovers.",
                "Check network, VPN, or proxy settings if retries keep failing.",
                "The upstream service is limiting requests. Wait briefly, then retry.",
                "Honor provider retry windows such as `Retry-After` when available.",
            ] {
                ensure!(!stderr.contains(remote_hint), "{stderr}");
            }
        }
        Ok(())
    }
}
