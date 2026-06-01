use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("workspace parent")
        .to_path_buf()
}

fn run_xtask(args: &[&str]) -> (String, String, bool) {
    let root = workspace_root();
    let output = Command::new("cargo")
        .arg("run")
        .arg("-q")
        .arg("-p")
        .arg("xtask")
        .arg("--")
        .args(args)
        .current_dir(&root)
        .output()
        .expect("failed to run cargo xtask");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

#[test]
fn ci_actuals_help_mentions_receipt_inputs() {
    let (stdout, stderr, success) = run_xtask(&["ci-actuals", "--help"]);

    assert!(success, "ci-actuals --help failed. stderr: {stderr}");
    assert!(stdout.contains("--needs"), "stdout: {stdout}");
    assert!(stdout.contains("--timings"), "stdout: {stdout}");
    assert!(stdout.contains("--output"), "stdout: {stdout}");
}

#[test]
fn ci_actuals_writes_schema_stable_receipt() {
    let temp = tempfile::tempdir().expect("tempdir");
    let needs = temp.path().join("needs.json");
    let timings = temp.path().join("timings.json");
    let output = temp.path().join("ci-actuals.json");
    fs::write(
        &needs,
        r#"{
          "docs-check": {"result": "success", "outputs": {"docs": "ok"}},
          "mutation": {"result": "skipped", "outputs": {}}
        }"#,
    )
    .expect("needs json");
    fs::write(
        &timings,
        r#"{
          "docs-check": {"duration_seconds": 75.0, "runner": "ubuntu-latest", "cache_hit": true}
        }"#,
    )
    .expect("timings json");

    let (stdout, stderr, success) = run_xtask(&[
        "ci-actuals",
        "--needs",
        needs.to_str().expect("needs path"),
        "--timings",
        timings.to_str().expect("timings path"),
        "--output",
        output.to_str().expect("output path"),
        "--sha",
        "abc123",
    ]);

    assert!(success, "ci-actuals failed. stderr: {stderr}");
    assert!(
        stdout.contains("CI actuals receipt written"),
        "stdout: {stdout}"
    );
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output).expect("receipt body"))
            .expect("receipt json");
    assert_eq!(value["schema"], "tokmd.ci_actuals.v1");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["sha"], "abc123");
    assert_eq!(value["status"]["job_count"], 2);
    assert_eq!(value["status"]["timed_job_count"], 1);
    assert_eq!(value["status"]["missing_timing"][0], "mutation");
    assert_eq!(value["jobs"][0]["name"], "docs-check");
    assert_eq!(value["jobs"][0]["timing_status"], "measured");
    assert_eq!(value["jobs"][1]["name"], "mutation");
    assert_eq!(value["jobs"][1]["timing_status"], "missing");
}

#[test]
fn ci_required_uploads_ci_actuals_before_status_check() {
    let workflow = fs::read_to_string(workspace_root().join(".github/workflows/ci.yml"))
        .expect("read ci workflow");
    let ci_required_idx = workflow
        .find("  ci-required:")
        .expect("CI required job should exist");
    let ci_required = &workflow[ci_required_idx..];

    let checkout_idx = ci_required
        .find("actions/checkout@v6.0.2")
        .expect("CI required checkout step");
    let toolchain_idx = ci_required
        .find("dtolnay/rust-toolchain@stable")
        .expect("CI required toolchain step");
    let cache_idx = ci_required
        .find("Swatinem/rust-cache@v2")
        .expect("CI required cache step");
    let generate_idx = ci_required
        .find("Generate CI actuals receipt")
        .expect("generate CI actuals receipt step");
    let upload_idx = ci_required
        .find("Upload CI actuals receipt")
        .expect("upload CI actuals receipt step");
    let check_idx = ci_required
        .find("Check overall status")
        .expect("check overall status step");

    assert!(
        checkout_idx < toolchain_idx && toolchain_idx < cache_idx && cache_idx < generate_idx,
        "setup should happen before receipt generation"
    );
    assert!(
        generate_idx < upload_idx,
        "upload must follow receipt generation"
    );
    assert!(
        upload_idx < check_idx,
        "receipt upload must happen before final failure"
    );
    let setup_block = &ci_required[checkout_idx..generate_idx];
    let generate_block = &ci_required[generate_idx..upload_idx];
    let upload_block = &ci_required[upload_idx..check_idx];
    let check_block = &ci_required[check_idx..];

    assert_eq!(
        setup_block.matches("continue-on-error: true").count(),
        3,
        "checkout, toolchain, and cache should be best-effort telemetry setup"
    );
    assert!(generate_block.contains("if: always()"));
    assert!(generate_block.contains("continue-on-error: true"));
    assert!(
        generate_block.contains("printf '%s\\n' \"${NEEDS_JSON}\" > target/ci/needs.json"),
        "workflow should persist the raw needs payload"
    );
    assert!(
        generate_block.contains("cargo xtask ci-actuals"),
        "workflow should call the ci-actuals command"
    );
    assert!(
        generate_block.contains("--output target/ci/ci-actuals.json"),
        "workflow should write the stable ci-actuals path"
    );
    assert!(upload_block.contains("if: always()"));
    assert!(upload_block.contains("continue-on-error: true"));
    assert!(
        upload_block.contains("name: ci-actuals"),
        "workflow should upload a named ci-actuals artifact"
    );
    assert!(
        upload_block.contains("target/ci/needs.json")
            && upload_block.contains("target/ci/ci-actuals.json"),
        "workflow should upload both CI actuals files"
    );
    assert!(
        upload_block.contains("if-no-files-found: warn"),
        "receipt upload should not hide the existing aggregate failure summary"
    );
    assert!(
        !check_block.contains("continue-on-error: true"),
        "final required-status arbitration must remain blocking"
    );
}
