# Swarm User Path Smoke Run

Use this when your job is:

```text
I want to prove the swarm frontdoor still supports tokmd's review-to-handoff
evidence workflow on a real same-repo change.
```

This is a smoke transcript, not a generated packet dump. It records one run of
the user path in `EffortlessMetrics/tokmd-swarm` after the routed Rust Small
frontdoor and branch protection were enabled.

## Scenario

- Date: 2026-05-20.
- Repository: `EffortlessMetrics/tokmd-swarm`.
- Branch: `docs/swarm-user-path-smoke`.
- Base: `origin/main`.
- Head: `HEAD`.
- Change: this smoke-run document plus the examples index link.
- Local command note: the smoke used `cargo run -p tokmd --` for `cockpit`
  and `handoff` so it exercised the workspace binary; the command blocks below
  keep the user-facing `tokmd` form.

## Run First

Plan affected proof:

```bash
cargo xtask affected \
  --base origin/main \
  --head HEAD \
  --json-output target/proof/swarm-user-path-smoke/affected.json

cargo xtask proof \
  --profile affected \
  --base origin/main \
  --head HEAD \
  --plan \
  --plan-json target/proof/swarm-user-path-smoke/proof-plan.json \
  --evidence-json target/proof/swarm-user-path-smoke/proof-evidence.json
```

Run selected required proof when the plan selects it:

```bash
cargo xtask proof \
  --profile affected \
  --base origin/main \
  --head HEAD \
  --run-required \
  --allow-local-required-execution \
  --plan-json target/proof/swarm-user-path-smoke/proof-plan.json \
  --proof-run-summary target/proof/swarm-user-path-smoke/proof-run-summary.json
```

Generate and verify the review packet:

```bash
tokmd cockpit \
  --base origin/main \
  --head HEAD \
  --review-packet-dir .tokmd/swarm-user-path-smoke/review

cargo xtask review-packet-check \
  --dir .tokmd/swarm-user-path-smoke/review \
  --json target/tokmd/swarm-user-path-smoke/review-packet-check.json
```

Prepare an agent handoff:

```bash
tokmd handoff \
  --preset risk \
  --budget 128k \
  --strategy spread \
  --review-packet-dir .tokmd/swarm-user-path-smoke/review \
  --review-packet-check target/tokmd/swarm-user-path-smoke/review-packet-check.json \
  --affected target/proof/swarm-user-path-smoke/affected.json \
  --proof-plan target/proof/swarm-user-path-smoke/proof-plan.json \
  --out-dir .handoff/swarm-user-path-smoke
```

## Open First

1. `.tokmd/swarm-user-path-smoke/review/review-map.md`
2. `.tokmd/swarm-user-path-smoke/review/comment.md`
3. `.tokmd/swarm-user-path-smoke/review/evidence.json`
4. `target/tokmd/swarm-user-path-smoke/review-packet-check.json`
5. `.handoff/swarm-user-path-smoke/work-order.md`

## Observed Result

- Affected planning found 2 changed files, 1 matched scope (`user_guides`),
  and 0 unknown files.
- Proof planning selected 1 required command and 0 advisory commands:
  `cargo xtask docs --check`.
- Required proof executed 1 command and passed 1/1 in
  `target/proof/swarm-user-path-smoke/proof-run-summary.json`.
- Review packet verification passed with 5 packet artifacts, 5 verified hashes,
  and no verifier errors.
- The review packet reported 0 available, 0 degraded, 0 stale, 1 skipped,
  5 unavailable, and 0 missing evidence entries.
- The handoff bundle wrote `.handoff/swarm-user-path-smoke`, linked review and
  proof artifacts, bundled 60 of 1597 scanned files, and used the full 128k
  token budget.

## What To Check

- `review-map.md` starts from the changed docs and explains why they matter.
- `review-map.md` reproduction commands use
  `.tokmd/swarm-user-path-smoke/review`.
- `evidence.json` distinguishes available, missing, stale, degraded, skipped,
  and unavailable evidence.
- `proof-plan.json` is treated as planned proof until required proof runs.
- `work-order.md` points at linked review and proof artifacts without telling
  the agent to read itself as source evidence.
- `work-order.md` links the affected report and proof plan, then separately
  reminds the agent that a proof plan is not executed proof.

## What Was Clear

- `review-map.md` put `docs/examples/swarm-user-path-smoke-run.md` first and
  `docs/examples/README.md` second.
- Review-map reproduction commands preserved the actual packet directory,
  `.tokmd/swarm-user-path-smoke/review`.
- `work-order.md` summarized changed files, the matched proof scope, the linked
  review verifier, the proof-plan command, and stop conditions without requiring
  the agent to inspect every JSON file first.
- The executed proof receipt stayed separate from the proof plan, which keeps
  planned evidence and executed evidence distinct.

## What Was Deferred

- The cockpit packet did not import `proof-run-summary.json`; it linked proof
  through the handoff bundle instead. Import proof artifacts into cockpit when
  the review packet itself needs to display executed-proof evidence.
- This run did not produce browser, publishing, release, coverage, mutation,
  Codecov, Nix, macOS, Windows, or release-signing evidence.

## What Not To Infer

- A verified review packet is not a merge verdict.
- A handoff bundle links review and proof artifacts; it does not verify those
  external artifacts itself.
- Planned proof has not passed until the required proof command executes.
- Missing, unavailable, skipped, stale, or degraded evidence is not passing
  proof.
- Advisory proof, coverage, mutation, browser output, and Codecov upload remain
  advisory unless policy explicitly promotes them.
- This smoke run proves the workflow for the recorded range, not permanent
  readiness for future changes.
