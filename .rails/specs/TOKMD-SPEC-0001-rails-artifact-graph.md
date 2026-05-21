# TOKMD-SPEC-0001: Rails artifact graph contract

Status: accepted
Owner: repo-maintainers
Created: 2026-05-21
Linked proposal: TOKMD-PROP-0001
Linked ADRs: TOKMD-ADR-0001
Linked lane: rails-adoption
Linked issues:
Linked PRs:
Support-tier impact: documentation and contributor workflow clarity
Policy impact: none

## Problem

Without a defined graph contract, durable artifacts can drift and lose traceability.

## Behavior

- Rails artifacts must be indexed through `.rails/index.toml`.
- Owned artifact paths must live under `.rails/`.
- External namespaces may be listed but not owned.
- Specs define behavior, not PR order.
- Lane trackers define focused implementation sequencing.

## Non-goals

Owning or validating external tool/agent state.

## Required evidence

- `git diff --check`

## Acceptance examples

- Proposal/spec/ADR files under `.rails/` with IDs linked in index.
- Lane tracker path listed in index and present on disk.

## Test mapping

- Initial proof via `git diff --check`.

## Implementation mapping

- `.rails/index.toml`
- `.rails/proposals/`
- `.rails/specs/`
- `.rails/adr/`
- `.rails/lanes/`

## CI proof

- `git diff --check`

## Metrics / promotion rule

Promote once validator automation is added in a follow-up lane.

## Failure modes

Missing index links, out-of-tree owned artifacts, or unresolved linked IDs.
