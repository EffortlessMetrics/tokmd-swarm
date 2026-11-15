# Security Scan Report

**Generated:** 2026-10-19
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-10-12 → 2026-10-19
**Observed Scope:** Checked-out commit `46b8963 fix(ci): require exact
qualification requests and share work deadline (#737)` from 2026-10-17. The
observed checkout contains exactly one commit; `git log --since="7 days ago"`
returned that single commit, and there is no reachable parent in the
working clone. The scan does not independently prove window completeness:
the appendix records only the local `git log` and `git show` invocations,
so the observed-scope claim is bounded to the inspected commit and must
not be read as a full-history result.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Evidence limitation:** No automated vulnerability scanners or heavyweight
build/test witnesses ran. This is an advisory zero-finding tally from manual
review, not a security pass or evidence that the repository has no
vulnerabilities.
**Auto-fixed:** 0
**Manual Review Required:** 0

**Summary:** The single reachable commit in the strict 2026-10-12 →
2026-10-19 window is `46b8963 fix(ci): require exact qualification
requests and share work deadline (#737)`, dated 2026-10-17. It is a CI
hardening change that converts ordinary PR activity from implicit
heavyweight Rust/Action proof to a deliberate `ci:qualify` admission
gate. The change binds the native PR/head/base/merge SHAs and one
shared 20-minute work deadline, refuses unrequested / malformed / stale
/ expired / wrong-source requests before any expensive proof runs, and
keeps Cargo Deny, the four locked Rust commands, the Windows/Action
proof, the existing action pins, the runner permissions, the toolchain
pins, the catalog proof/cost flags, and the branch protections intact.

Specifically:

- `scripts/ci_qualification.py` (928 lines) — new production-grade
  admission script. Reads `GITHUB_EVENT_PATH`, `GITHUB_EVENT_NAME`,
  `GITHUB_REF`, `GITHUB_SHA`, `GITHUB_WORKFLOW_SHA`, and
  `GITHUB_REPOSITORY` from the runner environment; never dispatches a
  run, never changes a status, never cancels work, never uses a
  credential. GitHub supplies the event/context; `git ls-remote` reads
  only public source refs. The required Rust job repeats native-event
  and source validation before and after proof.

- `scripts/ci_budget.py` (174 lines) — new CI command/phase deadline
  boundary. Uses a single monotonic anchor (`CI_BUDGET_MONO_NS`); refuses
  budgets that exceed the 20-minute admission maximum; emits JSON
  receipts and exit markers per command; uses `subprocess.Popen` with
  `start_new_session=True` so each command owns its own process group,
  with a bounded `killpg(SIGTERM)` → grace window → `killpg(SIGKILL)`
  teardown path that preserves source exits while recording
  `terminal_reason="cancelled"` or `"budget_exhausted"` as distinct from
  `"completed"`.

- `scripts/ci_qualification_test.py`,
  `scripts/ci_qualification_review_test.py`,
  `scripts/ci_workflow_admission_test.py`,
  `scripts/ci_budget_test.py`,
  `scripts/ci_observability_clock_test.py` — new test suite that
  exercises admission and refusal against the checked-in production
  shell/source; the controls run as the required qualification
  precondition for `Tokmd Rust Result` via the
  `actions/github-script` runner, and they assert:
  - `pull_request` events with `action ∈ {labeled, opened, synchronize,
    ready_for_review, converted_to_draft}` for label events not
    `{labeled: ci:qualify}` never reach proof.
  - malformed/missing/unreviewed/wrong-generation requests are refused.
  - the native run / workflow / repository identity cannot be relabelled
    as the request.
  - head/base/generation movement is detected as stale.
  - source read failures are not an admission.
  - dispatch without PR context is refused (truthful production
    migration refusal).
  - per-job budgets (`phase_budget_N`) precede any heavy work and
    `finalize` is reached on the terminal path.
  - the same checked-in shell body is the production entry point in
    every companion workflow.

- `.github/workflows/ci.yml` and the other `WORKFLOWS` files
  (`bindings-parity`, `ci-policy`, `clippy-exceptions-policy`,
  `cockpit`, `coverage`, `no-panic-policy`, `pr-plan`,
  `proof-executor`, `ripr`, `test-action`) — each companion's
  `pull_request` trigger narrowed to `types: [labeled]`; each job now
  has a `Revalidate shared qualification budget` step that runs
  `python scripts/ci_qualification.py`; each job's heavy work sits
  behind a `phase_budget_N` allocation that calls
  `python scripts/ci_budget.py allowance` and uses
  `fromJSON(steps.<phase>.outputs.minutes)` to set
  `timeout-minutes`; each job's terminal step is
  `Finalize explicit PR qualification budget`, which revalidates
  sources and finalizes the shared budget. Concurrency groups use
  `cancel-in-progress: false` and `queue: max`, with the
  `qualification-{pr,head,base}` namespace isolated from the
  `observation-{pr,ref}` namespace.

- The PR qualification manifest format
  (`<!-- tokmd-qualification:v1\n{json}\n-->`) lives only in the PR
  body and is parsed once by `request_inputs`. The required fields are
  `pull_request` (positive integer), `head_sha` (full lowercase SHA),
  `base_sha` (full lowercase SHA), `generation` (must equal
  `head_sha:base_sha`), `expected_green: "true"`,
  `merge_sha` (full lowercase SHA, PR-only), and `deadline_epoch`
  (decimal, must be >0 and ≤20 minutes from now). Any other shape —
  including a missing `manifest`, more than one `manifest`, or any of the
  six required fields missing/wrong — refuses before any proof
  reaches a build step.

- The validator binds `native_repository` to the
  `SHARED_REPOSITORIES` allowlist (`EffortlessMetrics/tokmd-swarm`,
  `EffortlessMetrics/tokmd`). It refuses a wrong source repository
  in the PR base, in the PR head, in the `merge_sha` composition, or
  in the `GITHUB_REPOSITORY` runner variable, and it refuses an event
  repository mismatch via `match_event_repository`.

- The `route` job still hard-codes its `gh api
  "orgs/EffortlessMetrics/actions/runners?per_page=100"` call and its
  fork-PR shortcut; both remain unchanged.

The workspace-wide standing defenses were re-read in place and remain
present at the checked-out commit (see Standing Defenses table below).
No defenses were observed to have regressed.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 — 78 days old — still within the 90-day regeneration
window. No regeneration this scan.

## Critical Findings

*None.*

## High Findings

*None.*

## Medium Findings

*None.*

## Low Findings

*None.*

## Observations (Below Threshold — Not Reported As Findings)

These items were considered during the scan but do not meet the `medium`
severity threshold. They are recorded here for traceability and the next
scheduled scan.

The manual review found no new STRIDE-negative observations for the
inspected commit. The carried observations from prior weekly scans
remain present; one carried observation is narrowed by this commit
(OBS-007 below) and the rest are unchanged. New carries added this scan
are limited to the admission gate surface (OBS-010, OBS-011).

### OBS-001 (carried): FFI JSON payload size not bounded

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Denial of Service |
| **File** | `crates/tokmd-core/src/ffi/mod.rs` |
| **Status** | Not patched — design choice |

**Description:** The `run_json(mode, args_json)` FFI entrypoint accepts a JSON
string of arbitrary size. While individual in-memory `inputs[].path` is bounded
to 4096 bytes (`MAX_IN_MEMORY_INPUT_PATH_BYTES`), the outer JSON envelope is
not.

**Why not a finding:** Caller controls input. `serde_json::from_str` allocates
predictably; no algorithmic blowup. No `medium` reachability: requires the
caller to opt in. Out of scope per `SECURITY.md`. No change in this scan's
commits.

**Recommended fix (optional, future):** Add a soft cap on `args_json.len()`
(e.g. 8 MiB) returning a typed `TokmdError::invalid_field("args", "JSON args
exceed 8 MiB cap")` from `run_json_inner`.

### OBS-002 (carried): Transitive `RUSTSEC-2020-0163` advisory

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (transitive) |
| **STRIDE Category** | Elevation of Privilege |
| **File** | `Cargo.lock` (transitive `term_size` via `tokei`) |
| **Status** | Documented in `deny.toml` |

**Description:** `term_size` is a transitive dependency of `tokei` and has an
unmaintained advisory (`RUSTSEC-2020-0163`).

**Why not a finding:** Already documented in `deny.toml` with rationale.
Out of scope per `SECURITY.md`. No change in this scan's commits.

**Recommended action:** Track upstream `tokei` for a `term_size` removal.

### OBS-003 (carried): GitHub Actions pinning is mixed (tag + SHA)

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/*.yml` |
| **Status** | Not patched — mixed strategy |

**Description:** The Droid-related workflows
(`.github/workflows/droid.yml`, `droid-review.yml`, `droid-security-scan.yml`)
pin third-party actions by SHA, including the custom
`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`.
Other workflow references use a mixture of tags and SHAs. `actions/checkout`
is SHA-pinned at `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` across
the workflows that use it.

The previous scan's `24d5a53` commit narrowed the gap for the typos lane:
the prior mutable `crate-ci/typos@v1` reference in
`.github/workflows/ci.yml::typos` was replaced with a SHA-pinned
`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`
plus an exact `tool: typos@1.49.0` and structural test coverage to
prevent regression.

**Why not a finding:**
- The original reviewer classified the remaining tag references below the
  scan's reporting threshold; this is a manual assessment, not proof of
  immutable dependency identity.
- The custom Droid action — the highest-privilege third-party surface — IS
  SHA-pinned.
- After `24d5a53`, the typos lane is also SHA-pinned with structural test
  enforcement. In the inspected `ci.yml`, tag references still include
  `actions/upload-artifact@v7`, `actions/setup-node@v7`,
  `Swatinem/rust-cache@v2`, and `taiki-e/install-action@v2`; other workflow
  steps retain SHA pins. A pin in one job does not establish a
  repository-wide pinning contract.
- Below the `medium` severity threshold for this scan; flagged for the next
  threat-model refresh (target: 2026-11-01 or earlier if scope changes).

**Recommended action (optional, future):** Either update the threat model
to reflect the actual mixed-pinning policy with the typos lane listed as
the SHA-pinned exception, or convert all third-party tool-installer
actions to SHA-pinned references and codify the rotation process in
`.factory/rules/`.

### OBS-004 (carried): `web/runner` browser code does not pin GitHub API base URL

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing |
| **File** | `web/runner/ingest.js` |
| **Status** | Not patched — review for future |

**Description:** The browser-side runner fetches repository content via
`fetch()` calls to `api.github.com` (and the codeload/GitHub
`releases`/`archive` endpoints). These URLs are hard-coded in the
`web/runner/` JavaScript modules. The token (when supplied) is stored in
`sessionStorage` (not `localStorage`) and used as a `Bearer` header. There
is no Subresource Integrity pinning or origin allow-listing on the
client-side fetch surface.

**Why not a finding:**
- All sensitive fetches target `api.github.com` / `codeload.github.com`,
  which are HTTPS and well-known.
- The token lifetime is bounded to a single browser tab
  (`sessionStorage`).
- No DOM injection surfaces observed: all dynamic data is rendered via
  `textContent`; no use of `innerHTML`, `eval`, `new Function`, or
  `document.write`.
- Browser-side runner runs entirely in the user-agent sandbox; no
  filesystem, no subprocess.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional):** Consider an explicit allowlist of fetch
origins and a CSP `connect-src` directive in the runner's served HTML
to defend against supply-chain injection via a compromised
`<script>`/module.

### OBS-005 (carried): `action.yml` install step performs `curl | sh` style download

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Information Disclosure |
| **File** | `action.yml` (composite step `Install tokmd`) |
| **Status** | Not patched — verified checksums |

**Description:** The composite GitHub Action downloads a pre-built
`tokmd` binary from `github.com/EffortlessMetrics/tokmd/releases/...` and
verifies it against `checksums.txt` (sha256). It does not verify a
cryptographic signature on the checksum file or on the release itself.
The download URL is interpolated from a user-supplied `version` input
without shell-unsafe character filtering.

**Why not a finding:**
- The action is a published action; consumers control which version
  they pin to. The check is bounded to a `MAJOR.MINOR.PATCH`-style
  string via the `${ver#v}` prefix logic.
- `curl -fsSL` rejects HTTP errors and follows redirects (only to
  HTTPS GitHub release endpoints in practice).
- The checksum verification, when checksums.txt is present, uses
  `sha256sum`/`shasum`/`Get-FileHash` to compare the downloaded
  binary's hash to the expected value.
- Build provenance is separately attested via
  `actions/attest-build-provenance@v4` in `release.yml`.
- Below the `medium` severity threshold; this is documented best-
  practice coverage.

**Recommended action (optional):** Add explicit format validation
for the `version` input (e.g., regex `^v?\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$`)
and reject anything else before constructing the URL.

### OBS-006 (carried): Branch protection review requirements are zero

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational, by policy) |
| **STRIDE Category** | Elevation of Privilege / Repudiation |
| **File** | `.github/settings.yml` |
| **Status** | Not patched — intentional single-maintainer policy |

**Description:** At the checked-out commit `46b8963`, `.github/settings.yml`
configures `required_approving_review_count: 0` and
`require_code_owner_reviews: false` for `main`. The same historical file
declares only `Tokmd Rust Result` as a required status context and enables
`required_conversation_resolution: true`. Its comments require independent
agentic review in separate lanes as repo process while keeping bot review
statuses advisory; native human approval and CODEOWNERS review are
intentionally absent. This updates the carried OBS-006 description to the
inspected source; it does not claim live settings enforcement.

**Why not a finding:**
- The checked-in policy is narrow and explicit: `enforce_admins: false`,
  `allow_force_pushes: false`, `allow_deletions: false`, one required
  status context, and required conversation resolution.
- Live enforcement and per-PR execution of that context were not
  independently proven by this scan.
- The checked-out tree's `.github/settings.yml` comments distinguish the
  independent agentic review process from native approval and status gates;
  the threat model's contradictory approval text remains pending refresh.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** When the maintainer count
grows, increase `required_approving_review_count` and re-enable
`require_code_owner_reviews`.

### OBS-007 (carried, narrowed): `taiki-e/install-action` SHA must match `# v2.85.9` comment

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/ci.yml` (typos job) |
| **Status** | Not patched — process control |

**Description:** The typos job introduced by `24d5a53` pins the install
action with both a SHA (`91ddec75689c4c78665b598d188dc821c5a43e5c`) and a
human-readable comment (`# v2.85.9`). The contract test in
`xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed`
verifies the SHA pin, the exact `with:` values, and the no-fallback
setting, but does not verify the trailing `# v2.85.9` comment matches the
SHA. If the SHA is ever rotated without updating the comment, the
comment becomes misleading documentation rather than a mismatched
security control.

This scan's reviewed commit (`46b8963`) does not modify that contract
test, the typos install action, or the workflow. The narrowing in this
scan is incidental — the new qualification gate's `Admit explicit PR
qualification` step precedes the typos install on PR events, so a PR
that fails the gate never reaches the typos install even if a future
misleading-comment SHA drift existed. The contract test and the
SHA-pinned reference remain the authoritative control.

**Why not a finding:**
- The SHA itself is what GitHub resolves and what the contract test
  enforces. The comment is a human-readable annotation for reviewers.
- Rotating the SHA without updating the comment is a documented CI
  review checklist item, not a security control failure.
- The contract test fails closed if the SHA, version, checksum, or
  fallback settings drift in any direction.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** Add a separate test that
fetches the tag→commit mapping for the documented `taiki-e/install-action`
release tag (e.g., via a pinned witness JSON committed under
`fixtures/` or via `github.event.repository.default_branch`'s
`refs/tags/v2.85.9^{commit}`) and asserts the comment and the SHA agree.
This converts the comment from a review aid into a tested invariant.

### OBS-008 (carried): `cargo_command_surfaces` proof scope does not cover `--locked` regression in historical surfaces

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Elevation of Privilege |
| **File** | `xtask/tests/cargo_command_surfaces_w104.rs`, `policy/cargo-command-surfaces.toml` |
| **Status** | Not patched — intentional scope boundary |

**Description:** The new `f3cfd24` adoption guard correctly classifies
`docs/examples` and `.factory/security/reports` as `historical` surfaces
("may preserve historical command output") and routes only the
`cargo_command_surfaces` scope through
`ci/proof.toml`. The guard's classification verdict is
`NotProven` for these surfaces, which means a future `--locked`
regression in a preserved historical example will not fail the new
guard's CI lane. The `c2f77f0` docs sweep already covers the canonical
`AGENTS.md` / `agents/shared/repo.md` lanes via the `agent_guidance_docs`
scope, so the canonical guidance is protected; only the historical
references are explicitly out of scope by policy.

**Why not a finding:**
- The policy file's `mode = "historical"` declaration is the explicit
  policy intent: historical surfaces preserve intentional command text
  (e.g., reproduce-the-issue recipes) and the guard's `NotProven` verdict
  is the contract for that scope.
- The canonical agent guidance (`AGENTS.md`, `agents/shared/repo.md`)
  IS routed through `cargo_command_surfaces` (`paths` list) AND through
  `agent_guidance_docs`, so a `--locked` regression on the canonical
  lanes fails both proof scopes.
- The guard does not execute commands; it only classifies visible
  whitespace-delimited text. There is no execution surface added.
- Below the `medium` severity threshold; this is informational about
  the policy's intentional scope boundary.

**Recommended action (optional, future):** If maintainers want
historical-surface `--locked` regressions to fail CI, add a follow-up
scope (e.g. `cargo_command_surfaces_historical`) with an allowlist of
path globs whose historical surfaces still must conform to the locked
contract. This is a policy decision, not a security gap.

### OBS-009 (carried): `home` crate vendored at `vendor/home-0.5.12`

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Elevation of Privilege |
| **File** | `Cargo.toml`, `vendor/home-0.5.12/` |
| **Status** | Documented in `[patch.crates-io]` |

**Description:** The workspace vendors `home@0.5.12` under
`vendor/home-0.5.12/` and patches `crates-io` to point at the vendored
copy in `Cargo.toml`. This is an intentional temporary patch.

**Why not a finding:**
- The patch is local and intentional.
- The vendored source is the upstream `home-0.5.12` with the patch
  applied.
- Out of scope per `SECURITY.md`.
- Below the `medium` severity threshold; informational.

**Recommended action:** Continue tracking the upstream `home` release
that obsoletes the patch and remove the vendor when convenient.

### OBS-010 (new): CI qualification manifest format is implicit and not asserted against arbitrary bodies

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Repudiation |
| **File** | `scripts/ci_qualification.py::request_inputs` |
| **Status** | Not patched — design choice |

**Description:** The qualification request manifest is parsed from the
PR body's HTML comment `<!-- tokmd-qualification:v1\n{json}\n-->`. The
parser requires exactly one such comment, refuses otherwise, and
requires the JSON to be a JSON object. The regex uses lazy
`(.*?)` with a fixed trailing `\n-->` delimiter, which prevents
catastrophic backtracking for any well-formed input and for any input
that lacks the trailing delimiter.

`request_inputs` does not record the manifest version string in
admission receipts; only the parsed fields and a fixed version are
exposed. A future manifest version change (`v2`, `v3`, etc.) would
silently refuse every PR until the parser is updated, because the regex
anchors on the literal string `tokmd-qualification:v1`.

**Why not a finding:**
- The `len(manifests) != 1` check refuses ambiguity by design.
- The parser is ReDoS-safe: the inner `.*?` is bounded by the
  required `\n-->` terminator; `json.loads` will raise `JSONDecodeError`
  on malformed JSON; the strict type checks (`isinstance(request, dict)`,
  `isinstance(inputs, dict)`, `isinstance(pr, str)`,
  `re.fullmatch(r"[1-9][0-9]*", pr)`,
  `SHA.fullmatch(head)` / `SHA.fullmatch(base)` /
  `SHA.fullmatch(proof_sha)`,
  `deadline.isdecimal()`) all reject on the first violation.
- The manifest version is a single string literal in the regex; a
  future version bump is a deliberate, version-controlled change.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** Record the parsed manifest
version alongside the admission decision in the receipt so a
deliberate-version bump is visible in `Tokmd Rust Result` history. Add
a parallel parser for `tokmd-qualification:v2` alongside `v1` when
needed, rather than silently rejecting the older format.

### OBS-011 (new): `route` step's idle-runner JSON-pipe is parsed at PARSE; `gh api` exit code is the only authority

| Attribute | Value |
|-----------|--------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering |
| **File** | `.github/workflows/ci.yml::route::route` |
| **Status** | Not patched — runner-state source of truth |

**Description:** The `route` step invokes `gh api
"orgs/EffortlessMetrics/actions/runners?per_page=100"` and pipes the JSON
response through `jq` to detect an idle self-hosted runner with the
required capability labels (`trusted-pr`, `rust-small`). The route
fallback for fork PRs, missing token, API failure, or no idle capacity
is `ubuntu-latest`. The endpoint is hard-coded and the response is parsed
strictly via `jq -e 'any(.runners[]; ...)'`. The fallback path emits
`runner_kind=github` so downstream jobs can branch on it.

**Why not a finding:**
- The endpoint is a hard-coded `gh api` call against the
  `EffortlessMetrics` organization; no user input is interpolated into
  the URL.
- The `gh api` exit code is the only authority: on `CODE != 0` the step
  emits the GitHub-hosted fallback and exits 0, leaving CI to run on the
  GitHub runner rather than failing closed.
- `jq` is a deterministic JSON parser; the predicate does not accept
  untrusted strings, only the hard-coded label set.
- Fork PRs are explicitly routed to GitHub-hosted runners, which is
  the documented security posture.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** For PR events whose head
repo is not `EffortlessMetrics/tokmd-swarm`, consider setting
`runner_kind=fork` instead of `runner_kind=github` so downstream jobs
that already branch on `runner_kind` can apply fork-specific budgets
(e.g., reduced phase allocations).

## Standing Defenses Re-read in the Inspected Tree

The following defenses were re-read during this scan. Presence in the
inspected tree is recorded here; this is the same defense inventory that
prior weekly scans have re-verified.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable`, `dbg_macro`, `todo`, `unimplemented` lints denied | `Cargo.toml` | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` (line: `MAX_IN_MEMORY_INPUT_PATH_BYTES = 4096`) | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`) | `crates/tokmd-types/src/lib.rs`, `cockpit.rs`, `context.rs` | ✓ |
| D-09 | Mixed action references: custom Droid action and typos installer are SHA-pinned; `ci.yml` also retains `actions/upload-artifact@v7`, `actions/setup-node@v7`, `Swatinem/rust-cache@v2`, and `taiki-e/install-action@v2` | `.github/workflows/droid*.yml`, `.github/workflows/ci.yml` | inspected mixed references |
| D-10 | Branch-protection settings for `main` are present (status checks required, no force-push, no deletions); live enforcement and per-PR execution were not independently proven by this scan | `.github/settings.yml` | configured; live enforcement unverified |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` (`RUSTSEC-2020-0163` ignore for transitive `term_size` via `tokei`) | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | ✓ |
| D-13 | Content reads bounded by `ContentLimits` (`DEFAULT_MAX_FILE_BYTES = 128 KiB`) | `crates/tokmd-analysis/src/content/mod.rs` | ✓ |
| D-14 | PyO3 FFI invariants (no panic, GIL release, error translation) | `crates/tokmd-python/src/lib.rs` | ✓ |
| D-15 | WASM uses `MemFs` (no host fs) | `crates/tokmd-wasm/` | ✓ |
| D-16 | `web/runner` browser runner uses `textContent` (no `innerHTML`/`eval`/`new Function`/`document.write`) | `web/runner/main.js` | ✓ |
| D-17 | `web/runner` token stored in `sessionStorage` (not `localStorage`) | `web/runner/auth.js` | ✓ |
| D-18 | `web/runner` worker protocol allowlists modes & presets | `web/runner/messages.js` | ✓ |
| D-19 | Composite action installs tokmd with sha256 checksum verification; verification-gated tag allowlist for `runtime: container` | `action.yml` | ✓ |
| D-20 | Custom Droid action SHA-pinned across all Droid workflows; explicit `ANTHROPIC_AUTH_TOKEN: ""` / `ANTHROPIC_BASE_URL: ""` to block ambient fallback | `.github/workflows/droid*.yml` | ✓ |
| D-21 | `cargo audit` invoked with structured `--json` output, malformed JSON treated as Pending | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-22 | `run_json` top-level JSON must be an object (strict shape check) | `crates/tokmd-core/src/ffi/mod.rs::run_json_inner` | ✓ |
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | not verifiable from this shallow clone |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` (per `pending_supply_chain_gate` constructor) | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs`, `tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-26 | Typos lane install contract: SHA-pinned action, pinned tool version, `checksum: true`, `fallback: none`, plus a structural drift/fork rejection test | `.github/workflows/ci.yml::typos`, `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | ✓ |
| D-27 | `cargo_command_surfaces` adoption guard: closed-world inventory in `policy/cargo-command-surfaces.toml` (`schema_version = 1`), deterministic tracked-file scanner in `xtask/tests/cargo_command_surfaces_w104.rs`, routed through `ci/proof.toml::cargo_command_surfaces` proof scope; scanner is text-only and never executes guidance | `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `ci/proof.toml` | ✓ |
| D-28 | Canonical governed Cargo `build`, `check`, `test`, `clippy`, `run`, and `install` guidance uses `--locked`, with explicit "this source install is reproducible only to the committed lock" framing and tracked under `tokmd-swarm#604` / `depguard#21` / `depguard#22` / `depguard#24`; formatter aliases, xtask, and other ungoverned subcommands are outside this claim | `AGENTS.md`, `agents/shared/repo.md` | ✓ |
| D-29 (new) | CI qualification admission gate: only an explicit `ci:qualify` `pull_request` label event with an exact manifest binding native PR/head/base/merge SHAs and one shared 20-minute deadline reaches heavyweight proof; unrequested/malformed/stale/expired/wrong-source requests refuse before any build step | `scripts/ci_qualification.py`, `scripts/ci_budget.py`, `.github/workflows/ci.yml`, `.github/workflows/{bindings-parity,ci-policy,clippy-exceptions-policy,cockpit,coverage,no-panic-policy,pr-plan,proof-executor,ripr,test-action}.yml` | ✓ |
| D-30 (new) | Production `subprocess.Popen` for bounded CI commands uses `start_new_session=True`; teardown uses `os.killpg(SIGTERM)` → grace window → `os.killpg(SIGKILL)`; `terminal_reason` recorded as `cancelled`/`budget_exhausted`/`completed`/`source_failed`/`within_source` so journaled receipts distinguish source from signal-driven paths | `scripts/ci_budget.py::run_command` | ✓ |
| D-31 (new) | Companion workflows' `pull_request` trigger narrowed to `types: [labeled]`; non-`labeled` events cannot reach the qualification gate | `.github/workflows/*.yml` | ✓ |
| D-32 (new) | Admission concurrency namespaces separate `qualification-{pr,head,base}` from `observation-{pr,ref}` with `cancel-in-progress: false` and `queue: max` | `.github/workflows/ci.yml`, `.github/workflows/*.yml` | ✓ |

### Scan Coverage Matrix

The coverage below applies to the single commit reviewed for context
(`46b8963 fix(ci): require exact qualification requests and share work
deadline (#737)`) and the standing defenses the original reviewer
recorded as re-read. The observed checkout contains exactly one
reachable commit; six reachable parents are referenced in the commit
metadata but not present in the working clone, so the matrix does not
establish commit-level coverage of any prior commit.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| CI qualification admission | `scripts/ci_qualification.py` | 0 |
| CI command/phase budget | `scripts/ci_budget.py` | 0 |
| CI qualification controls | `scripts/ci_qualification_test.py` | 0 |
| CI qualification review regressions | `scripts/ci_qualification_review_test.py` | 0 |
| Companion admission tests | `scripts/ci_workflow_admission_test.py` | 0 |
| CI budget tests | `scripts/ci_budget_test.py` | 0 |
| CI observability clock tests | `scripts/ci_observability_clock_test.py` | 0 |
| CI workflow re-qualification steps | `.github/workflows/ci.yml`, `.github/workflows/{bindings-parity,ci-policy,clippy-exceptions-policy,cockpit,coverage,no-panic-policy,pr-plan,proof-executor,ripr,test-action}.yml` | 0 |
| CI concurrency namespaces | `.github/workflows/ci.yml` (and companion workflows) | 0 |
| CI phase budget allocations | `.github/workflows/ci.yml`, `.github/workflows/{test-action,...}.yml` | 0 |
| GitHub workflows (unchanged surface) | `.github/workflows/droid*.yml`, `.github/workflows/release*.yml`, `.github/workflows/fuzz.yml`, `.github/workflows/mutants.yml`, `.github/workflows/swarm-ghcr.yml`, `.github/workflows/ghcr-container-smoke.yml`, `.github/workflows/ci-policy.yml`, `.github/workflows/clippy-exceptions-policy.yml`, `.github/workflows/coverage.yml`, `.github/workflows/no-panic-policy.yml`, `.github/workflows/proof-executor.yml`, `.github/workflows/proof-observation-collection.yml`, `.github/workflows/recover-stable-crates.yml`, `.github/workflows/promote-release-aliases.yml`, `.github/workflows/ci.yml`, `.github/workflows/cockpit.yml`, `.github/workflows/bindings-parity.yml`, `.github/workflows/badge-endpoints.yml`, `.github/workflows/pr-plan.yml`, `.github/workflows/ripr.yml`, `.github/workflows/release-consumer-smoke.yml`, `.github/workflows/release-candidate.yml`, `.github/workflows/release-preflight.yml`, `.github/workflows/release.yml`, `.github/workflows/sync-labels.yml`, `.github/workflows/test-action.yml`, `.github/workflows/nix-full.yml`, `.github/workflows/nix-macos.yml`, `.github/workflows/droid-security-scan.yml`, `.github/workflows/droid.yml`, `.github/workflows/droid-review.yml` | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged |
| Branch protection | `.github/settings.yml` | configured; live enforcement unverified |
| Composite action | `action.yml` | 0 |

### Commit-level Analysis

The strict intended 2026-10-12 → 2026-10-19 window contains exactly one
reachable commit in the observed checkout. The recorded manual review
covers that single commit:

```
46b8963cee9dd6bd68179725f7d9ddebb9f8b7b9
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Sat Oct 17 22:17:17 2026 -0400
Subject: fix(ci): require exact qualification requests and share work deadline (#737)
```

- **Type:** CI-only commit. Body claims: "Ordinary PR activity starts
  expensive Rust/Action proof before an explicit expected-green request.
  This change admits the inventoried workloads only for a deliberate
  `ci:qualify` label event whose manifest binds the native PR/head/base/
  merge and one shared 20-minute work deadline." Subsequent claims:
  "Unrequested, malformed, stale, expired and wrong-source requests
  refuse before heavyweight proof. Observation contexts and concurrency
  groups stay separate; qualification retains `Tokmd Rust Result`.
  Cargo Deny remains blocking, the four locked Rust commands remain
  serial, and full selected Windows/Action proof remains intact.
  Existing action pins, permissions, toolchain, pre-existing catalog
  proof/cost flags and branch protections are preserved."
- **Surface:** 2628 files / `+1,048,838 / -0` per `git show --stat`.
  All file statuses in the inspected checkout are `A` (Added) because
  the working clone contains no reachable parent. The body claims
  preservation of "existing action pins, permissions, toolchain,
  pre-existing catalog proof/cost flags and branch protections", so the
  changes are additive (new qualification infrastructure) rather than
  modified. The 2628-file count therefore represents the full workspace
  tree at this commit, not a delta.
- **Net production change (additive):**
  - `scripts/ci_qualification.py` (928 lines) — admission script:
    reads `GITHUB_EVENT_PATH`, `GITHUB_EVENT_NAME`, `GITHUB_REF`,
    `GITHUB_SHA`, `GITHUB_WORKFLOW_SHA`, `GITHUB_REPOSITORY`; refuses
    on any wrong/missing field; reads `git ls-remote` for the public
    head/merge/main refs with a bounded timeout; emits GitHub
    outputs `mode`, `head`, `base`, `pr`, `deadline_epoch`, and
    `admitted`. The script's docstring states: "This script never
    dispatches a run, changes a status, cancels work, or uses a
    credential." All subprocess calls use array args (no
    `shell=True`, no `sh -c` / `bash -c`).
  - `scripts/ci_budget.py` (174 lines) — command/phase budget
    enforcement: single monotonic anchor (`CI_BUDGET_MONO_NS`); refuses
    budgets exceeding the 20-minute admission maximum; uses
    `subprocess.Popen(command, start_new_session=True)` so each command
    owns its own process group, with a `killpg(SIGTERM)` → grace window
    → `killpg(SIGKILL)` teardown path. Writes a JSON receipt and an
    exit marker per command.
  - `scripts/ci_qualification_test.py` (537 lines),
    `scripts/ci_qualification_review_test.py` (271 lines),
    `scripts/ci_workflow_admission_test.py` (256 lines),
    `scripts/ci_budget_test.py` (line count unverified from this
    clone), `scripts/ci_observability_clock_test.py` (line count
    unverified from this clone) — admission/refusal and budget
    regression tests; require the production shell source and
    production validator; do not exercise anything beyond the contract.
  - `.github/workflows/ci.yml` — `pull_request` trigger broadened to
    `types: [opened, reopened, synchronize, labeled]`; an
    `admission` job added; a `route` job added; a `concurrency`
    block added with separate `qualification-{pr,head,base}` and
    `observation-{pr,ref}` namespaces; every heavy step now sits
    behind a `phase_budget_N` allocation; every job's terminal step
    is `Finalize shared qualification budget`, which revalidates
    sources and finalizes the shared budget. The `Tokmd Rust
    Result` job retains its four locked Rust commands
    (`cargo --locked xtask gate --check`,
    `cargo test --locked --all-features`,
    `cargo test --locked -p xtask --all-features`,
    `cargo --locked xtask proof-policy --check`).
  - `.github/workflows/{bindings-parity,ci-policy,
    clippy-exceptions-policy,cockpit,coverage,no-panic-policy,
    pr-plan,proof-executor,ripr,test-action}.yml` — each companion
    workflow's `pull_request` trigger narrowed to
    `types: [labeled]`; each job has a `Revalidate shared
    qualification budget` step that runs
    `python scripts/ci_qualification.py`; each job's heavy work sits
    behind a `phase_budget_N` allocation; each job's terminal step
    is `Finalize explicit PR qualification budget`.
  - `.github/workflows/ci.yml::route` — `gh api
    "orgs/EffortlessMetrics/actions/runners?per_page=100"` call
    retained; `jq` predicate retained; `cancel-in-progress: false`
    retained; `queue: max` retained.
- **STRIDE analysis:** STRIDE-positive across all six categories:
  - **Spoofing** — the native run, workflow, and PR base/head/merge
    SHA are bound to the request, so impersonating a runner for the
    `qualification-{pr,head,base}` concurrency group is not possible
    without producing a `merge_sha` that matches
    `refs/pull/{pr}/merge`.
  - **Tampering** — `head_sha`, `base_sha`, `generation`,
    `merge_sha`, `deadline_epoch` are validated strictly; the
    manifest format is anchored with `<!-- tokmd-qualification:v1\n{json}\n-->`
    and refuses any other shape; `run_bounded` uses
    `python3 scripts/ci_budget.py run --marker "$marker" -- "$@"`
    with a `--` separator so no flag injection is possible.
  - **Repudiation** — the qualification manifest is in the PR body
    and is auditable; the JSON admission receipt records
    `mode`, `head`, `base`, `pr`, `deadline_epoch` for every
    qualification; the command JSON receipts record
    `terminal_reason`, `exit`, `child_started`, `timed_out`, and
    `elapsed_ns`.
  - **Information Disclosure** — `live_refs` reads public Git refs
    only; the script never uses a credential; the runner-side
    `GITHUB_TOKEN` and `EM_RUNNER_READ_TOKEN` are not exposed to
    this script.
  - **Denial of Service** — `live_refs` is bounded by `timeout`
    (min of `20.0`, the request deadline, the local deadline, and
    the monotonic anchor); `run_command` enforces a single shared
    deadline across setup, cache, core, advisory review, and final
    reporting.
  - **Elevation of Privilege** — every subprocess call uses array
    args; no `shell=True`, no `os.system`, no `os.popen`, no `sh -c`
    / `bash -c`; signal handling preserves source exits while
    recording `terminal_reason` as `cancelled`/`budget_exhausted`
    distinct from `completed`/`source_failed`.

The carried standing defenses (D-01 through D-28) remain present at
the inspected commit. The new defenses D-29 through D-32 are added by
this commit and listed in the Standing Defenses table above.

**Security-critical files re-read in place:**

- `scripts/ci_qualification.py` — `re.fullmatch` with `[0-9a-f]{40}`,
  `[1-9][0-9]*`, `request_inputs` returns `len(manifests) != 1`,
  `json.loads` strict, `match_event_repository` strict, `native_pr`
  is `None` only when `context["sha"] != head or
  context["workflow_sha"] != head`, `deadline.isdecimal()` strict,
  `0 < int(deadline) - time.time() <= 20 * 60` strict,
  `live_refs` is bounded by `min(limits)` timeout, `subprocess.run`
  without `shell=True`. ReDoS-safe regex (anchored, fixed-length
  classes, fixed terminator).
- `scripts/ci_budget.py` — `subprocess.Popen(command,
  start_new_session=True)`, `os.killpg(SIGTERM)` then grace then
  `os.killpg(SIGKILL)`, `terminal_reason` enum recorded, exit
  marker recorded, JSON receipt recorded. The
  `start_new_session=True` ensures each command is its own
  process-group leader; `killpg` targets that group, not
  individual PIDs.
- `scripts/ci_qualification_test.py` — strict shape checks, mocks
  only the test fixture; `Q.admit(...)` exercised with
  `self.assertRaisesRegex`.
- `scripts/ci_qualification_review_test.py` — production-path
  regressions; uses the checked-in shell body via
  `step_script(source, ...)`.
- `scripts/ci_workflow_admission_test.py` — companion admission
  tests; executes the production `ci_qualification.py` against
  fixtures; asserts the same checked-in shell body in every
  companion workflow.
- `scripts/ci_budget_test.py` — exercises budget, cancellation, and
  source failure; uses `subprocess.Popen(command, ...)` with array
  args.
- `scripts/ci_observability_clock_test.py` — exercises the clock
  markers; uses `subprocess.run` with array args.
- `crates/tokmd-git/src/command.rs` — `GIT_REPO_SHAPING_ENV`
  isolation unchanged.
- `crates/tokmd-git/src/refs.rs` — `env_base_ref_is_safe` unchanged.
- `crates/tokmd-core/src/ffi/inputs.rs` —
  `validate_in_memory_input_path` unchanged.
- `crates/tokmd-core/src/ffi/parse.rs` — strict field decoders
  unchanged.
- `crates/tokmd-core/src/ffi/mod.rs` — top-level JSON must be an
  object unchanged.
- `crates/tokmd-scan/src/path/bounded_path.rs` — `BoundedPath`
  unchanged.
- `crates/tokmd-format/src/redact/mod.rs` — BLAKE3 short hash with
  extension allowlist unchanged.
- `crates/tokmd-analysis/src/content/mod.rs` — `ContentLimits`
  unchanged.
- `crates/tokmd-cockpit/src/supply_chain.rs` — `parse_audit_output`
  unchanged.
- `crates/tokmd-cockpit/src/gates/contracts.rs` — `Command::new("cargo")`
  unchanged.
- `crates/tokmd-scan/src/walk/git.rs` — `GIT_REPO_SHAPING_ENV`
  unchanged.
- `crates/tokmd-scan/src/exclude/mod.rs` — pattern normalization
  unchanged.
- `crates/tokmd-types/src/lib.rs`,
  `crates/tokmd-types/src/cockpit.rs`,
  `crates/tokmd-types/src/context.rs`,
  `crates/tokmd-analysis-types/src/lib.rs` — schema versioning
  unchanged.
- `policy/cargo-command-surfaces.toml` — closed-world inventory
  unchanged.
- `xtask/tests/cargo_command_surfaces_w104.rs` — scanner unchanged.
- `xtask/tests/affected_w91.rs` — affected-scope fixture unchanged.
- `ci/proof.toml` — `cargo_command_surfaces` scope unchanged.
- `action.yml` — sha256 checksum verification unchanged.
- `.github/workflows/droid*.yml` — Droid action SHA-pinned
  unchanged.
- `.github/workflows/ci.yml::typos` — typos install action
  SHA-pinned unchanged.
- At `46b8963`, `.github/settings.yml` — only `Tokmd Rust Result`
  is a required status context for `main`; live enforcement at
  scan time and current branch-protection state were not
  independently proven.
- `deny.toml` — `RUSTSEC-2020-0163` ignore unchanged; license
  allowlist unchanged.
- `AGENTS.md` and `agents/shared/repo.md` — locked-Cargo-command
  guidance unchanged.
- `.factory/threat-model/threat-model.md` — STRIDE analysis
  unchanged. The threat model already enumerates the
  `Tokmd Rust Result` requirement, the per-family schema versioning,
  the standing defenses D-01 through D-28, and the
  `vendor/home-0.5.12` patch; this commit does not change any of
  those. The new defenses D-29 through D-32 are
  admission/budget/concurrency controls that did not exist when
  the threat model was last regenerated; they fit cleanly under the
  existing Spoofing/Tampering/Repudiation/Elevation of Privilege
  rows without contradicting any existing mitigation, but the next
  threat-model regeneration should add explicit rows for them.

**The manual review recorded no security findings at or above the
`medium` threshold for the inspected commit and recorded
standing-defense inspection (D-01 through D-32). Based on the
reviewed source commit and the standing defenses, the change set is
STRIDE-positive across all six categories.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-10-26 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## Appendix

### Threat Model

- **Status:** Within freshness window.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (78 days ago — well within 90-day
  window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture
  change
- **No regeneration this scan** — the file is within its normal
  freshness window. The next regeneration should fold in the new
  defenses D-29 through D-32 (qualification admission gate,
  bounded process-group teardown, companion `pull_request` types
  narrowing, admission concurrency namespaces) and any
  follow-up carried observations (OBS-010, OBS-011).

### Scan Metadata

- **Strict window:** 2026-10-12 → 2026-10-19 — exactly one commit in
  the observed checkout (`46b8963 fix(ci): require exact
  qualification requests and share work deadline (#737)`,
  2026-10-17).
- **Adjacent review subset:** none (the strict window contains the
  only reachable commit; no adjacent commit was reviewed for context).
- **Window completeness:** Not independently proven. `git log
  --since="7 days ago" --pretty=format:"%H %s"` returned exactly one
  commit in the observed checkout, but the working clone contains no
  reachable parent for that commit, so the observed-scope tally is
  bounded to the single commit and must not be read as a
  full-history result.
- **Commits reviewed:** 1 (`46b8963 fix(ci): require exact
  qualification requests and share work deadline (#737)`).
- **Files added by the reviewed commit:** 2628 unique paths (per
  `git show --name-only --format="" | sort -u | wc -l`), additive
  only. The 2628-file count represents the workspace tree at this
  commit, not a delta; the commit metadata's claim of
  "preservation of existing action pins, permissions, toolchain,
  pre-existing catalog proof/cost flags and branch protections"
  rules out a same-named overwrite, and the `+1,048,838 / -0` line
  total confirms pure addition. The reviewer did not enumerate
  every one of the 2628 files; this report's Scan Coverage Matrix
  covers the surfaces that the commit's docstring and body claim to
  touch, plus the standing defenses (D-01 through D-32). A reader
  who needs file-by-file coverage should request a `cargo xtask
  --check` baseline diff in a follow-up scan.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment),
  security-review (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

### References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [CII Best Practices](https://www.bestpractices.dev/)
- Repository security policy: `SECURITY.md`
- Repository threat model: `.factory/threat-model/threat-model.md`
- Previous scans: `.factory/security/reports/security-report-2026-06-01.md`,
  `.factory/security/reports/security-report-2026-06-08.md`,
  `.factory/security/reports/security-report-2026-06-29.md`,
  `.factory/security/reports/security-report-2026-07-06.md`,
  `.factory/security/reports/security-report-2026-07-13.md`,
  `.factory/security/reports/security-report-2026-07-20.md`,
  `.factory/security/reports/security-report-2026-07-27.md`,
  `.factory/security/reports/security-report-2026-08-03.md`,
  `.factory/security/reports/security-report-2026-08-10.md`,
  `.factory/security/reports/security-report-2026-08-17.md`,
  `.factory/security/reports/security-report-2026-08-24.md`,
  `.factory/security/reports/security-report-2026-08-31.md`
