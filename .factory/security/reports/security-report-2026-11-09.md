# Security Scan Report

**Generated:** 2026-11-09
**Scan Type:** Weekly Scheduled
**Repository:** EffortlessMetrics/tokmd-swarm
**Severity Threshold:** medium
**Intended Window:** 2026-11-02 → 2026-11-09
**Observed Scope:** Checked-out commit `07d878b` (`fix(ci): retain renamed paths in affected selectors (#771)`).
`git log --since="7 days ago"` returned **zero** commits in this checkout for the intended
window; the most recent commit on the working branch is dated 2026-10-26, fourteen days
before the scan date. Because the strict window is empty, the manual review covers the
**entire adjacent interval since the previous scan** (2026-08-31 → 2026-10-26), which is
**45 commits** — a superset of the intended window, not a substitute for it. The scan does
not independently prove window completeness: the appendix records only a bounded
`git fetch --depth=100 origin main`, so the zero-finding tally is limited to the observed
checkout and must not be read as a full-history claim.

## Executive Summary

| Severity | Count | Auto-fixed | Manual Required |
|----------|-------|------------|-----------------|
| CRITICAL | 0     | 0          | 0               |
| HIGH     | 0     | 0          | 0               |
| MEDIUM   | 0     | 0          | 0               |
| LOW      | 0     | 0          | 0               |

**Total Findings:** 0
**Evidence limitation:** This is an advisory zero-finding tally from manual review plus a
targeted adversarial probe and a full local build/test/clippy witness. No third-party
vulnerability scanner (`cargo audit`, `cargo deny`, Semgrep, CodeQL) was run in this
environment. The tally is not a security pass and is not evidence that the repository has
no vulnerabilities.
**Auto-fixed:** 0
**Manual Review Required:** 0

**Summary:** The strict 7-day window is empty. The 45 commits in the adjacent interval
since the last scan were reviewed file-by-file against the STRIDE threat model, with
focus on every commit that touched a subprocess boundary, a path-handling path, an FFI
entry point, an output renderer, an archive decoder, a CI workflow, or a dependency
version. No finding at or above the `medium` severity threshold was identified.

The interval is dominated by **STRIDE-positive hardening** rather than new attack
surface. The individually security-relevant commits were:

- `0aa2dd0 fix(deps): upgrade ZIP to 9.0.5 with archive regressions (#735)` — upgrades the
  optional ZIP decoder from 8.6.0 to 9.0.5 with `default-features = false, features =
  ["deflate"]`, keeping the trust surface minimal. Adds a `snapshot_from_zip_bytes` codec
  adapter that **bounded-inflates** each entry through a reader capped at
  `max_entry_size + 1` byte, so a hostile declared size cannot force unbounded
  allocation. This is a **Tampering / Elevation of Privilege reduction**.
- `fbf99ed fix(context): bound HeadTail bytes and retained memory (#695)` — replaces an
  unbounded `std::fs::read_to_string` of a whole source file with a fixed 8 KiB
  streaming read plus a 4-bytes-per-effective-token retention allowance and a byte-bounded
  tail ring buffer. This is a **Denial of Service reduction**.
- `92122cb fix(git): reap history children on limits and read errors (#724)` — terminates
  and reaps the direct Git child on the commit-limit and read-error paths instead of
  waiting for unused history or dropping the `Child`. This is a **Denial of Service
  reduction** (and removes an orphan-process condition).
- `44a3bd6 fix(scan): reject incomplete reads before complete receipts (#702)` — fails
  closed when Tokei marks a language `inaccurate`, so a receipt cannot present partial
  counts as complete. This is a **Repudiation / data-integrity reduction**.
- `b6cec60 fix(cli): keep local file paths out of network recovery hints (#654)` — matches
  typed `io::Error` causes and local producer context *before* text-only
  remote-service hint matching, so a local path is not echoed into network-recovery
  guidance. This is an **Information Disclosure reduction**.
- `873442d fix(diff): keep Git worktree chatter out of report stdout (#717)` — redirects
  worktree subprocess stdout away from the report stream while keeping Git diagnostics on
  stderr. This is a **Tampering reduction** on the machine-readable output contract.
- `07d878b fix(ci): retain renamed paths in affected selectors (#771)` — switches
  `git diff --name-only` to `--no-renames --name-only -z` (NUL-framed) so Git can
  neither quote nor split a filename at a newline, and applies `escape_debug` to every
  dynamic field in the human affected report. This is a **Tampering reduction** against
  filename injection into CI logs.
- `bfb509d fix(ci): write usable observability clock markers (#694)` — repairs four
  clock producers that wrote a literal `\n` (backslash-n) rather than a newline. A
  functional fix with no security surface; the repaired one-liner was executed locally
  and produces correct clock markers.
- `38b5899 fix(run): exclude retained output artifacts from scans (#666)` — excludes an
  existing explicit output directory from the scan using a literal escaped canonical
  path, preventing a run from counting its own retained artifacts.
- `e79d82d fix(gate): preserve other runs' target directories (#770)` — removes the
  age-only cross-run `find /tmp ... -exec rm -rf` sweep that could delete a *concurrent*
  run's target directory. This is a **Tampering / Availability reduction**.
- `40bb56c fix(release): fail closed on contradictory consumer evidence (#683)` — the
  release consumer smoke aggregator now rejects duplicate receipts claiming the same
  surface and fails a required surface that produced no receipt, so filesystem
  enumeration order cannot select a winning verdict.

The remaining commits in the interval are documentation, test-only, CI-planning,
performance, and CLI-recovery-routing changes with no adversarial input path.

## Critical Findings

None.

## High Findings

None.

## Medium Findings

None.

## Low Findings

None. (Two observations below the reporting threshold are recorded in the threat model
at §5.1 and §5.2 rather than as findings: unpinned third-party GitHub Actions by mutable
tag, and the unbounded total JSON argument size at the FFI boundary.)

## Independent Verification Performed

Because the strict window was empty, the scan's value depended on independently
re-verifying the security claims that the window's commits touched, rather than
accepting the repository's own test suite as proof. An out-of-tree adversarial probe
(`/tmp/ziptest`, built against the workspace crates) exercised the archive admission
engine directly:

| Probe | Expected | Observed |
|-------|----------|----------|
| Zip Slip entry `../../../../tmp/ziptest_escape.txt` | reject | **rejected** — parent-traversal path |
| Absolute entry `/tmp/ziptest_escape_abs.txt` | reject | **rejected** — absolute or drive/UNC path |
| Windows drive prefix `C:/ziptest_escape.txt` | reject | **rejected** — absolute or drive/UNC path |
| Backslash traversal `..\..\ziptest_escape_bs.txt` | reject | **rejected** — parent-traversal path |
| Zip bomb (2 MiB of `A`, deflated) | reject | **rejected** — compression ratio 2097152/2050 exceeds limit 100 |
| Case-insensitive duplicate `README.md` / `readme.md` | reject | **rejected** — duplicate entry |
| Symlink entry, `S_IFLNK` (`0o120000`) | reject | **rejected** — non-regular entry |
| FIFO / socket / char-device / block-device entries | reject | **rejected** — non-regular entry |
| Benign entry `src/main.rs` | admit | **admitted** (control) |
| Host-filesystem isolation under traversal entries | no writes | **confirmed** — host file byte-identical, root directory not created |

One probe initially appeared to admit a symlink entry. That was traced to the `zip`
crate's *writer* API: `unix_permissions()` masks off the file-type bits, so `0o120777`
is stored as `0o100777` and is genuinely indistinguishable from a regular file when
read back. A hand-crafted archive with correct central-directory external attributes was
rejected correctly. **The tokmd reader is not defective**; the masking is upstream
`zip` crate writer behavior. This distinction matters because a reader that trusts
`unix_mode()` type bits is only as strong as the archives it is fed, and the archive
supply chain is outside this repository's control.

The HTML and SVG output-escaping claims were verified by reading the renderers: every
interpolated field in `crates/tokmd-format/src/analysis/html/table.rs` passes through
`escape_html` (`& < > " '`), `html/report_json.rs` replaces `<`/`>` with `\u003c`/`\u003e`
to block `</script>` breakout, and `crates/tokmd-format/src/badge/mod.rs::escape_xml_text`
escapes both SVG badge text nodes.

## Appendix

### Threat Model
- Version: **regenerated 2026-11-09** (the previous model was dated 2026-08-02, i.e. 99
  days old, exceeding the 90-day regeneration threshold in §8 of the model itself)
- Location: `.factory/threat-model/threat-model.md`
- Regeneration added the two new trust boundaries introduced since the prior model
  (TB-11 archive bytes via WASM `runJsonBytes`, TB-12 CI event context), documented the
  ZIP codec adapter and the CI qualification admission control, corrected the
  "all third-party actions pinned by SHA" claim in §5 of the prior model (only
  `actions/checkout` and the in-repo `EffortlessMetrics/*` actions are SHA-pinned; see
  the tracked residual observation), and recorded the independent verification in §6.

### Scan Metadata
- Commits scanned: **45** (2026-08-31 → 2026-10-26, the full adjacent interval);
  **0** in the strict 7-day window
- Scan duration: ~35 minutes
- Skills used: threat-model-generation, commit-security-scan, vulnerability-validation,
  security-review
- Tools run:
  - `git fetch --depth=100 origin main` (bounded; window completeness not independently proven)
  - `cargo clippy --locked --workspace --all-features --all-targets -- -D warnings` — **passed**
  - `cargo fmt --all -- --check` — **passed** (exit 0)
  - `cargo test --locked --all-features` — **21 295 passed, 0 failed**
  - `cargo test --locked -p tokmd-io-port --all-features` — **passed** (archive admission suite)
  - `cargo test --locked -p tokmd-git --all-features` — **passed** (git subprocess boundary suite)
  - `cargo test --locked -p tokmd-core --all-features` — **passed** (FFI boundary suite)
  - Out-of-tree adversarial archive probe (`/tmp/ziptest`) — results in the table above
- Not run: `cargo audit`, `cargo deny`, fuzz campaigns, coverage, mutation testing, and
  any hosted CI lane. The local `cargo audit`/`cargo deny` advisories claim is inherited
  from the repository's own workflow, not re-verified here.
- Not independently proven: Windows and macOS behavior (all execution was Linux-native);
  published-release or installed-consumer parity; live branch-protection state.

### Manual Review Coverage

All 45 commits in the adjacent interval were reviewed. Commits whose security-relevant
content was examined in source (not only via commit message) are listed above; the
remainder are documentation, test-only, or CI-planning changes with no adversarial
input path:

`e3323ff`, `6c5aae9`, `7f0d675`, `65eabd7`, `4241d83`, `93dc2b9`, `8358a09`, `652ed4c`,
`8e734bf`, `b6cec60`, `abd89cf`, `038d017`, `fe37ac6`, `89df088`, `06657cf`, `bd965f9`,
`f3404c2`, `507dc4f`, `4c0bd3f`, `31f16b7`, `5f5ca66`, `38b5899`, `5726ac6`, `33c44b0`,
`bfb509d`, `fbf99ed`, `40bb56c`, `44a3bd6`, `7bc1d66`, `bf4a1f0`, `93e1ce2`, `6e0122a`,
`10e51f9`, `873442d`, `c8c34fe`, `12164b7`, `f490b40`, `d35f968`, `92122cb`, `46b8963`,
`33f6f23`, `0aa2dd0`, `83078a8`, `3e3dc2e`, `07d878b`, `e79d82d`.

### References
- [CWE Database](https://cwe.mitre.org/)
- [STRIDE Threat Model](https://docs.microsoft.com/en-us/azure/security/develop/threat-modeling-tool-threats)
- [CVSS 3.1 Calculator](https://www.first.org/cvss/calculator/3.1)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [RustSec Advisory Database](https://rustsec.org/advisories/)
