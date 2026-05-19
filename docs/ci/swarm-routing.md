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

## Rust Small Frontdoor

The first swarm-specific workflow is
`.github/workflows/em-routed-rust-small.yml`.

It creates one normalized check that should become the branch-protection check
after the proof sequence is complete:

```text
Tokmd Rust Small Result
```

Do not require the conditional implementation jobs:

```text
Route Tokmd Rust Small
Tokmd Rust Small on CX43
Tokmd Rust Small on CX33
Tokmd Rust Small on CX53
Tokmd Rust Small on GitHub Hosted
```

Only one implementation job runs in a normal workflow run, so those job names
are intentionally not stable branch-protection checks.

## Route Order

The router checks org self-hosted runner state with `EM_RUNNER_READ_TOKEN` and
selects the first idle trusted runner in this order:

```text
CX43 -> CX33 -> CX53 -> GitHub-hosted
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
required labels: em-ci, rust-small, trusted-pr, plus cx43/cx33/cx53
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
after runtime telemetry shows the lane remains small enough for CX43/CX33.

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

Use this order before enabling branch protection:

1. Routed workflow PR passes.
2. Manual `workflow_dispatch` on `tokmd-swarm/main` passes.
3. A tiny same-repo PR proves the same-repo PR path.
4. A forced or occupied-runner backfill proof selects CX33, CX53, or
   GitHub-hosted after CX43 is unavailable.
5. After 3-5 clean runs, require only `Tokmd Rust Small Result`.

If CX33 shows disk or runtime problems, remove it from the route and keep:

```text
CX43 -> CX53 -> GitHub-hosted
```

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
