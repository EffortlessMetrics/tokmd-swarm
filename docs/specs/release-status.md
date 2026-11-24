# Spec: Release status receipt

- Status: active
- Schema family: `tokmd.release_status.v1`
- Formal schema: [release-status.schema.json](../release-status.schema.json)
- Related ADRs: `docs/adr/0005-release-train-and-rc-semantics.md`
- Related proof scopes: `release_metadata`, `workspace_dependency_graph`

`cargo --locked xtask release-status` is a read-only inspection surface for release
state. It does not publish crates, create or edit GitHub Releases, move tags,
change GHCR aliases, or move the Action `v1` ref.

## Inputs

Supply a Git tag and, optionally, a JSON output path. Plain local inspection and
offline `--fixture` validation perform no network queries. Fixture mode checks
structure and cross-field consistency. The mutually exclusive
`--preflight-manifest PATH` option looks up hosted preflight evidence after
local source and tag verification passes.

## Outputs

The command writes the `tokmd.release_status.v1` receipt described below. It
reports each release surface independently and derives `complete` from those
states rather than trusting an imported boolean. The formal JSON Schema binds
the serialized structure and state-dependent fields; the command additionally
checks tag/version relationships, commit identity and the exact completion
calculation. Schema validity alone does not authenticate imported evidence.

This first slice derives completion only for stable semver tags. RC and other
prerelease tags can be inspected, but always remain incomplete: stable alias
promotion is not an RC requirement. Policy-aware RC completion and explicit
non-promotion evidence are tracked in
[#634](https://github.com/EffortlessMetrics/tokmd-swarm/issues/634).

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

Inspect the exact hosted preflight named by a manifest:

```text
cargo --locked xtask release-status --tag v1.15.1 \
  --preflight-manifest preflight-input.json --json target/release/status.json
```

Plain local inspection records remote surfaces as `not_run`. Requesting
preflight inspection adds a `preflight` fact and keeps the other remote
surfaces at `not_run`. Offline fixture input must omit `preflight`, including
an explicit null. Previously serialized receipts without this field retain
their original wire representation and fixture semantics.

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

A matching tag/HEAD also requires clean tracked source. Staged or unstaged
tracked modifications fail the source fact; a failed status query is
`unavailable`. The status query disables optional Git index writes. Untracked
files are outside this source-identity check, including requested receipt
outputs; this is not a hermetic build or consumer-execution claim.

Local Git subprocesses discard inherited repository, object-store, discovery,
and command-config overrides, so source and tag facts refer to the discovered
workspace even when invoked from a Git hook carrying another repository's
environment.

Tag-name validation does not launch Git, so offline fixture validation does not
require Git to be installed. It follows the Git refname rules for
`refs/tags/<tag>` and also rejects Unicode whitespace in the supplied tag.

## Live preflight manifest

Provide one JSON object with the following fields. Numeric IDs are positive
JSON integers and hashes refer to immutable objects.

| Field | Identifies |
| --- | --- |
| `repository` | `EffortlessMetrics/tokmd` or `EffortlessMetrics/tokmd-swarm` on GitHub.com |
| `tag` | The exact requested Git tag |
| `source_sha`, `affected_base_sha` | Full source and affected-plan base commit IDs |
| `release_kind` | `stable` or `rc` |
| `run_id`, `run_attempt`, `job_id` | The workflow run, its attempt, and its job |
| `artifact_id`, `artifact_digest` | The artifact ID and `sha256:` followed by its lowercase hexadecimal digest |

The manifest supplies lookup coordinates. The verified local tag and source
supply the expected commit and workspace version. If either local fact fails,
the preflight fact is `unavailable` and the command makes no hosted lookup.

A live inspection uses the host's `gh` executable, its GitHub.com credentials,
and read access to repository contents, Actions metadata, logs, and artifacts.
It resolves the hosted tag to a commit, including annotated tags, and checks
the workflow bytes against the release-preflight workflow embedded in the
installed verifier. The supported provenance is a manual `workflow_dispatch`
of that workflow. Reusable workflow calls and unsupported execution layouts
yield `unavailable`.

The verifier checks the attempt, its sole expected job, and the upload step
against service metadata. In that attempt’s log archive, the numbered upload
step must report the artifact ID and digest. A filename or a timestamp alone
cannot associate an artifact with a rerun. The artifact download must match
that digest, and its terminal preflight receipt and command observations must
match the source, base, version, and explicit dispatch release kind. The
verifier hashes the downloaded bytes in process. Denied access, expired
evidence, or unverifiable associations produce an unavailable preflight fact.
GitHub requests have time and byte limits.

A successful preflight references the hosted run, attempt, job, artifact
and digest. It establishes one release component. Publication, registry health,
consumer proofs, aliases, finalization, and release completion still require
their respective evidence. The operator trusts the installed verifier, local
tools, and authenticated GitHub service responses. The operation is read-only.

## Contract

The receipt schema is `tokmd.release_status.v1`. It reports independent facts
for:

- source version and exact tag SHA;
- hosted preflight provenance and terminal proof when requested;
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

The vocabulary applies to legacy fixture components and local inspection. In
this slice, `not_supported` can be supplied by a fixture; it is not emitted by
the local inspector, which leaves remote surfaces `not_run`. A fixture surface
marked `not_supported` keeps `complete` false, just like other non-passed
states. No adapter capability is inferred from the presence of that enum value.

`complete` is derived, not trusted from prose: it is true only for a stable
semver tag when every release surface is `passed`, publication has exactly two
parents, and both repository graph counters are zero. The source/tag SHA and publication merge
SHA must identify the same commit (hexadecimal letter case is insignificant).
Fixtures that mark both surfaces passed while naming different commits are
rejected even if they claim `complete: false`. A status receipt can therefore be useful
for diagnosing an incomplete release without claiming that the release is
complete.

The fixture validator rejects schema/version mismatches, tag mismatches, and
stale `complete` claims. A structurally complete fixture therefore means that
all supplied facts satisfy this receipt contract, not that the fixture itself
proves its evidence came from a trusted release system. The registry, hosted
Release, publication graph, alias, and consumer receipt adapters remain the
authoritative follow-up seams.

JSON output uses a synced temporary file in the destination directory before
atomic replacement. Preparation failures preserve a previous receipt and clean
up the temporary file. Abrupt termination can leave a temporary file; directory
metadata crash durability is not claimed. Fixture reads are capped during the
read, including when the input file grows concurrently.

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
- Executable fixtures reject staged and unstaged tracked changes at a matching
  tag and preserve source contents. Full receipt bytes match a checked-in golden
  fixture, and injected output failure preserves the previous valid receipt.
- Release completion must remain false for prerelease or non-semver tags, or
  unless every required surface is
  `passed`, source and publication identify the same commit, publication has
  two parents, and graph ahead/behind is `0/0`.
- Serialized complete/incomplete receipts must satisfy the formal schema;
  fixtures cover unknown fields/states, invalid types, missing required fields
  and passed-state prerequisites. Semantic tests separately reject mixed commit
  identities and stale completion claims.

## Open Questions

The other remote receipt adapters and the closeout consumer are separate
follow-up slices. They must name their authoritative source and preserve the
same state vocabulary before being wired into this command.
