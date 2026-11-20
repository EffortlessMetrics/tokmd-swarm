//! Required-gate locked-command contract (#608).
//!
//! `AGENTS.md` and `agents/shared/repo.md` claim that the required
//! `Tokmd Rust Result` gate resolves Cargo commands against the committed
//! `Cargo.lock`. This test binds that claim to the live workflow and to the
//! gate task's own steps, so the guidance cannot drift ahead of the contract
//! again.
//!
//! Scope: this proves the required commands *carry* `--locked`. That the flag
//! preserves a committed lock and fails visibly when the lock is missing or
//! stale is proven separately by the executed Cargo fixture in
//! `cargo_command_surfaces_w104.rs`.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};

/// Cargo subcommands that resolve the dependency graph, mirroring the governed
/// set in `policy/cargo-command-surfaces.toml`.
///
/// `xtask` is this repository's own alias, which `.cargo/config.toml` expands to
/// `run -p xtask --`. The launcher therefore resolves dependencies exactly like
/// the governed `run` it becomes, and an unlocked launcher could rewrite
/// `Cargo.lock` before the gate's own locked steps ever execute. Cargo applies
/// global options before expanding an alias, so the flag belongs ahead of the
/// alias: `cargo --locked xtask …`.
const GOVERNED: &[&str] = &[
    "build",
    "check",
    "test",
    "clippy",
    "run",
    "install",
    "update",
    "generate-lockfile",
    "xtask",
];

/// Resolve the repository root from this crate's manifest directory.
fn workspace_root() -> Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask manifest has no workspace parent")?
        .to_path_buf())
}

/// Read a repository-relative file as UTF-8.
fn read(relative: &str) -> Result<String> {
    let path = workspace_root()?.join(relative);
    fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

/// Extract the `tokmd-rust-result` job body, ending at the next top-level job
/// key so adjacent non-required lanes are never mistaken for gate commands.
fn required_gate_job(workflow: &str) -> Result<&str> {
    let start = workflow
        .find("\n  tokmd-rust-result:\n")
        .context("required gate job `tokmd-rust-result` missing")?;
    let body = workflow.get(start + 1..).context("job body truncated")?;
    let end = body
        .match_indices("\n  ")
        .find(|(offset, _)| {
            body.get(offset + 3..)
                .and_then(|rest| rest.split('\n').next())
                .is_some_and(|line| {
                    !line.starts_with(char::is_whitespace)
                        && !line.starts_with('#')
                        && line.ends_with(':')
                        && line != "tokmd-rust-result:"
                })
        })
        .map_or(body.len(), |(offset, _)| offset);
    body.get(..end).context("job body slice out of range")
}

/// Drop the shell and Markdown quoting that wraps commands in YAML and prose.
fn strip_quotes(token: &str) -> &str {
    token.trim_matches(['`', '\'', '"'])
}

/// Options accepted before the subcommand that consume the following token.
///
/// This list can never be complete, so it is a fast path rather than the
/// safety net: `unlocked_invocations` keeps scanning past an unrecognised
/// value instead of mistaking it for the subcommand.
fn takes_value(token: &str) -> bool {
    matches!(
        token,
        "--manifest-path"
            | "--config"
            | "--color"
            | "-C"
            | "-Z"
            | "--target"
            | "--target-dir"
            | "--package"
            | "-p"
            | "--exclude"
            | "--jobs"
            | "-j"
            | "--profile"
            | "--features"
            | "-F"
            | "--bin"
            | "--example"
            | "--test"
            | "--bench"
    )
}

/// Report every governed Cargo invocation in `text` that does not pin the lock.
///
/// Each `cargo` token starts a segment that ends at the next `cargo` token, so
/// a wrapper line that quotes a label and then runs the command is judged as
/// the two invocations it actually contains.
fn unlocked_invocations(text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let starts: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| strip_quotes(token) == "cargo")
            .map(|(index, _)| index)
            .collect();
        for (position, &start) in starts.iter().enumerate() {
            let end = starts.get(position + 1).copied().unwrap_or(tokens.len());
            let Some(segment) = tokens.get(start..end) else {
                continue;
            };
            let mut index = 1;
            let mut subcommand = None;
            let mut after_option = false;
            while let Some(&token) = segment.get(index) {
                let token = strip_quotes(token);
                if token == "--" {
                    // Everything past the separator belongs to the built binary.
                    break;
                }
                if takes_value(token) {
                    index += 2;
                    after_option = false;
                    continue;
                }
                if token.starts_with('+') || token.starts_with('-') {
                    index += 1;
                    after_option = true;
                    continue;
                }
                // A bare word right after an option may be that option's value
                // rather than the subcommand. Concluding "ungoverned" here would
                // silently exempt the real command that follows, so keep
                // scanning; `takes_value` only shortcuts the pairs whose value
                // could itself collide with a governed name.
                if after_option && !GOVERNED.contains(&token) {
                    index += 1;
                    after_option = false;
                    continue;
                }
                subcommand = Some(token);
                break;
            }
            let Some(subcommand) = subcommand else {
                continue;
            };
            if !GOVERNED.contains(&subcommand) {
                continue;
            }
            // `xtask` is an alias ending in `--`, so every token after the alias
            // name is forwarded to the built binary. Only a global option ahead
            // of the alias reaches Cargo itself. Built-in subcommands accept the
            // flag on either side, up to their own `--` separator.
            let cargo_arguments = if subcommand == "xtask" {
                segment.get(..index).unwrap_or_default()
            } else {
                segment
                    .iter()
                    .position(|token| strip_quotes(token) == "--")
                    .map_or(segment, |separator| {
                        segment.get(..separator).unwrap_or_default()
                    })
            };
            if !cargo_arguments
                .iter()
                .any(|token| matches!(strip_quotes(token), "--locked" | "--frozen"))
            {
                findings.push(line.trim().to_string());
            }
        }
    }
    findings
}

#[test]
fn required_gate_runs_every_governed_cargo_command_locked() -> Result<()> {
    let workflow = read(".github/workflows/ci.yml")?;
    let job = required_gate_job(&workflow)?;

    ensure!(
        job.contains("name: Tokmd Rust Result"),
        "extracted block is not the required gate job"
    );
    // The extraction must stop before the platform lanes, which are documented
    // as outside the locked-gate claim.
    ensure!(
        !job.contains("Build & Test (Windows)"),
        "required-gate extraction leaked into a non-required lane"
    );

    for command in [
        "cargo test --locked --all-features",
        "cargo test --locked -p xtask --all-features",
        "cargo --locked xtask gate --check",
        "cargo --locked xtask proof-policy --check",
    ] {
        ensure!(
            job.contains(command),
            "required gate no longer runs `{command}`"
        );
    }
    // The alias launcher must not appear unlocked anywhere in the job, receipt
    // and summary strings included, or the receipts would name a command the
    // gate did not run.
    ensure!(
        !job.contains("cargo xtask "),
        "required gate still has an unlocked `cargo xtask` launcher"
    );

    let findings = unlocked_invocations(job);
    ensure!(
        findings.is_empty(),
        "unlocked Cargo commands in the required gate: {findings:?}"
    );
    Ok(())
}

/// Collect the literal `args` array of the gate step carrying `label`.
fn gate_step_args(source: &str, label: &str) -> Result<Vec<String>> {
    let marker = format!("label: \"{label}\",");
    let tail = source
        .split_once(&marker)
        .with_context(|| format!("gate step `{label}` missing"))?
        .1;
    let body = tail
        .split_once("args: &[")
        .with_context(|| format!("gate step `{label}` has no args array"))?
        .1
        .split_once(']')
        .with_context(|| format!("gate step `{label}` args array is unterminated"))?
        .0;
    Ok(body
        .split('"')
        .skip(1)
        .step_by(2)
        .map(ToOwned::to_owned)
        .collect())
}

#[test]
fn gate_task_locks_dependency_resolving_steps() -> Result<()> {
    let source = read("xtask/src/tasks/gate.rs")?;
    for (label, subcommand) in [
        ("check (warm graph)", "check"),
        ("clippy", "clippy"),
        ("test (compile-only)", "test"),
    ] {
        let args = gate_step_args(&source, label)?;
        ensure!(
            args.first().map(String::as_str) == Some(subcommand),
            "gate step `{label}` does not start with `{subcommand}`, found {args:?}"
        );
        // Arguments after `--` are forwarded to the tool, not to Cargo.
        let cargo_arguments = args
            .iter()
            .position(|argument| argument == "--")
            .map_or(args.as_slice(), |separator| {
                args.get(..separator).unwrap_or_default()
            });
        ensure!(
            cargo_arguments
                .iter()
                .any(|argument| argument == "--locked"),
            "gate step `{label}` does not pass --locked to Cargo, found {args:?}"
        );
    }

    // `cargo fmt` resolves no dependencies and rejects the flag; it is the
    // documented positive control for the contract.
    let fmt = gate_step_args(&source, "fmt")?;
    ensure!(
        fmt == ["fmt", "--all"],
        "the fmt step should stay unlocked as the contract's positive control, found {fmt:?}"
    );
    Ok(())
}

#[test]
fn guidance_states_the_locked_claim_and_its_boundary() -> Result<()> {
    for path in ["AGENTS.md", "agents/shared/repo.md"] {
        let text = read(path)?;
        ensure!(
            text.contains("The required workflow executes that sequence with `--locked`"),
            "{path} does not bind the claim to the required workflow"
        );
        ensure!(
            text.contains("non-required platform, coverage, mutation,"),
            "{path} does not document the claim boundary"
        );
    }
    Ok(())
}

#[test]
fn scanner_has_positive_and_negative_controls() -> Result<()> {
    // Positive controls: locked, non-governed, and prose forms are accepted.
    for line in [
        "cargo test --locked --all-features",
        "cargo test --frozen -p xtask --all-features",
        "cargo --locked xtask gate --check",
        "cargo --locked xtask proof-policy --check",
        "cargo fmt --all -- --check",
        "cargo +stable --manifest-path Cargo.toml check --locked",
        "cargo -q --color never build --locked",
        "cargo --locked --target x86_64-unknown-linux-gnu build",
        "cargo --locked --jobs 4 build",
        "run_bounded \"cargo test --locked\" marker log cargo test --locked",
        "no cargo invocation here",
    ] {
        ensure!(
            unlocked_invocations(line).is_empty(),
            "false positive for {line:?}"
        );
    }

    // Negative controls: an unlocked governed command is always reported.
    for line in [
        "cargo test --all-features",
        "cargo test -p xtask --all-features",
        "cargo build --release",
        "cargo run -- --locked",
        "cargo -q check",
        "cargo +stable --manifest-path Cargo.toml clippy",
        // The alias expands to `run -p xtask --`, so an unlocked launcher
        // resolves dependencies before any inner locked step runs.
        "cargo xtask gate --check",
        "cargo xtask proof-policy --check",
        // `--locked` after the alias reaches the xtask binary, not Cargo.
        "cargo xtask gate --check --locked",
        // A value-taking option must not let its value pose as the subcommand
        // and exempt the governed command behind it.
        "cargo --target x86_64-unknown-linux-gnu build",
        "cargo --jobs 4 build",
    ] {
        ensure!(
            unlocked_invocations(line).len() == 1,
            "missed unlocked command in {line:?}"
        );
    }

    // A wrapper that labels one command and runs another is judged per
    // invocation, so a half-migrated line cannot pass.
    ensure!(
        unlocked_invocations(
            "run_bounded \"cargo test --all-features\" marker log cargo test --locked --all-features"
        )
        .len()
            == 1,
        "a locked executable command must not hide its unlocked receipt label"
    );
    Ok(())
}

#[test]
fn job_extraction_stops_at_the_next_job_key() -> Result<()> {
    let workflow = "jobs:\n  tokmd-rust-result:\n    name: Tokmd Rust Result\n    steps:\n      - run: cargo test --locked\n  other:\n    name: Other\n    steps:\n      - run: cargo test\n";
    let job = required_gate_job(workflow)?;
    ensure!(job.contains("cargo test --locked"));
    ensure!(!job.contains("name: Other"));
    ensure!(unlocked_invocations(job).is_empty());
    ensure!(required_gate_job("jobs:\n  other:\n").is_err());
    Ok(())
}

// Resource safety at the same required-gate entrypoint.
#[cfg(unix)]
mod target_lifecycle {
    use anyhow::{Result, ensure};
    use std::fs::{self, File, FileTimes};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, SystemTime};

    fn executable(path: &Path, script: &str) -> Result<()> {
        fs::write(path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    #[test]
    fn gate_preserves_old_targets_and_refuses_actual_disk_pressure() -> Result<()> {
        // Refuse to exercise the historical destructive implementation even on a
        // disposable fixture. The previous sweep must be absent before we run gate.
        ensure!(!include_str!("../src/tasks/gate.rs").contains("cleanup_stale_scoped_dirs"));
        ensure!(!include_str!("../src/tasks/build_guard.rs").contains("cleanup_stale_scoped_dirs"));

        let floor_kib = 8 * 1024 * 1024;
        for (available_kib, admitted) in [(floor_kib - 1, false), (floor_kib, true)] {
            let fixture = tempfile::tempdir()?;
            let root = fixture.path();
            let bin = root.join("bin");
            let tmp = root.join("tmp");
            fs::create_dir_all(&bin)?;
            fs::create_dir_all(root.join("ci"))?;
            fs::write(
                root.join("ci/proof.toml"),
                include_str!("../../ci/proof.toml"),
            )?;

            let old = tmp.join(format!("tokmd-gate-target-{}-1", std::process::id()));
            fs::create_dir_all(old.join("debug/deps"))?;
            let old_time = SystemTime::now() - Duration::from_secs(4 * 60 * 60);
            File::open(&old)?.set_times(FileTimes::new().set_modified(old_time))?;
            let old_mtime = fs::metadata(&old)?.modified()?;
            let output = old.join("debug/deps/recent-output");
            fs::write(&output, "warm build evidence")?;
            ensure!(fs::metadata(&old)?.modified()? == old_mtime);
            ensure!(fs::metadata(&output)?.modified()? > old_mtime);
            let unrelated = tmp.join("unrelated-evidence");
            fs::write(&unrelated, "keep")?;

            executable(&bin.join("git"), "#!/bin/sh\nexit 0\n")?;
            executable(
                &bin.join("cargo"),
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$CALLS\"\nexit 0\n",
            )?;
            executable(
                &bin.join("df"),
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$DF_CALLS\"\nprintf 'Filesystem 1024-blocks Used Available Capacity Mounted on\\nfixture 99999999 0 %s 0%% /\\n' \"$AVAILABLE_KIB\"\n",
            )?;
            let calls = root.join("cargo-calls");
            let df_calls = root.join("df-calls");
            let run = Command::new(env!("CARGO_BIN_EXE_xtask"))
                .args(["gate", "--check"])
                .current_dir(root)
                .env("PATH", &bin)
                .env("TMPDIR", &tmp)
                .env("CALLS", &calls)
                .env("DF_CALLS", &df_calls)
                .env("AVAILABLE_KIB", available_kib.to_string())
                .env_remove("CARGO_TARGET_DIR")
                .env_remove("TOKMD_MIN_FREE_GB")
                .output()?;
            ensure!(
                run.status.success() == admitted,
                "unexpected gate result: stdout={} stderr={}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            );
            let commands = fs::read_to_string(calls)?;
            ensure!(commands.starts_with("fmt --all -- --check\n"));
            ensure!(commands.lines().count() == if admitted { 4 } else { 1 });
            ensure!(fs::read_to_string(df_calls)?.lines().count() == if admitted { 3 } else { 1 });
            if !admitted {
                ensure!(String::from_utf8_lossy(&run.stderr).contains("need at least 8.0 GiB"));
            }
            ensure!(fs::read_to_string(&output)? == "warm build evidence");
            ensure!(fs::metadata(&old)?.modified()? == old_mtime);
            ensure!(fs::read_to_string(&unrelated)? == "keep");
            // Only the current invocation's disposable directory was removed.
            ensure!(fs::read_dir(&tmp)?.count() == 2);
        }
        Ok(())
    }
}
