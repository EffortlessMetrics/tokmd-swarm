# Spec: Terminal release preflight receipt

- Status: active
- Schema family: `tokmd.release_preflight.v1`
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
`tokmd.release_preflight_input.v1`. The input records:

- an immutable 40- or 64-character `source_sha`;
- an immutable 40- or 64-character `affected_base_sha`;
- a non-empty expected version;
- `release_kind` (`rc` or `stable`); and
- command results for the required IDs:
  `affected_plan`, `fmt_check`, `gate_check`, `version_consistency`,
  `publish_surface`, `doc_artifacts`, `docs_check`, `proof_policy`,
  `no_panic`, `workspace_tests`, `clippy`, `cargo_deny`, `publish_dry_run`,
  `browser_tests`, and `browser_wasm_archive`.

## Outputs

The command writes `tokmd.release_preflight.v1`, containing the exact source
and affected-base identities, expected version, release kind, aggregate
status, and required command observations. Missing required results are
materialized as `not_run`, so an incomplete input cannot pass.

The receipt is evidence about the exact source/base identity and command
observations. It does not prove that a release was published, tagged, or
promoted.

## Compatibility

The workflow boundary owns command execution, log/artifact collection,
timeouts, and input assembly. The reusable/manual workflow must invoke this
aggregator and consume its receipt for go/no-go instead of reinterpreting raw
logs. Existing consumer-smoke receipts remain authoritative for
released-artifact consumer proof.

## Proof Requirements

- fixture tests cover complete pass, missing required command, failed versus
  unavailable precedence, mixed input validation, and invalid identity;
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
