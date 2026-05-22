# Spec: Dependency Maintenance

- Status: active
- Schema family, if any: n/a
- Related ADRs:
  `docs/adr/0001-production-package-publishability.md`,
  `docs/adr/0003-publish-surface-taxonomy.md`
- Related proof scopes: `workspace_dependency_graph`, `project_truth_docs`

## Contract

Dependency maintenance is the repository process for classifying, proving, and
closing changes to package manifests, lockfiles, dependency-policy files, and
dependency advisory exceptions.

The process must keep three facts separate:

- what dependency state is present in the checked repository;
- what action is available to this repository without forking upstream or
  changing product behavior;
- what evidence proves the dependency state after a change.

Dependency-maintenance work may update manifests, lockfiles, dependency policy,
or documentation when there is a concrete dependency issue, audit finding,
toolchain change, publish-surface need, or CI failure. It must not treat queue
cleanliness, advisory silence, or a newer version number as sufficient evidence
by itself.

A dependency advisory ignore is a temporary classification, not a resolution.
It is allowed only when the repository records why the finding cannot be fixed
directly, how it is scoped, and what future upstream or local event should
trigger reassessment.

The current `RUSTSEC-2020-0163` `term_size` finding is the model case:
`term_size` is transitive through `tokei`, the latest crates.io `tokei` release
still carries it, and `deny.toml` ignores the advisory with a transitive-upstream
reason. That state is acceptable as a tracked mitigation, but it does not close
the underlying dependency issue until upstream removes the dependency or tokmd
adopts a deliberate local replacement strategy.

## Inputs

Dependency-maintenance evidence comes from checked repository state and
explicit commands:

| Input | Owner | Used for |
| --- | --- | --- |
| `Cargo.toml` and crate manifests | Cargo workspace | Direct dependency declarations, workspace inheritance, publishability, and feature surface. |
| `Cargo.lock` | Cargo workspace | Resolved dependency versions and transitive dependency evidence. |
| `deny.toml` | Cargo-deny policy | Advisory ignores, license policy, source policy, and banned-crate policy. |
| `ci/proof.toml` `workspace_dependency_graph` scope | Proof policy | Affected proof routing for dependency graph and cargo-deny changes. |
| `.github/dependabot.yml` | Dependabot policy | Scheduled dependency update source and label behavior. |
| `.github/workflows/ci.yml` Cargo Deny job | CI workflow | Hosted advisory, license, source, and ban checks. |
| Issue or PR context | Maintainers and agents | Concrete reason for the dependency change or deferral. |
| Upstream package state | Cargo registry or upstream repository | Whether a fix exists without local forking or behavior changes. |

Commands that query package registries or upstream repositories are current-state
evidence and may go stale. PRs should record enough checked-repo evidence that a
reviewer can reproduce the classification later.

## Outputs

Dependency-maintenance work should leave one of these outcomes:

| Outcome | Means | Required evidence |
| --- | --- | --- |
| Direct update | A manifest or lockfile update resolves the issue within accepted policy. | Manifest diff, lockfile diff, `cargo update` or equivalent package-manager evidence, cargo-deny result, and affected proof plan. |
| Direct removal | A dependency is no longer needed by checked code or metadata. | Usage search, manifest/lockfile diff, targeted build or test, cargo-deny result, and affected proof plan. |
| Policy exception | The dependency state remains but is intentionally accepted for now. | Narrow `deny.toml` or policy entry with reason, dependency-tree evidence, cargo-deny result, and explicit reassessment trigger. |
| Upstream-blocked | The repository cannot resolve the finding without forking, replacing a core upstream dependency, or changing product behavior. | Current upstream/version check, dependency-tree evidence, issue link or tracking note, and a non-closure statement. |
| Declined change | A proposed dependency change is broader, unsafe, duplicate, or misaligned. | PR/issue disposition that names the actual reason and preserves any useful follow-up. |

Issue closure requires the outcome to resolve the issue as stated. A policy
exception or upstream-blocked classification can document the risk and keep CI
honest, but it should not be presented as a fixed dependency unless the
dependency graph no longer contains the finding or a maintainer explicitly
accepts closure semantics.

## Compatibility

This spec does not change dependency versions, manifest contents, lockfile
state, cargo-deny behavior, CI gates, release workflow behavior, public `tokmd`
CLI behavior, or receipt schemas.

Existing tools remain authoritative for their domains:

- Cargo owns manifest and lockfile resolution;
- cargo-deny owns advisory, license, ban, and source checks;
- `ci/proof.toml` owns affected proof routing;
- Dependabot owns scheduled dependency update PR creation;
- maintainers own whether a documented exception is acceptable and when an
  issue can close.

Consumers must be able to ignore this spec and continue using Cargo,
cargo-deny, and the existing CI jobs directly.

## Proof Requirements

For documentation-only changes to this contract:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-dependency-maintenance.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-dependency-maintenance.json --evidence-json target/proof/proof-evidence-dependency-maintenance.json
cargo fmt-check
git diff --check
```

For dependency graph or policy changes, include the narrow relevant dependency
proof:

```bash
cargo tree -i <crate>
cargo deny --all-features check
```

When a manifest or lockfile changes, use Cargo or the relevant package manager
to produce the lockfile update. Do not hand-edit lockfiles except for a clearly
documented emergency repair.

When an advisory ignore is added or retained, proof should include:

- the exact advisory id;
- direct versus transitive dependency path;
- the package and version that still carries the finding;
- why a direct repository fix is unavailable or not selected;
- the command or upstream event that should trigger reassessment.

## Open Questions

- Whether dependency advisory exceptions should get a small machine-readable
  local ledger beyond `deny.toml` reasons.
- Whether cargo-deny output should eventually be summarized into a
  repo-owned receipt for cockpit or handoff bundles.
- Whether upstream-blocked dependency issues should stay open indefinitely or
  move to a distinct tracked-mitigation label.
