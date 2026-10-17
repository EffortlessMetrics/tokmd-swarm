# Qualification request candidate (#734)

This is an **unpublished, incomplete migration candidate**, not an accepted CI
contract. It must not replace the production workflow in its present state.
Current main and the qualified #732 source have not been changed by this work.
The deliberate dispatch refusal in `scripts/ci_qualification.py` is a concrete
implementation constraint, not an approval flag to flip.

The user's requirement is: “I do not want CI runs longer than 20 minutes, and I
do not want CI runs early.” Native evidence establishes early admission. It does
not establish that the two cited runs exceeded 20 minutes: #732 replacement run
39067784935 has a 19m59s creation-to-update interval; #733's reported interval is
13m48s, without a native head/run receipt independently supplied to this lane.
Run lifetime, job duration and command execution duration are distinct measures.

## Implemented candidate control path

The existing `workflow_dispatch` interface now accepts a PR number, exact head
and main SHAs, an exact `head:base` generation, and an explicit expected-green
assertion. Admission verifies both native run SHA and native workflow-source SHA
against the requested head, then reads the PR head and main Git source refs.
Malformed, missing, stale, ordinary PR, Draft and Ready activity cannot admit
heavyweight proof. Source read errors fail closed; no alternate access is tried.
The main push path retains post-integration proof.

All 19 existing jobs depend on admission. The existing `Tokmd Rust Result` name
is retained with an always-scheduled, hosted refusal path: missing or failed
admission cannot turn its job into a successful skipped qualification. No token,
runner credential, permission block, required command, dependency or protection
is relaxed. Refused requests do not enable the advisory review action either.

Qualification concurrency uses PR plus the requested head/base generation;
observation events use another group. Cancellation is disabled. `queue: max`
uses GitHub's bounded native pending queue. Repeated requests serialize; **they
are not deduplicated**. A new generation does not cancel another valid run.
An obsolete request is refused on source-ref revalidation when its admission
starts; the candidate does not cancel already running work or claim to detect
head/base movement after admission. Native queue ordering and capacity remain
platform behavior, not guarantees established by these local tests.

The Rust producer uses one admission deadline for all four retained serial Cargo
obligations, including elapsed admission/routing/setup time; it no longer resets
four 15-minute command allowances. Expired budget writes timeout markers without
launching Cargo. The obsolete unbounded retry and synchronous marker wait are
removed. The current job timeout is reduced to 20 minutes as a final backstop.
These changes **do not demonstrate successful complete qualification within 20
minutes**. Satellite actions, queue waits, advisory review and platform teardown
are not yet governed by that shared deadline.

## Why valid dispatch currently refuses proof

After positive request validation, production admission deliberately refuses with
`dispatch lacks the native PR context required by the current risk/proof lanes`.
Removing this refusal would silently omit existing obligations:

* `affected-plan` and `fast-proof-run` require a `pull_request` event.
* Risk routing reads native PR labels and compares its base; dispatch has neither
  `github.event.pull_request` nor `github.base_ref`.
* UB Review is PR-context dependent; the default checkout on dispatch is the
  selected source ref, whereas ordinary PR proof uses the merge ref.
* A Git source ref verifies commits, not whether a PR is still open or its full
  native metadata. The separate admission job has only existing `contents: read`.
  The existing required job's PR token was not copied or expanded into it.

Consequently the candidate preserves all old commands and predicates but does
not claim a positive end-to-end dispatch qualification. A complete implementation
needs a bounded way to obtain/revalidate native PR context, bind the tested merge
composition and preserve every risk/default proof lane before admitting work.
This is source/control-path work still required, not an external approval gate.

A second concrete gap is fork binding: manual dispatch's native run SHA is the
selected branch/tag in the workflow repository. Checking out a fork-only SHA
does not change that native check subject. The head/workflow comparison refuses
that substitution. A fork-safe explicit-request path preserving the existing
required check attachment must be established before this migration can land.
No external status writer or new controller is proposed.

Finally, existing native timings offer almost no evidence of spare headroom for
serialized baseline refusal and full proof. Cargo Deny remains an independent
blocking obligation in the installed lane catalog. This candidate neither moves
it behind a same-repository guard nor turns its failure advisory. It also does
not yet place Cargo Deny before expensive Rust proof. A separately measured
optimization/baseline admission step and an actual shared execution budget for
all retained work are needed; a 20-minute timeout alone is insufficient.

## Local proof and next action

Run `python3 scripts/ci_qualification_test.py`. Tests execute the actual admission
entrypoint, required-check refusal shell, and core producer shell with controlled
commands. They cover positive subject validation, malformed/missing request,
Draft/Ready/label activity, stale head/base/generation, wrong native source,
source-read failure, repeated/concurrent subject grouping, and expired budget.
The positive entrypoint case asserts its truthful migration refusal, not a green
qualification. No full CI run, GitHub dispatch or Cargo build is needed.

The next bounded source step is to preserve PR context and tested composition in
the supported dispatch path without widening credentials, while proving a native
fork-safe required-check path. Then exercise positive/refusal/concurrency cases
again and independently review the exact head. Only an accepted, fully qualified
migration should replace live legacy enforcement. #729 dependency maintenance
and #732 implementation/integration remain with their existing owners.

Platform references: [dispatch and PR event subjects](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows),
[native concurrency and queue behavior](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
These sources explain platform semantics; they do not prove this repository's
live protection settings or candidate runtime behavior.
