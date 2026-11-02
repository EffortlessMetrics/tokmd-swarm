# Threat Model — tokmd-swarm

**Generated:** 2026-11-02
**Scope:** Repository-wide STRIDE analysis for `EffortlessMetrics/tokmd-swarm`
**Methodology:** STRIDE (Microsoft Threat Modeling)
**Last Reviewed:** 2026-11-02
**Supersedes:** 2026-08-02 revision

## 1. System Overview

**tokmd** is a Rust workspace that wraps the `tokei` library to generate
"inventory receipts" and analytics of code repositories. It produces:

- Human-readable summaries (Markdown, TSV)
- Machine-friendly datasets (JSON, JSONL, CSV, CycloneDX SBOM)
- Library facades for Python, Node.js, and WASM consumers
- A CLI binary (`tokmd`) and library API (`tokmd-core`)

**Distribution surfaces:**

| Surface | Mechanism | Trust boundary |
|---------|-----------|----------------|
| CLI binary `tokmd` | Local execution, package managers (cargo, brew, AUR, Docker) | User → tool |
| `tokmd-core` Rust library | Crates.io, source builds | Library user → tool |
| `tokmd-python` (PyO3) | PyPI | Python user → tool |
| `tokmd-node` (napi-rs) | npm | JS user → tool |
| `tokmd-wasm` (wasm-bindgen) | npm (browser/worker) | Web user → tool |
| GitHub Action `action.yml` | GitHub Marketplace | CI pipeline → tool |
| Sensor envelope output | CI artifact consumption | CI → downstream consumers |

**Non-distribution surface (repository control plane).** `xtask/` is the
repo-local control plane (`cargo xtask ...`). It is executed by developers and
by CI in the repository checkout, and is **not** published as a library or
binary. It has broader local authority than the shipped crates (it can invoke
`cargo`, `git`, `df`, and write into the workspace), so it is analysed as a
separate trust zone (TB-11) rather than as part of the user-facing tool.

## 2. Trust Boundaries

| ID | Boundary | Crossed by |
|----|----------|------------|
| TB-1 | User shell -> CLI argv | CLI flags, paths, globs |
| TB-2 | Untrusted repository contents -> tokmd file walker | Filesystem, symlinks, `.gitignore` |
| TB-3 | tokmd -> external `git` binary | `git log`, `git diff`, `git rev-parse` (via `std::process::Command`) |
| TB-4 | tokmd -> external `cargo` binary | `cargo audit` (cockpit supply-chain gate) |
| TB-5 | Python/Node -> tokmd FFI | JSON-string args to `run_json()` |
| TB-6 | CI workflow -> repository secrets | `secrets.*` (e.g., `MINIMAX_API_KEY`, `FACTORY_API_KEY`) |
| TB-7 | Scanner -> output destination | User-specified `--output`, `--bundle-dir`, `--log` paths |
| TB-8 | Untrusted inputs (paths) -> inclusion policy | `--paths`, `--include`, in-memory `inputs[]` |
| TB-9 | In-memory mode -> boundary path canonicalization | ReDoS on path normalization |
| TB-10 | WASM host -> in-memory filesystem | Sandboxed JS environment |
| TB-11 | CI/developer shell -> `xtask` control plane | `xtask` argv, git refs, policy files, workspace writes |
| TB-12 | GitHub Actions runner -> third-party action code | `uses:` references in `.github/workflows/*.yml` |
| TB-13 | xtask -> shared runner filesystem (`/tmp`, `target/`) | Disposable target directories, cache directories |

## 3. STRIDE Analysis

### Spoofing (S)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| Fake git history impersonation | LOW | Git is invoked as a subprocess; tokmd reads stdout, not metadata. No trust placed in commit author fields beyond display. | Author field displayed in receipts; do not gate on it. |
| Impersonation via FFI arg fields | LOW | Strict JSON parsing with explicit type validation in `tokmd-core/src/ffi/parse.rs` rejects type mismatches. | Low - caller controls input. |
| GitHub Action impersonation | MEDIUM (if reached) | Mixed pinning: 98 of 212 third-party `uses:` references are SHA-pinned, 114 are tag-pinned. The highest-privilege third-party surface (the custom Droid action) IS SHA-pinned. | MEDIUM - a tag-pinned action can be re-pointed upstream. Mitigated by the absence of `pull_request_target`, and by the fact that write-scoped workflows trigger only on tag push, dispatch, or `main` push. |
| CI planner git ref spoofing | LOW | `cargo xtask affected` and `cargo xtask ci-plan` accept `--base`/`--head`. In CI these are `origin/<base_ref>` and `HEAD` (or validated 40/64-hex SHAs in `release-preflight.yml`). | LOW - a local caller controls their own ref selection. |

**Result:** One `medium` residual item (mixed action pinning, carried as OBS-003).
No `high` spoofing vectors.

### Tampering (T)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| Unsanitized git refs becoming shell args | HIGH (if reached) -> MITIGATED | `tokmd-git/src/command.rs` uses `Command::new("git").arg()` (not shell). `tokmd-git/src/refs.rs::env_base_ref_is_safe` rejects `--`, whitespace, control, and `\\` in refs. `--end-of-options` separator used. `GIT_REPO_SHAPING_ENV` is `env_remove`'d. | LOW - defense in depth applied. |
| Path traversal via scan roots | HIGH (if reached) -> MITIGATED | `tokmd-scan/src/path/bounded_path.rs` rejects empty paths, `..` segments, absolute paths. `ValidatedRoot` requires canonicalization. `BoundedPath::existing_relative` enforces `ensure_under_root`. | LOW - tested. |
| Path traversal via FFI in-memory inputs | HIGH (if reached) -> MITIGATED | `tokmd-core/src/ffi/inputs.rs::validate_in_memory_input_path` rejects: empty, >4096 bytes, control chars, leading `/` or `\\`, Windows drive prefixes, `..` segments, all-`.` paths. | LOW - exhaustive rejection. |
| File content write at attacker-chosen path | MEDIUM (if reached) -> MITIGATED | `tokmd/src/commands/run.rs` writes to user-provided `--output-dir` or `.runs/tokmd/<id>/` (no traversal). `--output`, `--bundle-dir`, `--log` are added to exclude patterns to prevent recursive read. | LOW - output dirs are user-chosen. |
| Cargo lockfile tampering | MEDIUM | `tokmd-cockpit/src/supply_chain.rs` invokes `cargo audit --json` and parses structured output. `parse_audit_output` tolerates malformed JSON by returning `Pending`, never `Pass`. | LOW - supply-chain evidence is informational. |
| Workspace lint bypass via file mutation | LOW | `cargo xtask lint-fix` and `cargo xtask gate --check` run as pre-push hooks. CI requires both. | LOW. |
| Injection into .tokeignore / config files | LOW | `.tokeignore` patterns are passed to `tokei`/`ignore` crate, not eval'd. `ConfigMode::Auto` reads tokei config files; `ConfigMode::None` skips. | LOW. |
| Cross-run deletion of a shared runner's build artifacts | HIGH (if reached) -> MITIGATED | Removed 2026-10-26 (`e79d82d`). The age-based sweeps `cleanup_stale_scoped_dirs` (Rust) and `find /tmp -maxdepth 1 -type d -name 'tokmd-gate-target-*' -mmin +30 -exec rm -rf {} +` (CI) are gone. Current-invocation cleanup (`ScopedTempDir::drop`) and the disk-pressure refusal (`ensure_min_free_space`) are preserved. | LOW - directory age cannot distinguish an abandoned target from an active sibling. |
| Git filename injection into affected/CI discovery | HIGH (if reached) -> MITIGATED | `xtask/src/tasks/affected.rs::parse_diff_paths` and `xtask/src/tasks/ci_plan.rs::git_diff_names` decode `git diff -z` NUL framing, so filenames containing newlines, quotes, or backslashes cannot split a record or inject a field. Human-facing report fields are `escape_debug`'d. `--no-renames` retains both endpoints of a rename. | LOW - verified by unit tests. |
| CI planner scope-evasion via unmatched path | MEDIUM (if reached) -> MITIGATED | A removed path with no scope stays in `unknown_files`; under `fail_on_unknown_non_rust` it fails discovery. The CI planner records unmatched paths in `unmatched_files` and refuses the `docs_only_change` claim when any path is unmatched. | LOW - fail-closed by default. |

**Result:** All `high`/medium tampering vectors are mitigated with defense in depth.

### Repudiation (R)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| "I never ran that scan" claims | LOW | Receipts include `schema_version`, `generated_at_ms`, scan args, and tool metadata. JSON outputs are signed by inclusion in audit pipelines. | LOW - receipts are not cryptographically signed. |
| Audit log tampering | LOW | `cargo xtask gate --check` is the gate; receipts are deterministic. | LOW. |
| Affected-scope receipt integrity | LOW | `cargo xtask affected --json-output` writes a structured `AffectedReport` (`schema`, `ok`, `base`, `head`, `changed_files`, `scopes`, `unknown_files`) consumed by CI as a required artifact. | LOW - the receipt records what was discovered; it does not attest who ran it. |

**Result:** LOW residual risk. Receipts are auditable but not signed.

### Information Disclosure (I)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| Secrets in redacted output | MEDIUM (if reached) -> MITIGATED | `tokmd-format/src/redact/mod.rs` uses BLAKE3, 16-char prefix, with extension-preserving allowlist (`tokmd-format/src/redact/extensions.rs`). Reverts to bare hash for unknown/unsafe compound suffixes. | LOW - non-allowlisted extensions get bare hash. |
| Path disclosure via receipts | LOW | `--redact=paths|all` mode available. Receipts do not include file contents by default (only counts). | LOW. |
| File content disclosure via `--content` analysis | MEDIUM (if reached) -> MITIGATED | `tokmd-analysis/src/content/mod.rs` uses `ContentLimits` (default `max_file_bytes=128KiB`, total `max_bytes`) to bound read. `is_text_like` check skips binary blobs. | LOW - content is read once into memory. |
| Environment variable leakage into receipts | MEDIUM (if reached) -> MITIGATED | `tokmd-git/src/command.rs` strips `GIT_DIR`, `GIT_WORK_TREE`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_ASKPASS`, `GIT_PAGER`, `GIT_EDITOR`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF` before spawning git. | LOW - env is not propagated to receipts. |
| Stderr exposure of repository internals | LOW | `git log` runs with `Stdio::null()` for stderr; only stdout is parsed. | LOW. |
| Information disclosure via error messages | LOW | Errors return `TokmdError` enum with typed codes; internal paths not exposed. | LOW. |
| Runner filesystem residue across runs | LOW | Disposable target directories are removed on clean exit by `ScopedTempDir::drop`. Killed runs may leave residue, but no other run's directory is deleted (see Tampering). | LOW - disk pressure is now reported rather than resolved by deletion. |

**Result:** All `medium`/high I-disclosure vectors mitigated.

### Denial of Service (D)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| Pathological regex via exclude patterns | MEDIUM (if reached) -> MITIGATED | `tokmd-scan/src/exclude/` normalizes patterns. `tokmd-scan/src/walk/` uses bounded traversal. `tokmd-scan/src/lib.rs::scan` enforces `max_commits` / `max_commit_files` for git history. | LOW. |
| ReDoS in path normalization | LOW | `tokmd-format/src/redact/mod.rs::clean_path` uses bounded loops with `String::replace` for `/./`. Bounded by input length. | LOW. |
| Resource exhaustion via large git history | MEDIUM (if reached) -> MITIGATED | `tokmd-git/src/lib.rs::collect_history` honors `max_commits` and `max_commit_files`. Streaming `BufReader` does not load full history. | LOW. |
| Resource exhaustion via large file read | MEDIUM (if reached) -> MITIGATED | `tokmd-analysis/src/content/mod.rs` enforces `DEFAULT_MAX_FILE_BYTES = 128 KiB` and total `max_bytes` limit. | LOW. |
| Pathological FFI JSON payload | LOW | `serde_json::from_str` is recursive; no DoS-specific bounds. The 4 KiB path limit bounds `inputs[]`; arbitrary JSON `args` size is not bounded by the FFI layer itself. | MEDIUM - large-but-valid JSON could spike memory. Observed but not at severity threshold for a finding. |
| Unbounded diff-path accumulation on a hostile repository | MEDIUM (if reached) -> MITIGATED | `git diff --name-only -z` output is bounded by the number of paths in the commit range. The affected/CI planner sorts and dedupes; the pack router matches by glob against a compiled `GlobSet`. | LOW - a repository with millions of paths in one commit range is bounded by git itself, not by the planner. |
| Runner disk exhaustion from orphaned target dirs | MEDIUM (if reached) -> MITIGATED | The age-based sweep was removed; disk pressure is now surfaced as a refusal (`ensure_min_free_space`, floor `TOKMD_MIN_FREE_GB`, default 8 GiB) rather than silently deleted. `ScopedTempDir::drop` removes the current run's directory. | MEDIUM - orphan accumulation on a long-lived self-hosted runner now requires manual operator cleanup rather than automatic reclamation. Availability trade-off accepted deliberately in `e79d82d`. |

**Result:** All `medium`/high DoS vectors mitigated or bounded. Two `medium`
residual items are observations, not findings (below the reporting threshold
for this scan's window).

### Elevation of Privilege (E)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|---------------|
| Code execution via git hooks | MEDIUM (if reached) -> MITIGATED | `GIT_REPO_SHAPING_ENV` (`GIT_DIR`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_ASKPASS`, `GIT_PAGER`, `GIT_EDITOR`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF`) is `env_remove`'d before every git subprocess. | LOW. |
| Code execution via Python `pyo3` bindings | MEDIUM (if reached) -> MITIGATED | `tokmd-python/src/lib.rs` documents FFI safety invariants. `?` operator used for error propagation. `.expect()` prohibited in production. GIL released via `py.detach()` for long scans. | LOW. |
| Code execution via `Command::new` arg construction | HIGH (if reached) -> MITIGATED | All `Command::new` invocations use `arg()` (not shell). No use of `sh -c` or `bash -c` in production code. Verified across `tokmd-git`, `tokmd-cockpit`, `tokmd-scan`, `tokmd-core`, `tokmd`. | LOW - the only `sh -c` site is a `#[cfg(all(test, unix))]` fixture helper in `crates/tokmd-git/src/lib.rs`. |
| Privilege escalation via supply chain | MEDIUM | `Cargo.lock` is committed. `deny.toml` enforces advisory check (cargo-deny). `RUSTSEC-2020-0163` (transitive `term_size`) is documented as an upstream limitation. | MEDIUM - transitive advisory remains. Observed but not at severity threshold for a finding. |
| Code execution via WASM host | MEDIUM (if reached) -> MITIGATED | `tokmd-wasm` uses `MemFs` (no host fs). Sandboxed by the WASM runtime. | LOW. |
| CI privilege escalation via untrusted PR | HIGH (if reached) -> MITIGATED | No workflow uses `pull_request_target`. `pull_request`-triggered workflows run with a read-only token and receive no repository secrets for fork PRs. The Droid review workflow additionally gates on `github.event.pull_request.head.repo.full_name == github.repository`. | LOW - verified across all 29 workflow files. |
| CI privilege escalation via `workflow_run` | MEDIUM (if reached) -> MITIGATED | The only `workflow_run` trigger (`nix-full.yml`) is gated on `github.repository == 'EffortlessMetrics/tokmd'`, requires a completed `CI` run on `main` with `conclusion == 'success'`, checks out `github.event.workflow_run.head_sha` with `persist-credentials: false`, downloads no artifacts, and holds only `contents: read` + `id-token: write`. | LOW - the `id-token: write` grant is latent (no step currently mints an OIDC token). Recorded as OBS-010 in the 2026-11-02 scan. |
| Third-party action compromise on a write-scoped workflow | MEDIUM | Write-scoped workflows (`release.yml`, `promote-release-aliases.yml`, `swarm-ghcr.yml`) are triggered by tag push, `workflow_dispatch`, or `main` push - never by an untrusted PR. | MEDIUM - combined with mixed tag pinning, a compromised tag-pinned action in a write-scoped workflow would inherit its token permissions. |

**Result:** All `high` elevation vectors mitigated. Three `medium` residual items
(transitive advisory; mixed action pinning; the latent `id-token` grant on the
single `workflow_run` job).

## 4. Out-of-Scope

- Issues in third-party crates (report upstream; not actionable here).
- Theoretical attacks without realistic exploitation paths.
- Performance regressions that are not denial-of-service.
- The `home` crate vendored at `vendor/home-0.5.12` (intentional temporary patch - `Cargo.toml` `[patch.crates-io]`).
- The `xtask` control plane's local-authority assumptions. `xtask` runs with the
  privileges of whoever invokes it in a checkout it already controls; it is not
  an attacker-reachable surface for a published artifact.

## 5. Standing Defenses

These defenses are baked into the workspace and should not regress:

1. **Workspace lints** (`Cargo.toml`): `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `unreachable = "deny"`, `dbg_macro = "deny"`, `unimplemented = "deny"`, `todo = "deny"`, plus a long list of correctness lints (numerical, file/process/path, async, etc.).
2. **Git subprocess isolation**: `git_cmd()` constructor strips `GIT_REPO_SHAPING_ENV` (applied in `tokmd-git`, `tokmd`, and `tokmd-scan`).
3. **FFI input validation**: `validate_in_memory_input_path` rejects every known bypass path.
4. **Bounded path handling**: `BoundedPath` enforces canonical under-root invariant.
5. **Strict JSON parsing**: `parse.rs` rejects type mismatches (no silent fallback to defaults).
6. **Receipt schema versioning**: Per-family constants in `tokmd-types` (`SCHEMA_VERSION = 2`, `COCKPIT_SCHEMA_VERSION = 3`, `HANDOFF_SCHEMA_VERSION = 5`, `CONTEXT_SCHEMA_VERSION = 4`, `CONTEXT_BUNDLE_SCHEMA_VERSION = 2`) and `ANALYSIS_SCHEMA_VERSION = 9` in `tokmd-analysis-types`.
7. **NUL-framed git path decoding**: affected discovery and CI planning decode `git diff -z` output; human-readable report fields are `escape_debug`'d.
8. **Fail-closed affected discovery**: unknown (unscoped) paths fail discovery under `fail_on_unknown_non_rust`; unmatched paths block the `docs_only_change` claim.
9. **No cross-run artifact deletion**: the disposable-target sweep was removed; disk pressure is refused, not resolved by deleting another run's files.
10. **No privileged PR triggers**: no `pull_request_target` in any workflow; the Droid review lane requires a same-repository head. The single `workflow_run` trigger (`nix-full.yml`) is gated on the publication repository (`github.repository == 'EffortlessMetrics/tokmd'`), on a successful `main` CI run, and checks out a specific head SHA with `contents: read` only.
11. **Action pinning (partial)**: the custom Droid action and the typos installer are SHA-pinned; other third-party actions are tag-pinned (see OBS-003).
12. **License allowlist + advisory check**: `deny.toml` configures `cargo-deny`.
13. **Branch protection policy**: the checked-in settings require a `Tokmd Rust Result` status check and `required_conversation_resolution: true`, with `allow_force_pushes: false` and `allow_deletions: false`. `required_approving_review_count` is `0` and `require_code_owner_reviews` is `false` by explicit single-maintainer policy. Live protection is repository-specific control-plane state and must be queried separately; the settings file is not proof that public `main` is currently protected.
14. **Constrained merge policy**: normal swarm and public development PRs use squash merges; the publication repository also permits deliberate two-parent merge commits for swarm imports so the shared graph is preserved. Direct pushes, unrelated merge commits, and force-pushes remain prohibited by operating policy and branch controls. `allow_merge_commit = true` and `required_linear_history = false` are topology requirements for that import path, not standing security defenses.
15. **Deterministic receipts**: `BTreeMap` ordering and stable sort keys throughout, so receipts are reproducible and diffable.

## 6. Review Cadence

- **Regenerate when:** architecture changes, new external surface added, new subprocess invocation, or 90 days since last review.
- **Source:** weekly `droid-security-scan` workflow (`.github/workflows/droid-security-scan.yml`).
- **Owner:** EffortlessMetrics/tokmd-swarm maintainers.
- **Next review due:** 2027-02-01, or earlier on architecture change.
