# Spec: Terminal release preflight receipt

- Status: active
- Schema family: `tokmd.release_preflight.v2`
- Related ADRs: none
- Related proof scopes: `release_metadata`
- Tracking: issue #527

## Contract

The Rust-owned `release-preflight` command validates immutable release
identity, normalizes the required command set, and writes one terminal
decision receipt. It does not execute release commands or publish any artifact.

The aggregate is `passed` only when every required command is `passed`.
Missing, failed, cancelled, unavailable, or not-run commands remain terminal
non-passing outcomes. Input validation reports all empty, unknown, and
duplicate command IDs together so a failed preflight gives operators one
complete repair list.

## Inputs

`cargo xtask release-preflight` consumes JSON with schema
`tokmd.release_preflight_input.v2`. The input records:

- an immutable 40- or 64-character `source_sha`;
- an immutable 40- or 64-character `affected_base_sha`;
- a non-empty expected version;
- `release_kind` (`rc` or `stable`); and
- command results for the required IDs:
  `affected_plan`, `proof_plan`, `fmt_check`, `gate_check`, `version_consistency`,
  `publish_surface`, `doc_artifacts`, `docs_check`, `proof_policy`,
  `no_panic`, `workspace_tests`, `clippy`, `cargo_deny`, `publish_dry_run`,
  `browser_tests`, and `browser_wasm_archive`.

## Outputs

The command writes `tokmd.release_preflight.v2`, containing the exact source
and affected-base identities, expected version, release kind, aggregate
status, and required command observations. Missing required results are
materialized as `not_run`, so an incomplete input cannot pass.

The receipt is evidence about the exact source/base identity and command
observations. It does not prove that a release was published, tagged, or
promoted.

## Compatibility

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

## Proof Requirements

- fixture tests cover complete pass, missing required command, failed versus
  unavailable precedence, mixed input validation, and invalid identity;
- `affected_plan` runs `cargo xtask affected` against the immutable base/head
  and requires zero unknown files, while `proof_plan` separately runs
  `cargo xtask proof --profile affected --plan` for that same identity;
- `cargo fmt-check` and `cargo test -p xtask release_preflight` run against the
  exact committed source when Cargo is available;
- `.github/workflows/release-preflight.yml` passes actionlint and records a
  terminal result, duration, and log path for every required command;
- the workflow checks out exact source/base SHAs, uploads evidence under
  `always()`, and fails closed when the receipt is missing or non-passing; and
- no release publication, tagging, alias promotion, or consumer-smoke
  duplication belongs to this contract.

## Open Questions

- Which RC/stable preparation workflow should become the first caller of the
  reusable preflight after this standalone manual lane is hosted-green.
