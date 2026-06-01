# CI lane whitelist

The lane whitelist is the single map from CI item → purpose, cost,
trigger, owner, and evidence. The TOML lives at
`policy/ci-lane-whitelist.toml`. This doc is the human-facing pointer.

## Schema

Each `[[lane]]` entry records:

| Field | Required | Meaning |
|-------|----------|---------|
| `id` | yes | Stable identifier; referenced by PR Plan, exceptions, and the linter. |
| `workflow` | yes | The `.github/workflows/*.yml` file the lane lives in. |
| `job` | yes | The job's `name:` in the workflow. |
| `kind` | yes | `rust` / `policy` / `mutation` / `wasm` / `packaging` / `release` / `lint` / `summary` / etc. |
| `tier` | yes | `frontdoor` (cheap default) / `risk-gated` (default with risk pack) / `deep` (label/main/nightly) / `summary`. |
| `default_pr` | yes | Whether this lane runs on ordinary PRs today. |
| `blocking` | yes | Whether failure currently blocks merge. |
| `runner` | yes | Runner label for LEM multiplier; `mixed` for matrix jobs. |
| `base_lem` | yes | Static LEM floor used until actuals exist. |
| `owner` | yes | Team/role that owns the lane. |
| `intent` | yes | What the lane is *for*. |
| `failure_mode` | yes | What slips through if the lane is removed. |
| `proof_obligation` | yes | The literal commands or evidence the lane must produce. |
| `evidence` | for blocking | Concrete artifact / log entries that demonstrate the obligation was met. |
| `allowed_triggers` | yes | `pull_request` / `push` / `schedule` / `workflow_dispatch`. |
| `expensive` | optional | `true` if the lane needs a default-PR exception. |
| `default_pr_exception` | optional | ID in `policy/ci-whitelist-exceptions.toml` that allows the expensive default. |
| `duplicate_of` | optional | List of lane IDs that already cover this surface. `future:*` references are permitted to forward-declare. |
| `review_after` / `expires` | yes | Calendar dates — when to re-review and when the entry must be re-justified. |

## Exceptions

`policy/ci-whitelist-exceptions.toml` holds the carve-outs needed during
the rollout. Each exception names the lane it covers, why it is allowed,
and when it must be reviewed and retired.

## Lifecycle

1. New lane added → entry in `ci-lane-whitelist.toml` + (if expensive
   default) entry in `ci-whitelist-exceptions.toml`.
2. CI runs `cargo xtask ci-lane-whitelist --strict` so workflow jobs must
   match whitelist metadata and active exceptions must stay valid.
3. Local advisory runs can omit `--strict` to write a report without failing
   on findings while a narrow follow-up is being prepared.
4. Exceptions never extend silently — `expires` is enforced in strict mode.

## Budget

```toml
[budget]
preferred_default_lem = 25
default_limit_lem     = 35
elevated_limit_lem    = 75
hard_limit_lem        = 125
```

These are the bands the PR Plan job and budget guard read.
