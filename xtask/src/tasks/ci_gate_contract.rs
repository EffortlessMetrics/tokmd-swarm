//! Single-tight CI gate contract checker for ub-review adoption (#226).
//!
//! Validates that a workflow file encodes the family gate shape documented in
//! `docs/specs/ub-review-ci-gate.md`: one required check, advisory route job,
//! deterministic core floor, and non-blocking ub-review in a separate job.

use std::fs;

use anyhow::{Context, Result, bail};

use crate::cli::CiGateContractArgs;

const REQUIRED_MARKERS: &[&str] = &[
    "Route CI runner",
    "EM_RUNNER_READ_TOKEN",
    "gh api \"orgs/EffortlessMetrics/actions/runners",
    "runner_kind",
    "self-hosted",
    "em-ci",
    "trusted-pr",
    "fromJSON(needs.route.outputs.runner)",
    "github.event.pull_request.head.repo.fork == false",
];

const GATE_MARKERS: &[&str] = &[
    "name: Tokmd Rust Result",
    "dtolnay/rust-toolchain",
    "Swatinem/rust-cache@v2",
    "Fast precontext and launch core gate",
    "cargo xtask gate --check",
    "core_exit",
    "Assert core gate verdict",
];

const ADVISORY_MARKERS: &[&str] = &[
    "name: UB Review (Advisory)",
    "Record advisory review outcome",
    "EffortlessMetrics/ub-review@",
    "mode: review-direct",
    "posting: review",
    "setup-rust: false",
    "provider-policy: minimax-primary",
    "minimax-model: MiniMax-M3",
    "opencode-model: deepseek-v4-flash",
    "install-mode: source",
    "continue-on-error: true",
];

const FORBIDDEN_MARKERS: &[&str] = &[
    "name: CI (Required)",
    "route-rust-small",
    "router_target=",
    "Rust Small on CPX42",
    "Rust Small on CX43",
    "Rust Small on CX53",
    "Rust Small Fallback on GitHub Hosted",
    "Rust Small Blocked (capacity/config)",
    "fallback_mode=full",
    "no-github-fallback",
    "name: Tokmd Rust Small Result",
    "em-ci-rust:1.95",
    "docker run --rm",
    "pr-thread-context: target/ci-core/precontext.md",
];

pub fn run(args: CiGateContractArgs) -> Result<()> {
    let text = fs::read_to_string(&args.workflow)
        .with_context(|| format!("failed to read {}", args.workflow.display()))?;

    let mut errors = Vec::new();

    for needle in REQUIRED_MARKERS {
        if !text.contains(needle) {
            errors.push(format!("missing required marker: {needle}"));
        }
    }

    if text.contains("repos/${") && text.contains("/actions/runners") {
        errors.push(
            "must not use repository runner discovery (org-level orgs/EffortlessMetrics only)"
                .to_string(),
        );
    }

    for forbidden in FORBIDDEN_MARKERS {
        if text.contains(forbidden) {
            errors.push(format!("forbidden retired marker present: {forbidden}"));
        }
    }

    let gate = top_level_job_block(&text, "tokmd-rust-result");
    let advisory = top_level_job_block(&text, "ub-review");
    match gate {
        Some(gate) => {
            for marker in GATE_MARKERS {
                if !gate.contains(marker) {
                    errors.push(format!(
                        "Tokmd Rust Result job missing required marker: {marker}"
                    ));
                }
            }
            if gate.contains("EffortlessMetrics/ub-review@") {
                errors.push(
                    "advisory ub-review must be a separate job, not a step in Tokmd Rust Result"
                        .to_string(),
                );
            }
        }
        None => errors.push("missing top-level job: tokmd-rust-result".to_string()),
    }
    match advisory {
        Some(advisory) => {
            if !advisory.contains("EffortlessMetrics/ub-review@")
                && text.contains("EffortlessMetrics/ub-review@")
            {
                errors.push(
                    "ub-review action must be inside the top-level ub-review job".to_string(),
                );
            }
            for marker in ADVISORY_MARKERS {
                if !advisory.contains(marker) {
                    errors.push(format!(
                        "advisory ub-review job missing required marker: {marker}"
                    ));
                }
            }
        }
        None => errors.push("missing top-level job: ub-review".to_string()),
    }

    if errors.is_empty() {
        println!("ci-gate-contract OK: {}", args.workflow.display());
        return Ok(());
    }

    for error in &errors {
        eprintln!("ci-gate-contract: {error}");
    }

    if args.check {
        bail!(
            "ci-gate-contract: {} error(s) in {}",
            errors.len(),
            args.workflow.display()
        );
    }

    println!(
        "ci-gate-contract: {} finding(s) in {} (advisory mode)",
        errors.len(),
        args.workflow.display()
    );
    Ok(())
}

fn top_level_job_block(text: &str, job: &str) -> Option<String> {
    let marker = format!("  {job}:");
    let mut in_job = false;
    let mut block = String::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if line == marker {
            in_job = true;
        } else if in_job && line.starts_with("  ") && !line.starts_with("    ") {
            break;
        }
        if in_job {
            block.push_str(line);
            block.push('\n');
        }
    }
    in_job.then_some(block)
}
