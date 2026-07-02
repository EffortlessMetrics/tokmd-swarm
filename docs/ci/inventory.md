# tokmd CI inventory snapshot

Snapshot of CI lanes as of 2026-07-02, reconciled with
`policy/ci-lane-whitelist.toml` after the phase-2/3 single-tight-gate
consolidation (#226, #299). The whitelist is the source of truth; this table is
its human-readable companion. Update on rollout PRs.

Branch protection requires exactly one status check, `Tokmd Rust Result`
(`tokmd_rust_result`). Every other lane is advisory or risk-routed. The routed
Rust Small frontdoor (`Route Tokmd Rust Small`, `Tokmd Rust Small on Self
Hosted`/`GitHub Hosted`, `Tokmd Rust Small Result`) and the `CI (Required)`
aggregator were retired; their check/test work is now folded into the single
required gate, which runs on a runner chosen by the advisory `Route CI runner`
job. See `docs/ci/default-pr-gate.md` and `docs/specs/ub-review-ci-gate.md` for
the authoritative gate shape.

The `Blocking` column reflects the lane's `blocking` field in the whitelist
(whether a failure fails its own job); it does not mean the lane is a required
branch-protection context. Only `Tokmd Rust Result` is a required context.

## Default-PR lanes (`default_pr = true`)

These lanes run on ordinary pull requests.

| Lane ID | Job | Workflow | Runner | Base LEM | Blocking | Notes |
|---------|-----|----------|--------|----------|----------|-------|
| `route_ci_runner` | Route CI runner | `ci.yml` | ubuntu | 1 | no | Self-hosted primary vs GitHub-hosted overflow selector. |
| `tokmd_rust_result` | Tokmd Rust Result | `ci.yml` | mixed | 25 | yes | **Only required context.** Concurrent `gate --check`, `test --all-features`, `proof-policy --check`, advisory ub-review. |
| `msrv_check` | MSRV Check | `ci.yml` | ubuntu | 5 | yes | `cargo check` on the MSRV toolchain (1.95.0). |
| `ci_detect_risk_packs` | Detect risk packs | `ci.yml` | ubuntu | 1 | yes | Classifies changed paths into risk packs for conditional routing. |
| `affected_proof_plan` | Affected Proof Plan | `ci.yml` | ubuntu | 4 | yes | PR affected proof artifacts (`affected`, `proof --profile affected`). |
| `fast_proof_run_advisory` | Fast Proof Run (Advisory) | `ci.yml` | ubuntu | 5 | no | Advisory fast proof-run observation. |
| `feature_boundaries` | Feature Boundaries | `ci.yml` | ubuntu | 10 | yes | Analysis feature combinations + `boundaries-check`. |
| `typos` | Typos | `ci.yml` | ubuntu | 1 | yes | crate-ci/typos. |
| `cargo_deny` | Cargo Deny | `ci.yml` | ubuntu | 4 | yes | Advisories + licenses. |
| `version_consistency` | Version consistency | `ci.yml` | ubuntu | 2 | yes | Release metadata alignment. |
| `docs_check` | Docs Check | `ci.yml` | ubuntu | 4 | yes | `docs --check` + `doc-artifacts --check`. |
| `publish_surface` | Publish Surface | `ci.yml` | ubuntu | 8 | yes | `publish-surface --json --verify-publish`. |
| `ci_actuals_advisory` | CI Actuals (Advisory) | `ci.yml` | ubuntu | 1 | no | LEM timing + actuals receipts; not aggregated for branch protection. |
| `no_bare_self_hosted` | No Bare Self-Hosted Routing | `ci-policy.yml` | ubuntu | 1 | yes | Runner routing policy guard. |
| `ci_lane_whitelist` | CI Lane Whitelist | `ci-policy.yml` | ubuntu | 3 | no | Advisory lane-whitelist policy inventory. |
| `ci_gate_contract_reference` | CI Gate Contract (reference) | `ci-policy.yml` | ubuntu | 2 | yes | Reference gate fixture matches contract markers. |
| `ci_gate_contract_live_gap` | CI Gate Contract (live gap) | `ci-policy.yml` | ubuntu | 2 | yes | Live `ci.yml` keeps the single-tight gate contract shape. |
| `pr_cockpit_report` | PR Cockpit Report | `cockpit.yml` | ubuntu | 3 | no | PR cockpit metrics comment. |
| `no_panic_family` | No-panic Family | `no-panic-policy.yml` | ubuntu | 3 | yes | Panic-family allowlist + stale-entry policy. |
| `clippy_exceptions` | Clippy Exceptions | `clippy-exceptions-policy.yml` | ubuntu | 3 | yes | Clippy exception ledger + ledger-linked `expect`. |
| `pr_plan_advisory` | PR Plan (advisory) | `pr-plan.yml` | ubuntu | 1 | no | LEM-aware advisory PR plan. |
| `ripr_advisory` | ripr (advisory) | `ripr.yml` | ubuntu | 2 | no | Static oracle-gap signal on changed Rust files. |
| `scoped_coverage_executor_non_required` | Scoped Coverage Executor (Non-Required) | `proof-executor.yml` | ubuntu | 12 | no | Advisory proof executor on opted-in PR shapes. |

## Risk-gated / conditional lanes (`default_pr = false`)

Selected by push to `main`, an explicit label, or a matching path risk pack.

| Lane ID | Job | Workflow | Runner | Base LEM | Trigger |
|---------|-----|----------|--------|----------|---------|
| `build_test_windows` | Build & Test (Windows) | `ci.yml` | windows | 20 | push, `windows`, `full-ci`, or Windows path risk. |
| `wasm_compile_test` | Wasm Compile & Test | `ci.yml` | ubuntu | 25 | push, `wasm`, `full-ci`, or WASM path risk. |
| `proptest_smoke` | Proptest Smoke | `ci.yml` | ubuntu | 8 | push, `property-tests`, `full-ci`, or core-receipts/analysis path risk. |
| `nix_pr_package_gate` | Nix PR Package Gate | `ci.yml` | ubuntu | 35 | publication repo only; push, `nix`, `release-check`, `full-ci`, or release path risk. |
| `mutation_required` | Mutation Testing | `ci.yml` | ubuntu | 45 | push, `mutation`, or `full-ci`. |
| `bindings_parity` | Bindings Parity (advisory) | `bindings-parity.yml` | ubuntu | 8 | advisory bindings/FFI envelope parity. |

## Push / scheduled / dispatch lanes

| Lane ID | Job | Workflow | Runner | Base LEM | Trigger |
|---------|-----|----------|--------|----------|---------|
| `build_macos_push` | Build & Test (macOS) | `ci.yml` | macOS | 60 | push to `main` (10× runner multiplier). |
| `rust_coverage` | Codecov Coverage | `coverage.yml` | ubuntu | 30 | push, dispatch, `coverage`, or `full-ci`. |
| `collect_proof_executor_observations` | Collect Proof Executor Observations | `proof-observation-collection.yml` | ubuntu | 5 | dispatch, schedule. |
| `nightly_fuzz` | Fuzz Targets | `fuzz.yml` | ubuntu | 60 | schedule, dispatch. |
| `mutation_testing_calibration` | Mutation Testing | `mutants.yml` | ubuntu | 90 | dispatch, schedule. |
| `swarm_ghcr_publish` | Build and Push Swarm GHCR Image | `swarm-ghcr.yml` | ubuntu | 25 | push, dispatch. |
| `ghcr_container_smoke` | GHCR Container Smoke | `ghcr-container-smoke.yml` | ubuntu | 4 | dispatch. |

## Release lanes (tag push)

| Lane ID | Job | Workflow | Runner | Base LEM |
|---------|-----|----------|--------|----------|
| `release_build` | Build Release | `release.yml` | ubuntu | 30 |
| `release_wasm` | Build tokmd-wasm Artifact | `release.yml` | ubuntu | 15 |
| `release_create` | Create GitHub Release | `release.yml` | ubuntu | 5 |
| `release_publish_crates` | Publish to crates.io | `release.yml` | ubuntu | 12 |
| `release_docker` | Build and Push Docker Image | `release.yml` | ubuntu | 20 |
| `test_action_main` | Test tokmd-receipt Action | `test-action.yml` | ubuntu | 8 |
| `test_action_formats` | Test export formats | `test-action.yml` | ubuntu | 6 |
| `test_action_modes` | Test explicit modes | `test-action.yml` | ubuntu | 6 |
| `test_action_packet` | Test packet mode | `test-action.yml` | ubuntu | 6 |

## Retired lanes

Folded into `tokmd_rust_result` in phase 2 (#226): `quality_gate` (Quality
Gate), `proof_policy` (Proof Policy), `build_test_linux` (Build & Test (Linux)).

Removed in phase 3 (#299) when the duplicate `em-routed-rust-small.yml`
frontdoor was retired: `tokmd_rust_small_route`, `tokmd_rust_small_self_hosted`,
`tokmd_rust_small_github`, `tokmd_rust_small_result`. Branch protection already
required only `Tokmd Rust Result`, so no required context changed.

## Estimated default-PR LEM today

Sum of `base_lem` over `default_pr = true` lanes (LEM counts wall-clock job
minutes × runner multiplier regardless of blocking status):

```text
tokmd_rust_result                       25
scoped_coverage_executor_non_required   12
feature_boundaries                      10
publish_surface                          8
msrv_check                               5
fast_proof_run_advisory                  5
affected_proof_plan                      4
cargo_deny                               4
docs_check                               4
ci_lane_whitelist                        3
pr_cockpit_report                        3
no_panic_family                          3
clippy_exceptions                        3
version_consistency                      2
ci_gate_contract_reference               2
ci_gate_contract_live_gap                2
ripr_advisory                            2
route_ci_runner                          1
ci_detect_risk_packs                     1
typos                                    1
ci_actuals_advisory                      1
no_bare_self_hosted                      1
pr_plan_advisory                         1
                                       ----
                                        103  (high-cost band; below the 125 hard ceiling)
```

Expensive Windows, WASM, Nix, mutation, proptest, coverage, fuzz, macOS, and
release lanes are label, path-risk, push, dispatch, or tag routed instead of
ordinary PR defaults.
