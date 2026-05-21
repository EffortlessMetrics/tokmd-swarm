# Spec rails style guide

This repository keeps its durable spec control plane in `.tokmd-spec/`.

## Namespace model

- `.tokmd-spec/` = durable repo knowledge base and spec rails
- `docs/` = human-facing explanation and contributor guidance
- `policy/` = live enforcement ledgers, referenced where relevant
- `plans/` = only when already part of the repo's non-agent planning surface

External tool/session directories are awareness-only for this lane:

- `.codex/` (Codex execution state)
- `.spec/` (Spec Kit state)
- `.claude/` (Claude/session state)
- `.jules/` (Jules/session state)

## Separation of concerns

Keep these concerns in separate artifacts instead of combining everything into one document:

- **Why**: proposals / PRDs
- **What**: behavior specs
- **Decision**: ADRs
- **How**: lane trackers and implementation plans
- **What proves it**: evidence and CI proof mappings
- **What happened**: closeouts

## Durable chain

`roadmap -> proposal -> spec -> ADR -> lane tracker -> implementation plan -> PRs -> proof -> support/policy references -> closeout`

## Index requirement

Every durable artifact should be linked through `.tokmd-spec/index.toml`.
Artifact paths in index entries must not live under `.codex/`, `.spec/`, `.claude/`, or `.jules/`.
