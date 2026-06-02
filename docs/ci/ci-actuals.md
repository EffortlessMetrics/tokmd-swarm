# CI Actuals Receipt

`cargo xtask ci-actuals` emits `tokmd.ci_actuals.v3`, a small receipt for
GitHub Actions job results, stable lane identity, and optional measured
durations.

The command is intentionally receipt-only. It does not decide budget policy,
change required gates, or promote skipped lanes. Hosted PR Plan can consume
recent successful `main` receipts as a best-effort cache for advisory learned
estimates; when that cache is unavailable, static `base_lem` estimates remain
the fallback.

## Inputs

```bash
cargo xtask ci-actuals \
  --needs target/ci/needs.json \
  --timings target/ci/timings.json \
  --output target/ci/ci-actuals.json \
  --github-summary "$GITHUB_STEP_SUMMARY"
```

- `--needs` reads the literal `${{ toJson(needs) }}` payload from an aggregate
  GitHub Actions job.
- `--timings` is optional. It may contain either `{ "job-id": 12.5 }` or
  `{ "job-id": { "duration_seconds": 12.5, "runner": "ubuntu-latest" } }`.
  Timing objects may also include `queue_seconds` and `actual_lem` when a
  workflow can observe them.
- `--output` writes the receipt path, creating parent directories as needed.
- `--github-summary` is optional. It appends a compact Markdown table for
  human workflow output without changing the JSON receipt or required gate.

When timing data is absent, the receipt records `timing_status: "missing"` and
leaves duration fields `null`. Missing timing is not coerced to zero.

## CI Workflow Artifact

The `CI (Required)` aggregate job writes the raw needs payload to
`target/ci/needs.json`, attempts to write hosted job durations to
`target/ci/timings.json`, then writes `target/ci/ci-actuals.json` with
`cargo xtask ci-actuals`. The job uploads the raw needs payload, any available
timing sidecar, and the final receipt as the `ci-actuals` artifact before the
aggregate job performs its final pass/fail status check.

The same step also appends a `CI Actuals (advisory)` table to the workflow
summary. The table names each canonical lane, the observed result, whether the
job was selected, expected and actual LEM when available, duration, queue time,
route target, whether a learned estimate source was observed, and explicit skip
reasons. It is a reader aid over `ci-actuals.json`; the artifact remains the
machine-readable source of truth.

The uploaded receipt is observation-only. It does not change required-status
selection, make learned estimates gating, or promote skipped lanes into passing
evidence. PR Plan may later read recent successful `main` `ci-actuals`
artifacts with `--actuals-dir` to report advisory learned estimate sources, but
final pass/fail status remains owned by the aggregate status check over the
original `needs` payload. The aggregate job attempts receipt setup, generation,
and upload as best-effort telemetry. Hosted timing
collection uses the read-only GitHub Actions jobs API for the current run
attempt, maps successful job display names back to aggregate `needs` keys, and
records the first hosted runner label when GitHub exposes one. If that API
lookup fails, or if a skipped, failed, cancelled, or incomplete job has no
successful timing sample, the receipt still records `timing_status: "missing"`
rather than inventing a duration.

Each job row keeps the GitHub Actions aggregate `needs` key in `name` and
`summary_key`, then records a canonical `lane_id` and `aliases` array for later
planner lookup. For example, `build` maps to `build_test_linux`, `mutation` maps
to `mutation_required`, and hyphenated keys such as `docs-check` also expose
their underscore-normalized alias.

`selected` is execution telemetry derived from the aggregate job result:
`success`, `failure`, and `cancelled` jobs are selected; `skipped` jobs are not.
Skipped rows record `skip_reason` from an explicit job output when present, or
`github_actions_condition_false` when GitHub only reports a skipped condition.
This is an execution skip reason, not proof-policy authorization.

`route_target`, `estimated_lem`, `actual_lem`, and `queue_seconds` are nullable.
They are populated only when the aggregate job outputs or timing sidecar provide
them. Missing values mean the workflow did not observe that datum.
`estimate_source` is also nullable and records whether the corresponding
planner estimate came from a static floor or an advisory learned estimate when
the aggregate job exposed that source. Missing values mean the workflow did not
observe the planner estimate source.

## Output

```json
{
  "schema": "tokmd.ci_actuals.v3",
  "schema_version": 3,
  "repo": "tokmd",
  "workflow": "CI",
  "sha": "<commit>",
  "jobs": [
    {
      "name": "docs-check",
      "summary_key": "docs-check",
      "lane_id": "docs_check",
      "aliases": ["docs-check", "docs_check"],
      "selected": true,
      "result": "success",
      "route_target": "hosted",
      "skip_reason": null,
      "estimated_lem": 3.0,
      "actual_lem": 1.25,
      "queue_seconds": null,
      "estimate_source": "static",
      "output_keys": [],
      "runner": "ubuntu-latest",
      "duration_seconds": 75.0,
      "duration_minutes": 1.25,
      "timing_status": "measured",
      "cache_hit": true
    }
  ],
  "status": {
    "ok": true,
    "job_count": 1,
    "timed_job_count": 1,
    "missing_timing": [],
    "unused_timing": []
  }
}
```

The receipt is sorted by job name so downstream summaries remain stable.
