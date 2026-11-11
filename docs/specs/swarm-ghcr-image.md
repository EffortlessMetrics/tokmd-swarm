# Spec: Swarm Workbench GHCR Image

- Status: active
- Schema family, if any: internal CI qualification and publication receipts
- Related ADRs: `docs/adr/0003-publish-surface-taxonomy.md`,
  `docs/adr/0005-release-train-and-rc-semantics.md`
- Related proof scopes: `proof_control_plane`, `swarm_ghcr_publish`, `project_truth_docs`
- Tracked decisions: #264 (historical image visibility), #760 (publication boundary)

## Contract

Normal protected integration into `EffortlessMetrics/tokmd-swarm:main` must not
publish an image. `.github/workflows/swarm-ghcr.yml` builds and tests a local
Linux amd64 OCI archive without registry authentication or package-write
permission. The separate `swarm-ghcr-publish.yml` starts only from a deliberate
main-branch dispatch naming the exact source and previously qualified bytes.
Neither workflow runs on ordinary PR edits. #734's explicit `ci:qualify`
admission, shared work budget and required `Tokmd Rust Result` remain unchanged.
Both image jobs have a 20-minute job ceiling. A cold build exceeding it is
unqualified; the old 120-minute bootstrap allowance is not retained. This is
not a guarantee about provider queue time or a measured cold-build speedup.

The sole swarm destination is `ghcr.io/effortlessmetrics/tokmd-swarm`.
`ghcr.io/effortlessmetrics/tokmd`, crates.io, GitHub releases, release tags and
Action aliases remain publication-repository surfaces. This workflow cannot
select those destinations.

### Qualification

The actual checked-out commit/tree and OCI source/revision labels must agree.
BuildKit exports one `linux/amd64` image, with attestations disabled so the
archive contains one explicitly supported image manifest. The controller
validates layout, member names, byte bounds, every blob digest and descriptor
length before importing it into the local Docker daemon. It verifies the
loaded image's config digest against the archive and executes that image ID,
with networking disabled, for `--version` and `--help`.

Only successful smoke and cleanup produce `qualification.json`. The immutable
Actions artifact contains exactly `image.tar` and that receipt. It names source
SHA/tree, workflow/run/attempt, version, platform, smoke results, archive length
and SHA-256, OCI manifest digest and config/image ID. The Actions artifact ZIP
digest is a separate transport identity; it is not the image digest. Failed
builds, missing smoke, image substitution and failed artifact retention do not
qualify anything. A manually dispatched branch qualification can test candidate
source, but publication admits only successful qualification of **main**, from
a push or deliberate main-branch dispatch. Image-input paths select automatic
qualification. After a docs-only or other non-image main change, deliberately
qualify current main before publication; an older artifact remains stale.

### Explicit publication

A maintainer's deliberate dispatch supplies:

- `source_sha` and `source_tree`: exact full lowercase identities, still current main;
- `qualification_run_id` and `artifact_id`: the successful main run and exact retained artifact;
- `archive_sha256` and `manifest_digest`: digests from the reviewed qualification receipt, each with `sha256:` prefix;
- `move_main`: explicit false or true;
- `expected_main_digest`: empty when false; when true, the independently observed previous manifest digest or the literal `absent`.

The immutable tag is derived as `sha-<full 40-character source SHA>`.
Legacy short-SHA tags are historical objects and are never rewritten by this
path. Semver/latest aliases are not accepted. The mutable `main` tag moves only
when explicitly requested; immutable-only publication is the default.

The publisher verifies native dispatch repository/ref/source, current main
SHA/tree, successful qualification workflow/run/attempt, artifact membership,
expiry and ZIP digest, then rechecks the retained receipt and OCI bytes. It
copies that archive with digest preservation. It does **not** rebuild.
Only this job has package-write permission. Existing credentials are used
transiently; no credential, package visibility or protection setting is changed.

Before either write, current source and relevant tag expectations are checked.
An existing immutable tag is refused, even if its digest matches, so duplicate
or uncertain requests require inspection rather than automatic replay. Only a
registry HTTP 404 with the exact `MANIFEST_UNKNOWN` / `manifest unknown`
response proves absence. Reads accept both leaf manifests and indexes: GHCR
also uses that error code for an existing index when media negotiation fails.
Such negotiation failures and unknown response shapes are unavailable;
authentication, transport, malformed data and other failures are unavailable.
Each write is followed by an independently fetched manifest digest comparison.

One fixed concurrency group serializes this workflow's publishers without
canceling an active one. GHCR tag writes have no atomic compare-and-swap:
external writers are outside that serialization, and read/check/write cannot
prove their absence. Do not describe tags as technically immutable or this
protocol as an atomic registry transaction. Stop and reconcile conflicts; never
overwrite a known conflicting immutable tag or claim rollback happened.

### Failure, cancellation and recovery

The controller writes an atomic local checkpoint and log marker before each
registry write: `immutable_write_attempted`, `immutable_verified`,
`main_write_attempted`, `main_verified`, then `complete`. Ordinary exceptions
retain the last state as failed/interrupted. The always-run artifact upload
fails if no receipt exists or retention fails; no green wrapper substitutes for
that evidence. Runner termination can prevent final upload, so a missing
terminal receipt is unknown, not "nothing was published". Preserve run logs and
inspect both exact tags before any new authorization or retry.

A partial immutable publication does not authorize moving `main`, and a
failed main move does not authorize deleting the immutable tag. No automatic
retry, destructive recovery, tag deletion or rollback is implemented. A future
recovery requires its own exact state and intended-action decision.

### Visibility and support status

The existing policy is public, **workbench/experimental**, not an end-user
semver promise. The 2026-06-24 public `:main` observation in
`docs/releases/1.13-ledger.md` is historical, not proof of today's registry
visibility. Publication keeps an unauthenticated advisory manifest check for
the new immutable tag. A failed check means public access is unverified; it
does not change package visibility or erase successful authenticated writes.
Do not advertise a new image as public without a current visibility receipt.

## Inputs

- Protected swarm source and `Dockerfile` (locked Rust build).
- Exact qualification run/artifact and independent expected digests.
- Deliberate publication request with an explicit mutable-tag policy.
- Existing `GITHUB_TOKEN` only in the dispatch-only publishing job.

## Outputs

- Nonpublishing `tokmd.swarm_image_qualification.v1` receipt and OCI archive.
- On separately authorized publication, `tokmd.swarm_image_publication.v1`
  checkpoints and verified registry manifest identities.
- An advisory public-visibility observation, never a package-settings mutation.

## Compatibility

Public CLI/receipt schemas, publication-repository workflows, existing releases,
required checks, #752's retained budget wrappers and branch protection stay
unchanged. Old automatic/short-SHA publishing and its 120-minute bootstrap are
historical. Queued product PRs remain individually reviewed and qualified;
#761 owns integration after this side-effect boundary lands.

## Proof Requirements

Run `python3 -m unittest scripts/ci_swarm_image_test.py`, the existing CI-control
suite, actionlint, `cargo xtask ci-lane-whitelist --strict`,
`cargo xtask proof-policy --check`, `cargo xtask doc-artifacts --check`,
`cargo xtask docs --check` and required exact-source qualification. Tests must
exercise the invoked controller and actual workflow wiring, not an unused
publisher simulation. Retain OCI corruption/substitution, missing/stale run or
artifact, wrong source, duplicate request, registry-unavailable, conflicting
tag, uncertain write and partial-main negatives.

Before #760 closes, retain an actual matching-main source integration whose
image build/smoke succeeds without any registry authentication/write. Local
fixtures or a manual branch run do not substitute for that main-push receipt.
No real publication is necessary or authorized by PR qualification.

## Open Questions

- Native cold-build completion under the 20-minute image-job ceiling must be
  measured; failure stays explicit and does not raise the ceiling.
- Additional architectures require separate native artifact/consumer proof.
- Protection against writers outside this workflow requires a separate registry
  ownership decision; this source change neither grants nor revokes access.
