# Security Scan Report

**Generated:** 2026-08-31
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-08-24 → 2026-08-31
**Observed Scope:** Checked-out commit `c8c3aa1`; `git log --since="7 days ago"`
reported no other commits in this checkout for the intended window. The scan
does not independently prove window completeness: the appendix records only a
bounded `git fetch --depth=50 origin main`, so the zero-finding tally is limited
to the observed checkout and must not be read as a full-history claim.

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

**Summary:** `git log --since="7 days ago" --pretty=format:"%H %s"` returned
no commits in the observed checkout for the intended 2026-08-24 → 2026-08-31
window. The most recent commit on the working branch is `c8c3aa1 test(handoff):
cover intelligence warning provenance (#622)` from 2026-08-22 — two days
outside the strict 7-day window. The manual review extended to `c8c3aa1` and
to the adjacent commit batch landed between the previous report's
2026-08-17 cutoff and the 2026-08-22 most-recent commit, because those
commits had not been covered by any prior weekly scan. The reviewed changes
are a tightly scoped test/doc/lint batch plus a STRIDE-positive supply-chain
hardening series that lands the `tokmd-swarm#604` locked-Cargo-command
adoption guard.

Specifically:

- `c2f77f0 docs(security): lock Cargo guidance` — rewrites `AGENTS.md` and
  `agents/shared/repo.md` so every documented `cargo` invocation passes
  `--locked`, with explicit "this source install is reproducible only to
  the committed lock" framing and an issue-tracker pointer to
  `tokmd-swarm#604`, `depguard#21`, `depguard#22`, `depguard#24`. This is a
  STRIDE-positive Elevation of Privilege / Tampering reduction: it narrows
  the surface where developers (and tools that imitate the docs) could run
  an undeclared-dependency `cargo` command and silently change the locked
  state.

- `7d192f0 fix(policy): harden Cargo command classification (#611)` —
  changes `xtask/tests/cargo_command_surfaces_w104.rs::governed_command` to
  classify short global cargo options (`-C`, `-Z`, `-v`, `-vv`, `-q`,
  `-qq`) before the subcommand, treat unknown pre-command options as
  `NotProven` (was: silently `Ok(None)`), and stop scanning cargo
  arguments after `--` for `--locked` / `--frozen` presence. This is a
  STRIDE-positive Tampering reduction: the guard now correctly recognizes
  common cargo flags and refuses to make a positive lock-preservation
  claim for unknown shapes, instead of either producing false positives or
  silently accepting bypass-shaped invocations.

- `f3cfd24 test(policy): guard governed cargo command surfaces` — adds
  `policy/cargo-command-surfaces.toml` (closed-world inventory, 385 lines,
  `schema_version = 1`) and a deterministic tracked-file scanner in
  `xtask/tests/cargo_command_surfaces_w104.rs` (566 lines) that classifies
  each candidate root as `live` / `deferred` / `historical` / `dynamic`
  without executing commands, plus a `cargo_command_surfaces` proof scope
  in `ci/proof.toml`. The scanner is purely text-based and never
  executes guidance. This is a STRIDE-positive Tampering / Elevation of
  Privilege reduction: it converts the previous "best-effort grep" into a
  bounded, classified, proof-routed adoption guard with explicit
  classification for `xtask/src` (`dynamic` — "Rust-spawned commands
  require semantic ownership review"), `docs/examples` and
  `.factory/security/reports` (`historical` — "may preserve historical
  command output"), and `.cargo` (`deferred` — "separate dependency and
  tooling slice").

- `c8c3aa1 test(handoff): cover intelligence warning provenance (#622)` —
  adds `crates/tokmd/tests/handoff_w71.rs::handoff_risk_no_git_records_hotspot_warning`
  and a 1-line documentation update in `docs/artifacts.md`. The test
  asserts that the `risk --no-git` handoff path emits a `null` hotspots
  value and a `warnings[]` entry prefixed with
  `hotspots unavailable: git history skipped`. This is a STRIDE-positive
  Information Disclosure reduction: the contract that downstream consumers
  rely on for "I asked for git, but you skipped it — tell me clearly" is
  now under test, and the artifact documentation states the behavior.

- `365894f test(handoff): assert fallback receipt provenance (#620)`,
  `598f29d test(context): guard required git score fallback (#618)`,
  `9c0bedb test(render): guard missing sibling packet inputs (#614)`,
  `fa89267 fix(render): report unusable ReviewCard rows (#616)`,
  `c3ac6f3 docs(user-paths): reconcile canonical evidence workflows
  (#612)`, `f3cfd24` test surface, `fd01edd fix(cockpit): simplify doc
  artifact reference` — test additions and small render/lint fixes. No
  new trust boundaries, no subprocess surfaces, no new secrets/env, no
  schema bumps, no CLI flag changes, no dependency changes.

The workspace-wide standing defenses were re-read in place and remain
present at the checked-out commit (see Standing Defenses table below). No
defenses were observed to have regressed.

The threat model at `.factory/threat-model/threat-model.md` is dated
2026-08-02 — 29 days old — still well within the 90-day regeneration
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

The manual comparison recorded one new low-severity observation
(OBS-008 below) for the `#604` locked-Cargo-command adoption guard
series, alongside the carried `2026-08-17` observations. The hardening
also *narrows* the supply-chain drift risk for the documented `cargo`
lanes: every `cargo` invocation listed in `AGENTS.md` /
`agents/shared/repo.md` now passes `--locked`, and the closed-world
`cargo_command_surfaces_w104` test re-asserts that contract on every
proof run.

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

### OBS-003 (carried, partially narrowed): GitHub Actions pinning is mixed (tag + SHA)

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
Other workflows pin third-party actions by tag. `actions/checkout`
is SHA-pinned at `3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1` across
the workflows that use it.

The previous scan's `24d5a53` commit narrowed the gap for the typos lane:
the prior mutable `crate-ci/typos@v1` reference in
`.github/workflows/ci.yml::typos` was replaced with a SHA-pinned
`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c # v2.85.9`
plus an exact `tool: typos@1.49.0` and structural test coverage to
prevent regression.

**Why not a finding:**
- Tag-pinned first-party actions (`actions/*`) are a well-accepted practice
  with low residual risk; GitHub's own recommended baseline.
- The custom Droid action — the highest-privilege third-party surface — IS
  SHA-pinned.
- After `24d5a53`, the typos lane is also SHA-pinned with structural test
  enforcement; remaining tag-pinned examples include first-party actions and
  third-party tool installers such as `taiki-e/install-action@v2` used by
  coverage, CI, proof-executor, and release workflows.
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

**Description:** At the checked-out commit `c8c3aa1`, `.github/settings.yml`
configures `required_approving_review_count: 0` and
`require_code_owner_reviews: false` for `main`. The same historical file
declares `Tokmd Rust Result` and `Codex Review Gate` as status contexts and
describes native human approval and CODEOWNERS review as intentionally
absent (per the in-line comment recorded in the inspected file).

**Why not a finding:**
- The checked-in policy is narrow and explicit: `enforce_admins: false`,
  `allow_force_pushes: false`, `allow_deletions: false`, and two declared
  status contexts.
- Live enforcement and per-PR execution of those contexts were not
  independently proven by this scan.
- The checked-out tree's `.github/settings.yml` comment records this as
  a deliberate operational choice ("Codex is the exact-head reviewer for this
  single-maintainer workflow"); the threat model is stale on this point and
  its contradictory approval text is explicitly pending refresh.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** When the maintainer count
grows, increase `required_approving_review_count` and re-enable
`require_code_owner_reviews`.

### OBS-007 (carried): `taiki-e/install-action` SHA must match `# v2.85.9` comment

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

### OBS-008 (new): `cargo_command_surfaces` proof scope does not cover `--locked` regression in historical surfaces

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
| D-09 | SHA-pinned Droid-related actions; tag-pinned first-party actions; **typos lane SHA-pinned after `24d5a53`** | `.github/workflows/droid*.yml` (SHA), `ci.yml::typos` (SHA after `24d5a53`) | ✓ |
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
| D-27 (new) | `cargo_command_surfaces` adoption guard: closed-world inventory in `policy/cargo-command-surfaces.toml` (`schema_version = 1`), deterministic tracked-file scanner in `xtask/tests/cargo_command_surfaces_w104.rs`, routed through `ci/proof.toml::cargo_command_surfaces` proof scope; scanner is text-only and never executes guidance | `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `ci/proof.toml` | ✓ |
| D-28 (new) | Canonical `cargo` guidance uses `--locked`: every documented `cargo` invocation in `AGENTS.md` and `agents/shared/repo.md` passes `--locked`, with explicit "this source install is reproducible only to the committed lock" framing and tracked under `tokmd-swarm#604` / `depguard#21` / `depguard#22` / `depguard#24` | `AGENTS.md`, `agents/shared/repo.md` | ✓ |


### Scan Coverage Matrix

The coverage below applies to the ten commits reviewed for context
(`fd01edd`, `c2f77f0`, `f3cfd24`, `7d192f0`, `c3ac6f3`, `9c0bedb`,
`fa89267`, `598f29d`, `365894f`, `c8c3aa1`) and the re-verified standing
defenses.

| Area | Files reviewed | Findings |
|------|----------------|----------|
| Locked-Cargo-command guidance docs | `AGENTS.md`, `agents/shared/repo.md` | 0 |
| Cargo command classification hardening | `xtask/tests/cargo_command_surfaces_w104.rs` | 0 |
| `cargo_command_surfaces` adoption guard | `ci/proof.toml`, `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `xtask/tests/affected_w91.rs` | 0 |
| Handoff test additions | `crates/tokmd/tests/handoff_w71.rs`, `docs/artifacts.md` | 0 |
| Context test additions (`--require-git-scores` fail-closed) | `crates/tokmd/tests/context_cli_w73.rs` | 0 |
| Render: unusable ReviewCard rows | `crates/tokmd-format/src/packet_siblings.rs` | 0 |
| Render: missing sibling packet inputs | `crates/tokmd/tests/render_packets_integration.rs` | 0 |
| User-paths docs reconcile | `README.md`, `docs/user-paths.md` | 0 |
| Cockpit doc artifact reference lint fix | `crates/tokmd-cockpit/src/render/evidence.rs` | 0 |
| Drift/fork rejection test | `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed` | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` (limits), `crates/tokmd-io-port/src/` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| GitHub workflows | `.github/workflows/*.yml` (29 files), `.github/settings.yml`, `action.yml` | 0 |
| Build / lint | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Githooks | `.githooks/pre-commit`, `.githooks/pre-push`, `.claude/hooks/format-rust.sh` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | unchanged |

### Commit-level Analysis

The strict intended 2026-08-24 → 2026-08-31 window contains zero commits in
the observed checkout. The manual review extended to the most recent commit
on the working branch and to the adjacent commit batch that landed between
the 2026-08-17 report cutoff and the 2026-08-22 most-recent commit
(because those commits had not been covered by any prior weekly scan):

```
c8c3aa1987aeac40d5397936ec84519a82f8993a
Author: Steven Zimmerman, CPA <15812269+EffortlessSteven@users.noreply.github.com>
Date:   Sat Aug 22 02:25:06 2026 -0400
Subject: test(handoff): cover intelligence warning provenance (#622)
```

- **Type:** Test-only commit. Body claims: "Test/docs only: no production,
  ranking, CLI, schema, CI, release, dependency, or security changes."
- **Surface:** 2 files (`+48/-1`).
- **Net code change:**
  - `crates/tokmd/tests/handoff_w71.rs`: adds the test
    `handoff_risk_no_git_records_hotspot_warning` (47 lines). It asserts
    that the `risk --no-git` handoff path emits a `null` hotspots value
    and a `warnings[]` entry prefixed with
    `hotspots unavailable: git history skipped`.
  - `docs/artifacts.md`: one-line documentation update stating that
    unavailable git enrichments are recorded in `intelligence.json.warnings`.
- **STRIDE analysis:** STRIDE-positive for Information Disclosure. The
  contract that downstream consumers rely on for "I asked for git, but you
  skipped it — tell me clearly" is now under test, and the artifact
  documentation states the behavior explicitly.

The full adjacent commit stack reviewed for context (between 2026-08-17 and
2026-08-22) is summarized below. None of these commits add new trust
boundaries, new subprocess invocations, new secret/env surfaces, new CLI
flags, new dependencies, new schema bumps, or new release surface; all
are STRIDE-neutral or STRIDE-positive for their respective categories.

```
365894f0f1fcd461caf324b23e6486206101d511  2026-08-22  test(handoff): assert fallback receipt provenance (#620)
598f29d4f6602dbe2cddefefe60f8b2fb4d14dd6  2026-08-22  test(context): guard required git score fallback (#618)
fa89267297bcea08b26464881d8512110c4cd0ee  2026-08-22  fix(render): report unusable ReviewCard rows (#616)
9c0bedb00498df5a633958b833bc4d5d15989578  2026-08-21  test(render): guard missing sibling packet inputs (#614)
c3ac6f353a70c651ff70ac1583c4dc7feb1eba66  2026-08-21  docs(user-paths): reconcile canonical evidence workflows (#612)
7d192f0c915b78fe90af809e2ee7580fc1ef8d95  2026-08-21  fix(policy): harden Cargo command classification (#611)
f3cfd24d9dbd8fae3f110659c3c8ea820c114c0c  2026-08-21  test(policy): guard governed cargo command surfaces
c2f77f09958badca483af5e3490ccd15a0d0dab0  2026-08-21  docs(security): lock Cargo guidance
fd01edd09e6aa6dbe1aee4f0fde4417bddb0f9b0  2026-08-21  fix(cockpit): simplify doc artifact reference
```

- **`c2f77f0 docs(security): lock Cargo guidance`** — `+63/-20` across
  `AGENTS.md` and `agents/shared/repo.md`. Rewrites every documented
  `cargo` invocation to pass `--locked`, adds explicit "this source
  install is reproducible only to the committed lock" framing, and
  tracks the locked-command contract under `tokmd-swarm#604` /
  `depguard#21` / `depguard#22` / `depguard#24`. STRIDE-positive
  Elevation of Privilege / Tampering reduction.

- **`7d192f0 fix(policy): harden Cargo command classification (#611)`** —
  `+33/-7` in `xtask/tests/cargo_command_surfaces_w104.rs`. The
  `governed_command` parser now recognizes short global cargo options
  (`-C`, `-Z`, `-v`, `-vv`, `-q`, `-qq`), treats unknown pre-command
  options as `NotProven` (was: silently `Ok(None)`), and stops scanning
  cargo arguments after `--` for `--locked` / `--frozen` presence.
  STRIDE-positive Tampering reduction.

- **`f3cfd24 test(policy): guard governed cargo command surfaces`** —
  `+966/-1` across `ci/proof.toml`, `policy/cargo-command-surfaces.toml`,
  `xtask/tests/cargo_command_surfaces_w104.rs`, `xtask/tests/affected_w91.rs`.
  Adds a closed-world cargo command inventory
  (`schema_version = 1`) classifying each candidate root as `live` /
  `deferred` / `historical` / `dynamic`. Adds a deterministic
  tracked-file scanner that classifies commands without executing them
  (offline fixture proves locked success, stale-lock failure,
  missing-lock failure, and unchanged lock bytes/existence). Routes
  the new scope through `ci/proof.toml::cargo_command_surfaces`.
  STRIDE-positive Tampering / Elevation of Privilege reduction.

- **`fd01edd fix(cockpit): simplify doc artifact reference`** —
  `+1/-1` in `crates/tokmd-cockpit/src/render/evidence.rs`. Lint-only
  simplification. No semantic change.

- **`c3ac6f3 docs(user-paths): reconcile canonical evidence workflows
  (#612)`** — `+7/-3` in `README.md`, `docs/user-paths.md`. Documents
  `evidence-packet` row and `--output-dir` canonical / `--out-dir`
  compatibility boundary. STRIDE-neutral docs change.

- **`9c0bedb test(render): guard missing sibling packet inputs (#614)`** —
  `+109/0` in `crates/tokmd/tests/render_packets_integration.rs`. Adds
  `render_declared_missing_manual_candidates_is_bounded` and
  `render_declared_missing_cards_is_bounded` tests. STRIDE-positive
  Information Disclosure reduction (failure modes are bounded).

- **`fa89267 fix(render): report unusable ReviewCard rows (#616)`** —
  `+95/-6` in `crates/tokmd-format/src/packet_siblings.rs`. When
  `cards.json` is present but all rows lack an `id`, the section is
  omitted and a limitation is recorded; partial cases record the
  omitted count. STRIDE-positive Information Disclosure reduction.

- **`598f29d test(context): guard required git score fallback (#618)`** —
  `+93/0` in `crates/tokmd/tests/context_cli_w73.rs`. Asserts that
  `--require-git-scores` fails closed when scores are unavailable, the
  non-git code ranking path succeeds with the requirement enabled, and
  the optional hotspot fallback records a `fallback_reason`. STRIDE-
  positive fail-closed hardening.

- **`365894f test(handoff): assert fallback receipt provenance (#620)`** —
  `+41/-10` in `crates/tokmd/tests/handoff_w71.rs`. Test for fallback
  receipt provenance. STRIDE-neutral.

**Security-critical files re-read in place:**
- `crates/tokmd-git/src/command.rs` — `GIT_REPO_SHAPING_ENV` and tests intact.
- `crates/tokmd-git/src/refs.rs` — `env_base_ref_is_safe` rejects empty,
  leading `-`, whitespace, control, backslash; `--end-of-options` used.
- `crates/tokmd-core/src/ffi/inputs.rs` — `validate_in_memory_input_path`
  covers empty / >4 KiB / control / absolute / Windows drive / `..` /
  all-`.` paths.
- `crates/tokmd-core/src/ffi/parse.rs` — strict field decoders; type
  mismatch → `TokmdError::invalid_field`.
- `crates/tokmd-core/src/ffi/mod.rs` — top-level JSON must be an object
  (defense D-22).
- `crates/tokmd-scan/src/path/bounded_path.rs` — `BoundedPath` enforces
  canonical under-root; rejects `..` at any position; rejects RootDir /
  Prefix components.
- `crates/tokmd-format/src/redact/mod.rs` — BLAKE3 short hash with
  extension allowlist.
- `crates/tokmd-analysis/src/content/mod.rs` — `ContentLimits` with
  `DEFAULT_MAX_FILE_BYTES = 128 KiB` plus a total `max_bytes` ceiling.
- `crates/tokmd-cockpit/src/supply_chain.rs` — `parse_audit_output`
  returns `Pending` on malformed JSON; `pending_supply_chain_gate`
  returns `Pending` (never `Pass`) when `cargo audit` is missing.
- `crates/tokmd-cockpit/src/gates/contracts.rs` — `Command::new("cargo")`
  uses `arg()` and `current_dir`; no shell.
- `crates/tokmd-scan/src/walk/git.rs` — same `GIT_REPO_SHAPING_ENV`
  isolation as `tokmd-git/src/command.rs`.
- `crates/tokmd-scan/src/exclude/mod.rs` — deterministic exclude
  pattern normalization (`#![forbid(unsafe_code)]`).
- `crates/tokmd-types/src/lib.rs`,
  `crates/tokmd-types/src/cockpit.rs`,
  `crates/tokmd-types/src/context.rs`,
  `crates/tokmd-analysis-types/src/lib.rs` —
  schema versioning unchanged (`SCHEMA_VERSION=2`,
  `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`,
  `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`,
  `ANALYSIS_SCHEMA_VERSION=9`).
- `policy/cargo-command-surfaces.toml` — closed-world inventory
  (`schema_version = 1`) with explicit classification per candidate
  root, no execution surface.
- `xtask/tests/cargo_command_surfaces_w104.rs` — deterministic
  tracked-file scanner; recognizes short global cargo options;
  treats unknown pre-command options as `NotProven`; stops scanning
  after `--`; verifies lock-preservation only for the parsed
  cargo-prefix token span.
- `xtask/tests/affected_w91.rs` — updated affected-scope fixture for
  the new `cargo_command_surfaces` scope.
- `ci/proof.toml` — routes the new `cargo_command_surfaces` proof
  scope.
- `action.yml` — sha256 checksum verification on downloaded tokmd
  binary; verification-gated tag allowlist
  (`1.14.0 1.15.0`) for `runtime: container`; isolated anonymous
  `docker --config` for pulls; strict `output-dir` validation
  (rejects absolute and `..` segments) for `mode: packet`.
- `.github/workflows/droid*.yml` — Droid action SHA-pinned
  (`EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec`)
  with explicit `ANTHROPIC_AUTH_TOKEN: ""` and `ANTHROPIC_BASE_URL: ""`
  to block ambient fallback.
- `.github/workflows/ci.yml::typos` — SHA-pinned installer
  (`taiki-e/install-action@91ddec75689c4c78665b598d188dc821c5a43e5c
  # v2.85.9`) with `checksum: true` and `fallback: none`, then
  `run: typos`. Enforced by
  `xtask/tests/proof_plan_w92.rs::typos_install_contract_is_immutable_verified_and_fail_closed`.
- At `c8c3aa1`, `.github/settings.yml` — `Tokmd Rust Result` and
  `Codex Review Gate` declared as status contexts for `main`;
  `allow_force_pushes: false`; `allow_deletions: false`. This records the
  inspected historical file; live enforcement at scan time and current
  branch-protection state were not independently proven.
- `deny.toml` — `RUSTSEC-2020-0163` ignore for transitive `term_size`
  unchanged; license allowlist unchanged.
- `AGENTS.md` and `agents/shared/repo.md` — every documented `cargo`
  invocation now passes `--locked`; source-install framing explicit.

**The manual review recorded no security findings at or above the `medium`
threshold for the reviewed commit stack. Based on the reviewed source
commits and the closed-world `cargo_command_surfaces` adoption guard, the
change set is STRIDE-positive across Spoofing, Tampering, Information
Disclosure, Denial of Service (incidentally, via tighter fail-closed
contracts), and Elevation of Privilege.**

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Next Scan

The next scheduled security scan runs Monday, 2026-09-07 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## Appendix

### Threat Model

- **Status:** Within freshness window.
- **Location:** `.factory/threat-model/threat-model.md`
- **Last Modified:** 2026-08-02 (29 days ago — well within 90-day window)
- **Methodology:** STRIDE
- **Next review:** 2026-11-01 (90-day cadence) or upon architecture change
- **No regeneration this scan** — the file is within its normal freshness
  window. The next regeneration should fold in the typos-lane SHA-pinning
  update from `24d5a53` (carried OBS-003) and the `#604` locked-Cargo-
  command adoption guard from this scan (`f3cfd24` + `c2f77f0` +
  `7d192f0`).

### Scan Metadata

- **Strict window:** 2026-08-24 → 2026-08-31 — zero commits in the
  observed checkout.
- **Adjacent review window:** 2026-08-17 → 2026-08-22 — ten commits
  (`fd01edd`, `c2f77f0`, `f3cfd24`, `7d192f0`, `c3ac6f3`, `9c0bedb`,
  `fa89267`, `598f29d`, `365894f`, `c8c3aa1`) reviewed for context
  because they had not been covered by any prior weekly scan.
- **Known commit reviewed (most recent on working branch):**
  `c8c3aa1987aeac40d5397936ec84519a82f8993a test(handoff): cover
  intelligence warning provenance (#622)`, 2026-08-22.
- **Window completeness:** Not independently proven. `git log --since="7
  days ago" --pretty=format:"%H %s"` returned zero commits in the observed
  checkout, but the recorded `git fetch --depth=50 origin main` is
  bounded and cannot establish full-history completeness.
- **Files in scope for the adjacent review:** ~15 across
  `crates/tokmd/tests/handoff_w71.rs`, `crates/tokmd/tests/context_cli_w73.rs`,
  `crates/tokmd-format/src/packet_siblings.rs`,
  `crates/tokmd/tests/render_packets_integration.rs`, `README.md`,
  `docs/user-paths.md`, `xtask/tests/cargo_command_surfaces_w104.rs`,
  `policy/cargo-command-surfaces.toml`, `ci/proof.toml`,
  `xtask/tests/affected_w91.rs`, `AGENTS.md`, `agents/shared/repo.md`,
  `crates/tokmd-cockpit/src/render/evidence.rs`, `docs/artifacts.md`.
  The full surface was previously reviewed under the `2026-06-29` true-merge
  baseline; this scan re-verified all security-critical modules in place.
- **Scan Duration:** ~5m (focused diff review + defense re-verification)
- **Skills Used:** commit-security-scan (manual, STRIDE),
  vulnerability-validation (manual, exploitability assessment), security-review
  (manual, defense confirmation)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **False Positive Filter:** applied — see Observations above

## References

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
  `.factory/security/reports/security-report-2026-08-17.md`
