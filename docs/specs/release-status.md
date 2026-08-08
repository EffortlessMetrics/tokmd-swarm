# Release status receipt

`cargo xtask release-status` is a read-only inspection surface for release
state. It does not publish crates, create or edit GitHub Releases, move tags,
change GHCR aliases, or move the Action `v1` ref.

## Commands

Inspect the local source and tag facts:

```text
cargo xtask release-status --tag v1.15.1 --json target/release/status.json
```

Validate an offline receipt assembled from release-system evidence:

```text
cargo xtask release-status \
  --tag v1.15.1 \
  --fixture target/release/status-fixture.json \
  --json target/release/status-checked.json
```

The current first slice reads the workspace version and local Git tag. Remote
surfaces are recorded as `not_run` until their authoritative receipts are
provided through `--fixture`; prose, missing artifacts, and upstream job
success are never promoted to `passed`.

## Contract

The receipt schema is `tokmd.release_status.v1`. It reports independent facts
for:

- source version and exact tag SHA;
- publication merge SHA, parent count, and repository graph ahead/behind;
- GitHub Release state and asset inventory;
- registry inventory;
- exact and mutable GHCR references;
- exact Action and mutable `v1` references;
- consumer, Nix, and WASM/browser proof; and
- finalization state.

Each surface uses one of these states:

| State | Meaning |
| --- | --- |
| `missing` | The expected object or reference was not found. |
| `pending` | The surface exists but its terminal proof is not complete. |
| `passed` | The supplied evidence proves the surface's local contract. |
| `failed` | Evidence exists and contradicts the contract. |
| `unavailable` | The authoritative source could not be queried. |
| `not_supported` | The surface is outside the current inspection capability. |
| `not_run` | No evidence was supplied or the check was intentionally not executed. |

`complete` is derived, not trusted from prose: it is true only when every
release surface is `passed`, publication has exactly two parents, and both
repository graph counters are zero. A status receipt can therefore be useful
for diagnosing an incomplete release without claiming that the release is
complete.

The fixture validator rejects schema/version mismatches, tag mismatches, and
stale `complete` claims. It is intended to become the input seam for the
registry, hosted Release, alias, and consumer receipt adapters in follow-up
slices.
