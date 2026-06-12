# Routed Rust Small dogfood

Status: live dogfood note for the routed Rust Small front door.

Date: 2026-06-12

## Scope

This note records what the routed Rust Small workflow proved during
`tokmd-swarm` PR #247, which added route telemetry to the normalized result
receipt.

It is a dogfood note, not a release gate and not a claim that every route mode
has been observed under real fleet conditions.

## Live PR run

PR:

```text
https://github.com/EffortlessMetrics/tokmd-swarm/pull/247
```

Routed Rust Small workflow:

```text
https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27394843448
```

CI aggregate workflow:

```text
https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27394843445
```

Branch protection API at the time of the run required only:

```text
Tokmd Rust Small Result
```

## Observed route

The live PR route selected GitHub-hosted execution before dispatching an
implementation job:

| Field | Observed value |
| --- | --- |
| Event | `pull_request` |
| Target | `github-hosted` |
| Reason | `self_hosted_capacity_full` |
| Router job | passed |
| Self-hosted implementation | skipped |
| GitHub-hosted implementation | passed |
| Normalized result | passed |
| CI aggregate | passed |

Downloaded `target/ci/routed-rust-small-result.json` from run
`27394843448` showed:

```json
{
  "router": {
    "target": "github-hosted",
    "reason": "self_hosted_capacity_full",
    "selected_runner_label": "ubuntu-24.04"
  },
  "selected": {
    "job": "rust-small-github",
    "result": "success"
  },
  "jobs": {
    "github": "success",
    "self_hosted": "skipped"
  },
  "telemetry": {
    "duration_seconds": 627.0,
    "queue_seconds": 1.0,
    "runner_group": "GitHub Actions",
    "runner_labels": ["ubuntu-latest"],
    "cache_note": "GitHub-hosted rust-cache restore only; PR runs do not save cache"
  }
}
```

This proves the important fallback invariant for the observed case:

```text
capacity-full route -> GitHub-hosted implementation runs
self-hosted implementation skips
Tokmd Rust Small Result normalizes the selected implementation result
```

## Observed timing

The selected GitHub-hosted implementation took about 10.45 minutes and reported
about 1 second of queue time. That timing is an observation for this run only.
It is not a service-level objective and should not be used as a learned lane
estimate without the normal CI actuals path.

The result receipt correctly preserved missing/available telemetry as
observation data instead of turning it into route authority.

## Manual proof runs

After the first PR dogfood run, the manual proof modes were dispatched
sequentially on `main` so workflow-level concurrency could not cancel an older
pending dispatch. Each run selected GitHub-hosted before implementation
dispatch, skipped the self-hosted implementation job, completed the selected
GitHub-hosted implementation, and passed `Tokmd Rust Small Result`.

| Mode | Run | Target | Reason | Self-hosted | GitHub-hosted | Result |
| --- | --- | --- | --- | --- | --- | --- |
| `simulate-full` | [`27396775371`](https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27396775371) | `github-hosted` | `self_hosted_capacity_full` | `skipped` | `success` | `success` |
| `simulate-unhealthy` | [`27397251268`](https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27397251268) | `github-hosted` | `runner_health_degraded` | `skipped` | `success` | `success` |
| `simulate-api-unavailable` | [`27397718187`](https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27397718187) | `github-hosted` | `runner_api_unavailable` | `skipped` | `success` | `success` |
| `simulate-untrusted` | [`27398214166`](https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27398214166) | `github-hosted` | `untrusted_event` | `skipped` | `success` | `success` |
| `force-github-hosted` | [`27398703674`](https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27398703674) | `github-hosted` | `manual_force_github_hosted` | `skipped` | `success` | `success` |

The downloaded `route-rust-small` and `routed-rust-small-result` artifacts for
those runs showed the expected route reason, selected job
`rust-small-github`, selected result `success`, and final status `success`.
The hosted implementation duration was about 10.5 to 10.9 minutes with 1 to 2
seconds of queue time in these observations.

## Cases not yet observed live

No routed Rust Small route class remains intentionally unobserved in this
dogfood note. The fallback proof modes above cover hosted fallback behavior;
the healthy self-hosted proof below covers the trusted idle-capacity path.

The workflow-contract tests cover the proof-mode wiring, and the route helper
tests cover the decision table. Those tests are not substitutes for live fleet
observations.

## Healthy self-hosted proof

PR:

```text
https://github.com/EffortlessMetrics/tokmd-swarm/pull/254
```

Routed Rust Small workflow:

```text
https://github.com/EffortlessMetrics/tokmd-swarm/actions/runs/27432612954
```

The live PR route selected self-hosted execution before dispatching an
implementation job:

| Field | Observed value |
| --- | --- |
| Event | `pull_request` |
| Target | `self-hosted` |
| Reason | `trusted_capacity_available` |
| Eligible runners | `7` |
| Busy runners | `2` |
| Healthy runners | `7` |
| Selected runner label | `em-ci-small` |
| Route-selected runner candidate | `em-ci-hel2-cpx42-rust-01` |
| Actual execution runner | `em-ci-hel2-cx53-rust-01` |
| Router job | passed |
| Self-hosted implementation | passed |
| GitHub-hosted implementation | skipped |
| Normalized result | passed |

Downloaded `target/ci/route-rust-small.json` from run `27432612954` showed:

```json
{
  "target": "self-hosted",
  "reason": "trusted_capacity_available",
  "eligible_runners": 7,
  "busy_runners": 2,
  "healthy_runners": 7,
  "selected_runner_label": "em-ci-small",
  "selected_runner": "em-ci-hel2-cpx42-rust-01",
  "warnings": [],
  "errors": []
}
```

Downloaded `target/ci/routed-rust-small-result.json` from the same run showed:

```json
{
  "router": {
    "target": "self-hosted",
    "reason": "trusted_capacity_available"
  },
  "selected": {
    "job": "rust-small-self-hosted",
    "result": "success"
  },
  "jobs": {
    "self_hosted": "success",
    "github": "skipped"
  },
  "telemetry": {
    "runner_name": "em-ci-hel2-cx53-rust-01",
    "runner_group": "em-ci-small",
    "runner_labels": ["self-hosted", "linux", "x64", "em-ci", "trusted-pr", "rust-small"],
    "duration_seconds": 497.0,
    "queue_seconds": 2.0,
    "cache_note": "self-hosted run-scoped Cargo home with scratch target cleanup"
  }
}
```

The route receipt's `selected_runner` is a pre-dispatch idle-runner candidate
from the runner API. GitHub still owns final self-hosted assignment for the
label/group match. The actual execution runner is recorded in
`routed-rust-small-result.json` telemetry for the selected implementation job.

This proves the healthy-capacity invariant for the observed case:

```text
trusted same-repo PR + healthy idle Rust Small runner
  -> self-hosted implementation runs
  -> GitHub-hosted implementation skips
  -> Tokmd Rust Small Result normalizes the selected implementation result
```

## Confusing points

- The route helper compiles `xtask` inside the GitHub-hosted router job. In the
  observed run, the router job took 1 minute 22 seconds. That is acceptable for
  the first routed lane but worth watching if route latency grows.
- The external `droid-review` workflow took 16 minutes 28 seconds. It did not
  block the routed result check, but it can keep PR rollup state `UNSTABLE`
  after required checks are green.
- GitHub emitted a Node.js 20 deprecation annotation for `oven-sh/setup-bun`.
  That warning is unrelated to routed Rust Small behavior.
- Before PR #254, the runner API adapter compared labels case-sensitively
  against `linux` and `x64`, while GitHub returned built-in labels as `Linux`
  and `X64`. It also did not require the `rust-small` lane label. PR #254
  normalized API labels and aligned the route predicate with the self-hosted
  dispatch labels.

## Follow-ups

- Keep branch protection pinned to `Tokmd Rust Small Result`; do not require
  the route or conditional implementation jobs directly.
- Watch router-job duration. If it becomes noisy, consider a narrower route
  helper execution path or cache strategy without changing the routing
  contract.

## Non-claims

This dogfood note does not prove:

- every future PR will select GitHub-hosted under capacity pressure;
- every future trusted PR will select self-hosted when a runner appears idle;
- all manual simulation modes have been executed live;
- route telemetry is a CI actuals source of truth;
- routed Rust Small behavior generalizes to release, publish, signing, full
  matrix, fuzz, mutation, Codecov, macOS, Windows, or secret-heavy lanes.
