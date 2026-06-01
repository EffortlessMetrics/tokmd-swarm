# Spec: Coverage Evidence

- Status: active
- Schema family, if any: `tokmd.coverage_receipt.v1`,
  `tokmd.coverage_workflow_status.v1`; scoped executor artifacts use existing
  proof executor receipt families
- Related ADRs: `docs/adr/0009-proof-observation-promotion-boundary.md`
- Related proof scopes: `proof_control_plane`, `project_truth_docs`

## Contract

Coverage evidence is advisory execution-surface telemetry. It can show that a
set of tests executed Rust code and produced coverage artifacts. It must not be
read as proof that assertions are strong, mutation adequacy is high, required
proof passed, browser or WASM behavior works, release packaging is valid, or a
PR can merge.

Coverage evidence has two current producers:

- the `Coverage` workflow, which runs whole-workspace `cargo-llvm-cov` and can
  upload the `rust` flag to Codecov when a token is configured;
- the `Proof Executor Experiment`, which runs planner-selected scoped coverage
  commands as non-required proof evidence and can upload scoped LCOV files to
  Codecov only from an explicit manual dispatch.

Both producers are visibility surfaces. Neither producer promotes coverage into
the required CI aggregate, enables default Codecov upload, creates release
evidence, changes public `tokmd` CLI behavior, or replaces the affected proof
plan, proof policy, cargo-deny, mutation, Nix, publish-surface, or release
lanes.

Coverage may inform future proof-promotion decisions only through an explicit
maintainer decision backed by fresh observation evidence and a deliberate policy
change. Until then, coverage remains advisory.

## Inputs

Coverage evidence is derived from checked repository state and explicit workflow
inputs:

| Input | Owner | Used for |
| --- | --- | --- |
| `.github/workflows/coverage.yml` | GitHub Actions workflow | Whole-workspace coverage generation, artifact upload, and optional Codecov upload behavior. |
| `.github/workflows/proof-executor.yml` | GitHub Actions workflow | Planner-selected scoped coverage execution, proof executor receipts, and manual scoped Codecov upload. |
| `codecov.yml` | Codecov policy | Informational status settings, thresholds, disabled comments, disabled annotations, and ignored paths. |
| `ci/proof.toml` `[executor.pr]` | Proof policy | PR executor default-enabled, non-required, max-command, and Codecov-off defaults. |
| `ci/proof.toml` `[executor.promotion]` | Proof policy | Observation-window and promotion-floor declarations. |
| `cargo xtask proof-policy --json-output <path>` | Proof policy command | Machine-readable source for executor policy resolution and collector thresholds. |
| `cargo xtask affected ...` and `cargo xtask proof --profile affected ...` | Proof planner | Selection of scoped coverage commands and associated proof evidence. |
| `CODECOV_TOKEN` | GitHub secret | Optional upload credential for the whole-workspace coverage workflow. |
| `workflow_dispatch upload_codecov=true` | Manual operator input | Explicit scoped executor Codecov upload request. |

Inputs must be checked or explicit. Coverage conclusions must not depend on
operator memory, hidden local files, unrecorded dashboard state, or a Codecov
badge alone.

## Outputs

Current coverage outputs are:

| Output | Producer | Means | Does not mean |
| --- | --- | --- | --- |
| `target/coverage/coverage-status.json` | Coverage workflow | Coverage command status receipt with `cargo-llvm-cov` exit codes, skipped report steps, and observed coverage artifact presence, uploaded before the workflow re-raises a command failure. | It does not prove coverage reports are complete, sufficient, uploaded to Codecov, or required. |
| `coverage.json` | Coverage workflow | `cargo-llvm-cov` JSON report exists for the workspace run. | It does not prove assertion quality or merge readiness. |
| `coverage.txt` | Coverage workflow | Human-readable coverage report exists for the workspace run. | It is not a required proof verdict. |
| `lcov.info` | Coverage workflow | LCOV report exists and may be uploaded to Codecov. | It does not imply Codecov accepted the upload. |
| `target/coverage/coverage-receipt.json` | `cargo xtask coverage-receipt` | Non-empty coverage artifacts were checked and recorded as `tokmd.coverage_receipt.v1`. | It does not validate semantic adequacy of tests. |
| `coverage-report` artifact | Coverage workflow | The coverage reports and receipt were uploaded for review. | It is not release evidence. |
| `target/proof/executor-summary.json` | Proof executor | Selected scoped coverage commands executed or were skipped with explicit status. | It does not make scoped coverage required. |
| `target/proof/executor-manifest.json` | Proof executor | Scoped coverage artifact inventory. | It does not replace source proof artifacts. |
| `target/proof/proof-execution-artifacts-check.json` | Proof executor verifier | Executor artifacts are internally checkable. | It does not approve the PR. |
| `target/proof/proof-executor-observation*.json` | Proof executor observation | Compact observation evidence for later aggregation. | It does not promote coverage. |
| Codecov project or patch status | Codecov | Informational dashboard signal under `codecov.yml`. | It is not a blocking GitHub status. |

`coverage-status.json` owns workflow-step arbitration for the whole-workspace
coverage workflow. It is available even when coverage command failure prevents
`coverage-receipt.json` from being generated.

`coverage-receipt.json` owns only artifact-presence and metadata facts for
successful whole-workspace coverage reports. It is not a wrapper for all
coverage evidence and does not replace scoped executor receipts.

## Compatibility

This spec does not change workflow triggers, Codecov configuration, proof
policy, required checks, artifact names, public `tokmd` CLI behavior, receipt
schemas, release behavior, or upload defaults.

Existing artifacts remain authoritative for their own domains:

- `coverage-status.json` owns whole-workspace coverage workflow command status;
- `coverage-receipt.json` owns whole-workspace coverage artifact presence;
- `executor-summary.json` and `executor-manifest.json` own scoped coverage
  execution and artifact inventory;
- `proof-execution-artifacts-check.json` owns executor artifact verification;
- `proof-workflow-status.json` owns workflow status arbitration for the scoped
  executor;
- `ci/proof.toml` owns executor defaults and promotion floors;
- `codecov.yml` owns Codecov status visibility.

Consumers must be able to ignore this spec and continue reading the existing
workflow artifacts directly.

## Proof Requirements

For documentation-only changes to this contract:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-coverage-evidence.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-coverage-evidence.json --evidence-json target/proof/proof-evidence-coverage-evidence.json
cargo fmt-check
git diff --check
```

For workflow or policy implementation changes, proof should also include the
focused current behavior:

- `cargo xtask proof-policy --check`;
- `cargo test -p xtask coverage_receipt --verbose`, if the whole-workspace
  coverage receipt changes;
- `cargo test -p xtask proof_executor_pr_policy --verbose`, if executor PR
  defaults change;
- `cargo test -p xtask proof_workflow_status --verbose`, if scoped executor
  status packets change;
- hosted observation that PR coverage remains label-gated and non-required;
- hosted observation that scoped executor Codecov upload remains manual-only;
- explicit maintainer decision evidence before any required-gate or default
  Codecov-upload promotion.

Any change that makes coverage required, changes Codecov upload defaults, or
changes Codecov status from informational to blocking must update this spec,
`docs/ci/coverage.md`, `ci/proof.toml`, and workflow behavior in the same
review.

## Open Questions

- Whether whole-workspace `coverage-receipt.json` should eventually link to
  scoped executor receipts or remain a separate workflow receipt.
- Whether cockpit or handoff should surface coverage evidence through direct
  coverage-specific summaries or only through imported proof artifacts.
- Whether Codecov dashboard state should ever be captured in a repo-owned
  receipt, or remain an external visibility surface.
