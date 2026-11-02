# Security Scan Report

**Generated:** 2026-11-02
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-10-26 → 2026-11-02
**Observed Scope:** The checkout was a shallow clone (`depth=1`) containing a single
commit. The scan deepened it with `git fetch --deepen=100 origin main`, which
restored 128 commits of reachable history and established a clean window
boundary: the commit immediately preceding the window (`46b8963`, 2026-10-17)
is 9 days older than the window's oldest commit (`3e3dc2e`, 2026-10-26). All
three commits in the intended window were reviewed. The remaining history in
the deepened clone was used to confirm that no additional commit falls inside
the window.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Auto-fixed:** 0
**Manual Review Required:** 0

**Evidence limitation:** No automated vulnerability scanner (cargo-audit,
cargo-deny, Semgrep, CodeQL) and no heavyweight build witness ran in this
scan. This is an advisory zero-finding tally from manual STRIDE review of the
three in-window commits, plus re-verification of the standing defense
inventory. It is not a security pass and not evidence that the repository has
no vulnerabilities. The one threat-model item that reached `medium` residual
risk (mixed third-party action pinning) is a pre-existing condition, was
carried in the prior report, and is not attributable to the in-window
commits.

**Summary:** All three in-window commits were reviewed. Two are
STRIDE-positive changes that *reduce* attack surface; one is documentation
only. No commit in the window introduced a new trust boundary, a new
subprocess invocation on a production path, a new secret or environment
surface, a new CLI flag, a new dependency, a schema bump, or a release-surface
change.

Specifically:

- `07d878b fix(ci): retain renamed paths in affected selectors (#771)` — the
  largest change in the window (`+589/-30` across 6 files). It replaces
  newline-delimited `git diff --name-only` parsing with NUL-framed (`-z`)
  decoding in both the affected-scope discovery path
  (`xtask/src/tasks/affected.rs`) and the CI planner
  (`xtask/src/tasks/ci_plan.rs`), adds `--no-renames` so both endpoints of a
  rename are retained, escapes every dynamic field in the human-facing
  affected report with `escape_debug`, and adds fail-closed handling so a
  removed-but-unmatched path can no longer produce a `docs_only_change`
  claim. This is STRIDE-positive for Tampering (filename/field injection) and
  for Elevation of Privilege (scope-evasion via a removed path). It also
  removes the `normalize_path` backslash-rewriting helper that would have
  conflated a literal backslash in a Git filename with a directory separator.

- `e79d82d fix(gate): preserve other runs' target directories (#770)` —
  `+146/-190` across 7 files. It removes two age-based sweeps that deleted
  directories belonging to *other* concurrent runs on a shared runner: the
  Rust `cleanup_stale_scoped_dirs` in `xtask/src/tasks/build_guard.rs` and
  the CI step
  `find /tmp -maxdepth 1 -type d -name 'tokmd-gate-target-*' -mmin +30 -exec rm -rf {} +`
  in `.github/workflows/ci.yml`. This is STRIDE-positive for Tampering: it
  eliminates a cross-run destructive path. It preserves the two legitimate
  controls — current-invocation cleanup via `ScopedTempDir::drop`, and the
  disk-pressure refusal via `ensure_min_free_space` (floor
  `TOKMD_MIN_FREE_GB`, default 8 GiB), which still fails closed before each
  governed Cargo command.

- `3e3dc2e docs: clarify same-session review and evidence reuse (#769)` —
  documentation only. It updates `AGENTS.md`, `agents/shared/repo.md`, and
  `agents/shared/roles/critic.md` to place independent reviewers as subagents
  inside the working session and to preserve valid earlier evidence with its
  original subject and provenance. No code, workflow, dependency, schema, or
  release surface changed.

The window's three commits were also checked against the standing defense
inventory. All defenses previously recorded by prior scans remain present at
the scanned commit; no defense regressed. Two defenses were *added* by the
window (`NUL`-framed Git path decoding and the removal of cross-run artifact
deletion), and two new standing-defense entries were added to the regenerated
threat model as a result.

The threat model at `.factory/threat-model/threat-model.md` was dated
2026-08-02 — 92 days old at scan time, 2 days past its stated 90-day
regeneration window. It was **regenerated** in this scan.

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
severity threshold for a finding. They are recorded for traceability and for
the next scheduled scan. None of them is attributable to the in-window
commits.

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
caller to opt in. Out of scope per `SECURITY.md`. Unchanged by the in-window
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
Out of scope per `SECURITY.md`. Unchanged by the in-window commits.

**Recommended action:** Track upstream `tokei` for a `term_size` removal.

### OBS-003 (carried, re-measured): Third-party GitHub Actions pinning is mixed (98 SHA-pinned / 114 tag-pinned)

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Spoofing / Tampering / Elevation of Privilege |
| **File** | `.github/workflows/*.yml` |
| **Status** | Not patched — mixed strategy |

**Description:** This scan re-measured the pinning inventory across all 29
workflow files rather than restating the prior report's partial list.

- Total third-party `uses:` references (excluding local `./` actions): **212**
- SHA-pinned (`@<40-hex>`): **98**
- Tag-pinned: **114**

The tag-pinned set, by frequency: `dtolnay/rust-toolchain@stable` (34) and
`@nightly` (1), `actions/upload-artifact@v7.0.1` (26),
`Swatinem/rust-cache@v2` (23), `taiki-e/install-action@v2` (5),
`DeterminateSystems/nix-installer-action@v22` (3),
`DeterminateSystems/magic-nix-cache-action@v14` (3),
`codecov/codecov-action@v7` (2), `actions/setup-node@v7` (2),
`DeterminateSystems/flakehub-cache-action@v3` (2),
`DeterminateSystems/determinate-nix-action@v3` (2),
`thollander/actions-comment-pull-request@v3` (1),
`softprops/action-gh-release@v3` (1), `peter-evans/create-pull-request@v8` (1),
`docker/setup-buildx-action@v4` (1), `docker/metadata-action@v6` (1),
`docker/login-action@v4` (1), `docker/build-push-action@v7` (1),
`actions/download-artifact@v8` (1), `actions/cache@v6` (1),
`actions/attest-build-provenance@v4` (1), `EndBug/label-sync@v2` (1).

The custom Droid action — the highest-privilege third-party surface — **is**
SHA-pinned (`EffortlessMetrics/droid-action-safe@7c1377cc…`) across all
Droid workflows, and the typos lane installer is SHA-pinned with a contract
test enforcing the pin.

**Why not a finding:** The standard escalation path for a compromised
tag-pinned action is a workflow trigger that runs with write permissions or
secrets on an untrusted pull request. That path is not present:

- No workflow uses `pull_request_target` (verified across all 29 workflow
  files). `pull_request`-triggered workflows receive no repository secrets
  for fork PRs.
- One workflow uses `workflow_run`: `nix-full.yml`. It is gated on
  `github.repository == 'EffortlessMetrics/tokmd'` (so it does not run in
  the swarm repository at all), fires only for a completed `CI` run on
  `main` with `conclusion == 'success'`, checks out the specific
  `workflow_run.head_sha` rather than PR code, downloads no artifacts, and
  holds only `contents: read` + `id-token: write` (no `contents: write`).
  The `id-token` grant is scoped to OIDC and should not be relied on to
  authenticate to GitHub without a subject/audience restriction — see
  OBS-010.
- The `droid-review.yml` lane additionally gates on
  `github.event.pull_request.head.repo.full_name == github.repository`.
- Every write-scoped workflow is triggered only by a tag push
  (`release.yml`), `workflow_dispatch` (`promote-release-aliases.yml`,
  `ghcr-container-smoke.yml`), or a `main` push (`swarm-ghcr.yml`) — all
  of which require push or dispatch rights on the repository.
- No `rust-toolchain.toml` exists in the repository, so the
  `dtolnay/rust-toolchain@stable` action cannot be steered by a
  pull-request-authored toolchain override.

Exploitation therefore requires compromising the upstream action repository
*and* a maintainer pushing to `main` or dispatching a write-scoped workflow.
That is a supply-chain compromise, not a repository-side vulnerability.

**Recommended action (optional, future):** Convert the remaining tag-pinned
references to SHA pins, prioritising the write-scoped workflows
(`release.yml`, `promote-release-aliases.yml`, `swarm-ghcr.yml`) and the
`actions/attest-build-provenance@v4` reference, and record the rotation
procedure in `.factory/rules/`. A CI check that rejects any non-SHA third-party
`uses:` reference would keep the property from regressing.

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
  `document.write`. Re-verified in this scan across `web/runner/*.js`.
- Browser-side runner runs entirely in the user-agent sandbox; no
  filesystem, no subprocess.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional):** Consider an explicit allowlist of fetch
origins and a CSP `connect-src` directive in the runner's served HTML
to defend against supply-chain injection via a compromised
`<script>`/module.

### OBS-005 (carried): `action.yml` install step does not validate the `version` input format, and checksum verification can be skipped

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Information Disclosure |
| **File** | `action.yml` (composite step `Install tokmd`) |
| **Status** | Not patched — verified checksums |

**Description:** The composite GitHub Action downloads a pre-built
`tokmd` binary from `github.com/EffortlessMetrics/tokmd/releases/...` and
verifies it against `checksums.txt` (sha256). The `version` input is
interpolated into the download URL without format validation. Separately,
checksum verification is **skipped** (with a warning, not an error) when
`checksums.txt` cannot be fetched, or when the expected asset is not listed
in it.

**Why not a finding:**
- The URL scheme and host are hard-coded, so `version` cannot redirect the
  download off GitHub. This scan confirmed empirically that a traversal
  payload in `version` (e.g. `v1.0.0/../../evil`) normalizes to a path
  *under* `github.com/…/releases/` and returns HTTP 404 — it cannot escape
  to an arbitrary host.
- The check is bounded to a `MAJOR.MINOR.PATCH`-style string via the
  `${ver#v}` prefix logic for the container runtime, which additionally
  enforces a verification-gated tag allowlist.
- `curl -fsSL` rejects HTTP errors and follows redirects (only to
  HTTPS GitHub release endpoints in practice).
- The checksum verification, when `checksums.txt` is present, uses
  `sha256sum`/`shasum`/`Get-FileHash` to compare the downloaded
  binary's hash to the expected value.
- Build provenance is separately attested via
  `actions/attest-build-provenance@v4` in `release.yml`.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional):** Add explicit format validation
for the `version` input (e.g., regex `^v?\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$`)
and reject anything else before constructing the URL. Separately, consider
treating a missing or non-matching `checksums.txt` as a hard failure rather
than a warning for the binary runtime, so an unverifiable download cannot
proceed silently.

### OBS-006 (carried): Branch protection review requirements are zero

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational, by policy) |
| **STRIDE Category** | Elevation of Privilege / Repudiation |
| **File** | `.github/settings.yml` |
| **Status** | Not patched — intentional single-maintainer policy |

**Description:** At the scanned commit, `.github/settings.yml` configures
`required_approving_review_count: 0` and `require_code_owner_reviews: false`
for `main`. The same historical file declares only `Tokmd Rust Result` as a
required status context and enables `required_conversation_resolution: true`.
Its comments require independent agentic review in separate lanes as repo
process while keeping bot review statuses advisory; native human approval and
CODEOWNERS review are intentionally absent.

**Why not a finding:**
- The checked-in policy is narrow and explicit: `enforce_admins: false`,
  `allow_force_pushes: false`, `allow_deletions: false`, one required
  status context, and required conversation resolution.
- Live enforcement and per-PR execution of that context were not
  independently proven by this scan.
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

**Description:** The typos job pins the install action with both a SHA
(`91ddec75689c4c78665b598d188dc821c5a43e5c`) and a human-readable comment
(`# v2.85.9`). The contract test in
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
fetches the tag-to-commit mapping for the documented `taiki-e/install-action`
release tag (e.g., via a pinned witness JSON committed under
`fixtures/`) and asserts the comment and the SHA agree.

### OBS-008 (carried): `cargo_command_surfaces` proof scope does not cover `--locked` regression in historical surfaces

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Elevation of Privilege |
| **File** | `xtask/tests/cargo_command_surfaces_w104.rs`, `policy/cargo-command-surfaces.toml` |
| **Status** | Not patched — intentional scope boundary |

**Description:** The `cargo_command_surfaces` adoption guard correctly
classifies `docs/examples` and `.factory/security/reports` as `historical`
surfaces ("may preserve historical command output") and routes only the
`cargo_command_surfaces` scope through `ci/proof.toml`. The guard's
classification verdict is `NotProven` for these surfaces, which means a
future `--locked` regression in a preserved historical example will not fail
the new guard's CI lane. The canonical `AGENTS.md` / `agents/shared/repo.md`
lanes are covered by both `cargo_command_surfaces` and `agent_guidance_docs`.

**Why not a finding:**
- The policy file's `mode = "historical"` declaration is the explicit
  policy intent: historical surfaces preserve intentional command text
  and the guard's `NotProven` verdict is the contract for that scope.
- The guard does not execute commands; it only classifies visible
  whitespace-delimited text. There is no execution surface added.
- Below the `medium` severity threshold; informational only.

**Recommended action (optional, future):** Add a follow-up scope
(e.g. `cargo_command_surfaces_historical`) with an allowlist of path globs
whose historical surfaces still must conform to the locked contract. This is
a policy decision, not a security gap.

### OBS-009 (new): `xtask` git range arguments are not validated against option-like or malformed refs

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Tampering / Elevation of Privilege |
| **File** | `xtask/src/tasks/affected.rs::changed_files`, `xtask/src/tasks/ci_plan.rs::git_diff_names` |
| **Status** | Not patched — fail-closed by construction |

**Description:** Both affected discovery and the CI planner pass a
caller-supplied `--base` and `--head` into `git diff`. In
`affected.rs::changed_files` they are passed as two separate arguments
followed by `--`; in `ci_plan.rs::git_diff_names` they are combined as
`format!("{base}...{head}")` into a single argument followed by `--`. Neither
site validates that the value is a well-formed revision.

**Why not a finding:** The scan verified empirically that this is not
exploitable:

- `git diff` rejects an option-shaped revision argument with a usage error
  and a non-zero exit (tested with `--upload-pack=…`), so a leading-`-`
  ref cannot be reinterpreted as a git option.
- A malformed range such as `HEAD......HEAD` fails with
  `fatal: ambiguous argument … unknown revision`, and both call sites
  convert a non-zero exit into a hard `bail!` — fail closed.
- The `--` separator prevents any revision from being reinterpreted as a
  pathspec.
- In CI, the values are `origin/${github.base_ref}` and `HEAD`. The
  `origin/` prefix guarantees the value cannot begin with `-`.
  `release-preflight.yml` additionally validates its SHA inputs against
  `^([0-9a-f]{40}|[0-9a-f]{64})$` before use.
- A local caller selecting their own revision range does not cross a trust
  boundary: the caller already has shell access to the checkout.

This differs from the shipped-crate path, where
`tokmd-git/src/refs.rs::env_base_ref_is_safe` applies an explicit allowlist
to user-supplied refs. The `xtask` control plane deliberately does not
duplicate that check, because its inputs come from the CI workflow definition
rather than from an untrusted end user.

**Recommended action (optional, future):** If the control plane's inputs
ever widen (for example, accepting a base ref from an untrusted
`workflow_dispatch` input), reuse
`tokmd-git/src/refs.rs::env_base_ref_is_safe` or an equivalent check rather
than relying on the `origin/` prefix.

### OBS-010 (new): `nix-full.yml` holds `id-token: write` in a `workflow_run`-triggered job

| Attribute | Value |
|-----------|-------|
| **Severity** | LOW (informational) |
| **STRIDE Category** | Elevation of Privilege / Spoofing |
| **File** | `.github/workflows/nix-full.yml` |
| **Status** | Not patched — gated by repository and trigger conditions |

**Description:** `nix-full.yml` is the only workflow in the repository that
uses the `workflow_run` trigger. It requests `permissions: contents: read`
and `id-token: write`. The `workflow_run` trigger family is the documented
class of trigger where a privileged workflow executes after an unprivileged
one, and `id-token: write` allows the job to mint a GitHub OIDC token for
the repository's identity provider.

**Why not a finding:** The exploitable pattern that makes `workflow_run`
dangerous — a privileged workflow checking out or downloading artifacts from
an untrusted pull request — is not present here:

- The job is gated on `github.repository == 'EffortlessMetrics/tokmd'`, so
  it does not execute at all in the `tokmd-swarm` repository.
- The trigger is filtered to `branches: [ "main" ]` and requires
  `github.event.workflow_run.conclusion == 'success'`, so it fires only
  after CI succeeds on `main` — never on pull-request code.
- The checkout uses `ref: ${{ github.event.workflow_run.head_sha }}`, a
  specific commit SHA on `main`, with `persist-credentials: false`. No
  artifact from the triggering run is downloaded.
- The permission set is `contents: read` + `id-token: write`; there is no
  `contents: write` and no `packages: write`.
- The workflow does not currently request an OIDC token (no
  `id-token:` audience/permissions parameter is passed to any step), so the
  grant is unused today.

**Why it is still recorded:** the `id-token: write` grant is latent
capability rather than a needed permission for the job's current steps. If a
future step begins minting an OIDC token without a subject/audience
restriction, the combination of a `workflow_run` trigger and an unrestricted
OIDC grant is the shape that enables cloud-identity impersonation.

**Recommended action (optional, future):** Drop `id-token: write` from
`nix-full.yml` unless a step actually needs it. If a future step does need
OIDC, restrict the request with an explicit `permissions: id-token: write`
plus a subject/audience claim (for example a `subject` bound to the
workflow ref) so the minted token is not replayable against unrelated
repositories or environments.

## Standing Defenses Re-read in the Inspected Tree

The following defenses were re-read during this scan at commit `07d878b`.
Presence in the inspected tree is recorded here.

| ID | Defense | Location | Verified |
|----|---------|----------|----------|
| D-01 | `unsafe_code = "forbid"` workspace lint | `Cargo.toml` | ✓ |
| D-02 | `unwrap_used`, `expect_used`, `panic`, `unreachable`, `dbg_macro`, `todo`, `unimplemented` lints denied | `Cargo.toml` | ✓ |
| D-03 | Git subprocess env isolation (`GIT_REPO_SHAPING_ENV`) | `crates/tokmd-git/src/command.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-04 | Git ref validation (`env_base_ref_is_safe` + `--end-of-options`) | `crates/tokmd-git/src/refs.rs` | ✓ |
| D-05 | Bounded path canonicalization under root | `crates/tokmd-scan/src/path/bounded_path.rs` | ✓ |
| D-06 | FFI in-memory input path validation | `crates/tokmd-core/src/ffi/inputs.rs` (`MAX_IN_MEMORY_INPUT_PATH_BYTES = 4096`) | ✓ |
| D-07 | Strict JSON parsing with type validation | `crates/tokmd-core/src/ffi/parse.rs` | ✓ |
| D-08 | Per-family schema versioning (`SCHEMA_VERSION=2`, `COCKPIT_SCHEMA_VERSION=3`, `HANDOFF_SCHEMA_VERSION=5`, `CONTEXT_SCHEMA_VERSION=4`, `CONTEXT_BUNDLE_SCHEMA_VERSION=2`, `ANALYSIS_SCHEMA_VERSION=9`) | `crates/tokmd-types/src/lib.rs`, `cockpit.rs`, `context.rs`, `crates/tokmd-analysis-types/src/lib.rs` | ✓ |
| D-09 | Mixed action references: the custom Droid action and the typos installer are SHA-pinned; 98 of 212 third-party references are SHA-pinned, 114 are tag-pinned | `.github/workflows/*.yml` | measured |
| D-10 | Branch-protection settings for `main` are present (status checks required, no force-push, no deletions); live enforcement and per-PR execution were not independently proven by this scan | `.github/settings.yml` | configured; live enforcement unverified |
| D-11 | `cargo-deny` advisory + license allowlist | `deny.toml` (`RUSTSEC-2020-0163` ignore for transitive `term_size` via `tokei`) | ✓ |
| D-12 | BLAKE3 redaction with extension allowlist | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | ✓ |
| D-13 | Content reads bounded by `ContentLimits` (`DEFAULT_MAX_FILE_BYTES = 128 KiB`) | `crates/tokmd-analysis/src/content/mod.rs` | ✓ |
| D-14 | PyO3 FFI invariants (no panic, GIL release, error translation) | `crates/tokmd-python/src/lib.rs` | ✓ |
| D-15 | WASM uses `MemFs` (no host fs) | `crates/tokmd-wasm/` | ✓ |
| D-16 | `web/runner` browser runner uses `textContent` (no `innerHTML`/`eval`/`new Function`/`document.write`) | `web/runner/*.js` | ✓ |
| D-17 | `web/runner` token stored in `sessionStorage` (not `localStorage`) | `web/runner/auth.js` | ✓ |
| D-18 | `web/runner` worker protocol allowlists modes & presets | `web/runner/messages.js` | ✓ |
| D-19 | Composite action installs tokmd with sha256 checksum verification; verification-gated tag allowlist for `runtime: container` | `action.yml` | ✓ |
| D-20 | Custom Droid action SHA-pinned across all Droid workflows; explicit `ANTHROPIC_AUTH_TOKEN: ""` / `ANTHROPIC_BASE_URL: ""` to block ambient fallback | `.github/workflows/droid*.yml` | ✓ |
| D-21 | `cargo audit` invoked with structured `--json` output, malformed JSON treated as Pending | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-22 | `run_json` top-level JSON must be an object (strict shape check) | `crates/tokmd-core/src/ffi/mod.rs::run_json_inner` | ✓ |
| D-23 | Author DAG import via true-merge commits (no force-push of publication history) | repository topology | not verifiable from this shallow clone |
| D-24 | `supply_chain` gate explicitly tolerates missing `cargo audit` binary by returning `Pending`, never `Pass` | `crates/tokmd-cockpit/src/supply_chain.rs` | ✓ |
| D-25 | `Command::new("cargo")` and `Command::new("git")` invocations use `arg()` (not shell) and `current_dir` for path control, no `sh -c` / `bash -c` in production code | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs`, `crates/tokmd-git/src/command.rs`, `crates/tokmd-scan/src/walk/git.rs` | ✓ |
| D-26 | Typos lane install contract: SHA-pinned action, pinned tool version, `checksum: true`, `fallback: none`, plus a structural drift/fork rejection test | `.github/workflows/ci.yml::typos`, `xtask/tests/proof_plan_w92.rs` | ✓ |
| D-27 | `cargo_command_surfaces` adoption guard: closed-world inventory, deterministic tracked-file scanner, routed through `ci/proof.toml`; scanner is text-only and never executes guidance | `policy/cargo-command-surfaces.toml`, `xtask/tests/cargo_command_surfaces_w104.rs`, `ci/proof.toml` | ✓ |
| D-28 | Canonical governed Cargo `build`, `check`, `test`, `clippy`, `run`, and `install` guidance uses `--locked` | `AGENTS.md`, `agents/shared/repo.md` | ✓ |
| D-29 (new) | NUL-framed Git path decoding (`git diff -z`) in affected discovery and CI planning; dynamic report fields `escape_debug`'d | `xtask/src/tasks/affected.rs::parse_diff_paths`, `xtask/src/tasks/ci_plan.rs::git_diff_names` | ✓ |
| D-30 (new) | No cross-run artifact deletion: age-based target sweeps removed; disk pressure refused via `ensure_min_free_space` instead of deleting another run's files | `xtask/src/tasks/build_guard.rs`, `xtask/src/tasks/gate.rs`, `.github/workflows/ci.yml` | ✓ |
| D-31 (new) | No `pull_request_target` trigger in any workflow; write-scoped workflows trigger only on tag push, dispatch, or `main` push. The single `workflow_run` trigger (`nix-full.yml`) is gated on the publication repo, a successful `main` CI run, and a specific head SHA, with `contents: read` only | `.github/workflows/*.yml` (29 files) | ✓ |

### Scan Coverage Matrix

| Area | Files reviewed | Findings |
|------|----------------|----------|
| Affected scope discovery | `xtask/src/tasks/affected.rs` | 0 |
| CI planner | `xtask/src/tasks/ci_plan.rs` | 0 |
| Gate / build guard | `xtask/src/tasks/gate.rs`, `xtask/src/tasks/build_guard.rs` | 0 |
| Self-hosted CI setup | `.github/workflows/ci.yml` | 0 |
| Proof policy wiring | `ci/proof.toml`, `xtask/tests/proof_policy_w90.rs` | 0 |
| New gate lifecycle test | `xtask/tests/required_gate_locked_commands_w108.rs` | 0 |
| Affected/CI planner tests | `xtask/tests/affected_w91.rs` | 0 |
| CI budget test | `scripts/ci_budget_test.py` | 0 |
| CI planner documentation | `docs/ci/pr-plan.md` | 0 |
| Contributor / troubleshooting guidance | `CONTRIBUTING.md`, `docs/troubleshooting.md` | 0 |
| Agent guidance | `AGENTS.md`, `agents/shared/repo.md`, `agents/shared/roles/critic.md` | 0 |
| Git subprocess isolation | `crates/tokmd-git/src/command.rs`, `crates/tokmd-git/src/refs.rs`, `crates/tokmd/src/git_support.rs`, `crates/tokmd-scan/src/walk/git.rs` | 0 |
| FFI inputs | `crates/tokmd-core/src/ffi/mod.rs`, `inputs.rs`, `parse.rs` | 0 |
| Path handling | `crates/tokmd-scan/src/path/bounded_path.rs`, `crates/tokmd-scan/src/exclude/mod.rs` | 0 |
| File content reads | `crates/tokmd-analysis/src/content/mod.rs` | 0 |
| Redaction / hashing | `crates/tokmd-format/src/redact/mod.rs`, `extensions.rs` | 0 |
| Subprocess audit/semver | `crates/tokmd-cockpit/src/supply_chain.rs`, `crates/tokmd-cockpit/src/gates/contracts.rs` | 0 |
| GitHub workflows | `.github/workflows/*.yml` (29 files), `.github/settings.yml`, `action.yml` | 0 |
| Build / lint / dependency policy | `Cargo.toml`, `deny.toml`, `clippy.toml`, `.cargo/config.toml` | 0 |
| Web runner (browser) | `web/runner/main.js`, `worker.js`, `auth.js`, `messages.js`, `runtime.js`, `ingest.js` | 0 |
| Threat model | `.factory/threat-model/threat-model.md` | regenerated |

### Commit-level Analysis

```
07d878beee199568dbfa7698686ca1999addb7c8  2026-10-26T13:40:00-04:00  fix(ci): retain renamed paths in affected selectors (#771)
e79d82d957d136b87805eb2e0f5373eef69d4144  2026-10-26T08:26:50-04:00  fix(gate): preserve other runs' target directories (#770)
3e3dc2e0c39c587d4dc3d28327b1bfe8e64a8d39  2026-10-26T07:17:34-04:00  docs: clarify same-session review and evidence reuse (#769)
```

The commit immediately preceding the window is `46b8963` (2026-10-17T22:17:17-04:00,
`fix(ci): require exact qualification requests and share work deadline (#737)`),
9 days before the oldest in-window commit. The window boundary is therefore
clean: all three in-window commits fall inside the intended 7-day interval and
no reachable commit falls between them.

#### `07d878b fix(ci): retain renamed paths in affected selectors (#771)`

- **Type:** CI / xtask behaviour change. 6 files, `+589/-30`.
- **Net code change:**
  - `xtask/src/tasks/affected.rs`: `changed_files` now invokes
    `git diff --no-renames --name-only -z <base> <head> --`. A new
    `parse_diff_paths` helper decodes the NUL-framed stdout, splits on `\0`,
    drops empty records, sorts, and dedupes. The previous
    `normalize_path` helper — which rewrote `\` to `/` — was deleted.
  - `xtask/src/tasks/ci_plan.rs`: `git_diff_names` gains the same
    `--no-renames -z` framing and delegates to `parse_diff_paths`.
  - `xtask/src/tasks/affected.rs::print_human_report`: `base`, `head`,
    scope `name`, scope `reason`, and each `unknown_files` entry are now
    rendered with `escape_debug()` instead of raw interpolation.
- **STRIDE analysis:** STRIDE-positive for Tampering and Elevation of
  Privilege. Before this change, a Git filename containing a newline would be
  split into multiple records by the newline-delimited parser, and a filename
  could therefore impersonate a second changed path. The `normalize_path`
  helper additionally conflated a literal backslash in a Git filename with a
  directory separator, which could make a filename match (or fail to match) a
  policy scope incorrectly. `--no-renames` additionally ensures a rename does
  not silently drop its removed endpoint from the affected surface.
- **Fail-closed property:** A removed path with no matching scope stays in
  `unknown_files` and fails discovery under `fail_on_unknown_non_rust`; the
  CI planner refuses the `docs_only_change` claim when `unmatched_files` is
  non-empty. This is asserted by the new test
  `ci_plan_unknown_removed_path_prevents_docs_only_claim`.

#### `e79d82d fix(gate): preserve other runs' target directories (#770)`

- **Type:** xtask / CI behaviour change. 7 files, `+146/-190`.
- **Net code change:**
  - `xtask/src/tasks/build_guard.rs`: removes `cleanup_stale_scoped_dirs`,
    `cleanup_stale_scoped_dirs_in`, `is_older_than`,
    `configured_stale_age`, `configured_stale_age_hours`, and the
    `TOKMD_GATE_STALE_HOURS` override.
  - `xtask/src/tasks/gate.rs`: the gate no longer sweeps stale sibling
    directories before allocating its own disposable target.
  - `.github/workflows/ci.yml`: the self-hosted setup step no longer runs
    `find /tmp -maxdepth 1 -type d -name 'tokmd-gate-target-*' -mmin +30 -exec rm -rf {} +`.
- **STRIDE analysis:** STRIDE-positive for Tampering. The removed sweep
  deleted any directory under `/tmp` matching `tokmd-gate-target-*` whose age
  exceeded the threshold. On a shared or long-lived self-hosted runner, a
  concurrently running job's target directory is younger than the threshold
  and is protected, but a job that is slow to produce output, or one whose
  directory was created before a long compile step, can exceed the age
  threshold while still being in active use — the sweep would then remove a
  live build directory belonging to another run. Removing the sweep eliminates
  this cross-run destructive path entirely.
- **Preserved controls (verified):** `ensure_min_free_space` is still called
  before each governed Cargo step in `xtask/src/tasks/gate.rs`, with the
  `TOKMD_MIN_FREE_GB` floor (default 8 GiB) and its override intact.
  `ScopedTempDir::drop` still removes the current invocation's directory on
  clean exit. The disk-pressure refusal therefore fails closed rather than
  being resolved by deleting another run's files.

#### `3e3dc2e docs: clarify same-session review and evidence reuse (#769)`

- **Type:** Documentation only. 3 files (`AGENTS.md`, `agents/shared/repo.md`,
  `agents/shared/roles/critic.md`). No code, workflow, dependency, schema, or
  release-surface change.
- **STRIDE analysis:** STRIDE-neutral. It changes guidance about where an
  independent reviewer runs and how earlier evidence is carried forward. It
  does not alter any workflow trigger, protection setting, or qualification
  activation.

### Patches Generated

No patches were generated this scan (no findings at or above `medium`).

### Validation Performed

The following checks were executed by this scan against commit `07d878b`:

| Check | Result |
|-------|--------|
| `cargo build --locked -p xtask` | pass |
| `cargo clippy --locked -p xtask --all-features -- -D warnings` | pass |
| `cargo fmt --all -- --check` | pass |
| `cargo xtask proof-policy --check` | pass (93 scopes) |
| `cargo test --locked -p xtask --test affected_w91` | pass (11 tests) |
| `cargo test --locked -p xtask --test required_gate_locked_commands_w108` | pass (6 tests) |
| `cargo test --locked -p xtask --bin xtask ci_plan` | pass (31 tests) |
| `python3 -m unittest ci_budget_test` (from `scripts/`) | pass (10 tests) |

The full required-gate sequence (`cargo --locked xtask gate --check`) and the
workspace-wide default-member test command were **not** executed by this
scan; the window's commits are already recorded as having passed their own
gate qualification.

### Next Scan

The next scheduled security scan runs Monday, 2026-11-09 via
`.github/workflows/droid-security-scan.yml` (cron `0 8 * * 1`).

## Appendix

### Threat Model

- **Status:** Regenerated this scan.
- **Location:** `.factory/threat-model/threat-model.md`
- **Previous revision:** 2026-08-02 (92 days old at scan time — 2 days past
  the stated 90-day regeneration window)
- **Current revision:** 2026-11-02
- **Methodology:** STRIDE
- **Next review due:** 2027-02-01, or earlier on architecture change

The regeneration added the `xtask` control plane as an explicitly scoped
non-distribution trust zone (TB-11), added trust boundaries for third-party
action code (TB-12) and the shared runner filesystem (TB-13), and folded in
the window's changes: the removal of the cross-run artifact-deletion surface,
the NUL-framed Git path decoding, and the fail-closed unmatched-path policy.
It also converted the previously-partial action-pinning statement into a
measured inventory (98 SHA-pinned / 114 tag-pinned) and recorded that no
workflow uses `pull_request_target`, and that the single `workflow_run`
trigger (`nix-full.yml`) is gated on the publication repository, a successful
`main` CI run, and a specific head SHA.

### Scan Metadata

- **Intended window:** 2026-10-26 → 2026-11-02
- **Commits scanned:** 3 (`07d878b`, `e79d82d`, `3e3dc2e`)
- **Files changed in window:** 16
- **Window completeness:** Established for the observed checkout. The clone
  was deepened with `git fetch --deepen=100 origin main` (128 reachable
  commits total). The nearest commit outside the window is 9 days older than
  the window's oldest commit, so no reachable commit is unaccounted for at
  the window boundary. This does not prove the absence of commits on other
  refs or in unreachable history.
- **Scan Duration:** ~25m (diff review, defense re-verification, targeted
  validation runs, threat-model regeneration)
- **Skills Used:** threat-model-generation (manual, STRIDE),
  commit-security-scan (manual, STRIDE), vulnerability-validation (manual,
  exploitability and reachability assessment), security-review (manual,
  defense confirmation and patch assessment)
- **Manual Reviewers:** 1 (Droid scheduled security scan)
- **Automated scanners run:** None
- **False Positive Filter:** applied — see Observations above

### References

- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Advisory Database](https://rustsec.org/)
- [GitHub Actions security hardening](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
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
