# TOKMD-ADR-0001: Rails framework footprint in `.rails/`

Status: accepted
Date: 2026-05-21
Owner: repo-maintainers
Linked proposal: TOKMD-PROP-0001
Linked specs: TOKMD-SPEC-0001

## Decision

Long-term proposal/spec/ADR/lane/closeout Rails artifacts live under `.rails/`. Agent/tool-specific state remains external.

## Context

A consistent portable framework footprint is required across repositories adopting Rails.

## Consequences

Durable artifacts become easier to discover and automate while preserving boundaries with `.codex/`, `.spec/`, `.claude/`, and `.jules/`.

## Alternatives considered

Repo-specific naming was rejected due to poor portability and weaker product identity.

## Follow-up specs / plans

Define artifact graph rules and lane sequencing through TOKMD-SPEC-0001 and the rails-adoption lane tracker.
