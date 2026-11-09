# Threat Model — tokmd-swarm

**Generated:** 2026-08-02
**Regenerated:** 2026-11-09
**Scope:** Repository-wide STRIDE analysis for `EffortlessMetrics/tokmd-swarm`
**Methodology:** STRIDE (Microsoft Threat Modeling)
**Last Reviewed:** 2026-11-09
**Reviewed Commit:** `07d878beee199568dbfa7698686ca1999addb7c8` (`fix(ci): retain renamed paths in affected selectors (#771)`)

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

## 2. Trust Boundaries

| ID | Boundary | Crossed by |
|----|----------|------------|
| TB-1 | User shell → CLI argv | CLI flags, paths, globs |
| TB-2 | Untrusted repository contents → tokmd file walker | Filesystem, symlinks, `.gitignore` |
| TB-3 | tokmd → external `git` binary | `git log`, `git diff`, `git rev-parse` (via `std::process::Command`) |
| TB-4 | tokmd → external `cargo` binary | `cargo audit` (cockpit supply-chain gate) |
| TB-5 | Python/Node → tokmd FFI | JSON-string args to `run_json()` |
| TB-6 | CI workflow → repository secrets | `secrets.*` (e.g., `MINIMAX_API_KEY`, `FACTORY_API_KEY`) |
| TB-7 | Scanner → output destination | User-specified `--output`, `--bundle-dir`, `--log` paths |
| TB-8 | Untrusted inputs (paths) → inclusion policy | `--paths`, `--include`, in-memory `inputs[]` |
| TB-9 | In-memory mode → boundary path canonicalization | ReDoS on path normalization |
| TB-10 | WASM host → in-memory filesystem | Sandboxed JS environment |
| TB-11 | WASM host → archive bytes | `runJsonBytes` ZIP decode (added 2026-09/10) |
| TB-12 | CI event context → workflow admission | `GITHUB_EVENT_PATH`, `GITHUB_*` env (added 2026-10) |

## 3. STRIDE Analysis

### Spoofing (S)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| Fake git history impersonation | LOW | Git is invoked as a subprocess; tokmd reads stdout, not metadata. No trust placed in commit author fields beyond display. | Author field displayed in receipts; do not gate on it. |
| Impersonation via FFI arg fields | LOW | Strict JSON parsing with explicit type validation in `tokmd-core/src/ffi/parse.rs` rejects type mismatches. | Low — caller controls input. |
| GitHub Action impersonation | MEDIUM | `actions/checkout` is pinned by SHA (`3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1`) in all workflows. | MEDIUM — many *other* third-party actions remain pinned only by mutable tag (see §5.1). |
| CI qualification spoofing | MEDIUM | `scripts/ci_qualification.py` requires an exact `<!-- tokmd-qualification:v1 -->` manifest, a full 40-hex head/base SHA, a matching `generation`, an open PR targeting `main`, and live `git ls-remote` confirmation that `refs/pull/N/head`, `refs/pull/N/merge` and `refs/heads/main` match the request. | LOW — the live-ref check is the authoritative anti-spoofing control. |

**Result:** No `high`/critical spoofing vectors. Two MEDIUM residual supply-chain observations tracked in §5.1 and §5.2.

### Tampering (T)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| Unsanitized git refs becoming shell args | HIGH (if reached) → MITIGATED | `tokmd-git/src/command.rs` uses `Command::new("git").arg()` (not shell). `tokmd-git/src/refs.rs::env_base_ref_is_safe` rejects `--`, whitespace, control, and `\\` in refs. `--end-of-options` separator used. `GIT_REPO_SHAPING_ENV` is `env_remove`'d. | LOW — defense in depth applied. |
| Path traversal via scan roots | HIGH (if reached) → MITIGATED | `tokmd-scan/src/path/bounded_path.rs` rejects empty paths, `..` segments, absolute paths. `ValidatedRoot` requires canonicalization. `BoundedPath::existing_relative` enforces `ensure_under_root`. | LOW — tested. |
| Path traversal via FFI in-memory inputs | HIGH (if reached) → MITIGATED | `tokmd-core/src/ffi/inputs.rs::validate_in_memory_input_path` rejects: empty, >4096 bytes, control chars, leading `/` or `\\`, Windows drive prefixes, `..` segments, all-`.` paths. | LOW — exhaustive rejection. |
| Path traversal via archive entries (Zip Slip) | HIGH (if reached) → MITIGATED | `tokmd-io-port/src/archive.rs::validate_and_normalize_name` rejects absolute, drive/UNC, `..`, NUL, and empty-normalizing names before any entry is admitted. `snapshot_from_entries` fails the whole snapshot on the first violation. | LOW — independently re-verified 2026-11-09 (see §6). |
| Symlink / special-file entries in archives | HIGH (if reached) → MITIGATED | `classify_zip_entry` masks `S_IFMT` and maps symlink, FIFO, socket, char/block device to `EntryKind::Other`, which the admission engine rejects. | LOW — independently re-verified 2026-11-09 (see §6). |
| File content write at attacker-chosen path | MEDIUM (if reached) → MITIGATED | `tokmd/src/commands/run.rs` writes to user-provided `--output-dir` or `.runs/tokmd/<id>/` (no traversal). `--output`, `--bundle-dir`, `--log` are added to exclude patterns to prevent recursive read. | LOW — output dirs are user-chosen. |
| Cargo lockfile tampering | MEDIUM | `tokmd-cockpit/src/supply_chain.rs` invokes `cargo audit --json` and parses structured output. `parse_audit_output` tolerates malformed JSON by returning `Pending`, never `Pass`. | LOW — supply-chain evidence is informational. |
| Workspace lint bypass via file mutation | LOW | `cargo xtask lint-fix` and `cargo xtask gate --check` run as pre-push hooks. CI requires both. | LOW. |
| Injection into .tokeignore / config files | LOW | `.tokeignore` patterns are passed to `tokei`/`ignore` crate, not eval'd. `ConfigMode::Auto` reads tokei config files; `ConfigMode::None` skips. | LOW. |
| Filename injection into CI affected-discovery output | MEDIUM (if reached) → MITIGATED | `xtask/src/tasks/affected.rs` switched `git diff --name-only` to `--no-renames --name-only -z` (NUL-framed) so Git cannot quote or split a filename at a newline, and applies `escape_debug` to every dynamic field in the human report. | LOW — fixed 2026-10-26 (#771). |

**Result:** All `high`/medium tampering vectors are mitigated with defense in depth.

### Repudiation (R)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| "I never ran that scan" claims | LOW | Receipts include `schema_version`, `generated_at_ms`, scan args, and tool metadata. JSON outputs are signed by inclusion in audit pipelines. | LOW — receipts are not cryptographically signed. |
| Audit log tampering | LOW | `cargo xtask gate --check` is the gate; receipts are deterministic. | LOW. |
| Qualification-run provenance | MEDIUM | `scripts/ci_qualification.py` records head/base/merge SHA and a `deadline_epoch`; the observability clock markers (`target/ci-core/*_mono_ns`) bind run duration. | LOW — the clock markers are workspace artifacts, not signed. |

**Result:** LOW residual risk. Receipts are auditable but not signed.

### Information Disclosure (I)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| Secrets in redacted output | MEDIUM (if reached) → MITIGATED | `tokmd-format/src/redact/mod.rs` uses BLAKE3, 16-char prefix, with extension-preserving allowlist (`tokmd-format/src/redact/extensions.rs`). Reverts to bare hash for unknown/unsafe compound suffixes. | LOW — non-allowlisted extensions get bare hash. |
| Path disclosure via receipts | LOW | `--redact=paths|all` mode available. Receipts do not include file contents by default (only counts). | LOW. |
| File content disclosure via `--content` analysis | MEDIUM (if reached) → MITIGATED | `tokmd-analysis/src/content/mod.rs` uses `ContentLimits` (default `max_file_bytes=128KiB`, total `max_bytes`) to bound read. `is_text_like` check skips binary blobs. | LOW — content is read once into memory. |
| Environment variable leakage into receipts | MEDIUM (if reached) → MITIGATED | `tokmd-git/src/command.rs`, `tokmd/src/git_support.rs` and `tokmd-scan/src/walk/git.rs` each strip `GIT_DIR`, `GIT_WORK_TREE`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_ASKPASS`, `GIT_PAGER`, `GIT_EDITOR`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF` before spawning git. | LOW — env is not propagated to receipts. |
| Stderr exposure of repository internals | LOW | `git log` runs with `Stdio::null()` for stderr; only stdout is parsed. | LOW. |
| Information disclosure via error messages | LOW | Errors return `TokmdError` enum with typed codes; internal paths not exposed. `fix(cli): keep local file paths out of network recovery hints` (#654) classifies stable local file failures before text-only remote-service hint matching. | LOW. |
| XSS via HTML analysis report | MEDIUM (if reached) → MITIGATED | `tokmd-format/src/analysis/html/format.rs::escape_html` escapes `& < > " '` for all text and attribute interpolation in `html/table.rs`; `html/report_json.rs` replaces `<`/`>` with `\u003c`/`\u003e` in the embedded JSON to block `</script>` breakout. | LOW — verified 2026-11-09. |
| XSS via SVG badge | MEDIUM (if reached) → MITIGATED | `tokmd-format/src/badge/mod.rs::escape_xml_text` escapes `& < > " '` for both badge text nodes. | LOW. |

**Result:** All `medium`/high I-disclosure vectors mitigated.

### Denial of Service (D)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| Pathological regex via exclude patterns | MEDIUM (if reached) → MITIGATED | `tokmd-scan/src/exclude/` normalizes patterns. `tokmd-scan/src/walk/` uses bounded traversal. `tokmd-scan/src/lib.rs::scan` enforces `max_commits` / `max_commit_files` for git history. | LOW. |
| ReDoS in path normalization | LOW | `tokmd-format/src/redact/mod.rs::clean_path` uses bounded loops with `String::replace` for `/./`. Bounded by input length. | LOW. |
| Resource exhaustion via large git history | MEDIUM (if reached) → MITIGATED | `tokmd-git/src/lib.rs::collect_history` honors `max_commits` and `max_commit_files`. Streaming `BufReader` does not load full history. `fix(git): reap history children on limits and read errors` (#724) terminates and reaps the direct child instead of waiting for unused history. | LOW. |
| Resource exhaustion via large file read | MEDIUM (if reached) → MITIGATED | `tokmd-analysis/src/content/mod.rs` enforces `DEFAULT_MAX_FILE_BYTES = 128 KiB` and total `max_bytes` limit. | LOW. |
| Resource exhaustion via context head/tail excerpt | MEDIUM (if reached) → MITIGATED | `fix(context): bound HeadTail bytes and retained memory` (#695) replaced an unbounded `read_to_string` with a fixed 8 KiB streaming read plus a 4-bytes-per-token retention allowance and an explicit byte-bounded tail ring buffer. | LOW. |
| Zip bomb via archive ingestion | HIGH (if reached) → MITIGATED | `tokmd-io-port/src/archive.rs::ArchiveLimits` enforces per-entry (64 MiB), total (1 GiB), count (65 536) and compression-ratio (100x) caps. The ZIP codec inflates each entry through a reader capped at `max_entry_size + 1` byte so a hostile declared size cannot force unbounded allocation, and the admission engine re-checks every limit authoritatively. | LOW — independently re-verified 2026-11-09 (see §6). |
| Pathological FFI JSON payload | LOW | `serde_json::from_str` is recursive; no DoS-specific bounds. The 4 KiB path limit bounds `inputs[]`; arbitrary JSON `args` size is not bounded by the FFI layer itself. | MEDIUM — large-but-valid JSON could spike memory. Observed but not at severity threshold for a finding. |
| Fuzz target coverage | LOW | 19 fuzz targets in `fuzz/` directory. | LOW. |

**Result:** All `medium`/high DoS vectors mitigated or have explicit resource limits. One observation noted but not at severity threshold.

### Elevation of Privilege (E)

| Threat | Severity | Mitigation | Residual risk |
|--------|----------|------------|------------|
| Code execution via git hooks | MEDIUM (if reached) → MITIGATED | `GIT_REPO_SHAPING_ENV` (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_OBJECT_DIRECTORY`, `GIT_ALTERNATE_OBJECT_DIRECTORIES`, `GIT_COMMON_DIR`, `GIT_CEILING_DIRECTORIES`, `GIT_SSH`, `GIT_SSH_COMMAND`, `GIT_ASKPASS`, `GIT_PAGER`, `GIT_EDITOR`, `GIT_PROXY_COMMAND`, `GIT_EXTERNAL_DIFF`) is `env_remove`'d before every git subprocess, in all three git-command constructors. | LOW. |
| Code execution via Python `pyo3` bindings | MEDIUM (if reached) → MITIGATED | `tokmd-python/src/lib.rs` documents FFI safety invariants. `?` operator used for error propagation. `.expect()` prohibited in production. GIL released via `py.detach()` for long scans. | LOW. |
| Code execution via `Command::new` arg construction | HIGH (if reached) → MITIGATED | All production `Command::new` invocations use `arg()`/`.args()` (not a shell string). No production use of `sh -c`/`bash -c`; the only `sh -c` in the tree is inside a `#[cfg(all(test, unix))]` fixture in `tokmd-git/src/lib.rs`. Verified across `tokmd-git`, `tokmd-cockpit`, `tokmd-core`, `tokmd-scan`, `tokmd`. | LOW. |
| Privilege escalation via supply chain | MEDIUM | `Cargo.lock` is committed. `deny.toml` enforces advisory check (cargo-deny). `RUSTSEC-2020-0163` (transitive `term_size`) is documented as an upstream limitation. | MEDIUM — transitive advisory remains. Observed but not at severity threshold for a finding. |
| Code execution via WASM host | MEDIUM (if reached) → MITIGATED | `tokmd-wasm` uses `MemFs` (no host fs). Sandboxed by the WASM runtime. | LOW. |
| Archive bytes reaching the host filesystem | HIGH (if reached) → MITIGATED | `snapshot_from_zip_bytes` decodes into an in-memory `MemFs`; it never extracts to the host. The root directory is not even created on a rejected archive. | LOW — independently re-verified 2026-11-09 (see §6). |

**Result:** All `high` elevation vectors mitigated. One supply-chain observation noted.

## 4. Out-of-Scope

- Issues in third-party crates (report upstream; not actionable here).
- Theoretical attacks without realistic exploitation paths.
- Performance regressions that are not denial-of-service.
- The `home` crate vendored at `vendor/home-0.5.12` (intentional temporary patch — `Cargo.toml` `[patch.crates-io]`).

## 5. Residual Observations (below the medium reporting threshold, tracked)

### 5.1 Third-party GitHub Actions pinned by mutable tag

Many workflows reference actions by tag rather than SHA (`dtolnay/rust-toolchain@stable`,
`Swatinem/rust-cache@v2`, `actions/upload-artifact@v7.0.1`,
`DeterminateSystems/nix-installer-action@v22`, `taiki-e/install-action@v2`,
`codecov/codecov-action@v7`, `softprops/action-gh-release@v3`, `EndBug/label-sync@v2`,
`thollander/actions-comment-pull-request@v3`, `peter-evans/create-pull-request@v8`,
`docker/*@v4`/`@v6`/`@v7`, `actions/setup-node@v7`, `actions/cache@v6`,
`actions/download-artifact@v8`, `actions/attest-build-provenance@v4`).

`actions/checkout` and the `EffortlessMetrics/droid-action-safe` / `ub-review`
actions *are* SHA-pinned. The unpinned set is a mutable-tag supply-chain risk
(a compromised upstream tag would be pulled by every run), but it is
pre-existing, is not introduced by any commit in the current scan window, and
is a repository-wide policy question rather than a defect in the reviewed
changes. It is reported here as a tracked residual observation rather than a
scanned finding.

### 5.2 Unbounded JSON argument size at the FFI boundary

`tokmd-core/src/ffi` parses the caller's `args_json` with `serde_json::from_str`
without an overall byte cap. The per-input path cap (4 KiB) bounds
`inputs[]` entries but not the total payload. A caller that already controls
the process can supply an arbitrarily large valid JSON document. Recorded as
an observation; the FFI layer is not a network-facing boundary.

## 6. Independent Verification Performed 2026-11-09

The 2026-11-09 weekly scan re-verified the archive-ingestion and output-escaping
claims with an out-of-tree adversarial probe rather than relying on the
repository's own tests:

- Zip Slip (`../../..`), absolute path, Windows drive prefix, and backslash
  traversal entries — **all rejected** by `validate_and_normalize_name`.
- Zip bomb (2 MiB of `A` compressed) — **rejected** by the ratio guard.
- Case-insensitive duplicate entries — **rejected** by the duplicate check.
- Symlink, FIFO, socket, char-device and block-device entries with correct
  `S_IFMT` bits — **all rejected** as non-regular entries.
- Host-filesystem isolation — a traversal-bearing archive left the host
  filesystem byte-identical and did not create the root directory.
- HTML analysis report — every interpolated field passes through `escape_html`;
  the embedded JSON replaces `<`/`>` with `\u003c`/`\u003e`.

One probe initially appeared to admit a symlink entry. That was traced to the
`zip` crate's *writer* API (`unix_permissions()` masks off the file-type bits,
so `0o120777` is stored as `0o100777` and is genuinely indistinguishable from a
regular file on read). A hand-crafted archive with correct external attributes
was rejected correctly. The tokmd reader is not defective; the writer-side
masking is an upstream `zip` crate behavior.

## 7. Standing Defenses

These defenses are baked into the workspace and should not regress:

1. **Workspace lints** (`Cargo.toml`): `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, `panic = "deny"`, `unreachable = "deny"`, `dbg_macro = "deny"`, `unimplemented = "deny"`, `todo = "deny"`, plus a long list of correctness lints (numerical, file/process/path, async, etc.). No `unsafe` block exists in any production crate.
2. **Git subprocess isolation**: `git_cmd()` constructors in `tokmd-git`, `tokmd` and `tokmd-scan` strip `GIT_REPO_SHAPING_ENV`.
3. **FFI input validation**: `validate_in_memory_input_path` rejects every known bypass path.
4. **Bounded path handling**: `BoundedPath` enforces canonical under-root invariant.
5. **Archive admission policy**: `tokmd-io-port/src/archive.rs` is a single authoritative, fail-closed admission engine for untrusted archive entries.
6. **Strict JSON parsing**: `parse.rs` rejects type mismatches (no silent fallback to defaults).
7. **Receipt schema versioning**: Per-family constants in `tokmd-types` (`SCHEMA_VERSION`, `COCKPIT_SCHEMA_VERSION`, etc.) — see `CLAUDE.md`.
8. **Pinned `actions/checkout`**: `actions/checkout` is pinned by SHA with a version comment in every workflow. See §5.1 for the unpinned remainder.
9. **License allowlist + advisory check**: `deny.toml` configures `cargo-deny`.
10. **Branch protection policy**: the checked-in settings require 1 PR approval with CODEOWNERS review and the `Tokmd Rust Result` status check. Live protection is repository-specific control-plane state and must be queried separately; the settings file is not proof that public `main` is currently protected.
11. **Constrained merge policy**: normal swarm and public development PRs use squash merges; the publication repository also permits deliberate two-parent merge commits for swarm imports so the shared graph is preserved. Direct pushes, unrelated merge commits, and force-pushes remain prohibited by operating policy and branch controls. `allow_merge_commit = true` and `required_linear_history = false` are topology requirements for that import path, not standing security defenses.
12. **CI qualification admission**: `scripts/ci_qualification.py` binds a CI run to an exact, live-verified head/base/merge SHA triple with a bounded deadline.

## 8. Review Cadence

- **Regenerate when:** architecture changes, new external surface added, new subprocess invocation, or 90 days since last review.
- **Source:** weekly `droid-security-scan` workflow (`.github/workflows/droid-security-scan.yml`).
- **Owner:** EffortlessMetrics/tokmd-swarm maintainers.
