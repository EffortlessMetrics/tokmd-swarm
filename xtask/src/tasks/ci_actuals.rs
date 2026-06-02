use crate::cli::CiActualsArgs;
use crate::tasks::ci_plan::{actual_lane_keys, ci_needs_key_lane_alias};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CI_ACTUALS_SCHEMA: &str = "tokmd.ci_actuals.v3";

#[derive(Debug, Deserialize)]
struct NeedEntry {
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    outputs: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TimingInput {
    Seconds(f64),
    Object(TimingObject),
}

#[derive(Debug, Deserialize)]
struct TimingObject {
    duration_seconds: Option<f64>,
    seconds: Option<f64>,
    queue_seconds: Option<f64>,
    actual_lem: Option<f64>,
    runner: Option<String>,
    cache_hit: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
struct TimingRecord {
    duration_seconds: Option<f64>,
    queue_seconds: Option<f64>,
    actual_lem: Option<f64>,
    runner: Option<String>,
    cache_hit: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CiActualsReceipt {
    schema: String,
    schema_version: u32,
    repo: String,
    workflow: String,
    sha: String,
    github: GithubContext,
    jobs: Vec<CiJobActual>,
    status: CiActualsStatus,
}

#[derive(Debug, Serialize)]
struct GithubContext {
    run_id: Option<String>,
    run_attempt: Option<String>,
    event_name: Option<String>,
    ref_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CiJobActual {
    name: String,
    summary_key: String,
    lane_id: String,
    aliases: Vec<String>,
    selected: bool,
    result: String,
    route_target: Option<String>,
    skip_reason: Option<String>,
    estimated_lem: Option<f64>,
    actual_lem: Option<f64>,
    queue_seconds: Option<f64>,
    estimate_source: Option<String>,
    output_keys: Vec<String>,
    runner: Option<String>,
    duration_seconds: Option<f64>,
    duration_minutes: Option<f64>,
    timing_status: String,
    cache_hit: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CiActualsStatus {
    ok: bool,
    job_count: usize,
    timed_job_count: usize,
    missing_timing: Vec<String>,
    unused_timing: Vec<String>,
}

pub fn run(args: CiActualsArgs) -> Result<()> {
    let root = workspace_root()?;
    let receipt = ci_actuals_receipt(&root, &args)?;

    if let Some(parent) = args.output.parent() {
        let output_parent = resolve_path(&root, parent);
        fs::create_dir_all(&output_parent)
            .with_context(|| format!("create {}", output_parent.display()))?;
    }

    let output = resolve_path(&root, &args.output);
    let json = serde_json::to_string_pretty(&receipt).context("serialize CI actuals receipt")?;
    fs::write(&output, format!("{json}\n"))
        .with_context(|| format!("write {}", output.display()))?;
    println!(
        "CI actuals receipt written to {} ({} job(s), {} timed)",
        output.display(),
        receipt.status.job_count,
        receipt.status.timed_job_count
    );

    if let Some(summary_path) = &args.github_summary {
        let summary = render_step_summary(&receipt);
        let path = resolve_path(&root, summary_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let mut existing = fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&summary);
        fs::write(&path, existing).with_context(|| format!("append {}", path.display()))?;
        println!("CI actuals summary appended to {}", path.display());
    }

    Ok(())
}

fn ci_actuals_receipt(root: &Path, args: &CiActualsArgs) -> Result<CiActualsReceipt> {
    let needs_path = resolve_path(root, &args.needs);
    let needs: BTreeMap<String, NeedEntry> = read_json(&needs_path)?;
    let timings = match &args.timings {
        Some(path) => read_timings(&resolve_path(root, path))?,
        None => BTreeMap::new(),
    };

    let mut used_timings = BTreeSet::new();
    let mut missing_timing = Vec::new();
    let mut timed_job_count = 0;
    let mut jobs = Vec::new();

    for (name, need) in needs {
        let timing = timings.get(&name);
        if timing.is_some() {
            used_timings.insert(name.clone());
        }

        let result = need.result.unwrap_or_else(|| "unknown".to_string());
        let selected = job_selected(&result);
        let route_target = output_string(
            &need.outputs,
            &[
                "route_target",
                "route-target",
                "selected_target",
                "selected-target",
                "runner_target",
                "runner-target",
            ],
        );
        let skip_reason = job_skip_reason(&result, &need.outputs, selected);
        let estimated_lem = output_f64(
            &need.outputs,
            &[
                "estimated_lem",
                "estimated-lem",
                "lem_estimate",
                "lem-estimate",
            ],
        );
        let queue_seconds = timing
            .and_then(|record| record.queue_seconds)
            .or_else(|| output_f64(&need.outputs, &["queue_seconds", "queue-seconds"]));
        let actual_lem = timing
            .and_then(|record| record.actual_lem)
            .or_else(|| output_f64(&need.outputs, &["actual_lem", "actual-lem"]));
        let estimate_source = output_string(
            &need.outputs,
            &[
                "estimate_source",
                "estimate-source",
                "lem_estimate_source",
                "lem-estimate-source",
            ],
        );
        let duration_seconds = timing.and_then(|record| record.duration_seconds);
        let duration_minutes = duration_seconds.map(|seconds| seconds / 60.0);
        if duration_seconds.is_none() {
            missing_timing.push(name.clone());
        } else {
            timed_job_count += 1;
        }

        let output_keys = need.outputs.keys().cloned().collect::<Vec<_>>();
        let lane_id = canonical_lane_id(&name);
        jobs.push(CiJobActual {
            name: name.clone(),
            summary_key: name.clone(),
            lane_id,
            aliases: actual_lane_keys(&name),
            selected,
            result,
            route_target,
            skip_reason,
            estimated_lem,
            actual_lem,
            queue_seconds,
            estimate_source,
            output_keys,
            runner: timing.and_then(|record| record.runner.clone()),
            duration_seconds,
            duration_minutes,
            timing_status: if duration_seconds.is_some() {
                "measured".to_string()
            } else {
                "missing".to_string()
            },
            cache_hit: timing.and_then(|record| record.cache_hit),
        });
    }

    jobs.sort_by(|left, right| left.name.cmp(&right.name));
    missing_timing.sort();

    let mut unused_timing = timings
        .keys()
        .filter(|name| !used_timings.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    unused_timing.sort();
    let job_count = jobs.len();

    Ok(CiActualsReceipt {
        schema: CI_ACTUALS_SCHEMA.to_string(),
        schema_version: 3,
        repo: args.repo.clone(),
        workflow: args.workflow.clone(),
        sha: receipt_sha(args),
        github: GithubContext {
            run_id: env_non_empty("GITHUB_RUN_ID"),
            run_attempt: env_non_empty("GITHUB_RUN_ATTEMPT"),
            event_name: env_non_empty("GITHUB_EVENT_NAME"),
            ref_name: env_non_empty("GITHUB_REF_NAME").or_else(|| env_non_empty("GITHUB_REF")),
        },
        jobs,
        status: CiActualsStatus {
            ok: true,
            job_count,
            timed_job_count,
            missing_timing,
            unused_timing,
        },
    })
}

fn render_step_summary(receipt: &CiActualsReceipt) -> String {
    let mut out = String::new();
    out.push_str("\n## CI Actuals (advisory)\n\n");
    out.push_str(&format!(
        "- jobs observed: **{}** ({} timed)\n",
        receipt.status.job_count, receipt.status.timed_job_count
    ));
    if !receipt.status.missing_timing.is_empty() {
        out.push_str(&format!(
            "- missing timing: {}\n",
            receipt.status.missing_timing.join(", ")
        ));
    }
    if !receipt.status.unused_timing.is_empty() {
        out.push_str(&format!(
            "- unused timing: {}\n",
            receipt.status.unused_timing.join(", ")
        ));
    }
    out.push_str(
        "- advisory only: this summary does not change required status or proof selection\n\n",
    );

    out.push_str("| Lane | Result | Selected | Expected LEM | Actual LEM | Duration | Queue | Route | Learned estimate |\n");
    out.push_str("|------|--------|----------|-------------:|-----------:|---------:|------:|-------|------------------|\n");
    for job in &receipt.jobs {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            table_cell(&job.lane_id),
            table_cell(&job.result),
            if job.selected { "yes" } else { "no" },
            format_optional_number(job.estimated_lem),
            format_optional_number(job.actual_lem),
            format_optional_seconds(job.duration_seconds),
            format_optional_seconds(job.queue_seconds),
            format_optional_text(job.route_target.as_deref()),
            learned_estimate_label(job.estimate_source.as_deref()),
        ));
        if let Some(reason) = &job.skip_reason {
            out.push_str(&format!(
                "| `{}` skip reason |  |  |  |  |  |  |  | {} |\n",
                table_cell(&job.summary_key),
                table_cell(reason)
            ));
        }
    }

    out
}

fn learned_estimate_label(source: Option<&str>) -> String {
    match source {
        Some(source) if source.starts_with("learned-") => format!("yes (`{}`)", table_cell(source)),
        Some(source) => format!("no (`{}`)", table_cell(source)),
        None => "unknown".to_string(),
    }
}

fn format_optional_number(value: Option<f64>) -> String {
    value
        .map(format_number)
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_optional_seconds(value: Option<f64>) -> String {
    value
        .map(|seconds| format!("{}s", format_number(seconds)))
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_optional_text(value: Option<&str>) -> String {
    value
        .map(table_cell)
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_number(value: f64) -> String {
    let formatted = format!("{value:.2}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn table_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '|' => escaped.push_str("\\|"),
            '\r' | '\n' => escaped.push(' '),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let body = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("parse json {}", path.display()))
}

fn read_timings(path: &Path) -> Result<BTreeMap<String, TimingRecord>> {
    let inputs: BTreeMap<String, TimingInput> = read_json(path)?;
    inputs
        .into_iter()
        .map(|(name, input)| {
            let record = timing_record(input)
                .with_context(|| format!("invalid timing entry `{name}` in {}", path.display()))?;
            Ok((name, record))
        })
        .collect()
}

fn timing_record(input: TimingInput) -> Result<TimingRecord> {
    let record = match input {
        TimingInput::Seconds(seconds) => TimingRecord {
            duration_seconds: Some(seconds),
            queue_seconds: None,
            actual_lem: None,
            runner: None,
            cache_hit: None,
        },
        TimingInput::Object(object) => TimingRecord {
            duration_seconds: object.duration_seconds.or(object.seconds),
            queue_seconds: object.queue_seconds,
            actual_lem: object.actual_lem,
            runner: object.runner,
            cache_hit: object.cache_hit,
        },
    };

    validate_non_negative("duration_seconds", record.duration_seconds)?;
    validate_non_negative("queue_seconds", record.queue_seconds)?;
    validate_non_negative("actual_lem", record.actual_lem)?;

    Ok(record)
}

fn canonical_lane_id(name: &str) -> String {
    ci_needs_key_lane_alias(name)
        .map(str::to_string)
        .unwrap_or_else(|| name.replace('-', "_"))
}

fn job_selected(result: &str) -> bool {
    !matches!(result.to_ascii_lowercase().as_str(), "skipped" | "unknown")
}

fn job_skip_reason(
    result: &str,
    outputs: &BTreeMap<String, Value>,
    selected: bool,
) -> Option<String> {
    if selected {
        return None;
    }

    output_string(
        outputs,
        &[
            "skip_reason",
            "skip-reason",
            "skipped_reason",
            "skipped-reason",
            "reason",
        ],
    )
    .or_else(|| {
        if result.eq_ignore_ascii_case("skipped") {
            Some("github_actions_condition_false".to_string())
        } else {
            Some("missing_needs_result".to_string())
        }
    })
}

fn output_string(outputs: &BTreeMap<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        outputs
            .get(*key)
            .and_then(value_to_string)
            .filter(|value| !value.is_empty())
    })
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn output_f64(outputs: &BTreeMap<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| outputs.get(*key).and_then(value_to_f64))
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite() && *value >= 0.0)
}

fn validate_non_negative(field: &str, value: Option<f64>) -> Result<()> {
    if let Some(value) = value
        && (!value.is_finite() || value < 0.0)
    {
        bail!("{field} must be a finite non-negative number");
    }

    Ok(())
}

fn receipt_sha(args: &CiActualsArgs) -> String {
    args.sha
        .clone()
        .or_else(|| env_non_empty("GITHUB_SHA"))
        .unwrap_or_else(|| "HEAD".to_string())
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn workspace_root() -> Result<PathBuf> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("locate workspace root")?;
    Ok(metadata.workspace_root.into_std_path_buf())
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_json(path: &Path, value: &serde_json::Value) {
        let body = serde_json::to_string_pretty(value).expect("serialize json fixture");
        fs::write(path, body).expect("write json fixture");
    }

    #[test]
    fn receipt_preserves_missing_timing_as_missing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let needs = temp.path().join("needs.json");
        write_json(
            &needs,
            &serde_json::json!({
                "docs-check": {"result": "success", "outputs": {"docs": "ok"}},
                "mutation": {"result": "skipped", "outputs": {}}
            }),
        );

        let args = CiActualsArgs {
            needs,
            output: temp.path().join("out.json"),
            sha: Some("abc123".to_string()),
            ..CiActualsArgs::default()
        };

        let receipt = ci_actuals_receipt(Path::new("."), &args).expect("receipt");
        assert_eq!(receipt.schema, CI_ACTUALS_SCHEMA);
        assert_eq!(receipt.sha, "abc123");
        assert_eq!(receipt.status.job_count, 2);
        assert_eq!(receipt.status.timed_job_count, 0);
        assert_eq!(receipt.status.missing_timing, ["docs-check", "mutation"]);
        assert_eq!(receipt.jobs[0].summary_key, "docs-check");
        assert_eq!(receipt.jobs[0].lane_id, "docs_check");
        assert!(receipt.jobs[0].selected);
        assert_eq!(receipt.jobs[1].summary_key, "mutation");
        assert_eq!(receipt.jobs[1].lane_id, "mutation_required");
        assert_eq!(receipt.jobs[1].aliases, ["mutation", "mutation_required"]);
        assert!(!receipt.jobs[1].selected);
        assert_eq!(
            receipt.jobs[1].skip_reason.as_deref(),
            Some("github_actions_condition_false")
        );
        assert!(
            receipt
                .jobs
                .iter()
                .all(|job| job.duration_seconds.is_none())
        );
        assert!(
            receipt
                .jobs
                .iter()
                .all(|job| job.timing_status == "missing")
        );
    }

    #[test]
    fn receipt_merges_timing_sidecar_and_sorts_jobs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let needs = temp.path().join("needs.json");
        let timings = temp.path().join("timings.json");
        write_json(
            &needs,
            &serde_json::json!({
                "z-build": {"result": "success", "outputs": {}},
                "a-docs": {
                    "result": "success",
                    "outputs": {
                        "cache-hit": "true",
                        "route_target": "hosted",
                        "estimated_lem": "3.5",
                        "estimate_source": "learned-p50"
                    }
                }
            }),
        );
        write_json(
            &timings,
            &serde_json::json!({
                "a-docs": {
                    "duration_seconds": 90.0,
                    "queue_seconds": 4.0,
                    "actual_lem": 2.0,
                    "runner": "ubuntu-latest",
                    "cache_hit": true
                },
                "unused": 12.0,
                "z-build": 120.0
            }),
        );

        let args = CiActualsArgs {
            needs,
            timings: Some(timings),
            output: temp.path().join("out.json"),
            ..CiActualsArgs::default()
        };

        let receipt = ci_actuals_receipt(Path::new("."), &args).expect("receipt");
        assert_eq!(receipt.status.job_count, 2);
        assert_eq!(receipt.status.timed_job_count, 2);
        assert_eq!(receipt.status.unused_timing, ["unused"]);
        assert_eq!(receipt.jobs[0].name, "a-docs");
        assert_eq!(receipt.jobs[0].duration_seconds, Some(90.0));
        assert_eq!(receipt.jobs[0].duration_minutes, Some(1.5));
        assert_eq!(receipt.jobs[0].route_target.as_deref(), Some("hosted"));
        assert_eq!(receipt.jobs[0].estimated_lem, Some(3.5));
        assert_eq!(receipt.jobs[0].actual_lem, Some(2.0));
        assert_eq!(receipt.jobs[0].queue_seconds, Some(4.0));
        assert_eq!(
            receipt.jobs[0].estimate_source.as_deref(),
            Some("learned-p50")
        );
        assert_eq!(receipt.jobs[0].runner.as_deref(), Some("ubuntu-latest"));
        assert_eq!(receipt.jobs[0].cache_hit, Some(true));
        assert_eq!(receipt.jobs[1].name, "z-build");
    }

    #[test]
    fn negative_timing_is_rejected() {
        let result = timing_record(TimingInput::Seconds(-1.0));
        assert!(result.is_err());
    }

    #[test]
    fn step_summary_shows_lane_actuals_and_skip_reasons() {
        let receipt = CiActualsReceipt {
            schema: CI_ACTUALS_SCHEMA.to_string(),
            schema_version: 3,
            repo: "tokmd".to_string(),
            workflow: "CI".to_string(),
            sha: "abc123".to_string(),
            github: GithubContext {
                run_id: None,
                run_attempt: None,
                event_name: None,
                ref_name: None,
            },
            jobs: vec![
                CiJobActual {
                    name: "build".to_string(),
                    summary_key: "build".to_string(),
                    lane_id: "build_test_linux".to_string(),
                    aliases: vec![],
                    selected: true,
                    result: "success".to_string(),
                    route_target: Some("hosted".to_string()),
                    skip_reason: None,
                    estimated_lem: Some(8.0),
                    actual_lem: Some(13.25),
                    queue_seconds: Some(6.0),
                    estimate_source: Some("learned-p50".to_string()),
                    output_keys: vec![],
                    runner: Some("ubuntu-latest".to_string()),
                    duration_seconds: Some(123.4),
                    duration_minutes: Some(2.056),
                    timing_status: "measured".to_string(),
                    cache_hit: None,
                },
                CiJobActual {
                    name: "mutation".to_string(),
                    summary_key: "mutation".to_string(),
                    lane_id: "mutation_required".to_string(),
                    aliases: vec![],
                    selected: false,
                    result: "skipped".to_string(),
                    route_target: None,
                    skip_reason: Some("not_selected_by_policy".to_string()),
                    estimated_lem: None,
                    actual_lem: None,
                    queue_seconds: None,
                    estimate_source: None,
                    output_keys: vec![],
                    runner: None,
                    duration_seconds: None,
                    duration_minutes: None,
                    timing_status: "missing".to_string(),
                    cache_hit: None,
                },
            ],
            status: CiActualsStatus {
                ok: true,
                job_count: 2,
                timed_job_count: 1,
                missing_timing: vec!["mutation".to_string()],
                unused_timing: vec![],
            },
        };

        let summary = render_step_summary(&receipt);

        assert!(summary.contains("## CI Actuals (advisory)"), "{summary}");
        assert!(
            summary.contains("| `build_test_linux` | `success` | yes | 8 | 13.25 | 123.4s | 6s | hosted | yes (`learned-p50`) |"),
            "{summary}"
        );
        assert!(
            summary.contains(
                "| `mutation` skip reason |  |  |  |  |  |  |  | not_selected_by_policy |"
            ),
            "{summary}"
        );
    }
}
