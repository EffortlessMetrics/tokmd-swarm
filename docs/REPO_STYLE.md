# Repo style

This repo is operated as an evidence machine.

Rust and `xtask` are the default construction material. Non-Rust files,
unsafe, panic paths, lint suppressions, generated files, workflow behavior,
process/network access, expensive CI lanes, and release claims must be owned
and receipted.

Static evidence runs first:

- `cargo-allow` for source exceptions.
- `ripr` for static mutation-exposure analysis.
- `unsafe-review` for unsafe-contract reviewability when unsafe seams exist.
- rustc and Clippy for code-shape policy.

Runtime evidence runs where it pays:

- Focused tests on PRs.
- Targeted mutation for risk PRs.
- Broader mutation, Miri, fuzzing, and coverage on nightly and release lanes.

CI is designed for proof per Linux-equivalent minute. Default PRs are cheap,
deterministic, and high-signal. Deep validation is preserved, but routed by
risk pack, label, main, nightly, or release.

Agents work one review-fast PR at a time. Review-fast does not mean tiny; it
means coherent seam, nearby proof, efficient verification, and honest claim
boundary. Do not broaden scope to satisfy CI. Do not add invisible exceptions.

## Tool roles

`xtask` is the repo control plane, not a replacement for upstream tools. It
wraps the durable engines, aggregates receipts, and enforces tokmd-local glue.

| Tool | Repo role | Primary question |
|------|-----------|------------------|
| `cargo-allow` | Durable source-exception ledger | Is this source exception visible, owned, and receipted? |
| `ripr` | Static mutation-exposure analysis | Did the changed behavior expose a weak oracle before runtime mutation is worth spending? |
| `unsafe-review` | Unsafe-contract reviewability | Does the changed unsafe seam have a contract, guard, test reach, and witness route? |
| `cargo-mutants` | Runtime mutation backstop | Do focused tests kill concrete mutants where static signal says risk remains? |
| Miri | Concrete UB execution backstop | Did a selected execution expose undefined behavior? |
| Codecov / coverage | Execution-surface telemetry | Which code paths are exercised, and where are the blind spots? |

## Exception rule

There are no invisible source exceptions. Retained exceptions must have an
owner, a reason, a scope, and evidence in the appropriate receipt or policy
ledger.

The preferred durable ledger is `cargo-allow` through `policy/allow.toml`.
Specialty ledgers are reserved for semantics that a source-exception ledger
cannot express, such as CI lane economics or unsafe witness routing.

## PR operating rule

Every PR should keep the good path easy:

```text
change code
run one command
see exception diffs
see weak-oracle gaps
see unsafe review cards
add focused proof
keep receipts
merge when green
```

A review-fast PR is coherent, not necessarily tiny. It should have a clear seam,
nearby proof, efficient verification, and a claim boundary that says what was
proved, what was not proved, and which follow-ups remain.
