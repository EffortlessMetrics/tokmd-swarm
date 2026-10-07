//! Executable routing controls against the checked-in proof policy.
//! Synthetic observations exercise collector validation, not coverage execution
//! or the contents of an LCOV artifact. No planned command is executed here.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const CLI_RECOVERY_PATH: &str = "crates/tokmd/tests/file_path_recovery_w904.rs";
const ROUTING_TEST_PATH: &str = "xtask/tests/cli_recovery_routing_w905.rs";
const UNKNOWN_RUST_PATH: &str = "unowned_routing_w905.rs";
const UNKNOWN_NON_RUST_PATH: &str = "unowned_routing_w905.txt";
const CLI_COVERAGE_COMMAND: &str = "cargo llvm-cov -p tokmd --all-features --lcov --output-path target/proof/coverage/tokmd_cli.lcov";

fn checked_in_policy() -> Result<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask manifest directory must have a workspace parent")?;
    let policy = root.join("ci/proof.toml");
    ensure!(policy.is_absolute(), "checked-in policy must be absolute");
    ensure!(policy.is_file(), "checked-in policy is missing");
    Ok(policy)
}

fn scrub_git_repository_env(command: &mut Command) {
    // Git's repository-local environment can override current_dir. Remove it
    // only from this child; the caller's environment remains untouched.
    for name in [
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_OBJECT_DIRECTORY",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_IMPLICIT_WORK_TREE",
        "GIT_GRAFT_FILE",
        "GIT_INDEX_FILE",
        "GIT_NO_REPLACE_OBJECTS",
        "GIT_REPLACE_REF_BASE",
        "GIT_PREFIX",
        "GIT_SHALLOW_FILE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(name);
    }
}

fn run_git(repo: &Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new("git");
    command.args(args).current_dir(repo);
    scrub_git_repository_env(&mut command);
    let output = command
        .output()
        .with_context(|| format!("run fixture git {}", args.join(" ")))?;
    ensure!(
        output.status.success(),
        "fixture git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn changed_path_fixture(changed_path: &str) -> Result<TempDir> {
    let temp = tempfile::tempdir().context("create routing fixture")?;
    let repo = temp.path();
    run_git(repo, &["init", "-q"])?;
    // These settings belong only to this disposable fixture repository.
    run_git(repo, &["config", "user.email", "fixture@example.invalid"])?;
    run_git(repo, &["config", "user.name", "Routing Fixture"])?;
    run_git(repo, &["config", "commit.gpgsign", "false"])?;
    run_git(repo, &["config", "tag.gpgsign", "false"])?;

    let target = repo.join(changed_path);
    let parent = target.parent().context("fixture path must have a parent")?;
    fs::create_dir_all(parent).context("create changed-path parent")?;
    fs::write(&target, "before\n").context("write fixture base file")?;
    run_git(repo, &["add", "--force", "--", changed_path])?;
    run_git(repo, &["commit", "-q", "-m", "fixture base"])?;
    fs::write(&target, "after\n").context("write fixture head file")?;
    run_git(repo, &["add", "--force", "--", changed_path])?;
    run_git(repo, &["commit", "-q", "-m", "fixture head"])?;
    Ok(temp)
}

fn routing_output(repo: &Path, plan: bool) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    if plan {
        command.args(["proof", "--plan", "--profile", "affected"]);
    } else {
        command.args(["affected", "--json"]);
    }
    command
        .args(["--base", "HEAD^", "--head", "HEAD", "--policy"])
        .arg(checked_in_policy()?)
        .current_dir(repo);
    scrub_git_repository_env(&mut command);
    command.output().context("run routing executable")
}

fn expect_exit(output: &Output, code: i32) -> Result<()> {
    ensure!(
        output.status.code() == Some(code),
        "expected exit {code}, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn output_json(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "parse executable JSON; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn expect_json(actual: &Value, expected: Value, label: &str) -> Result<()> {
    ensure!(
        *actual == expected,
        "{label}: expected {expected}, got {actual}"
    );
    Ok(())
}

fn array<'a>(report: &'a Value, key: &str) -> Result<&'a [Value]> {
    report
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .with_context(|| format!("report {key} must be an array"))
}

fn expect_single_scope(report: &Value, name: &str, changed_path: &str) -> Result<()> {
    expect_json(&report["schema"], json!("tokmd.affected.v1"), "schema")?;
    expect_json(&report["ok"], json!(true), "routing result")?;
    expect_json(
        &report["changed_files"],
        json!([changed_path]),
        "changed files",
    )?;
    expect_json(&report["unknown_files"], json!([]), "unknown files")?;
    let scopes = array(report, "scopes")?;
    ensure!(scopes.len() == 1, "expected one scope, got {scopes:?}");
    let scope = scopes.first().context("one routed scope must exist")?;
    expect_json(&scope["name"], json!(name), "scope name")?;
    expect_json(&scope["kind"], json!("rust"), "scope kind")?;
    expect_json(
        &scope["matched_files"],
        json!([changed_path]),
        "scope files",
    )
}

fn write_passed_observation(repo: &Path, routing: &Value) -> Result<PathBuf> {
    // Preserve the executable's actual unknown paths rather than fixing the
    // routing output inside the fixture. All other fields form one valid,
    // non-required, synthetic passed coverage observation.
    array(routing, "unknown_files")?;
    array(routing, "changed_files")?;
    let observation = json!({
        "schema": "tokmd.proof_executor_observation.v1",
        "status": "passed",
        "execution_status": "executed",
        "profile": "affected",
        "base": "HEAD^",
        "head": "HEAD",
        "family": "coverage",
        "required": false,
        "ok": true,
        "execution_guard": {
            "enabled": true,
            "ci": false,
            "reason": "local_explicit_opt_in_enabled"
        },
        "counts": {
            "selected": 1,
            "executed": 1,
            "passed": 1,
            "failed": 0,
            "artifacts": 1
        },
        "scopes": [{
            "name": "tokmd_cli",
            "kind": "coverage",
            "command": CLI_COVERAGE_COMMAND,
            "artifact_path": "target/proof/coverage/tokmd_cli.lcov",
            "status": "passed",
            "exit_code": 0
        }],
        "changed_files": routing["changed_files"],
        "unknown_files": routing["unknown_files"]
    });
    let path = repo.join("passed-observation.json");
    fs::write(&path, serde_json::to_vec_pretty(&observation)?)
        .context("write synthetic passed observation")?;
    Ok(path)
}

fn collect_observation(repo: &Path, observation: &Path) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    command
        .args(["proof-execution-observations-summary", "--observation"])
        .arg(observation)
        .args([
            "--min-observations",
            "1",
            "--min-executed",
            "1",
            "--min-scopes",
            "1",
            "--min-artifacts",
            "1",
        ])
        .current_dir(repo);
    scrub_git_repository_env(&mut command);
    command
        .output()
        .context("run observation collector executable")
}

#[test]
fn proof_plan_cli_recovery_path_routes_only_to_tokmd_cli() -> Result<()> {
    let temp = changed_path_fixture(CLI_RECOVERY_PATH)?;
    let output = routing_output(temp.path(), false)?;
    expect_exit(&output, 0)?;
    let report = output_json(&output)?;
    expect_single_scope(&report, "tokmd_cli", CLI_RECOVERY_PATH)
}

#[test]
fn proof_plan_cli_recovery_path_plans_exact_cli_coverage_command() -> Result<()> {
    let temp = changed_path_fixture(CLI_RECOVERY_PATH)?;
    let output = routing_output(temp.path(), true)?;
    expect_exit(&output, 0)?;
    let report = output_json(&output)?;
    expect_json(
        &report["schema"],
        json!("tokmd.proof_plan.v1"),
        "plan schema",
    )?;
    expect_json(&report["ok"], json!(true), "plan result")?;
    expect_json(&report["profile"], json!("affected"), "plan profile")?;
    expect_json(
        &report["changed_files"],
        json!([CLI_RECOVERY_PATH]),
        "changed files",
    )?;
    expect_json(&report["unknown_files"], json!([]), "unknown files")?;
    let commands = array(&report, "commands")?;
    ensure!(!commands.is_empty(), "mapped path must plan commands");
    for command in commands {
        expect_json(&command["scope"], json!("tokmd_cli"), "command scope")?;
    }
    let coverage = commands
        .iter()
        .filter(|command| command["kind"] == "coverage")
        .cloned()
        .collect::<Vec<_>>();
    expect_json(
        &json!(coverage),
        json!([{
            "scope": "tokmd_cli",
            "kind": "coverage",
            "required": false,
            "command": CLI_COVERAGE_COMMAND
        }]),
        "coverage commands",
    )
}

#[test]
fn proof_plan_cli_recovery_routing_allows_complete_passed_observation_collection() -> Result<()> {
    let temp = changed_path_fixture(CLI_RECOVERY_PATH)?;
    let routing_output = routing_output(temp.path(), false)?;
    expect_exit(&routing_output, 0)?;
    let routing = output_json(&routing_output)?;
    expect_json(
        &routing["changed_files"],
        json!([CLI_RECOVERY_PATH]),
        "changed files",
    )?;
    let observation = write_passed_observation(temp.path(), &routing)?;
    let output = collect_observation(temp.path(), &observation)?;
    expect_exit(&output, 0)?;
    let report = output_json(&output)?;
    expect_json(&routing["unknown_files"], json!([]), "routed unknown files")?;
    expect_json(
        &report["schema"],
        json!("tokmd.proof_executor_observation_collection.v1"),
        "collection schema",
    )?;
    expect_json(&report["ok"], json!(true), "collection result")?;
    expect_json(
        &report["counts"],
        json!({"observations": 1, "selected": 1, "executed": 1,
            "passed": 1, "failed": 0, "artifacts": 1}),
        "collection counts",
    )?;
    expect_json(
        &report["scopes"],
        json!([{"name": "tokmd_cli", "kind": "coverage", "family": "coverage",
            "observations": 1, "executed": 1, "artifacts": 1}]),
        "collection scopes",
    )?;
    ensure!(
        array(&report, "sources")?.len() == 1,
        "one observation source required"
    );
    Ok(())
}

#[test]
fn proof_plan_routing_regression_path_routes_only_to_proof_control_plane() -> Result<()> {
    let temp = changed_path_fixture(ROUTING_TEST_PATH)?;
    let output = routing_output(temp.path(), false)?;
    expect_exit(&output, 0)?;
    let report = output_json(&output)?;
    expect_single_scope(&report, "proof_control_plane", ROUTING_TEST_PATH)
}

#[test]
fn proof_plan_unknown_rust_path_is_preserved_and_strict_collector_rejects_it() -> Result<()> {
    let temp = changed_path_fixture(UNKNOWN_RUST_PATH)?;
    let output = routing_output(temp.path(), false)?;
    expect_exit(&output, 0)?;
    let report = output_json(&output)?;
    expect_json(
        &report["schema"],
        json!("tokmd.affected.v1"),
        "routing schema",
    )?;
    expect_json(&report["ok"], json!(true), "unknown Rust routing result")?;
    expect_json(
        &report["changed_files"],
        json!([UNKNOWN_RUST_PATH]),
        "changed files",
    )?;
    expect_json(
        &report["unknown_files"],
        json!([UNKNOWN_RUST_PATH]),
        "unknown Rust path",
    )?;
    expect_json(&report["scopes"], json!([]), "unknown Rust scopes")?;
    let plan_output = routing_output(temp.path(), true)?;
    expect_exit(&plan_output, 0)?;
    let plan = output_json(&plan_output)?;
    expect_json(&plan["schema"], json!("tokmd.proof_plan.v1"), "plan schema")?;
    expect_json(&plan["ok"], json!(true), "unknown Rust plan result")?;
    expect_json(
        &plan["changed_files"],
        json!([UNKNOWN_RUST_PATH]),
        "planned changed files",
    )?;
    expect_json(
        &plan["unknown_files"],
        json!([UNKNOWN_RUST_PATH]),
        "planned unknown Rust path",
    )?;
    expect_json(
        &plan["commands"],
        json!([]),
        "unknown Rust planned commands",
    )?;
    let observation = write_passed_observation(temp.path(), &report)?;
    let collected = collect_observation(temp.path(), &observation)?;
    expect_exit(&collected, 1)?;
    ensure!(
        collected.stdout.is_empty(),
        "rejected observation must not emit collection JSON"
    );
    let stderr = String::from_utf8_lossy(&collected.stderr);
    ensure!(
        stderr.contains("proof executor observation reports 1 unknown file(s)"),
        "expected strict unknown-file rejection, got {stderr}"
    );
    Ok(())
}

#[test]
fn proof_plan_unknown_non_rust_path_is_preserved_and_affected_fails() -> Result<()> {
    let temp = changed_path_fixture(UNKNOWN_NON_RUST_PATH)?;
    let output = routing_output(temp.path(), false)?;
    expect_exit(&output, 1)?;
    let report = output_json(&output)?;
    expect_json(
        &report["schema"],
        json!("tokmd.affected.v1"),
        "routing schema",
    )?;
    expect_json(
        &report["ok"],
        json!(false),
        "unknown non-Rust routing result",
    )?;
    expect_json(
        &report["changed_files"],
        json!([UNKNOWN_NON_RUST_PATH]),
        "changed files",
    )?;
    expect_json(
        &report["unknown_files"],
        json!([UNKNOWN_NON_RUST_PATH]),
        "unknown non-Rust path",
    )?;
    expect_json(&report["scopes"], json!([]), "unknown non-Rust scopes")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(
        stderr.contains("affected proof scope discovery found 1 unknown file(s)"),
        "expected unknown-file discovery failure, got {stderr}"
    );
    Ok(())
}

#[test]
fn proof_plan_child_git_environment_keeps_fixture_repository_isolated() -> Result<()> {
    let outer = changed_path_fixture("outer_fixture_w905.rs")?;
    let inner = changed_path_fixture("xtask/src/tasks/affected.rs")?;
    let outer_git = outer.path().join(".git");
    let marker = "isolation_marker_w905.rs";
    // Both repositories have this untracked name, so a misplaced Git add can
    // succeed while modifying the wrong index. The contents distinguish them.
    fs::write(outer.path().join(marker), "outer marker\n")
        .context("write outer isolation marker")?;
    fs::write(inner.path().join(marker), "inner marker\n")
        .context("write inner isolation marker")?;
    let outer_head = run_git(outer.path(), &["rev-parse", "HEAD"])?.stdout;
    let outer_index = fs::read(outer_git.join("index")).context("read outer fixture index")?;
    let outer_config = fs::read(outer_git.join("config")).context("read outer fixture config")?;

    let mut add = Command::new("git");
    add.args(["add", "--force", "--", marker]);
    let mut locate = Command::new("git");
    locate.args(["rev-parse", "--show-toplevel"]);
    let mut affected = Command::new(env!("CARGO_BIN_EXE_xtask"));
    affected
        .args([
            "affected", "--json", "--base", "HEAD^", "--head", "HEAD", "--policy",
        ])
        .arg(checked_in_policy()?);
    for command in [&mut add, &mut locate, &mut affected] {
        command
            .current_dir(inner.path())
            .env("GIT_DIR", &outer_git)
            .env("GIT_WORK_TREE", outer.path())
            .env("GIT_INDEX_FILE", outer_git.join("index"))
            .env("GIT_OBJECT_DIRECTORY", outer_git.join("objects"))
            .env("GIT_COMMON_DIR", &outer_git)
            .env("GIT_CONFIG", outer_git.join("config"));
        scrub_git_repository_env(command);
    }
    let added = add.output().context("run isolated child Git add")?;
    expect_exit(&added, 0)?;
    let located = locate
        .output()
        .context("locate isolated child Git repository")?;
    expect_exit(&located, 0)?;
    let located_root = String::from_utf8(located.stdout).context("decode isolated Git root")?;
    ensure!(
        fs::canonicalize(located_root.trim()).context("canonicalize located Git root")?
            == fs::canonicalize(inner.path()).context("canonicalize inner fixture")?,
        "child Git must locate the inner fixture, got {located_root}"
    );
    let tracked = run_git(inner.path(), &["ls-files", "--error-unmatch", "--", marker])?;
    ensure!(
        String::from_utf8(tracked.stdout)
            .context("decode staged marker")?
            .trim()
            == "isolation_marker_w905.rs",
        "child Git add must stage the marker in the inner fixture"
    );
    let routed = affected
        .output()
        .context("run isolated child affected planner")?;
    expect_exit(&routed, 0)?;
    expect_single_scope(
        &output_json(&routed)?,
        "proof_control_plane",
        "xtask/src/tasks/affected.rs",
    )?;
    ensure!(
        run_git(outer.path(), &["rev-parse", "HEAD"])?.stdout == outer_head,
        "outer fixture HEAD must remain unchanged"
    );
    ensure!(
        fs::read(outer_git.join("index")).context("reread outer fixture index")? == outer_index,
        "outer fixture index must remain unchanged"
    );
    ensure!(
        fs::read(outer_git.join("config")).context("reread outer fixture config")? == outer_config,
        "outer fixture config must remain unchanged"
    );
    Ok(())
}
