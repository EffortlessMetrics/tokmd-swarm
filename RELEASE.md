# Release Process

This repository uses a lockstep microcrate publishing model. All publishable
workspace crates share one version, and publish order is derived from workspace
dependency topology.

This file is the short entry point. The canonical operation, stop conditions,
recovery rules, and evidence checklist live in
[`docs/releases/release-checklist.md`](docs/releases/release-checklist.md).
Use [`docs/release-readiness.md`](docs/release-readiness.md) to interpret the
pre-release checks and [`docs/ci/swarm-routing.md`](docs/ci/swarm-routing.md)
for the history-preserving publication topology.

## Repository boundary

```text
EffortlessMetrics/tokmd-swarm
  normal implementation and release-preparation PRs

EffortlessMetrics/tokmd
  publication import, tags, GitHub Releases, crates.io, GHCR, attestations,
  signing, and mutable release aliases
```

Do not tag, publish, move `v1`, or create a GitHub Release from
`tokmd-swarm`. A release is cut only from the aligned publication merge commit
in `EffortlessMetrics/tokmd`.

## Release state is not one boolean

Keep these facts separate:

```text
source prepared
source reviewed
publication import merged
repositories aligned
candidate verified
tag exists
GitHub Release exists
assets complete
consumer proof passed
stable aliases promoted
closeout complete
```

A tag is not a GitHub Release. A green build workflow is not proof that a user
can download and run the artifacts. A queued job, timeout, missing receipt, or
unavailable required surface is not a pass.

## Publishing order

Preview the exact crate order:

```bash
cargo xtask publish --plan
```

Do not maintain a hard-coded crate list by hand.

## 1. Prepare the version in `tokmd-swarm`

Create one focused release-preparation PR after the intended product fixes have
landed.

```bash
cargo xtask bump <MAJOR.MINOR.PATCH[-rc.N]>
```

Update and verify:

- `CHANGELOG.md` and the version-specific release note;
- `CITATION.cff` version/date;
- the Action's baked-in exact binary version;
- workspace and binding versions;
- version-bearing snapshots, generated docs, SBOMs, and release receipts;
- the version-specific readiness report and ledger.

Run the committed-source preflight:

```bash
cargo fmt-check
cargo gate-check
cargo xtask version-consistency
cargo xtask publish-surface --json --verify-publish
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask check-no-panic-family --strict

cargo xtask affected \
  --base origin/main \
  --head HEAD \
  --json-output target/proof/affected-release.json

cargo xtask proof \
  --profile affected \
  --base origin/main \
  --head HEAD \
  --plan \
  --plan-json target/proof/proof-plan-release.json \
  --evidence-json target/proof/proof-evidence-release.json

cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny --all-features check
cargo xtask publish --dry-run

npm --prefix web/runner test
npm --prefix web/runner run build:wasm:archive-zip
```

On Windows, prefer the repo-native quality commands above over raw
`cargo fmt --all`; the workspace can exceed formatter argv limits.

`publish-surface --verify-publish` and `publish --dry-run` must run against a
clean committed tree. A local timeout is `not_run`, not passing evidence.

After the final push, require:

- a fresh exact-head Codex review with `blocking_findings=0`;
- terminal green `Tokmd Rust Result` and all other selected required checks;
- zero unknown files in affected planning.

The repository has one human maintainer. The review control is the exact-head
Codex receipt and repository checks, not a second-human approval object.

## 2. Import the exact swarm source into publication

Freeze unrelated swarm merges for the import window.

Prove the pre-import direction:

```bash
cargo xtask repo-graph \
  --publication public/main \
  --swarm origin/main \
  --expect swarm-ahead \
  --json target/repo-graph/pre-publication.json
```

Then:

1. record the exact swarm and publication SHAs;
2. push the exact swarm head to a publication branch;
3. open a PR against `EffortlessMetrics/tokmd:main`;
4. run publication CI and a fresh exact-head Codex review;
5. verify swarm `main` has not moved;
6. merge with an explicit **merge commit**—never squash or rebase;
7. assert the publication merge has exactly two parents;
8. fast-forward `tokmd-swarm/main` to that merge commit;
9. restore any temporary bypass or settings change;
10. require graph alignment:

```bash
cargo xtask repo-graph \
  --publication public/main \
  --swarm origin/main \
  --expect aligned \
  --json target/repo-graph/alignment.json
```

Completion requires `publication_ahead=0` and `swarm_ahead=0`.

## 3. Prove the unchanged final source

From `EffortlessMetrics/tokmd`, run the release-candidate proof against the
exact aligned publication commit. Bind the receipt to the source SHA, expected
version, immutable container/artifact digest, required platforms, anonymous
pull, exact version output, and mounted packet content.

Do not edit the source after candidate proof and then tag the new commit. Keep
pre-tag proof in workflow artifacts/job summaries, tag the unchanged proven
commit, and import durable receipt prose afterward.

## 4. Tag and publish through CI

The tag-driven publication workflow is the canonical production path:

```bash
git tag vX.Y.Z[-rc.N]
git push origin vX.Y.Z[-rc.N]
```

The operation occurs only in `EffortlessMetrics/tokmd`.

### Release candidate policy

An RC:

- is a GitHub prerelease and is not `latest`;
- publishes exact binaries, archive-enabled WASM, checksums, attestations, and
  an exact RC container tag;
- does not publish crates.io packages;
- does not move `v1`, `1`, `<major>.<minor>`, or `latest` aliases.

### Stable policy

Stable publication:

- builds and verifies exact assets;
- publishes crates in dependency order through `cargo xtask publish`;
- verifies exact Action and container behavior;
- moves mutable aliases only after exact artifacts and crates publication pass.

If crates publication fails mid-stream, resume only after classifying what was
published:

```bash
cargo xtask publish --from <crate-name>
```

## 5. Verify the GitHub Release and exact artifacts

Verify these facts separately:

- tag ref points at the intended source commit;
- GitHub Release object exists;
- draft/prerelease/latest state is correct;
- expected assets exist exactly once;
- checksums cover every distributed asset;
- attestations are retrievable and verify;
- exact GHCR tag resolves to the intended digest.

Do not use the repository sidebar's `Latest` card as the authority for RC
existence; prereleases intentionally leave the latest stable card unchanged.

Then run `.github/workflows/release-consumer-smoke.yml` against the exact
publication tag. It must download release artifacts rather than rebuild them.
Required failures, unavailable surfaces, missing receipts, and crashed jobs fail
closed.

A consumer failure rejects the RC. Fix the demonstrated defect in normal swarm
PRs, repeat the history-preserving import and candidate proof, and cut the next
RC number. Never move an existing RC tag.

## 6. Stable closeout

After stable consumer verification:

1. complete the version-specific readiness report and ledger;
2. record source/merge SHAs, run IDs, assets, hashes, digests, crates, Action,
   container, Nix, browser/WASM, and consumer outcomes;
3. restore a fresh `## [Unreleased]` section and comparison link;
4. update `docs/NOW.md`, `docs/NEXT.md`, and roadmap state;
5. close the release tracking issue;
6. perform one consolidated closeout import if the receipts were prepared in
   swarm;
7. leave both repositories graph-aligned;
8. prune release branches, worktrees, and task-specific targets created by the
   release lane.

## Recovery rule

If a tag exists but no GitHub Release object is proven, classify it as
`tag_only`; do not call it published and do not move or recreate it
automatically. If source, workflow, or artifact bytes must change, cut the next
RC. A same-tag metadata-only recovery requires an explicit maintainer decision
and a durable receipt.
