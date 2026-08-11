# Terminal release preflight receipt

Issue #527 owns the hosted release-preflight decision. The Rust aggregator is
the contract-bearing part of that workflow; it does not execute release
commands or publish any artifact.

## Input

`cargo xtask release-preflight` consumes a JSON input with schema
`tokmd.release_preflight_input.v1`. The input records:

- an immutable 40- or 64-character `source_sha`;
- an immutable 40- or 64-character `affected_base_sha`;
- the non-empty expected version;
- `release_kind` (`rc` or `stable`); and
- one result record per executed required command.

The required command IDs are stable and include affected planning, formatting,
the Rust gate, release metadata, documentation, policy, tests, lint, deny,
publish dry-run, and browser archive proof.

## Decision

The command writes `tokmd.release_preflight.v1`. Missing required results are
materialized as `not_run`, so an incomplete input cannot pass. The aggregate is
`passed` only when every required command is `passed`; `failed`, `cancelled`,
`unavailable`, and `not_run` remain terminal non-passing outcomes. A failed
command takes precedence over unavailable or not-run results.

The receipt is evidence about the exact source/base identity and command
observations. It does not prove that a release was published, tagged, or
promoted.

## Workflow boundary

`.github/workflows/release-preflight.yml` owns command execution, log/artifact
collection, 45-minute per-command bounds, exact-SHA checkout, and input
assembly. It invokes this aggregator and consumes its receipt for go/no-go
instead of reinterpreting raw logs. The workflow is reusable and manually
dispatchable, but is not a default PR lane. Existing consumer smoke receipts
remain authoritative for released-artifact consumer proof.

The workflow records one terminal result for every required command and uploads
the input, logs, affected-plan output, and decision receipt with `always()`. A
missing input or receipt is itself a failed workflow outcome; it is never
treated as a successful release preflight.
