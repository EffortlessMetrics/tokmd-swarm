# TOKMD-PROP-0001: Rails durable knowledge base

Status: accepted
Owner: repo-maintainers
Created: 2026-05-21
Target milestone: next
Linked specs: TOKMD-SPEC-0001
Linked ADRs: TOKMD-ADR-0001
Linked lanes: rails-adoption

## Problem

Durable planning and architecture knowledge currently risks blending with tool-specific execution state.

## Users and surfaces

Contributors, maintainers, release operators, and support workflows; surfaces include repo docs and durable planning artifacts.

## Success criteria

A portable `.rails/` footprint exists with durable artifact ownership and explicit external namespace boundaries.

## Proposed shape

Adopt `.rails/` as the durable source-of-truth space and index artifacts through `.rails/index.toml`.

## Alternatives considered

Repo-specific footprint naming (for example `.<repo>-spec/`) was rejected because it harms portability.

## Specs to create or update

- TOKMD-SPEC-0001

## Architecture decisions needed

- TOKMD-ADR-0001

## Implementation campaign shape

Establish footprint/docs/templates, then validate with index and lane trackers.

## Evidence plan

- `git diff --check`

## Risks

Overlapping mental models between Rails and external tool state.

## Non-goals

Migrating or editing `.codex/`, `.spec/`, `.claude/`, or `.jules/`.

## Exit criteria

Durable artifacts and lane tracker exist under `.rails/` and are indexed.
