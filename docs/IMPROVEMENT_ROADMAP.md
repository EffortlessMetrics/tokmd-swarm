# tokmd-swarm Improvement Roadmap

`tokmd-swarm` exists to turn useful improvement opportunities into narrow,
reviewable PRs.

This is not a release roadmap. It does not track package visibility, registry
state, release publishing, GHCR, crates.io, npm, GitHub release assets, or
distribution verification. Those belong in issue-specific release operations,
not in this improvement roadmap.

This roadmap is for selecting and shaping improvement work after the generated
PR drain and swarm stabilization work. The goal is to keep the swarm pointed at
real project improvement instead of process churn.

## Current Mode

The current mode is improvement selection.

There is no default implementation lane. New work should start only when there
is a concrete improvement target:

- a maintainer or contributor pain point,
- a user-facing rough edge,
- a confusing review artifact,
- a fragile workflow,
- a stale or misleading document,
- a test gap,
- a measured performance bottleneck,
- a shadow-research question with a clear decision boundary.

Closed lanes stay closed unless fresh evidence identifies a real gap.

## Non-Goals

This roadmap intentionally does not cover:

- release verification,
- package publishing,
- GHCR visibility,
- crates.io visibility,
- npm/package distribution,
- GitHub release asset validation,
- release checklists,
- version milestone planning,
- proof promotion into required gates,
- default Codecov upload,
- broad architecture consolidation,
- public AST schema changes,
- new wrapper receipts without a named consumer,
- generic process documentation,
- generated busywork PRs.

A PR that only restates existing process is not roadmap work.

## Improvement Principles

1. Improve something a maintainer, contributor, user, or reviewer actually
   touches.
2. Prefer small, reversible PRs.
3. Prefer clearer behavior over more artifacts.
4. Prefer existing surfaces over new commands or schemas.
5. Prefer measured performance work over speculative optimization.
6. Prefer docs that help someone act over docs that merely describe governance.
7. Keep advisory evidence advisory unless maintainers explicitly decide
   otherwise.
8. Keep AST and parser-expansion work in shadow/evidence mode until comparison
   results justify public behavior.
9. Do not create a new lane just because the swarm is idle.
10. Every improvement PR should be able to answer: what pain did this remove?

## Active Improvement Themes

The following themes are valid sources of swarm work. They are not release
milestones and they do not need to happen in order.

### Priority Order

| Priority | Theme | Why |
| --- | --- | --- |
| P0 | Review evidence usability | Builds directly on recent cockpit/proof metadata and helps maintainers immediately. |
| P1 | Contributor/developer experience | Converts swarm stability into faster, safer follow-on work. |
| P2 | User-facing CLI quality | High product value without large architecture risk. |
| P3 | Test quality and diagnostics | Small PRs, high leverage, low risk. |
| P4 | Documentation that enables action | Useful when tightly tied to user/contributor actions. |
| P5 | Measured performance and feedback speed | Valuable only when evidence-led. |
| P6 | Browser/WASM capability honesty | Valuable product surface, but should remain capability-bounded. |
| P7 | Shadow AST evidence | Worth tracking, but not active productization. |

---

## Theme 1: Review Evidence Usability

### Goal

Make cockpit and review-packet evidence easier for maintainers and agents to
read, trust, and act on.

### Good Work Packets

1. **Review packet reading guide** with artifact order and question mapping.
2. **Evidence field glossary** for repeated proof/review concepts.
3. **Missing-evidence wording improvements** that clearly separate required vs.
   advisory states.
4. **Hosted comment troubleshooting** for token, fork, stale-marker, and
   API/rate-limit cases.

### Do Not

- Add a new `tokmd review` command yet.
- Promote advisory evidence into required gates.
- Create new evidence artifacts without a named consumer.

### Done When

- Review packets are faster to interpret.
- Advisory evidence is not confused with required proof.
- Missing evidence language is actionable.

---

## Theme 2: Contributor and Developer Experience

### Goal

Make it easier for contributors and agents to make correct local changes, run
the right checks, and recover from common failures.

### Good Work Packets

1. **Contributor quickstart** for first useful contribution flow.
2. **Local check guide** distinguishing fast confidence vs. exhaustive paths.
3. **Common failure guide** with recovery steps for recurring local/CI issues.
4. **Test fixture map** to reduce guesswork around fixture ownership.

### Do Not

- Duplicate existing docs wholesale.
- Add process policy for its own sake.
- Add release verification content.

### Done When

- New contributors can land small changes with less guessing.
- Agents select checks more reliably.
- Common failures have concrete recovery guidance.

---

## Theme 3: User-Facing CLI Quality

### Goal

Improve direct `tokmd` UX: help text, examples, diagnostics, progress, and
command discoverability.

### Good Work Packets

1. **Help examples** for common user flows.
2. **Error-message context** with recovery direction.
3. **Progress consistency** (stderr progress, script-safe stdout).
4. **Config explainability** through docs/messages first; narrow surfaces only
   if needed.

### Do Not

- Change JSON/JSONL outputs casually.
- Add new public schemas as a UX cleanup shortcut.
- Mix CLI UX work with release/distribution concerns.

### Done When

- Commands are easier to discover and recover from.
- Long-running behavior is less opaque.
- Scripted output remains stable.

---

## Theme 4: Test Quality and Diagnostics

### Goal

Improve confidence by making tests targeted, readable, and easier to debug.

### Good Work Packets

1. Replace low-value `unwrap()`/opaque `panic!()` sites in tests.
2. Improve assertion messages with fixture/mode/path context.
3. Split flaky or oversized tests into clear named cases.
4. Add focused regression tests for fixed bugs.
5. Improve snapshot naming/organization/review clarity.

### Do Not

- Add tests just to raise count.
- Rewrite large suites without specific pain.
- Promote advisory proof into required CI.

### Done When

- Failures localize quickly.
- Regressions are covered with clear intent.
- Snapshot and fixture changes are easier to review.

---

## Theme 5: Documentation That Enables Action

### Goal

Improve docs that help users, contributors, maintainers, or agents perform
concrete tasks.

### Good Work Packets

1. **Extension guides** (enrichers, presets, fields, fixtures, etc.).
2. **Architecture entry points** mapping intent to code location.
3. **Stale-doc cleanup** for obsolete crate/workflow references.
4. **API examples** aligned with current crate layout.

### Do Not

- Add generic governance/process summaries.
- Rewrite closed plans.
- Add release/package verification docs under this roadmap.

### Done When

- Contributors find code faster.
- Docs reflect current architecture.
- Examples are checked where feasible.

---

## Theme 6: Measured Performance and Feedback Speed

### Goal

Improve runtime and developer feedback speed only where measurements show real
bottlenecks.

### Good Work Packets

1. Refresh reproducible timing baselines for common local workflows.
2. Maintain a small repeatable perf-smoke path.
3. Apply focused hot-path cleanups with before/after evidence.
4. Improve CI failure clarity so developers know what to run locally.

### Do Not

- Speculate without measurements.
- Add heavy performance machinery nobody runs.
- Hide slow checks by weakening proof.

### Done When

- Current bottlenecks are known.
- At least one measured improvement lands.
- Optimization PRs include evidence and rollback story.

---

## Theme 7: Browser/WASM Capability Honesty

### Goal

Improve browser/WASM behavior and docs so users know what works and what cannot
work in hostless contexts.

### Good Work Packets

1. Refresh capability matrix (supported/partial/unsupported + why).
2. Add practical browser examples for supported flows.
3. Investigate rootless preset feasibility with explicit evidence.
4. Improve browser-mode errors for unsupported host-backed capabilities.

### Do Not

- Claim host/git-backed features work in browser mode when they do not.
- Silently degrade important fields.
- Mix browser capability docs with distribution concerns.

### Done When

- Capability boundaries are explicit.
- Unsupported paths fail clearly.
- Browser docs match behavior.

---

## Theme 8: Shadow Analysis and AST Evidence

### Goal

Continue AST/parser exploration as shadow evidence until maintainers choose a
public behavior change.

### Good Work Packets

1. Corpus proposals with explicit constructs and success criteria.
2. Expanded shadow comparisons for targeted questions.
3. Mismatch taxonomy that supports decisions.
4. Candidate decision notes once evidence is broad enough.

### Do Not

- Make AST default.
- Change public receipts/schemas from shadow work.
- Present shadow evidence as product behavior.

### Done When

- Evidence is broader and better classified.
- Maintainers can make a concrete candidate decision.

---

## Parked Themes

- **MCP / Server mode** (parked pending stable command/artifact contracts).
- **Plugin system** (parked pending real consumer + extension contract).
- **New review command** (parked; improve cockpit/review packet UX first).
- **Architecture consolidation** (parked unless fresh evidence shows real pain).

## Work Selection Rules

A swarm PR is desirable when it removes concrete pain, improves user-facing
behavior, improves review evidence consumption, improves local development
feedback, fixes stale docs, adds focused regression coverage, or advances
measured/shadow evidence toward a decision.

A swarm PR is suspicious when it mainly restates process, layers additional
policy artifacts, edits closed plans without evidence, introduces wrapper
artifacts without consumers, promotes advisory proof by default, or adds
release/package verification content.

## Required Proposal Shape

Use this shape before starting a non-trivial lane:

```markdown
## Proposal: <name>

### Consumer

### Pain

### Evidence

### Smallest useful slice

### Files or surfaces touched

### Proof

### Non-goals

### Rollback
```

## PR Size Guidance

Prefer one primary purpose per PR.

Good shapes include:

- one doc plus one pointer,
- one help/error improvement plus tests,
- one review-packet wording improvement plus snapshots,
- one fixture map,
- one perf baseline,
- one targeted regression test,
- one stale-doc cleanup.

Avoid mixing broad docs+behavior+schema changes in a single PR.

## Improvement Review Checklist

Reviewers should be able to answer:

- What pain does this remove?
- Who benefits?
- Is scope narrow?
- Is rollback simple?
- Are public outputs stable unless intentionally changed?
- Are advisory and required evidence still separated?
- Does this avoid release/package concerns?
- Does this avoid process for process's sake?
- Are checks appropriate for the change?

If “what pain does this remove?” is unclear, rework or close the PR.
