# Plan: Product Evidence Alignment

- Status: active
- Related proposal:
- Related spec: [Product contract](../PRODUCT.md), [Handoff schema](../handoff-schema.md)
- Related ADR:
- Related issues: [#523](https://github.com/EffortlessMetrics/tokmd-swarm/issues/523), [#670](https://github.com/EffortlessMetrics/tokmd-swarm/issues/670)
- Evidence baseline: `5f5ca66c0bb353e969947519ac6dca219854acaf`, audited 2026-10-10

## Goal

Make tokmd's claims agree with what it actually selected, read, measured,
emitted, and verified. The named consumer is a person or coding agent using
inspect, cockpit review evidence, and context/handoff output. The release
consumer is the existing candidate/stable verifier, not a new controller.

This is a fresh product-gap selection under the criteria in [NEXT](../NEXT.md),
not reopening completed lanes by inertia. Historical closeouts remain evidence
of their original scope, not blanket qualification of newly identified cases.
The GitHub issues below own current work; Jules-local paused state is not this
plan's controller.

## Non-goals

No new review command, global merge controller, evidence backplane, wrapper
receipt, automatic schema promotion, default AST promotion, major dependency
wave, or general caching initiative. No publication, deployment, tag, image,
mutable alias, credential, or protection changes are authorized by this plan.

## Work Packets

| Packet | Required outcome | Boundary |
| --- | --- | --- |
| [#684](https://github.com/EffortlessMetrics/tokmd-swarm/issues/684) | Full and HeadTail packing use the recorded policy estimate. | Selection, not a hard rendering cap. Candidate: [#690](https://github.com/EffortlessMetrics/tokmd-swarm/pull/690). |
| [#685](https://github.com/EffortlessMetrics/tokmd-swarm/issues/685) | Bound head/tail bytes and retained memory even for huge lines. | Coordinate #684 accounting; define framing and UTF-8 cuts. |
| [#686](https://github.com/EffortlessMetrics/tokmd-swarm/issues/686) | Inventory and completion status agree with emitted files. | Required read failure is not silent success. |
| [#687](https://github.com/EffortlessMetrics/tokmd-swarm/issues/687) | Failed replacement cannot advertise a mixed complete generation. | Depends on #686 outcome semantics; cross-platform switch and receipt integrity. |
| [#688](https://github.com/EffortlessMetrics/tokmd-swarm/issues/688) | Record actual complexity coverage and read outcomes. | Measurement-pass facts, not reconstructed guesses. |
| [#689](https://github.com/EffortlessMetrics/tokmd-swarm/issues/689) | Ratios and comparisons use compatible measurement scopes. | Depends on #688; explicit schema/baseline compatibility decision. |
| [#691](https://github.com/EffortlessMetrics/tokmd-swarm/issues/691) | Measure fixed-budget scaling before optimizing membership lookup. | Follow-on lane; preserve semantic output and correctness. |

Every issue contains source evidence, a bounded PR scope, discriminating
regressions, and resource/compatibility limits. A passing arithmetic model is
not native execution. An open PR is not a completed implementation.

## Existing Release Work

Keep [#535](https://github.com/EffortlessMetrics/tokmd-swarm/issues/535) as the
single release-operation controller and #670 as source admission/freeze owner.
Bug corrections and larger schema changes need separate inclusion/version
decisions; do not silently add the entire table to v1.15.1.

* [#681](https://github.com/EffortlessMetrics/tokmd-swarm/pull/681) / #636:
  finish config-relative gate recovery-test migration and exact-head proof.
* [#682](https://github.com/EffortlessMetrics/tokmd-swarm/pull/682) / #673:
  preserve bounded visibility retries and fix the full three-attempt restoration
  sequence. A newer metadata-only artifact must not conceal an older
  unrecoverable publication receipt.
* [#683](https://github.com/EffortlessMetrics/tokmd-swarm/pull/683) / #674:
  reject duplicate consumer receipts and failed/missing producer outcomes.
  This is aggregate consistency, not authenticated source/run/digest binding.
* #674/#676 retain finalization and authenticated evidence ingestion; #675 owns
  the serialized alias promoter; #679 owns transaction negative tests.
* #677 protected import and #678 candidate/RC qualification precede #680 stable
  operational acceptance. #680 depends on #679, never itself. Post-publication
  evidence is not a prerequisite for implementing its verifier.

No item here permits a public semver-tag rewrite or bypassing required checks.
Do not discard #682's publisher tests when integrating #683's additions to the
same existing Python test module.

## Acceptance Matrix

| Consumer job | Positive and negative proof |
| --- | --- |
| Inspect | Stable inventory and grouping; explicit exclusions; repeat runs do not ingest their own output. |
| Review a change | Correct base/head; explicit dirty/shallow/missing-ref behavior; useful recovery. |
| Evaluate a gate | Config provenance independent of invocation directory; missing/incompatible evidence cannot pass. |
| Prepare context | Greedy/spread and multibyte fixtures; selection/charges/emitted bytes reconcile; omissions are visible. |
| Refresh a bundle | Failed writes preserve a usable prior generation or an explicit incomplete state; tampering is rejected. |
| Interpret metrics | Unsupported/unreadable/limited input cannot fabricate zero complexity or an improved grade. |
| Use installed artifacts | The exact candidate's installed binary, Action modes and released browser subset complete their documented journeys. |

Extend existing tests and #678/#680 qualification rather than assuming all
these cases are absent or creating a parallel release matrix. Unsupported
browser capabilities remain explicit under the [capability matrix](../browser-capability-matrix.md).
Do not promote workspace execution to installed-artifact proof.

## Proof and Resource Discipline

For the first two implementation candidates, retain exact source/blob identities
and independent review findings. Scoped local commands, when tools are present:

```sh
python -m unittest scripts/release_consumer_smoke_aggregate_test.py
cargo test --locked -p tokmd context_pack --verbose
cargo test --locked -p tokmd --test context_budget_accounting --verbose
```

These are execution instructions, not a record that they all ran. Require
nonzero execution of the selected regressions and retain existing proof-policy
routing. Preserve the required `Tokmd Rust Result`; no new mandatory workflow
or default advisory-proof/coverage promotion is part of this plan.

Use temporary kilobyte-scale fixtures and controlled faults before broader
benchmarks. CLI children must be bounded. Record memory/timing as unmeasured
where instrumentation is absent. No cache, parser, or runner expansion without
measured consumer benefit and a separate bounded scope.

## Closeout

Close individual issues only with implemented behavior, native or executable
regressions, compatible docs/schemas, independent exact-head review, and required
checks through the protected path. Source admission should update NEXT's selected
work summary without erasing historical checkpoint evidence. Release status,
installed qualification, and alias verification remain the existing graph's
responsibility. This plan does not declare the product or release complete.
