# tokmd repo-native spec namespace

The durable source-of-truth stack for tokmd lives in `.tokmd-spec/`.

This namespace owns long-term proposal/spec/ADR/lane/closeout rails and keeps them separate from tool-specific execution state.

## Durable ownership

The repo-native spec system owns content under:

- `.tokmd-spec/` (durable artifacts and indexes)
- `docs/` guidance that explains how contributors use these rails
- references to live `policy/*.toml` ledgers where relevant

## External and awareness-only namespaces

The following directories may exist but are not owned by this system:

- `.codex/`
- `.spec/`
- `.claude/`
- `.jules/`

Agents may read `.tokmd-spec/` to decide what to do, but durable rails must not be stored in those tool-specific directories.

## Source-of-truth chain

The expected durable chain is:

`roadmap -> proposal -> spec -> ADR -> lane tracker -> implementation plan -> PRs -> proof -> support/policy references -> closeout`

Artifacts in this chain are indexed in `.tokmd-spec/index.toml`.
