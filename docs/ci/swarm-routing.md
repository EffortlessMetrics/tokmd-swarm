# tokmd-swarm Routed CI

`EffortlessMetrics/tokmd-swarm` is the active-development swarm repo for
tokmd. `EffortlessMetrics/tokmd` remains the public/source repo until the
cutover is deliberately completed.

Do not retarget existing `tokmd` clones in place. Clone `tokmd-swarm`
side-by-side for new work.

## Seed Point

The initial swarm `main` branch was seeded as a single import commit from:

```text
source_repo: EffortlessMetrics/tokmd
source_branch: main
source_sha: a45db1e5d4cbd836ed0918cad351af74cae2108c
swarm_repo: EffortlessMetrics/tokmd-swarm
seed_commit: abaf450dbc493052a767f536b7e03e7ee8c8bbd9
```

Because `tokmd-swarm` was seeded as a single import commit, GitHub ahead/behind
counts against `EffortlessMetrics/tokmd` reflect history shape, not necessarily
content drift. Refresh swarm by content-sync PRs instead of merging the source
repo commit DAG into swarm.

## Current Content Sync Point

The current source-content sync point is:

```text
source_repo: EffortlessMetrics/tokmd
source_branch: main
source_sha: a45db1e5d4cbd836ed0918cad351af74cae2108c
swarm_repo: EffortlessMetrics/tokmd-swarm
swarm_sync_commit: pending sync PR merge
```

After a sync, swarm content should match the source repo at `source_sha` except
for the intentional swarm overlay:

```text
.github/workflows/em-routed-rust-small.yml
docs/ci/swarm-routing.md
```

## Rust Small Frontdoor

The first swarm-specific workflow is
`.github/workflows/em-routed-rust-small.yml`.

It creates one normalized branch-protection check:

```text
Tokmd Rust Small Result
```

Do not require the conditional implementation jobs:

```text
Route Tokmd Rust Small
Tokmd Rust Small on CX43
Tokmd Rust Small on CX53
Tokmd Rust Small on GitHub Hosted
```

Only one implementation job runs in a normal workflow run, so those job names
are intentionally not stable branch-protection checks.

`main` branch protection requires only `Tokmd Rust Small Result` with strict
status checks enabled.

## Route Order

The router checks org self-hosted runner state with `EM_RUNNER_READ_TOKEN` and
selects the first idle trusted runner in this order:

```text
CX43 -> CX53 -> GitHub-hosted
```

If the runner-read token is missing, the runner API fails, parsing fails, or no
matching runner is idle, the route falls back to GitHub-hosted.

Self-hosted jobs are allowed only for:

```text
workflow_dispatch
merge_group
same-repo pull_request
```

Fork PRs route to GitHub-hosted and must not run on self-hosted runners.

## Runner Contract

Self-hosted jobs use:

```text
runner_group: em-ci-small
image: em-ci-rust:1.95
cache: /mnt/ci-cache
scratch: /mnt/ci-scratch
required labels: em-ci, rust-small, trusted-pr, plus cx43/cx53
```

Disk guards run before Cargo:

```bash
ci-disk-guard /mnt/ci-scratch 100
ci-disk-guard /mnt/docker 20
ci-disk-guard /mnt/ci-cache 20
```

Scratch cleanup runs through the same container image so root-owned files do
not strand the runner. It runs before checkout for stale workspace scratch and
after Cargo for `TMPDIR`, `CARGO_TARGET_DIR`, and workspace-local test scratch.

Self-hosted Docker jobs mount the checkout's resolved git dir and common dir
read-only into the container. That keeps `HEAD` and other local refs available
to repository tests that exercise git-aware xtask commands.

## Initial Command Set

The Rust Small lane intentionally starts with:

```bash
cargo check --workspace --all-features --locked
cargo test --workspace --all-features --locked
```

`cargo xtask gate --check` is not in the first required frontdoor. Add it only
after runtime telemetry shows the lane remains small enough for the active
Rust Small route.

## Not Moved Yet

The inherited tokmd workflows may still exist in the seed, but they are not the
new swarm frontdoor and should not be made branch-protection requirements while
this lane is being proven.

Do not move these lanes to self-hosted swarm CI in the first pass:

```text
coverage/codecov
WASM compile/test
nightly fuzz
Nix package gate
mutation testing
release/publish/signing
Windows
macOS
full platform matrix
```

## Proof Sequence

The proof sequence for the current frontdoor is:

1. Routed workflow PR passes.
2. Manual `workflow_dispatch` on `tokmd-swarm/main` passes.
3. A tiny same-repo PR proves the same-repo PR path.
4. A forced or occupied-runner backfill proof selects CX53 or GitHub-hosted
   after CX43 is unavailable.
5. Branch protection requires only `Tokmd Rust Small Result`.

CX33 was removed from the tokmd Rust Small route after forced backfill proof
showed only 58GB free on `/mnt/ci-scratch`, below the 100GB disk guard.

## Current Proof

- PR route proof: #2 passed `Tokmd Rust Small Result` on CX43 in run
  `26125363201`.
- Main dispatch proof: `workflow_dispatch` on `main` passed
  `Tokmd Rust Small Result` on CX43 in run `26126610481`.
- CX33 backfill proof: forced `workflow_dispatch` selected CX33 in run
  `26128663213` and failed the disk guard because `/mnt/ci-scratch` had 58GB
  free, so tokmd keeps the smaller `CX43 -> CX53 -> GitHub-hosted` route.
- CX53 backfill proof: forced `workflow_dispatch` on `main` selected CX53 and
  passed `Tokmd Rust Small Result` in run `26129908319`.
- GitHub-hosted fallback proof: forced `workflow_dispatch` on `main` selected
  GitHub-hosted and passed `Tokmd Rust Small Result` in run `26144499931`.
- Branch protection proof: `main` requires only `Tokmd Rust Small Result`; the
  conditional route and implementation jobs are not required checks.

## Machine Cutover Rule

New tokmd work should target:

```text
EffortlessMetrics/tokmd-swarm
```

The old source repo remains:

```text
EffortlessMetrics/tokmd
```

for read/sync/reference use until final public cutover.

Rules:

- Do not push directly to `main`.
- Open PRs against `tokmd-swarm/main`.
- Wait for `Tokmd Rust Small Result`.
- Do not use self-hosted runners for fork PRs.
- Do not move release, publish, signing, macOS, Windows, fuzz, Nix, mutation,
  coverage, or full-matrix lanes unless explicitly assigned.
