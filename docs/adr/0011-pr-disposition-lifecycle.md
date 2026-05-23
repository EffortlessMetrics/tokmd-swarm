# ADR-0011: PR disposition lifecycle during release prep

- Status: accepted
- Date: 2026-05-23

## Context

tokmd uses pull requests as review packets, proof carriers, generated-work
intake, and release-prep work queues. During release hardening, a clean PR queue
can look attractive, but closing useful work merely because a release is near
destroys context and can hide release-relevant evidence.

The current durable guidance already says that PRs should be classified by
substance, not by whether they make the queue smaller. That rule appears in
agent guidance and the source-of-truth routing model, but the durable decision
and behavior contract need to be discoverable from ADR/spec artifacts as well.

## Decision

PR disposition is evidence classification, not queue grooming.

During release prep, RC hardening, or queue recovery, maintainers and agents
must classify each PR by its substance before merging, parking, restacking, or
closing it. Accepted disposition classes are:

- release blocker;
- safe aligned change;
- useful non-blocking work;
- duplicate of a merged keeper;
- invalid or incorrect work;
- stale branch needing restack;
- explicitly declined work.

Release blockers and safe aligned changes may merge after validation. Useful
non-blocking work should remain open, parked, labeled, or restacked for later.
A PR should close only for an intrinsic reason: it is invalid, duplicated or
superseded by a merged keeper, stale beyond practical restack, conflicts with
accepted direction, or was explicitly declined.

Queue cleanliness is not a release criterion. Release readiness is proven by
preflight, release-record accuracy, and clean release-surface evidence.

## Consequences

- Queue drain work must leave a disposition rationale that a later maintainer
  can audit.
- Generated, bot, Jules, Codex, or human PRs are reviewed by substance rather
  than by source.
- Release prep can proceed without pretending that every useful non-blocking PR
  must either merge immediately or close.
- Duplicate clusters should identify the keeper and preserve any useful
  follow-up from closed duplicates.
- Stale branches are restack candidates when the underlying work remains useful.

## Alternatives

- Close all non-release PRs during release prep.
- Merge only PRs that are already green and close the rest.
- Treat generated or agent-authored PRs as disposable by default.
- Keep disposition rules only in agent guidance.

These alternatives were rejected. They optimize queue appearance over release
truth, lose useful evidence, and make later queue recovery depend on chat or
agent-specific state instead of durable repository artifacts.

## Enforcement

- `docs/specs/pr-disposition.md` owns the behavior contract and disposition
  evidence expectations.
- `AGENTS.md` and `agents/shared/repo.md` should stay aligned with the spec for
  agent-facing operational guidance.
- PR bodies, comments, labels, or close comments carry review-local disposition
  evidence.
- Release ledgers or closeouts may summarize disposition groups, but they must
  not replace PR-level rationale when a PR is closed.

## Related specs

- `docs/specs/pr-disposition.md`
- `docs/source-of-truth.md`
