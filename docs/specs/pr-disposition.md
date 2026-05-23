# Spec: PR Disposition

- Status: active
- Schema family, if any: n/a
- Related ADRs: `docs/adr/0011-pr-disposition-lifecycle.md`
- Related proof scopes: `project_truth_docs`, `doc_artifacts_policy`

## Contract

PR disposition is the repository contract for deciding whether an open PR
should merge, remain open, park for later, be restacked, or close.

The contract applies especially during release prep, RC hardening, generated PR
drains, bot queue recovery, and swarm publication loops, where queue pressure
can otherwise obscure useful work. It preserves three separate facts:

- whether the PR's change is aligned with accepted product, release, or
  repository direction;
- whether the PR has enough current proof to merge now;
- whether the PR should close, park, restack, or stay open.

Closing a PR is a substantive decision. A PR must not close merely because a
release is near, because the queue is large, because the author is an agent or
bot, or because the PR includes checked-in provenance such as `.jules/**`.

## Inputs

Disposition decisions should use checked and reviewable evidence:

| Input | Owner | Used for |
| --- | --- | --- |
| PR title, body, diff, and comments | PR author and reviewers | Stated intent, scope, evidence, and review discussion. |
| Hosted checks and local proof output | CI and maintainers | Whether the current head is validated enough to merge. |
| Source-of-truth docs, specs, ADRs, plans, and policy files | Repository | Whether the change aligns with accepted direction and current contracts. |
| Release ledgers, release-readiness docs, or closeouts | Release lane | Whether a PR is release-blocking, safe aligned work, or non-blocking. |
| Duplicate or keeper PR links | Queue owner | Whether another PR already merged or owns the same useful slice. |
| Branch freshness and conflict state | Git/GitHub | Whether the PR can merge as-is, needs restack, or is stale beyond practical repair. |

## Outputs

Every material PR disposition should fit one of these classes:

| Class | Meaning | Expected action |
| --- | --- | --- |
| Release blocker | Needed before the current release or RC can proceed. | Fix, validate, and merge when aligned. |
| Safe aligned change | Narrow, correct, and compatible with the current lane. | Validate and merge when checks pass. |
| Useful non-blocking work | Aligned but not required for the current release or lane. | Leave open, park, label, or restack for later. |
| Duplicate of a merged keeper | Another merged PR already carried the useful change. | Close with keeper link and preserve any follow-up. |
| Invalid or incorrect work | The change is wrong, unsafe, misleading, or fails required proof. | Close or request replacement with the substantive reason. |
| Stale branch needing restack | The idea remains useful, but the branch cannot merge cleanly or safely now. | Restack when valuable; close only if practical restack is no longer worthwhile. |
| Explicitly declined work | Maintainers reject the direction after review. | Close with the accepted-direction reason. |

Merge decisions must include proof appropriate to the change. Closing decisions
must name the intrinsic reason and, when relevant, link the keeper PR, accepted
direction, failing evidence, or restack path.

## Compatibility

This spec does not change GitHub branch protection, required checks, merge
methods, auto-merge settings, release workflows, proof promotion, Codecov
defaults, or publication policy.

Existing PR bodies, comments, labels, and release ledgers remain valid evidence
surfaces. The spec only makes the disposition vocabulary and evidence
expectations durable so future agents and maintainers do not depend on chat
history or queue-cleanliness heuristics.

Generated, bot-authored, Jules-authored, Codex-authored, and human-authored PRs
use the same disposition classes. Provenance files are evaluated with the
substantive change; their presence alone is not a close reason.

## Proof Requirements

For documentation-only changes to this contract:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-pr-disposition.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-pr-disposition.json --evidence-json target/proof/proof-evidence-pr-disposition.json
cargo fmt-check
git diff --check
```

For actual queue-disposition work, proof should also include:

- current open PR list or the scoped PR cluster under review;
- per-PR classification and rationale;
- validation evidence for PRs merged;
- keeper links for duplicates closed;
- explicit stale/conflict evidence for PRs closed as impractical to restack.

## Open Questions

- Whether repeated queue drains should write a lightweight machine-readable
  ledger under `target/` for reviewer convenience.
- Whether PR labels should mirror the disposition classes exactly or remain
  local to each queue-drain lane.
