#![cfg(feature = "analysis")]

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};

/// "Docs as tests" - verify that the commands we recommend in README/Recipes actually work.
/// These run against `tests/data` to ensure stability.
fn tokmd() -> Command {
    let fixtures = fixture_dir();
    tokmd_in(&fixtures)
}

fn tokmd_in(cwd: &Path) -> Command {
    let mut cmd: Command = cargo_bin_cmd!("tokmd");
    cmd.current_dir(cwd);
    cmd
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

fn git_recipe_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();

    run_git(tmp.path(), &["init"]);
    run_git(tmp.path(), &["add", "."]);
    run_git(
        tmp.path(),
        &[
            "-c",
            "user.name=tokmd docs test",
            "-c",
            "user.email=tokmd@example.invalid",
            "commit",
            "-m",
            "initial",
        ],
    );
    tmp
}

fn run_git(cwd: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed with {status}");
}

#[test]
fn start_here_saved_analysis_gate() -> Result<(), Box<dyn std::error::Error>> {
    let docs = include_str!("../../../docs/start-here.md");
    let section = docs
        .split_once("Use policy gates only")
        .ok_or("missing saved-policy section")?
        .1
        .split_once("For this repository's contributor workflow")
        .ok_or("missing contributor-workflow boundary")?
        .0;
    let policy = section
        .split_once("```toml\n")
        .ok_or("missing standalone TOML policy")?
        .1
        .split_once("```")
        .ok_or("unterminated TOML policy")?
        .0;
    let workflow = section
        .split_once("```bash\n")
        .ok_or("missing saved-analysis commands")?
        .1
        .split_once("```")
        .ok_or("unterminated saved-analysis commands")?
        .0;
    let tmp = tempfile::tempdir()?;
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo)?;
    fs::write(repo.join("main.rs"), "fn main() {}\n")?;
    let selected_config = tmp.path().join("selected.toml");
    fs::write(&selected_config, "")?;
    let command = || {
        let mut cmd = tokmd_in(&repo);
        cmd.env("TOKMD_CONFIG", &selected_config)
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .args(["--config", "none", "--no-ignore-parent", "--no-progress"]);
        cmd
    };
    let policy_path = repo.join("tokmd-gate.toml");
    fs::write(&policy_path, policy)?;

    // Extract doc-specific argv; the command closure only isolates ambient
    // scan/TOML configuration, parent ignores, and progress output.
    let commands: Vec<Vec<&str>> = workflow
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.strip_prefix("tokmd ")
                .map(|args| args.split_whitespace().collect())
                .ok_or("expected a tokmd command")
        })
        .collect::<Result<_, _>>()?;
    assert_eq!(commands.len(), 2);
    for args in &commands {
        command().args(args).assert().success();
    }
    let gate_command = commands.get(1).ok_or("missing gate command")?;

    // Literal fixture oracle: one Rust line and six TOML policy lines yield
    // 70 inventory-estimated tokens, well below the documented 500000 cap.
    let output = command()
        .args(gate_command)
        .args(["--format", "json"])
        .output()?;
    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(result["passed"], true);
    assert_eq!(result["total_errors"], 0);
    assert_eq!(
        result["policy"]["rule_results"]
            .as_array()
            .ok_or("missing policy rule results")?
            .len(),
        1
    );
    assert_eq!(result["policy"]["rule_results"][0]["actual"], 70);

    // A passing process must still enforce the policy: lower the literal cap.
    assert!(policy.contains("value = 500000"));
    fs::write(&policy_path, policy.replace("value = 500000", "value = 0"))?;
    let failed = command()
        .args(gate_command)
        .args(["--format", "json"])
        .assert()
        .code(1);
    let result: serde_json::Value = serde_json::from_slice(&failed.get_output().stdout)?;
    assert_eq!(result["passed"], false);
    assert_eq!(result["total_errors"], 1);
    assert_eq!(result["policy"]["rule_results"][0]["actual"], 70);

    // The run manifest is a real JSON artifact, but has no derived metrics.
    fs::write(&policy_path, policy)?;
    command()
        .args([
            "gate",
            ".runs/current/receipt.json",
            "--policy",
            "tokmd-gate.toml",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "Pointer '/derived/totals/tokens' not found in receipt",
        ));

    // A missing saved analysis must not turn this workflow into a passing gate.
    fs::remove_file(repo.join(".runs/current/analysis.json"))?;
    command()
        .args(gate_command)
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(".runs/current/analysis.json"));
    Ok(())
}

#[test]
fn recipe_badge_generation() {
    let tmp = tempfile::tempdir().unwrap();

    // "tokmd badge --metric lines --output badge-lines.svg"
    let badge_lines_path = tmp.path().join("badge-lines.svg");
    tokmd()
        .arg("badge")
        .arg("--metric")
        .arg("lines")
        .arg("--output")
        .arg(&badge_lines_path)
        .assert()
        .success();
    assert!(badge_lines_path.exists());

    // "tokmd badge --metric hotspot --preset risk --output badge-hotspot.svg"
    let badge_hotspot_path = tmp.path().join("badge-hotspot.svg");
    tokmd()
        .arg("badge")
        .arg("--metric")
        .arg("hotspot")
        .arg("--preset")
        .arg("risk")
        .arg("--output")
        .arg(&badge_hotspot_path)
        .assert()
        .success();
    assert!(badge_hotspot_path.exists());
}

#[test]
fn recipe_analyze_presets() {
    // "tokmd analyze --preset receipt --format md"
    tokmd()
        .arg("analyze")
        .arg("--preset")
        .arg("receipt")
        .arg("--format")
        .arg("md")
        .assert()
        .success();

    // "tokmd analyze --preset risk --format md"
    tokmd()
        .arg("analyze")
        .arg("--preset")
        .arg("risk")
        .arg("--format")
        .arg("md")
        .assert()
        .success();

    // "tokmd analyze --preset estimate --effort-base-ref main --effort-head-ref HEAD --format md"
    let git_fixture = git_recipe_fixture();
    tokmd_in(git_fixture.path())
        .arg("analyze")
        .arg("--preset")
        .arg("estimate")
        .arg("--effort-base-ref")
        .arg("HEAD")
        .arg("--effort-head-ref")
        .arg("HEAD")
        .arg("--format")
        .arg("md")
        .assert()
        .success();

    // "tokmd analyze --preset estimate --effort-layer headline --format md"
    tokmd()
        .arg("analyze")
        .arg("--preset")
        .arg("estimate")
        .arg("--effort-layer")
        .arg("headline")
        .arg("--format")
        .arg("md")
        .assert()
        .success();

    // "tokmd analyze --preset estimate --monte-carlo --mc-seed 42 --format json"
    tokmd()
        .arg("analyze")
        .arg("--preset")
        .arg("estimate")
        .arg("--monte-carlo")
        .arg("--mc-seed")
        .arg("42")
        .arg("--format")
        .arg("json")
        .assert()
        .success();
}

#[test]
fn recipe_context_list() {
    // "tokmd context --budget 128k"
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .assert()
        .success();
}

#[test]
fn recipe_context_bundle() {
    // "tokmd context --budget 128k --mode bundle --output context.txt"
    let tmp = tempfile::tempdir().unwrap();
    let bundle_path = tmp.path().join("context.txt");
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .arg("--mode")
        .arg("bundle")
        .arg("--output")
        .arg(&bundle_path)
        .assert()
        .success();
    assert!(bundle_path.exists());
}

#[test]
fn recipe_context_spread() {
    // "tokmd context --budget 200k --strategy spread"
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("200k")
        .arg("--strategy")
        .arg("spread")
        .assert()
        .success();
}

#[test]
fn recipe_context_bundle_compress() {
    // "tokmd context --budget 100k --mode bundle --compress --output bundle.txt"
    let tmp = tempfile::tempdir().unwrap();
    let bundle_path = tmp.path().join("bundle.txt");
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("100k")
        .arg("--mode")
        .arg("bundle")
        .arg("--compress")
        .arg("--output")
        .arg(&bundle_path)
        .assert()
        .success();
    assert!(bundle_path.exists());
}

#[test]
fn recipe_context_json() {
    // "tokmd context --budget 128k --mode json --output selection.json"
    let tmp = tempfile::tempdir().unwrap();
    let json_path = tmp.path().join("selection.json");
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .arg("--mode")
        .arg("json")
        .arg("--output")
        .arg(&json_path)
        .assert()
        .success();
    assert!(json_path.exists());
}

#[test]
fn recipe_gate_default() {
    // "tokmd gate" (Requires a policy to succeed in tests, simulating inline config)
    let tmp = tempfile::tempdir().unwrap();
    let policy_path = tmp.path().join("policy.toml");
    std::fs::write(
        &policy_path,
        r#"
[[rules]]
name = "max_tokens"
pointer = "/derived/totals/tokens"
op = "lte"
value = 5000000
"#,
    )
    .unwrap();

    tokmd()
        .arg("gate")
        .arg("--policy")
        .arg(&policy_path)
        .assert()
        .success();
}

#[test]
fn recipe_gate_json() {
    // "tokmd gate --format json"
    let tmp = tempfile::tempdir().unwrap();
    let policy_path = tmp.path().join("policy.toml");
    std::fs::write(
        &policy_path,
        r#"
[[rules]]
name = "max_tokens"
pointer = "/derived/totals/tokens"
op = "lte"
value = 5000000
"#,
    )
    .unwrap();

    tokmd()
        .arg("gate")
        .arg("--format")
        .arg("json")
        .arg("--policy")
        .arg(&policy_path)
        .assert()
        .success();
}

#[test]
fn recipe_gate_fail_fast() {
    // "tokmd gate --fail-fast"
    let tmp = tempfile::tempdir().unwrap();
    let policy_path = tmp.path().join("policy.toml");
    std::fs::write(
        &policy_path,
        r#"
[[rules]]
name = "max_tokens"
pointer = "/derived/totals/tokens"
op = "lte"
value = 5000000
"#,
    )
    .unwrap();

    tokmd()
        .arg("gate")
        .arg("--fail-fast")
        .arg("--policy")
        .arg(&policy_path)
        .assert()
        .success();
}

#[test]
fn recipe_run_and_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let baseline_dir = tmp.path().join(".runs").join("baseline");
    let current_dir = tmp.path().join(".runs").join("current");

    // "tokmd run --analysis receipt --output-dir .runs/baseline"
    tokmd()
        .arg("run")
        .arg("--analysis")
        .arg("receipt")
        .arg("--output-dir")
        .arg(&baseline_dir)
        .assert()
        .success();
    assert!(baseline_dir.join("receipt.json").exists());

    // "tokmd run --analysis receipt --output-dir .runs/current"
    tokmd()
        .arg("run")
        .arg("--analysis")
        .arg("receipt")
        .arg("--output-dir")
        .arg(&current_dir)
        .assert()
        .success();
    assert!(current_dir.join("receipt.json").exists());

    // "tokmd diff .runs/baseline .runs/current"
    tokmd()
        .arg("diff")
        .arg(&baseline_dir)
        .arg(&current_dir)
        .assert()
        .success();
}

#[test]
fn recipe_release_comparison_uses_current_saved_run() -> Result<(), Box<dyn std::error::Error>> {
    let docs = include_str!("../../../docs/recipes.md");
    let workflow = docs
        .split_once("**Release comparison**:")
        .ok_or("missing release comparison recipe")?
        .1
        .split_once("```bash\n")
        .ok_or("missing release comparison commands")?
        .1
        .split_once("# After release:")
        .ok_or("missing post-release boundary")?
        .0;
    let commands: Vec<Vec<&str>> = workflow
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.strip_prefix("tokmd ")
                .map(|args| args.split_whitespace().collect())
                .ok_or("expected a tokmd command")
        })
        .collect::<Result<_, _>>()?;
    let diff_args = commands
        .iter()
        .find(|args| args.first() == Some(&"diff"))
        .ok_or("missing release diff command")?;

    let tmp = tempfile::tempdir()?;
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("src"))?;
    let source = repo.join("src/main.rs");
    fs::write(&source, "fn main() {}\n")?;
    let selected_config = tmp.path().join("selected.toml");
    fs::write(&selected_config, "")?;
    let command = || {
        let mut cmd = tokmd_in(&repo);
        cmd.env("TOKMD_CONFIG", &selected_config)
            .env_remove("TOKMD_PROFILE")
            .env_remove("TOKMD_PROGRESS_EVENTS")
            .args(["--config", "none", "--no-ignore-parent", "--no-progress"]);
        cmd
    };
    command()
        .args([
            "run",
            "--analysis",
            "receipt",
            "--output-dir",
            ".tokmd/baselines/v1.0.0",
        ])
        .assert()
        .success();
    let baseline_dir = repo.join(".tokmd/baselines/v1.0.0");
    let mut baseline_files = Vec::new();
    for entry in fs::read_dir(&baseline_dir)? {
        let path = entry?.path();
        baseline_files.push((path.clone(), fs::read(path)?));
    }
    assert!(!baseline_files.is_empty());
    fs::write(
        &source,
        "fn main() {\n    let value = 1;\n    println!(\"{value}\");\n}\n",
    )?;
    assert!(!repo.join("lang.json").exists());

    // A missing current run must fail before producing a Markdown report.
    // The exact documented diff argv is retried after its capture step below.
    command()
        .args(diff_args)
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Failed to load diff source"));

    let mut comparison = None;
    for args in &commands {
        let output = command().args(args).output()?;
        assert!(
            output.status.success(),
            "documented command {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args.first() == Some(&"diff") {
            comparison = Some(output);
        }
    }
    let comparison = comparison.ok_or("release recipe did not execute diff")?;
    let markdown = String::from_utf8(comparison.stdout)?;
    // Literal source oracle: the baseline has one Rust LOC, the checkout four.
    assert!(markdown.contains("|LOC|1|4|+3|"), "{markdown}");
    assert!(markdown.contains("|Rust|1|4|+3|"), "{markdown}");
    assert!(comparison.stderr.is_empty());

    assert_eq!(
        fs::read_dir(&baseline_dir)?
            .collect::<Result<Vec<_>, _>>()?
            .len(),
        baseline_files.len()
    );
    for (path, bytes) in baseline_files {
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "baseline changed: {}",
            path.display()
        );
    }
    assert!(!repo.join("lang.json").exists());
    Ok(())
}

#[test]
fn recipe_context_budget() {
    // "tokmd context --budget 128k"
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .assert()
        .success();
}

#[test]
fn recipe_tools_export_schemas() {
    // "tokmd tools --format openai --pretty"
    tokmd()
        .arg("tools")
        .arg("--format")
        .arg("openai")
        .arg("--pretty")
        .assert()
        .success();

    // "tokmd tools --format anthropic --pretty"
    tokmd()
        .arg("tools")
        .arg("--format")
        .arg("anthropic")
        .arg("--pretty")
        .assert()
        .success();

    // "tokmd tools --format jsonschema --pretty"
    tokmd()
        .arg("tools")
        .arg("--format")
        .arg("jsonschema")
        .arg("--pretty")
        .assert()
        .success();
}

#[test]
fn recipe_default_map() {
    // "tokmd module --top 20"
    tokmd()
        .arg("module")
        .arg("--top")
        .arg("20")
        .assert()
        .success();
}

#[test]
fn recipe_export_map_jsonl() {
    // "tokmd export --min-code 20 --max-rows 300 --redact paths > map.jsonl"
    tokmd()
        .arg("export")
        .arg("--min-code")
        .arg("0") // adjusted for small test data
        .arg("--max-rows")
        .arg("300")
        .arg("--redact")
        .arg("paths")
        .assert()
        .success();
}

#[test]
fn recipe_simple_lang_summary() {
    // "tokmd lang"
    tokmd().arg("lang").assert().success();
}

#[test]
fn recipe_module_summary_markdown() {
    // "tokmd module --format md"
    tokmd()
        .arg("module")
        .arg("--format")
        .arg("md")
        .assert()
        .success();
}

#[test]
fn recipe_export_full_json() {
    // "tokmd export --format json"
    tokmd()
        .arg("export")
        .arg("--format")
        .arg("json")
        .assert()
        .success();
}

#[test]
fn recipe_ci_workflow_snippet() {
    // From README: "tokmd module --format json > tokmd.module.json"
    // We don't redirect here, just check exit code.
    tokmd()
        .arg("module")
        .arg("--format")
        .arg("json")
        .assert()
        .success();
}

#[test]
fn recipe_generate_baseline() {
    // "tokmd baseline --output baseline.json"
    let tmp = tempfile::tempdir().unwrap();
    let baseline_path = tmp.path().join("baseline.json");
    tokmd()
        .arg("baseline")
        .arg("--output")
        .arg(&baseline_path)
        .assert()
        .success();
    assert!(baseline_path.exists());
}

#[test]
fn recipe_handoff_bundle() {
    // "tokmd handoff"
    let tmp = tempfile::tempdir().unwrap();
    let handoff_dir = tmp.path().join(".handoff");
    tokmd()
        .current_dir(tmp.path())
        .arg("handoff")
        .assert()
        .success();
    assert!(handoff_dir.exists());
    assert!(handoff_dir.join("manifest.json").exists());

    // "tokmd handoff --out-dir ./artifacts/handoff"
    let out_dir = tmp.path().join("artifacts").join("handoff");
    tokmd()
        .current_dir(tmp.path())
        .arg("handoff")
        .arg("--out-dir")
        .arg("./artifacts/handoff")
        .assert()
        .success();
    assert!(out_dir.exists());
    assert!(out_dir.join("manifest.json").exists());
}

#[test]
fn recipe_handoff_budget_spread() {
    // "tokmd handoff --budget 128k --strategy spread"
    let tmp = tempfile::tempdir().unwrap();
    let handoff_dir = tmp.path().join(".handoff");
    tokmd()
        .current_dir(tmp.path())
        .arg("handoff")
        .arg("--budget")
        .arg("128k")
        .arg("--strategy")
        .arg("spread")
        .assert()
        .success();
    assert!(handoff_dir.exists());
    assert!(handoff_dir.join("manifest.json").exists());
}

#[test]
fn recipe_handoff_no_git() {
    // "tokmd handoff --no-git"
    let tmp = tempfile::tempdir().unwrap();
    let handoff_dir = tmp.path().join(".handoff");
    tokmd()
        .current_dir(tmp.path())
        .arg("handoff")
        .arg("--no-git")
        .assert()
        .success();
    assert!(handoff_dir.exists());
    assert!(handoff_dir.join("manifest.json").exists());
}

#[cfg(feature = "git")]
#[test]
fn recipe_sensor_json() {
    // Skip if the fixtures directory is not inside a git repository
    // (e.g., in the Nix build sandbox where .git is absent)
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data");
    let in_git = std::process::Command::new("git")
        .arg("-C")
        .arg(&fixtures)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !in_git {
        eprintln!("Skipping recipe_sensor_json: not inside a git repository");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let report_path = tmp.path().join("report.json");
    tokmd()
        .env("TOKMD_GIT_BASE_REF", "HEAD")
        .arg("sensor")
        .arg("--format")
        .arg("json")
        .arg("--output")
        .arg(&report_path)
        .assert()
        .success();
    assert!(report_path.exists());
}

#[test]
#[cfg(feature = "content")]
fn recipe_gate_with_baseline() {
    // "tokmd gate --baseline baseline.json"
    let tmp = tempfile::tempdir().unwrap();
    let baseline_path = tmp.path().join("baseline.json");

    // First generate a baseline
    tokmd()
        .arg("baseline")
        .arg("--output")
        .arg(&baseline_path)
        .assert()
        .success();

    // Then gate against it (should pass since it's the same state)
    let ratchet_path = tmp.path().join("ratchet.toml");
    std::fs::write(
        &ratchet_path,
        r#"
[[rules]]
pointer = "/complexity/avg_cyclomatic"
max_increase_pct = 10.0
"#,
    )
    .unwrap();

    tokmd()
        .arg("gate")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--ratchet-config")
        .arg(&ratchet_path)
        .assert()
        .success();
}

#[test]
fn recipe_init_non_interactive() {
    let tmp = tempfile::tempdir().unwrap();
    tokmd()
        .arg("init")
        .arg("--non-interactive")
        .arg("--dir")
        .arg(tmp.path())
        .assert()
        .success();
}

#[test]
fn recipe_context_spread_compress() {
    let tmp = tempfile::tempdir().unwrap();
    let bundle_path = tmp.path().join("context.txt");

    // tokmd context --budget 128k --strategy spread --mode bundle --output context.txt
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .arg("--strategy")
        .arg("spread")
        .arg("--mode")
        .arg("bundle")
        .arg("--output")
        .arg(&bundle_path)
        .assert()
        .success();
    assert!(bundle_path.exists());

    let bundle_compressed_path = tmp.path().join("context_compressed.txt");
    // tokmd context --budget 128k --mode bundle --compress --output context.txt
    tokmd()
        .arg("context")
        .arg("--budget")
        .arg("128k")
        .arg("--mode")
        .arg("bundle")
        .arg("--compress")
        .arg("--output")
        .arg(&bundle_compressed_path)
        .assert()
        .success();
    assert!(bundle_compressed_path.exists());
}

#[test]
fn recipe_check_ignore() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("target/debug")).unwrap();
    std::fs::write(tmp.path().join(".tokeignore"), "target/**\n").unwrap();
    std::fs::write(tmp.path().join("target/debug/myapp"), "binary").unwrap();

    tokmd()
        .current_dir(tmp.path())
        .arg("check-ignore")
        .arg("target/debug/myapp")
        .assert()
        .success()
        .stdout(predicate::str::contains("target/debug/myapp: ignored"));
}

#[test]
fn recipe_check_ignore_verbose() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules/lodash")).unwrap();
    std::fs::write(tmp.path().join(".tokeignore"), "node_modules/**\n").unwrap();
    std::fs::write(
        tmp.path().join("node_modules/lodash/index.js"),
        "console.log('hi');",
    )
    .unwrap();

    tokmd()
        .current_dir(tmp.path())
        .arg("check-ignore")
        .arg("-v")
        .arg("node_modules/lodash/index.js")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "node_modules/lodash/index.js: ignored",
        ))
        .stdout(predicate::str::contains(".tokeignore: node_modules/**"));
}

#[test]
fn recipe_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let baseline_dir = tmp.path().join("baseline");
    let current_dir = tmp.path().join("current");
    std::fs::create_dir_all(&baseline_dir).unwrap();
    std::fs::create_dir_all(&current_dir).unwrap();

    let output1 = tokmd()
        .arg("lang")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    std::fs::write(baseline_dir.join("lang.json"), output1).unwrap();

    let output2 = tokmd()
        .arg("lang")
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    std::fs::write(current_dir.join("lang.json"), output2).unwrap();

    tokmd()
        .arg("diff")
        .arg(&baseline_dir)
        .arg(&current_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("## Diff:"));
}

#[test]
#[cfg(feature = "git")]
fn recipe_cockpit_format() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data");
    let in_git = std::process::Command::new("git")
        .arg("-C")
        .arg(&fixtures)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !in_git {
        return;
    }

    tokmd()
        .env("TOKMD_GIT_BASE_REF", "HEAD")
        .arg("cockpit")
        .arg("--format")
        .arg("md")
        .assert()
        .success();
}
