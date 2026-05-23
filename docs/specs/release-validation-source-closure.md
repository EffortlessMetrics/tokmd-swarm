# Spec: Release Validation Source Closure

- Status: active
- Schema family, if any: n/a
- Related ADRs:
  `docs/adr/0001-production-package-publishability.md`,
  `docs/adr/0003-publish-surface-taxonomy.md`,
  `docs/adr/0005-release-train-and-rc-semantics.md`
- Related proof scopes: `nix_release_validation`, `schema_contracts`,
  `project_truth_docs`

## Contract

Release validation source closure is the contract that says which checked
repository files must be present inside filtered Nix build and check sources.

The Nix package and check lanes may use filtered sources to keep closures small,
but those filters must not remove files required by compile-time includes,
schema sync tests, integration tests, or release-facing package checks. A
source filter that makes local or hosted Nix validation pass by omitting
production contracts is invalid.

The source closure must preserve enough repository state for these release
validation claims:

- package builds can compile `tokmd` and the `tokmd` alias package variants;
- flake checks can run schema, sync, fixture, and documentation-backed tests;
- embedded schema copies and published documentation schema copies can be
  compared inside the Nix sandbox;
- release package validation does not depend on untracked local files,
  operator worktrees, or network-only state.

Hosted Nix validation remains the authoritative Nix proof surface. Local
Windows proof may validate policy shape and schema tests, but it cannot replace
hosted Nix when the change is meant to repair or protect the Nix sandbox.

## Inputs

The source-closure contract is derived from these checked inputs:

| Input | Owner | Used for |
| --- | --- | --- |
| `flake.nix` | release/nix | Nix package, check, and filtered-source definitions. |
| `.github/workflows/nix-full.yml` | release/nix | Hosted full Nix validation on the publication repo. |
| `.github/workflows/nix-macos.yml` | release/nix | macOS Nix publication validation. |
| `.github/workflows/ci.yml` Nix PR package gate | CI | PR-time flake and package smoke coverage. |
| `ci/proof.toml` `nix_release_validation` | Proof policy | Local affected proof routing for flake source-filter changes. |
| `policy/non-rust-allowlist.toml` Nix and schema entries | File policy | Checked non-Rust ownership and proof references. |
| `crates/tokmd/schemas/**` | product/contracts | Embedded schemas consumed by `include_str!` and schema tests. |
| `docs/*.schema.json` and `docs/schema.json` | product/contracts | Published schema copies and schema-sync fixtures. |
| `docs/**/*.md`, root markdown, fixtures, tests, and snapshots | Docs and tests | Compile-time includes, integration tests, and validation fixtures. |

Filtered Nix sources should use repo-relative, checked files only. A file that
is required by a release validation command must be either included by the
filter or the command must no longer claim to validate that contract.

## Outputs

This contract produces validation outcomes rather than a new receipt:

| Output | Means | Does not mean |
| --- | --- | --- |
| `nix flake check --accept-flake-config` in hosted full validation | The filtered check source contains the files required by Nix checks for the current commit. | It does not publish packages, sign artifacts, move tags, or approve a release. |
| `nix build .#tokmd` or `.#tokmd-with-alias` | The package filtered source is sufficient to build the selected package. | It does not prove all test fixtures are present unless tests are run in that derivation. |
| `cargo test -p tokmd --test schema_validation --verbose` | Schema copies and sync fixtures are present for the checked source used by that run. | It does not prove Nix filters include those files unless run inside the filtered Nix source. |
| `cargo xtask proof-policy --check` | Proof policy still routes flake source-filter changes to the expected local checks. | It does not execute hosted Nix. |
| `cargo xtask affected ...` and `cargo xtask proof --profile affected ... --plan` | The changed files selected the expected proof scopes for the PR range. | It does not replace required hosted Nix evidence when Nix source filtering changed. |

Nix source-filter failures should be treated as release-validation defects, even
when the missing file is a schema, fixture, Markdown include, or test-only file.
If a test or compile-time include depends on the file during release
validation, the file is part of the source closure for that lane.

## Compatibility

This spec does not change Nix workflows, release workflow behavior, public CLI
behavior, receipt schemas, package contents, publishability, signing, Docker
publication, tags, or v1 alias movement.

Existing files remain authoritative for enforcement:

- `flake.nix` owns the actual source filters and package/check wiring;
- `.github/workflows/nix-full.yml` and `.github/workflows/nix-macos.yml` own
  publication-only hosted Nix validation;
- `.github/workflows/ci.yml` owns the PR package gate;
- `ci/proof.toml` owns affected proof routing;
- `policy/non-rust-allowlist.toml` owns non-Rust file classification;
- schema tests own schema-copy and schema-sync behavior.

Expanding the source filter is backward compatible when it only restores files
needed by existing validation. Narrowing the source filter is a release
validation behavior change and must prove that no package build, compile-time
include, schema sync test, fixture-backed test, or hosted Nix lane depends on
the removed files.

## Proof Requirements

For documentation-only changes to this contract:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-release-source-closure.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-release-source-closure.json --evidence-json target/proof/proof-evidence-release-source-closure.json
cargo fmt-check
git diff --check
```

For changes to `flake.nix`, Nix workflow files, schema source paths, or fixtures
that affect release validation, include the narrow local proof selected by
affected analysis and, when available:

```bash
cargo test -p tokmd --test schema_validation --verbose
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-nix-release-validation.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-nix-release-validation.json --evidence-json target/proof/proof-evidence-nix-release-validation.json
```

Hosted Nix evidence is required when a PR changes the Nix source filter or is
intended to repair a Nix sandbox failure. The expected hosted checks are:

```bash
nix flake check --accept-flake-config
nix build .#tokmd
```

Publication-only full Nix workflows must remain guarded to
`EffortlessMetrics/tokmd`. Swarm PRs may use the routed PR package gate and
local proof, but they must not move release, signing, publish, Docker, tag, or
v1 alias mutation into the swarm workbench flow.

## Open Questions

- Whether Nix source-filter coverage should eventually get a small checker that
  compares known include paths and schema fixtures against `flake.nix`.
- Whether hosted Nix source-closure evidence should be summarized into a
  release-readiness receipt after the existing publishing-evidence lane has a
  concrete consumer.
- Whether macOS Nix validation needs a separate source-closure matrix once the
  Linux full validation path is consistently green.
