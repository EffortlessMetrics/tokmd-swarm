# Qualification request candidate (#734)

This is an incomplete source candidate, not an activated migration or a runtime
qualification. The reviewed checkpoint `7610a14c` remains preserved. Continuation
work changes existing orchestration rather than adding a controller/status
service. Main and #732 source are independently owned and are not replaced by
this candidate.

The requirement is: “I do not want CI runs longer than 20 minutes, and I do not
want CI runs early.” A configured ceiling or passing local deadline fixture is
not evidence of successful complete qualification within that bound.

## Request and subject

A deliberate `ci:qualify` label event submits exactly one PR-body manifest:

```text
<!-- tokmd-qualification:v1
{"pull_request":"<number>","head_sha":"<full head SHA>","base_sha":"<full main SHA>","merge_sha":"<full PR merge SHA>","generation":"<head SHA>:<main SHA>","deadline_epoch":"<UTC epoch no more than 20 minutes ahead>","expected_green":"true"}
-->
```

Admission requires an open PR to main, matching native PR/head/base/merge and
workflow source, actual checkout and current Git head/main/merge refs. The single
request expiry is shared across workflow/job starts; later phases cannot renew
it. Each job anchors that remaining wall-clock allowance once to its monotonic
clock. Revalidation retains the anchor. Wrong/malformed/stale/expired subjects
refuse before proof. The supported dispatch interface validates exact inputs but
still refuses proof because it lacks the native PR context of the retained
risk/affected/review obligations. Checkout labels do not change native subjects.

All ten other inventoried PR workflows with Cargo/Rust work now select labeled
PR events and reuse the same validator before heavyweight setup. Branch/path
filters, non-PR triggers, action pins, existing permission blocks, commands and
actual proof requiredness are retained. No security/release workflow or registry
behavior is changed. Mere Draft/Ready activity does not select those expensive
paths. An unrelated label does not pass the validator.

The required Rust job uses `Tokmd Rust Result` for deliberate requests. Ordinary
observations use another name so they cannot overwrite its existing qualified
head or emit a skipped-success required context. **Remaining source work:** apply
and validate equivalent non-interference to the other check contexts before
publishing/activating this continuation; unrelated label runs can still affect
those context names. Do not treat concurrency isolation alone as check-name
isolation. No branch protection change is proposed.

## Shared work boundary

The existing core command boundary now owns one unique child process group and
uses one monotonic deadline. Timeout/cancellation sends TERM, retains the leader
PID without reaping during its finite grace, kills the remaining group and
records the distinct outcome. Source exits including 124 and 137 are not guessed
to be timeouts. Cancellation stops later proof commands; an independent new
invocation can recover. The four locked Cargo obligations remain intact.

Setup/toolchain/cache/review/upload allowance comes from the same anchor. The
existing cache and review caps remain tighter where applicable. Core work
reserves finalization time. Satellite and companion phases refresh their
remaining allowance and use native step timeouts, with final source/deadline
validation after body work. Windows retains its actual test command and uses
native runner cancellation, not the POSIX child-group implementation. That
Windows cancellation behavior has not been exercised by these local Linux tests.

The existing blocking risk-path classifier remains blocking and executes in
parallel with Cargo Deny, as in production. Both precede dependent platform
proof. Full Rust proof waits for Cargo Deny. No proof is removed/reclassified to
shorten the critical path. Every old catalog entry remains authoritative; new
admission/context metadata describes the proposed controls.

A narrow reporter optimization reuses an already installed compiler only when
its full version and toolchain equal the compiler selected by the current
required job. Otherwise the existing toolchain action runs. It changes neither
the selected compiler nor cache policy. The final finite Linux summary script
uses the existing command boundary, including sub-minute remaining time. Earlier
proof/source failures remain failures even when finalization succeeds.

Native job setup/download, queue delay, cancellation grace and post-job actions
remain platform behavior. Native action timeouts accept positive integer minutes;
a source-side timer cannot prove a strict service creation-to-completion bound.
No hard whole-service guarantee is claimed from these source controls. Native
qualification must measure the complete workflow, including Windows and final
reporting, not merely the named Rust job.

## Actual retained critical-path evidence

Main `0aa2dd065b51c8fbb927b752648b98b9cba124a9` has the exact tree of qualified
#735 head `463b885fd98a9ea430e69d37b45fdbdedb4b1c10`. All returned non-skipped jobs
of run39071174151 completed successfully. The Windows log reports **no cache
found**, compilation from21:31:42.166 to21:38:24.174 (6m42s observed window), then
test execution through21:48:07.828 (about9m44s). Its first/last log timestamps span
17m02.6s. These are log windows, not invented job timestamps.

Cargo Deny's log spans9.1s, risk detection73.5s, and final CI Actuals31.6s. The
last CI Actuals log line is21:48:46.902, 19m00.9s after the supplied run creation
21:29:46. That is supporting chronology, not an independently read native
workflow-completion timestamp. It includes the actual Windows path.

The reporter's toolchain action spent8.6s and reported an existing, unchanged
stable compiler (`rustc 1.99.1`). That supports the exact-compiler reuse path;
no cache hit, compiler-version change or cold-build time saving is assumed.
Local static-tool reuse used the separately pinned1.95.0 environment and is not
relabeled as that native compiler proof. No full Cargo build or remote run was
started for these controls, and active735 work was not interrupted.

## Validation and next native step

Local suites cover the actual workflow guard/phase expressions, ordinary versus
explicit request naming, exact source/expiry, Windows route selection, repeated
request/concurrency groups, real child timeout/cancellation/descendants/recovery,
unique source failures and final receipts. Source invariants/catalog checks and
independent exact-head review are required for the continuation; the old review
is not relabeled as review of this new source.

After completion/review and composition with accepted main, the supported native
activation is a candidate PR whose changed `pull_request` workflow runs from its
merge subject. Opening/Draft is a cheap observation; the single deliberate label
with a fresh exact manifest admits actual proof. A full expected-green request
must include Windows and all selected obligations. Manual dispatch does not
substitute for PR/fork attachment. A legitimate fork subject and any existing
platform approval must be evidenced without granting credentials or access.
No speculative full run or live activation has occurred.

Before integration inspect actual publication triggers. The existing Swarm GHCR
workflow publishes on selected main changes. The parent's hold on publication-
causing #732/#733 merges remains in force; registry/permission behavior must not
be disabled to bypass it. This candidate's own final main-relative path set must
be checked separately before any integration.

References: [workflow events and subjects](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows),
[native timeouts and steps](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax),
[native concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
