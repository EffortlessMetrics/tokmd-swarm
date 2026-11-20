//! Literal native-Git controls and complete CLI decisions for Git negation.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use anyhow::{Result, ensure};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self {
            dir: tempfile::tempdir()?,
        };
        fixture.write("empty-config", "")?;
        fixture.git(&["init", "-q"])?;
        fixture.write(".gitignore", "*.rs\n!keep.rs\n")?;
        for name in ["keep.rs", "drop.rs", "tracked.rs", "plain.txt"] {
            fixture.write(name, "fixture\n")?;
        }
        fixture.git(&["add", "-f", "--", "tracked.rs"])?;
        Ok(fixture)
    }

    fn write(&self, name: &str, contents: &str) -> Result<()> {
        std::fs::write(self.dir.path().join(name), contents)?;
        Ok(())
    }

    fn command(&self, executable: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(executable);
        command.current_dir(self.dir.path());
        // Remove all inherited Git/config overrides, including indexed -c values.
        for (name, _) in std::env::vars_os() {
            let key = name.to_string_lossy();
            if key.starts_with("GIT_") || key.starts_with("TOKMD_") {
                command.env_remove(name);
            }
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_SYSTEM", self.dir.path().join("empty-config"))
            .env("GIT_CONFIG_GLOBAL", self.dir.path().join("empty-config"))
            .env("TOKMD_CONFIG", self.dir.path().join("empty-config"));
        command
    }

    fn git(&self, args: &[&str]) -> Result<()> {
        let output = self.command("git").args(args).output()?;
        ensure!(output.status.success(), "git {args:?}: {output:?}");
        Ok(())
    }

    fn native(&self, args: &[&str], code: i32, stdout: &[u8]) -> Result<()> {
        let output = self.command("git").args(args).output()?;
        check_output(&output, code, stdout)
    }

    fn native_stdin(&self, input: &[u8], stdout: &[u8]) -> Result<()> {
        let mut child = self
            .command("git")
            .args(["check-ignore", "-v", "-z", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing Git stdin"))?;
        stdin.write_all(input)?;
        drop(stdin);
        check_output(&child.wait_with_output()?, 0, stdout)
    }

    fn tokmd(&self, args: &[&str], code: i32, stdout: &str) -> Result<()> {
        let output = self
            .command(env!("CARGO_BIN_EXE_tokmd"))
            .args(args)
            .output()?;
        check_output(&output, code, stdout.as_bytes())
    }
}

fn check_output(output: &Output, code: i32, stdout: &[u8]) -> Result<()> {
    ensure!(
        output.status.code() == Some(code),
        "expected exit {code}, got {output:?}"
    );
    ensure!(
        output.stdout == stdout,
        "expected stdout {stdout:?}, got {output:?}"
    );
    ensure!(output.stderr.is_empty(), "unexpected stderr: {output:?}");
    Ok(())
}

#[test]
fn git_negation_is_not_ignored_in_ordinary_mode() -> Result<()> {
    let fixture = Fixture::new()?;
    // This oracle uses Git's nonverbose decision, independent of verbose records.
    fixture.native(&["check-ignore", "--", "keep.rs"], 1, b"")?;
    fixture.native(
        &["check-ignore", "-v", "--", "keep.rs"],
        0,
        b".gitignore:2:!keep.rs\tkeep.rs\n",
    )?;
    fixture.tokmd(&["check-ignore", "keep.rs"], 1, "keep.rs: not ignored\n")
}

#[test]
fn git_negation_is_not_ignored_in_verbose_mode() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.native(&["check-ignore", "--", "keep.rs"], 1, b"")?;
    fixture.native(
        &["check-ignore", "-v", "--", "keep.rs"],
        0,
        b".gitignore:2:!keep.rs\tkeep.rs\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "-v", "keep.rs"],
        1,
        "keep.rs: not ignored\n",
    )
}

#[test]
fn same_argv_observes_rule_removal_and_restoration() -> Result<()> {
    let fixture = Fixture::new()?;
    for args in [
        vec!["check-ignore", "keep.rs"],
        vec!["check-ignore", "-v", "keep.rs"],
    ] {
        fixture.write(".gitignore", "*.rs\n")?;
        fixture.native(&["check-ignore", "--", "keep.rs"], 0, b"keep.rs\n")?;
        let ignored = if args.contains(&"-v") {
            "keep.rs: ignored\n  gitignore: .gitignore:1 -> *.rs\n"
        } else {
            "keep.rs: ignored\n"
        };
        fixture.tokmd(&args, 0, ignored)?;
        fixture.write(".gitignore", "*.rs\n!keep.rs\n")?;
        fixture.native(&["check-ignore", "--", "keep.rs"], 1, b"")?;
        fixture.tokmd(&args, 1, "keep.rs: not ignored\n")?;
        fixture.write(".gitignore", "*.rs\n")?;
        fixture.tokmd(&args, 0, ignored)?;
    }
    Ok(())
}

#[test]
fn positive_tracked_no_match_and_multi_path_controls() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.native(&["check-ignore", "--", "drop.rs"], 0, b"drop.rs\n")?;
    fixture.native(&["check-ignore", "--", "tracked.rs"], 1, b"")?;
    fixture.native(&["check-ignore", "--", "plain.txt"], 1, b"")?;
    fixture.tokmd(&["check-ignore", "drop.rs"], 0, "drop.rs: ignored\n")?;
    fixture.tokmd(
        &["check-ignore", "-v", "drop.rs"],
        0,
        "drop.rs: ignored\n  gitignore: .gitignore:1 -> *.rs\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "tracked.rs"],
        1,
        "tracked.rs: not ignored\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "-v", "tracked.rs"],
        1,
        "tracked.rs: not ignored\n  note: tracked by git; gitignore rules don't apply\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "plain.txt"],
        1,
        "plain.txt: not ignored\n",
    )?;
    fixture.tokmd(
        &[
            "check-ignore",
            "drop.rs",
            "keep.rs",
            "tracked.rs",
            "plain.txt",
        ],
        1,
        "drop.rs: ignored\nkeep.rs: not ignored\ntracked.rs: not ignored\nplain.txt: not ignored\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "drop.rs", "drop.rs"],
        0,
        "drop.rs: ignored\ndrop.rs: ignored\n",
    )
}

#[test]
fn independent_exclude_and_tokeignore_still_ignore_git_unignored_file() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.native(&["check-ignore", "--", "keep.rs"], 1, b"")?;
    fixture.tokmd(
        &["--exclude", "keep.rs", "check-ignore", "-v", "keep.rs"],
        0,
        "keep.rs: ignored\n  --exclude: keep.rs\n",
    )?;
    fixture.write(".tokeignore", "keep.rs\n")?;
    fixture.tokmd(
        &["check-ignore", "-v", "keep.rs"],
        0,
        "keep.rs: ignored\n  .tokeignore: keep.rs\n",
    )
}

#[test]
fn escaped_exclamation_and_option_like_paths_are_literal() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.write("!literal.rs", "fixture\n")?;
    fixture.write("-literal.rs", "fixture\n")?;
    fixture.write(".gitignore", "\\!literal.rs\n-literal.rs\n")?;
    fixture.native(&["check-ignore", "--", "!literal.rs"], 0, b"!literal.rs\n")?;
    fixture.tokmd(
        &["check-ignore", "-v", "--", "!literal.rs"],
        0,
        "!literal.rs: ignored\n  gitignore: .gitignore:1 -> \\!literal.rs\n",
    )?;
    fixture.tokmd(
        &["check-ignore", "-v", "--", "-literal.rs"],
        0,
        "-literal.rs: ignored\n  gitignore: .gitignore:2 -> -literal.rs\n",
    )
}

#[cfg(unix)]
#[test]
fn delimiter_bearing_source_pattern_and_path_preserve_git_evidence() -> Result<()> {
    let fixture = Fixture::new()?;
    let parent = "colon:tab\tdir";
    std::fs::create_dir(fixture.dir.path().join(parent))?;
    fixture.write(
        &format!("{parent}/.gitignore"),
        "keep:tab\t.rs\n!keep:tab\t.rs\ndrop:tab\t.rs\n",
    )?;
    for name in ["keep:tab\t.rs", "drop:tab\t.rs"] {
        fixture.write(&format!("{parent}/{name}"), "fixture\n")?;
    }
    let keep = format!("{parent}/keep:tab\t.rs");
    let drop = format!("{parent}/drop:tab\t.rs");
    fixture.native(&["check-ignore", "--", &keep], 1, b"")?;
    fixture.native_stdin(
        format!("{drop}\x00").as_bytes(),
        format!("{parent}/.gitignore\x003\x00drop:tab\t.rs\x00{drop}\x00").as_bytes(),
    )?;
    fixture.tokmd(
        &["check-ignore", "-v", &keep],
        1,
        &format!("{keep}: not ignored\n"),
    )?;
    fixture.tokmd(
        &["check-ignore", "-v", &drop],
        0,
        &format!("{drop}: ignored\n  gitignore: {parent}/.gitignore:3 -> drop:tab\t.rs\n"),
    )
}

#[test]
fn missing_path_remains_an_error() -> Result<()> {
    let fixture = Fixture::new()?;
    let output = fixture
        .command(env!("CARGO_BIN_EXE_tokmd"))
        .args(["check-ignore", "missing.rs"])
        .output()?;
    ensure!(output.status.code() == Some(1), "missing path: {output:?}");
    ensure!(output.stdout.is_empty(), "missing path stdout: {output:?}");
    let stderr = std::str::from_utf8(&output.stderr)?;
    ensure!(
        stderr.contains("path does not exist"),
        "missing path context: {stderr}"
    );
    Ok(())
}

#[test]
fn no_ignore_flags_preserve_existing_explanation_contract() -> Result<()> {
    let fixture = Fixture::new()?;
    for flag in ["--no-ignore", "--no-ignore-vcs", "--no-ignore-dot"] {
        fixture.tokmd(&[flag, "check-ignore", "drop.rs"], 0, "drop.rs: ignored\n")?;
    }
    Ok(())
}
