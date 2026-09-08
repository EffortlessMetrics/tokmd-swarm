# Spec: Release status receipt

- Status: active
- Schema family: `tokmd.release_status.v1`
- Related ADRs: `docs/adr/0005-release-train-and-rc-semantics.md`
- Related proof scopes: `release_metadata`, `workspace_dependency_graph`

`cargo --locked xtask release-status` is a read-only inspection surface for release
state. It does not publish crates, create or edit GitHub Releases, move tags,
change GHCR aliases, or move the Action `v1` ref.

## Inputs

The command requires a Git tag. It optionally accepts a JSON output path and an
offline receipt fixture assembled from release-system evidence. Fixture mode
performs structural and cross-field validation; it does not authenticate the
fixture's provenance or independently re-run remote release checks.

## Outputs

The command writes the `tokmd.release_status.v1` receipt described below. It
reports each release surface independently and derives `complete` from those
states rather than trusting an imported boolean.

## Commands

Inspect the local source and tag facts:

```text
cargo --locked xtask release-status --tag v1.15.1 --json target/release/status.json
```

Validate an offline receipt assembled from release-system evidence:

```text
cargo --locked xtask release-status \
  --tag v1.15.1 \
  --fixture target/release/status-fixture.json \
  --json target/release/status-checked.json
```

The current first slice reads the workspace version and local Git tag. Remote
surfaces are recorded as `not_run` until their authoritative receipts are
provided through `--fixture`; prose, missing artifacts, and upstream job
success are never promoted to `passed`.

Local inspection discovers the workspace from the current directory using the
parsed `workspace` table in `Cargo.toml`. A comment or string mentioning
`[workspace]` does not identify a workspace. Failure to locate or parse a
workspace candidate remains a command error because the inspection root cannot
be established. After a root is established, version-read failures and Git
query failures become `unavailable` facts with diagnostic context. A missing Git
executable or an extracted workspace without Git metadata therefore still
produces an incomplete receipt. A confirmed absent tag or unborn HEAD remains
`missing`; a version mismatch and a tag/HEAD commit mismatch are reported
separately. Invalid tag names and invalid fixture input remain command errors.

Local Git subprocesses discard inherited repository, object-store, discovery,
and command-config overrides, so source and tag facts refer to the discovered
workspace even when invoked from a Git hook carrying another repository's
environment.

Tag-name validation does not launch Git, so offline fixture validation does not
require Git to be installed. It follows the Git refname rules for
`refs/tags/<tag>` and also rejects Unicode whitespace in the supplied tag.

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

The vocabulary applies to supplied fixtures as well as local inspection. In
this slice, `not_supported` can be supplied by a fixture; it is not emitted by
the local inspector, which leaves remote surfaces `not_run`. A fixture surface
marked `not_supported` keeps `complete` false, just like other non-passed
states. No adapter capability is inferred from the presence of that enum value.

`complete` is derived, not trusted from prose: it is true only when every
release surface is `passed`, publication has exactly two parents, and both
repository graph counters are zero. A status receipt can therefore be useful
for diagnosing an incomplete release without claiming that the release is
complete.

The fixture validator rejects schema/version mismatches, tag mismatches, and
stale `complete` claims. A structurally complete fixture therefore means that
all supplied facts satisfy this receipt contract, not that the fixture itself
proves its evidence came from a trusted release system. The registry, hosted
Release, publication graph, alias, and consumer receipt adapters remain the
authoritative follow-up seams.

## Compatibility

The receipt is additive and versioned. Existing publish, graph, alias, and
consumer receipts remain authoritative for their own surfaces; this command
does not replace or reinterpret them. A future schema revision must preserve
`not_run` and unavailable evidence as distinct states.

## Proof Requirements

- Unit fixtures must discriminate a complete stable release from draft,
  partial-asset, partial-registry, exact-proof/alias-pending, and missing-proof
  states.
- Fixture validation must reject stale completion claims and tag/schema
  mismatches.
- Local checks must prove deterministic formatting and JSON field ordering.
- Local-source tests cover subdirectory discovery without changing process cwd,
  unavailable Git/version queries, missing refs, and distinct source mismatch
  diagnostics. Tag validation has valid/invalid reference controls.
- An executable-level test must inspect one tagged workspace while inheriting
  another repository's Git overrides and retain the workspace's source SHA.
- Release completion must remain false unless every required surface is
  `passed`, publication has two parents, and graph ahead/behind is `0/0`.

## Open Questions

Remote receipt adapters and the closeout consumer are intentionally separate
follow-up slices. They must name their authoritative source and preserve the
same state vocabulary before being wired into this command.
