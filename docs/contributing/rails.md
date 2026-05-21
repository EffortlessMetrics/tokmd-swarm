# Contributing: Rails artifacts

Use `.rails/` for durable source-of-truth artifacts.

## What goes where

- Proposals: `.rails/proposals/`
- Specs: `.rails/specs/`
- ADRs: `.rails/adr/`
- Lane trackers: `.rails/lanes/`
- Templates: `.rails/templates/`
- Closeouts: `.rails/closeouts/`
- Support maps: `.rails/support/`
- Policy references: `.rails/policy/`
- Optional receipts: `.rails/receipts/`
- Optional schemas: `.rails/schemas/`

## Required rules

1. Every durable proposal/spec/ADR/lane artifact must be linked from `.rails/index.toml`.
2. Rails-owned artifacts must live under `.rails/`.
3. Do not migrate, edit, or validate external namespaces (`.codex/`, `.spec/`, `.claude/`, `.jules/`) as part of Rails ownership.
4. Keep sequencing in focused lane trackers; avoid one giant shared global queue.
