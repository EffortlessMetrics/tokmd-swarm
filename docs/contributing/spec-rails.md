# Contributing: repo-native spec rails

When adding or updating durable planning/spec artifacts in this repository, use `.tokmd-spec/` as the source of truth.

## Scope owned by this system

- `.tokmd-spec/` artifacts and indexes
- supporting guidance in `docs/`
- references to live ledgers in `policy/*.toml` when relevant

## Out of scope for this lane

Do not migrate, rewrite, validate, or store durable artifacts in:

- `.codex/`
- `.spec/`
- `.claude/`
- `.jules/`

Those directories are tool-specific execution/session state.

## Artifact intent

- **Proposals** (`.tokmd-spec/proposals/`): why, user value, alternatives, success criteria
- **Specs** (`.tokmd-spec/specs/`): required behavior, evidence requirements, acceptance boundaries
- **ADRs** (`.tokmd-spec/adr/`): durable architecture decisions
- **Lane trackers** (`.tokmd-spec/lanes/<lane>/tracker.toml`): durable lane state and next PR-sized work items
- **Implementation plans** (`.tokmd-spec/lanes/<lane>/implementation-plan.md`): execution sequence
- **Support/policy references** (`.tokmd-spec/support/`, `.tokmd-spec/policy/`): claim/proof and ledger linkage
- **Closeouts** (`.tokmd-spec/closeouts/`): landed work, proof, remaining gaps

## Minimal external-state wording

If docs need to mention tool directories, keep wording minimal:

> This repo may contain `.codex/`, `.claude/`, `.jules/`, or similar tool-specific directories. Those directories are not the durable source of truth for this spec system.

> If `.spec/` exists, it is reserved for Spec Kit workflows. The repo-native long-term rails live in `.tokmd-spec/`.
