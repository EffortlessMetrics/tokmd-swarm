# Qualification request candidate (#734)

This is an **unpublished, incomplete migration candidate**, not an accepted CI
contract. It must not replace production until the remaining timing and native
integration gaps below are resolved. Main and #732 source remain unchanged.

The user's requirement is: “I do not want CI runs longer than 20 minutes, and I
do not want CI runs early.” Native evidence establishes early admission. The
cited runs do not establish execution exceeding 20 minutes: #732 replacement
39067784935 has a 19m59s creation-to-update interval. #733's reported interval is
13m48s, without a native head/run receipt independently supplied to this lane.
Run lifetime, job duration and command execution duration are distinct measures.

## Explicit source and composition request

The existing `workflow_dispatch` interface now accepts a PR number, exact head
and main SHAs, `head:base` generation, and an expected-green source/review
assertion. Admission compares both native run SHA and workflow-source SHA with
the requested head and reads live Git source refs. A valid dispatch then refuses
proof explicitly: dispatch lacks the native PR context required by the existing
risk, affected-proof and review lanes. This refusal is an implementation
constraint, not an approval flag to flip. Checkout or receipt labels cannot
change a dispatch run's native subject, including for fork-only commits.

A positive path instead preserves the native PR event and merge subject. The
requester puts exactly one manifest in the PR body, then deliberately adds the
`ci:qualify` label. A label alone, persistent label, ordinary Draft opening,
Ready change, unrelated label, synchronize event or changed request does not
admit proof. The manifest has this shape (replace every placeholder):

```text
<!-- tokmd-qualification:v1
{"pull_request":"734","head_sha":"<full head SHA>","base_sha":"<full main SHA>","merge_sha":"<full PR merge SHA>","generation":"<head SHA>:<main SHA>","expected_green":"true"}
-->
```

Admission requires an open PR targeting main, the event PR/head/base matching
the request, and native run/workflow source matching its exact merge commit.
Live head/main/merge source refs must still match. The actual checkout is also
checked against the native run SHA. Source reads fail closed and use no token.
The same source validation runs before setup and after proof, without resetting
the deadline. A moved head/base/merge invalidates the old request before a passing
verdict. Request body withdrawal after the event is not independently monitored;
this is not a controller or live cancellation service.

All 19 existing jobs depend on admission. `Tokmd Rust Result` retains its name
and an always-scheduled hosted refusal path, so failed admission or baseline
cannot become skipped-success qualification. Refusal does not enable advisory
review. Existing permission blocks, action pins, required commands and risk
predicates remain intact. The PR path retains native fork context and hosted fork
routing; its actual required-check attachment still needs native qualification.
Main push retains post-integration proof. Current live legacy enforcement is
unchanged because this candidate has not been published or activated.

Cargo Deny remains blocking and runs before Rust and the other substantive proof
lanes. Its failure refuses expensive proof rather than being reclassified. No
#729 dependency repair is included.

## Concurrency and work budget

Explicit native PR requests use PR plus native merge subject for concurrency;
dispatch requests use PR plus generation. Observation events have another group.
Cancellation is disabled. `queue: max` uses GitHub's bounded native pending queue.
Repeated exact requests serialize; **they are not deduplicated**. A new generation
does not cancel a valid run. Pending requests revalidate source at admission;
active proof revalidates before setup and after proof. No arbitrary run is
canceled. Native queue capacity/ordering is platform behavior, not a local test
result, and no end-to-end queue service bound is asserted.

A deadline starts at the first admission step, before checkout. All four retained
serial Cargo commands consume its remaining allowance instead of four fresh
15-minute budgets. Expiration writes timeout markers without launching Cargo.
The obsolete unbounded retry and synchronous marker wait are removed. The Rust
job has a 20-minute native backstop, and its observability receipt reports that
actual configured limit. These changes are a bounded core control, **not proof
that complete qualification succeeds within 20 minutes**.

Satellite actions, advisory review and native cleanup/teardown are not yet all
governed by the shared deadline. Queue delay can exceed any job execution bound.
Putting the blocking baseline before Rust also changes the critical path. The
19m59s observed run offers no measured margin to absorb that serial dependency;
no duration for the reordered candidate is claimed. A measured optimization and
complete action/deadline accounting are required before a full run. Simply
reducing timeouts would manufacture failure ceilings rather than meet the user's
successful-completion requirement. Cache setup work #567 remains separately
owned; this candidate does not take it over.

## Local proof and exact next action

Run `python3 scripts/ci_qualification_test.py` and
`python3 scripts/ci_observability_clock_test.py`. Tests execute the checked-in
admission entrypoint, checkout/refusal shells and core producer with controlled
commands. Positive PR request/composition admission, malformed/missing request,
Draft/Ready-only events, closed/wrong PR, stale head/base/merge/generation, wrong
native source, source-read failure, red baseline, and repeated/concurrent request
groups are covered. The actual group expression is evaluated locally for its
supported expression subset; this is not native concurrency service proof.
Positive budget tests retain all four commands and see one decreasing allowance;
expired-budget tests launch none. Source-failure markers remain failures.

The next bounded source step is complete shared action/cleanup budgeting and a
measured optimization of the reordered critical path. After exact-source review,
native qualification must establish named check attachment, fork behavior and
full obligation selection on an explicit request, while ordinary events remain
cheap refusals. No speculative full run, PR publication, permission expansion,
new status writer/controller, policy relaxation or #732 change is included.

Platform references: [dispatch and PR event subjects](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows),
[native concurrency and queue behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
These explain platform semantics, not this repository's live protections or a
measured result for this candidate.
