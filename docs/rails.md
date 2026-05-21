# Rails knowledge base

This repository uses `.rails/` as the durable Rails knowledge base.

- `.rails/` contains durable proposals, specs, ADRs, lane trackers, templates, support maps, policy references, receipts, and closeouts.
- `docs/` contains human-facing explanation and adoption guidance.

## Namespace boundaries

Rails owns `.rails/` and this documentation surface.

Rails does **not** own:

- `.codex/` (Codex execution state, awareness-only)
- `.spec/` (Spec Kit / speckit state, awareness-only)
- `.claude/` (external agent/session state, awareness-only)
- `.jules/` (external agent/session state, awareness-only)

No Rails-owned artifact path may live under those external namespaces.
