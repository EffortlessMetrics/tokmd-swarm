# Publishing Evidence Tree

Use this when your job is:

```text
Check publishing or release readiness without mutating release state.
```

Run first:

```bash
cargo xtask publish-surface --json --verify-publish
cargo xtask version-consistency
```

Sample layout when output is saved by CI or a maintainer script:

```text
target/publishing/
  publish-surface.json
  version-consistency.txt

target/proof/
  affected.json
  proof-plan.json
```

Open first:

1. `target/publishing/publish-surface.json`
2. `target/publishing/version-consistency.txt`
3. `target/proof/affected.json` when release metadata changed
4. `target/proof/proof-plan.json` when release metadata changed

What each file owns:

| File | Owns |
| --- | --- |
| `publish-surface.json` | Package-surface taxonomy, non-dev workspace closure, package-list checks, and violations. |
| `version-consistency.txt` | Version alignment output for workspace, crates, bindings, and release metadata. |
| `affected.json` | Release metadata or workflow routing, including unknown files. |
| `proof-plan.json` | Required publishing/release proof commands selected by policy. |

What not to infer:

- These checks do not publish crates.
- These checks do not create tags, GitHub releases, or Docker images.
- A clean publish-surface check is not approval to mutate release state.
- Release workflow artifacts exist only after an explicit release run.

Post-release layout when GHCR visibility is verified or recorded as pending:

```text
target/publishing/
  ghcr-visibility-1.13.1.md
```

The GHCR receipt is maintainer-written after an intentional stable release. It
records `verified-public`, `pending`, or `private-only` and belongs in the
release ledger. It does not change package visibility by itself.

Next action:

- Use [Release readiness](../release-readiness.md) for the short pre-release
  evidence command sequence.
- Fix publish-surface violations before release work.
- Pair publishing evidence with affected proof when release metadata changes.
- After stable release, follow [Post-Release GHCR Visibility Checks](../publishing-evidence.md#post-release-ghcr-visibility-checks).
- Treat publish, tag, and release creation as separate explicit maintainer
  decisions.
