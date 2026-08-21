# Shared Agent Repo Guide

This file is the canonical shared repo context for checked-in agent adapters in this repository.

## Project Overview

**tokmd** is a Rust CLI tool and library that generates deterministic inventory receipts and derived analytics for code repositories. It produces human-readable summaries and machine-friendly artifacts for AI-native workflows, LLM context generation, code analysis, and review pipelines.

## Developer Workflow

Common commands:

Routine workspace work preserves the committed `Cargo.lock`; use `--locked` so
a missing or stale lock fails visibly instead of changing dependency state:

```bash
cargo build --locked
cargo build --locked --release
cargo check --locked --workspace
cargo test --locked --all-features
cargo test --locked -p xtask --all-features
cargo fmt-check
cargo fmt-fix
cargo clippy --locked --all-features -- -D warnings
cargo run --locked -p tokmd -- --version
cargo xtask lint-fix
cargo xtask gate --check
just lint
just fmt
just publish-plan
```

Install from the workspace only when an installed binary is actually needed;
ordinary development should use the workspace commands above:

```bash
cargo install --path crates/tokmd --locked
```

This source install is reproducible only to the committed lock available in
the checkout. Registry consumer installation is governed by the published
package's lock and exact release proof, not by this workspace source-install
path.

`cargo test --locked --all-features` runs the workspace `default-members`; it does not
select `xtask` or `fuzz`. The required `Tokmd Rust Result` runs the following
serial proof sequence: `cargo xtask gate --check`, the default-member test
command above, `cargo test --locked -p xtask --all-features`, and
`cargo xtask proof-policy --check`. Use `cargo test --locked --workspace --all-features`
only when broad Cargo workspace package selection is intended. It is not the
required CI sequence and does not execute libFuzzer campaigns; scheduled fuzz
and platform lanes remain separate proof.

This locked-command contract is tracked in [tokmd-swarm#604](https://github.com/EffortlessMetrics/tokmd-swarm/issues/604)
and the shared [depguard#21](https://github.com/EffortlessMetrics/depguard/issues/21)
programme, with follow-up controls in [depguard#22](https://github.com/EffortlessMetrics/depguard/issues/22)
and [depguard#24](https://github.com/EffortlessMetrics/depguard/issues/24).

Optional git hooks:

```bash
git config core.hooksPath .githooks
```

## Dual-Repo Workbench Boundary

Normal tokmd development starts from `EffortlessMetrics/tokmd-swarm:main`.
Create narrow PRs there, wait for `Tokmd Rust Result`, and squash-merge
aligned work into the swarm repo.

`EffortlessMetrics/tokmd` is the publication repository. Do not push feature
work, release tags, GitHub releases, crates.io publishes, Docker pushes,
signing changes, or `v1` alias moves from swarm. Publication imports are
explicit merge-commit PRs in `tokmd`, followed by fast-forwarding
`tokmd-swarm/main` to the publication merge commit.

See `docs/ci/swarm-routing.md` for the shared-history topology and routing
rules.

## Review and Conversation Resolution

Conversation resolution is part of the merge process. Every actionable inline
comment from a bot or agent reviewer must be addressed, independently checked,
and its review thread resolved before merge. Resolution is necessary process
evidence, not proof by itself that the finding was fixed. A general bot summary
comment with no review thread is not an unresolved conversation. The
independent check is a workflow record made in the review reply, PR discussion,
or handoff; GitHub's conversation setting only enforces the final resolved state
and does not prove that the finding was fixed or that a separate identity
performed the check.

This is a single-maintainer repository: a separate human reviewer account,
native approval, and CODEOWNERS approval are intentionally not merge
requirements. Substantive PRs should receive independent agentic review passes
as part of the normal review process. Use separate agent lanes for those passes;
they may leave inline findings, and the resulting conversations must be
independently checked and resolved. This is an agent workflow requirement, not
a native approval or required-review status gate. Do not manufacture a second
GitHub reviewer account, approval identity, or status check merely to satisfy
one.

## Codex Commit / Push Policy

For PR-bound work, Codex may create scoped branches, commit scoped changes, push
branches, open PRs, update PR branches, and merge aligned PRs after validation
without asking for additional user confirmation.

PR-bound work includes requests to implement, review, improve, merge, drain PRs,
prepare release docs, update changelogs, fix tests, or otherwise carry a repo
task through completion.

Do not ask for extra permission merely because a commit, push, PR update, or
aligned merge is needed to finish that task.

Ask before committing, pushing, or merging only when:

- the user explicitly requested read-only or local-only work;
- the task is exploratory and no implementation was requested;
- the mutation would publish crates, create tags, create GitHub releases, move
  release aliases, push images, rotate secrets, or change external-service
  ownership;
- the diff is broad or ambiguous relative to the requested lane;
- the worktree contains unrelated user changes that cannot be isolated safely.

## Release-Prep PR Lifecycle

Never close a PR merely because a release is near.

During prep or RC hardening, classify each open PR by substance: release
blocker, safe aligned change, useful non-blocking work, duplicate of a merged
keeper, invalid or incorrect work, stale branch needing restack, or explicitly
declined work.

Merge release blockers and safe aligned PRs after validation. Leave useful
non-blocking PRs open, parked, labeled, or restacked for later. Close a PR only
when it is intrinsically invalid, duplicated or superseded by a merged keeper,
stale beyond practical restack, conflicts with accepted direction, or was
explicitly declined.

Queue cleanliness is not a release criterion. Release readiness is proven by
preflight, release-record accuracy, and clean release-surface evidence.

## Architecture

The codebase follows a tiered crate-and-module architecture:

`types -> scan/model -> format/adapters -> analysis/cockpit/gate -> core -> products`

Public crates represent durable contracts, facades, adapters, or products.
Implementation details that do not need an independent package live as
single-responsibility owner modules inside those crates.

Tier summary:

| Tier | Purpose | Example crates |
|------|---------|----------------|
| 0 | Contracts and settings | `tokmd-types`, `tokmd-analysis-types`, `tokmd-settings`, `tokmd-envelope`, `tokmd-io-port` |
| 1 | Core scan and aggregation | `tokmd-scan`, `tokmd-model`, `tokmd-sensor` |
| 2 | Adapters and rendering | `tokmd-format`, `tokmd-git` |
| 3 | Analysis and review orchestration | `tokmd-analysis`, `tokmd-cockpit`, `tokmd-gate` |
| 4 | Library facade | `tokmd-core` |
| 5 | End-user products | `tokmd`, `tokmd-python`, `tokmd-node`, `tokmd-wasm` |

Former helper microcrates such as redaction, scan-args, badge rendering,
analysis rendering, progress, module-key, path/exclude/math, tokeignore,
context policy/git, fun renderers, content/import enrichers, and tool-schema now
live as owner modules inside `tokmd-format`, `tokmd-scan`, `tokmd-model`,
`tokmd-analysis`, `tokmd-core`, or `tokmd`.

Dependency rule:

- Lower tiers must never depend on higher tiers.

## CLI Surface

- `tokmd` / `tokmd lang` - language summary
- `tokmd module` - module breakdown
- `tokmd export` - file-level inventory
- `tokmd run` - full scan with artifacts
- `tokmd analyze` - derived metrics and enrichments
- `tokmd badge` - SVG badge generation
- `tokmd diff` - compare runs or receipts
- `tokmd cockpit` - PR metrics and evidence gates
- `tokmd sensor` - sensor envelope output
- `tokmd packet` - one-command evidence packet workflow (`packet generate`)
- `tokmd gate` - policy evaluation
- `tokmd tools` - LLM tool definitions
- `tokmd context` - context packing under token budget
- `tokmd baseline` - baseline capture
- `tokmd handoff` - LLM handoff bundle generation
- `tokmd init` - generate `.tokeignore`
- `tokmd check-ignore` - explain ignore decisions
- `tokmd completions` - shell completions

## Critical Invariants

### Deterministic output

- Use `BTreeMap` instead of `HashMap` for stable ordering.
- Sort descending by code lines, then by name.
- Keep output byte-stable for snapshot and receipt diffs.

### Path normalization

- Normalize output paths to forward slashes (`/`) on every platform.
- Normalize before emitting output or computing module keys.

### Children and embedded languages

- `ChildrenMode::Collapse` merges embedded languages into parent totals.
- `ChildrenMode::Separate` emits explicit embedded rows.
- Apply this consistently across commands and receipt surfaces.

### Schema versioning

- Bump the relevant schema version when JSON structure changes.
- Update formal schema docs when structure changes.
- Receipt families currently version independently:
  - core receipts: `SCHEMA_VERSION = 2`
  - analysis receipts: `ANALYSIS_SCHEMA_VERSION = 9`
  - cockpit receipts: `COCKPIT_SCHEMA_VERSION = 3`
  - handoff manifests: `HANDOFF_SCHEMA_VERSION = 5`
  - context receipts: `CONTEXT_SCHEMA_VERSION = 4`
  - context bundles: `CONTEXT_BUNDLE_SCHEMA_VERSION = 2`

### Feature flags

- `git` enables git-history analysis.
- `content` enables file-content scanning.
- `walk` enables filesystem traversal helpers.
- `halstead` requires `content` plus `walk`.

### Git range syntax

| Syntax | Meaning | Use case |
|--------|---------|----------|
| `A..B` | commits reachable from `B` but not `A` | comparing tags or releases |
| `A...B` | symmetric difference from merge-base | CI workflows comparing branch divergence |

Rule:

- Use `..` in cockpit and diff flows comparing releases or tags.
- Use `...` only in CI workflows that want branch-divergence changes.

## Testing Notes

- Integration tests live under `crates/tokmd/tests/`.
- Golden snapshots use `insta`.
- Property testing uses `proptest`.
- Fuzz targets live in `fuzz/`.
- Mutation testing is configured in `.cargo/mutants.toml`.

Common targeted commands:

```bash
cargo test --locked test_name --verbose
cargo test --locked -p tokmd-scan properties
cargo mutants --file crates/tokmd-format/src/redact/mod.rs
cargo +nightly fuzz list
```

## Agent State Boundaries

`.jules/**` is Google Jules provenance and ambient suggestion state. Treat it as
useful repo input, not Codex's primary active-lane controller.

Codex should use `AGENTS.md`, `docs/NEXT.md`, accepted docs/plans/specs/ADRs, PR
context, and `.codex/**` state where present for Codex lane selection.

Do not tell Jules to stop acting or remove Jules suggestions merely because
Codex is working. Jules suggestions remain inputs to evaluate with the rest of
the repo evidence.

## Reference Docs

- `docs/architecture.md`
- `docs/design.md`
- `docs/requirements.md`
- `docs/implementation-plan.md`
- `docs/reference-cli.md`
- `docs/SCHEMA.md`
- `docs/schema.json`
- `docs/testing.md`
- `docs/PRODUCT.md`
- `docs/explanation.md`
- `ROADMAP.md`
- `CHANGELOG.md`
- `CONTRIBUTING.md`
