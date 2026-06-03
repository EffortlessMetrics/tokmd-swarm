# ADR-0012: Repo control-plane tool substrate

- Status: accepted
- Date: 2026-06-03

## Context

tokmd already uses `xtask` for repo-specific checks, proof planning,
receipts, and release-adjacent evidence. As the repository adds more proof
lanes, there is pressure to call upstream tools directly from CI workflows,
agent instructions, and local runbooks.

That direct-tool approach creates several problems:

- repo policy is scattered across shell snippets, YAML, and individual agent
  memory;
- exception handling becomes tool-local instead of receipt-backed;
- CI lanes optimize for tool availability rather than proof per minute;
- heavy engines such as mutation testing and Miri can become default PR tax;
- syntax-only findings can be mistaken for Rust-semantic authority;
- tool replacement becomes a breaking change for every contributor workflow.

tokmd needs a durable boundary between the repo-facing contract and the
upstream engines that execute parts of that contract.

## Decision

The repo-facing control surface is `cargo xtask ...`. Upstream tools are engine
room dependencies behind stable tokmd-shaped wrappers, receipts, and policy
files.

The standard substrate is:

| Plane | Upstream tools | Repo-facing role |
| --- | --- | --- |
| Syntax and codemods | `ast-grep`; Rust-specific syntax tooling where semantic identity is required | Candidate discovery, codemod worklists, and non-Rust policy probes |
| Workspace graph | `cargo_metadata`, `guppy` | Package inventory, reverse-dependency closure, feature/risk routing, and release planning |
| Test execution | `cargo-nextest`, plus `cargo test --doc` | PR and risk-pack test execution; doctests remain a separate lane |
| Coverage | `cargo-llvm-cov` | Execution-surface evidence and coverage receipts, not correctness claims |
| Static mutation exposure | `ripr` | PR-time weak-oracle and repair-packet evidence |
| Runtime mutation | `cargo-mutants` | Targeted PR backstop, broader nightly/release calibration, and readiness evidence |
| Unsafe and UB review | `unsafe-review`, Miri | Reviewable unsafe-contract cards plus targeted concrete UB witnesses |
| Source exceptions | `cargo-allow` | Durable exception ledgers and evidence links |
| Dependency trust | `cargo-deny`, `cargo-vet`, RustSec/`cargo-audit`, `cargo-auditable` | Dependency policy, advisories, audits, and shipped-binary auditability |
| Public API and release compatibility | `cargo-semver-checks`, rustdoc JSON | Release compatibility checks and custom API-surface evidence |
| Workflow policy | `actionlint`, `zizmor` | Workflow correctness and security posture checks |
| Text and config hygiene | `rustfmt`, Clippy, `taplo`, `typos`, Markdown link/style tooling | Formatting, linting, spelling, TOML, and documentation hygiene |
| Workspace hygiene | `cargo-udeps` scheduled/manual; `cargo-hakari` only when duplicate-build pain is proven | Dependency cleanup and build-graph optimization without default PR tax |
| CI cache | GitHub Rust cache by default; `sccache` only when economics justify it | Cache policy implementation, not a universal repo requirement |

The repo contract is:

```text
ast-grep finds syntactic candidates.
cargo_metadata and guppy understand the workspace.
cargo-nextest runs ordinary and risk-selected tests.
cargo-llvm-cov measures execution surface.
cargo-allow owns exception receipts.
ripr shifts mutation signal left.
unsafe-review makes unsafe changes reviewable.
cargo-mutants and Miri provide runtime backstops.
cargo-deny, cargo-vet, cargo-audit, and cargo-auditable own dependency trust.
cargo-semver-checks owns release compatibility.
xtask ties the tools into one repo-shaped control plane.
```

`ast-grep` findings are candidates. Rust-aware tooling or Rust-derived metadata
must decide authority when a rule depends on semantic identity, public API
facts, or durable source selectors.

`git ls-files -z` is the default source inventory for source exception and file
policy checks. Walkers that include ignored or untracked files must be explicit
about why they scan beyond tracked repo state.

## Consequences

- CI and agent documentation should prefer `cargo xtask ...` commands over raw
  upstream tool commands.
- Upstream engines can change without changing the repo-facing command surface
  when behavior and receipts remain compatible.
- Heavy lanes stay risk-routed: full mutation, full Miri, udeps, and hakari are
  not ordinary PR defaults.
- Coverage remains execution evidence only; it is not a release-readiness or
  test-adequacy claim by itself.
- Exceptions should be ledgered and linkable to evidence rather than hidden in
  ad hoc ignore flags.
- Workflow policy should move from scattered YAML to checked repo policy and
  `xtask` wrappers as lanes mature.
- New wrappers should produce human-readable summaries and, where the result is
  evidence, machine-readable receipts suitable for PR bodies and artifacts.

## Alternatives

- Expose upstream tools directly in every workflow and runbook. This was
  rejected because it makes each tool's CLI the durable repo contract.
- Build bespoke replacements for all upstream tools. This was rejected because
  tokmd should encode repo policy and receipts, not reimplement mature engines.
- Make every heavyweight lane a default PR blocker. This was rejected because
  proof should be selected by risk, cost, and release phase.
- Standardize only on shell scripts. This was rejected because `xtask` already
  owns repo-specific Rust logic, structured receipts, and workspace-aware
  planning.

## Enforcement

- New CI lanes should expose a stable `cargo xtask ...` entrypoint before they
  become required repo policy.
- Required policy should live in checked repo artifacts such as `ci/proof.toml`
  or `policy/*.toml`, with narrative docs explaining rather than replacing the
  checked contract.
- PR-time lanes must distinguish required proof from advisory evidence and
  skipped-by-policy evidence.
- Runtime mutation, Miri, udeps, hakari, and similar expensive checks must be
  targeted, scheduled, release-scoped, or explicitly justified before becoming
  default PR gates.
- Syntax-only scanners must not be documented as final authority for Rust
  semantic identity.
- Documentation that lists repo checks should name the `xtask` command first
  and upstream engines second.

## Related specs

- `docs/source-of-truth.md`
- `docs/workflows.md`
- `docs/adr/0008-ast-foundation.md`
- `ci/proof.toml`
- `policy/doc-artifacts.toml`
