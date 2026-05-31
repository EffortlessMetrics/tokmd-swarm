# NOW / NEXT / LATER

> One-screen operational truth. Updated after the post-v1.11 proof observation and cockpit review-packet checkpoints.

## NOW (active)

- **Browser runtime polish is closed on main**: cache semantics, worker/repo-load progress, retry/rate-limit guidance, and session-only authenticated fetch UX are implemented.
- **Proof control plane is observing, not promoting**: fast proof-run and scoped coverage observations stay advisory until maintainers explicitly promote them.
- **Cockpit review packets are stable for explicit proof imports**: keep `tokmd cockpit` as the PR-review evidence surface unless a fresh accepted contract selects a separate review orchestrator.
- **Main must stay boring**: keep CI green, keep `--no-default-features` builds honest, and avoid reintroducing release-only branch noise or operator caveats.
- **Docs and operator surfaces should match reality**: keep roadmap, release instructions, architecture docs, and repo-native commands aligned with what is actually implemented.

## NEXT (short horizon)

- **Selection-first work packets**: start new lanes only from a fresh consumer, missing artifact, workflow pain, or product gap; use `docs/ROADMAP.md` for durable ranking.
- **Review evidence consumption**: improve cockpit/review packet reading, hosted-comment, or missing-evidence behavior only when current evidence shows a concrete product or verifier gap.
- **Architecture consolidation remains paused**: do not continue consolidation by inertia; require fresh product or proof evidence for a real owner-module problem.

## LATER (roadmap)

- **Browser runner**: zipball ingestion remains later; in-browser analysis has shipped.
- **MCP/server mode**: streaming analysis, plugin system, and server surfaces.
- **AST depth**: higher-resolution syntax/AST integration on a later horizon.
