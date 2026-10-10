# Product Contract: tokmd

> This document defines the core product philosophy, invariants, and boundaries of `tokmd`.

## The Core Promise

**tokmd transforms code scans into actionable intelligence: receipts for automation, metrics for understanding, and signals for decision-making.**

It is not just a counter. It is the **fast deterministic code-intelligence and
review-receipt engine** in the Effortless Metrics evidence stack. It converts
raw counts into checkable code artifacts and derived insights without trying
to own the whole evidence transport layer. It must also remain useful as a
standalone tool for inspecting a repository, reviewing a change, and preparing
context for a human or coding agent.

## The Problems We Solve

1. **"Counting" is easy. Using the count is the pain.**
   * `tokei` gives numbers. Real work needs pasteable summaries, machine-readable payloads, and monorepo views.
   * `tokmd` replaces fragile `jq | column` chains with a single cross-platform binary.
2. **LLM workflows need a map, not a dump.**
   * Agents need a structured inventory: languages, modules, important files, exclusions, and estimated context cost.
   * `tokmd context` selects files using a model-neutral token estimate. Selection budgets are not a claim of exact model-token counts or a universal hard bound on rendered output.
3. **Automation fails by "confident narration".**
   * "I scanned the repo" is not evidence of what was read or executed.
   * A receipt supplies structured claims to validate against inputs, artifact contents, producer outcomes, and the relevant schema.
   * An artifact is not trusted merely because it exists or contains a passing status.
4. **Understanding requires more than counts.**
   * `tokmd analyze` derives signals about distribution, change, and risk.
   * Their measurement scope and limitations matter as much as the values.

## Product Invariants

These are the behavior contracts to implement and verify. They are not a
certificate that every current command already satisfies every invariant;
implementation gaps and acceptance work are tracked below.

### 1. One Scan, Many Views

Reuse the same scan inventory for language, module, and export views. Content
and Git analyses may require additional observations; report their own scope
and availability rather than implying that inventory alone proves analysis.

### 2. Deterministic Semantic Output is a Feature

For identical input content, selected Git identities, relevant settings,
capabilities, and tool/schema versions, semantic payloads must reproduce:

* Stable ordering and tie-breaks by name/path.
* Normalized paths and stable redaction behavior within the documented scope.
* Stable selection, metric interpretation, exclusions, and omission reasons.
* Versioned schemas and integrity hashes for the bytes they actually cover.

Execution timestamps and other documented run metadata may vary. Comparisons
must isolate those fields explicitly; they must not discard changed warnings,
missing files, capabilities, or measurements to manufacture equality.

### 3. Receipts Beat Reassurance Only When Checked

Each receipt family defines its required schema, tool version, mode, input
identity, settings, result, and integrity fields. A streaming export row and a
handoff manifest are different contracts; do not assume one universal envelope
or infer an absent field.

An integrity hash verifies covered bytes, not execution, freshness, source
identity, or success. A gate must validate the applicable schema and evidence
identity, reconcile producer outcomes, and reject missing or contradictory
required evidence. Successful receipt construction can describe a failed
operation; its process exit alone is not a passing consumer verdict.

### 4. Shape, Not Grade

`tokmd` is **not** a developer-productivity metric tool. It is a sensor for
inventory, distribution, risk signals, and blast radius, not individual
velocity or performance ranking.

### 5. Signals, Not Unsupported Judgments

"Doc density is 12%" and "File changed 47 times" are observations to interpret
in context, not absolute quality judgments. Derived ratios need compatible
numerator and denominator scopes. Unreadable, unsupported, skipped, and
truncated inputs must remain distinguishable from successfully measured input.
Missing measurement is not zero complexity, and a smaller measured population
is not evidence that the implementation improved.

### 6. Code Evidence, Not The Backplane

`tokmd` produces code evidence. It does not own the universal evidence bundle,
global merge verdict, or multi-tool inventory. In the wider Effortless Metrics
stack, `evidencebus` is the schema-first backplane for validating, bundling,
inventorying, and exporting evidence from producers such as `tokmd`,
`mergecode`, CI sensors, gates, and performance tools.

### 7. Planned Context Must Reconcile With Emitted Context

Selection, policy classification, packing charges, and rendering must have an
explicit accounting contract. Keep cheap inventory estimates, policy estimates,
selected charges, and observed output bytes distinguishable.

A selected file is not an emitted file. A required read failure must fail the
operation or be represented by an explicitly partial result. Payloads,
inventories, counts, diagnostics, and completion status must agree. Failed
replacement must not advertise mixed-generation artifacts as a complete bundle.
A small excerpt should have bounded retained memory and an explicit framing and
truncation policy rather than relying on average line length.

## Safety Posture

**"If you wouldn't email it, don't paste."**

Path redaction, path normalization, ignore profiles, and filters support safer
sharing. Path redaction is not source-content secret removal; inspect source
bundles before sharing them. Command-specific resource caps and output filters
are not interchangeable. In particular, `context --max-output-bytes` is a
warning threshold checked after writing, not a hard output limit.

## Capabilities

| Capability | Feature |
| :--- | :--- |
| **Human Summary** | Markdown tables, TSV, Top-N compaction, tree views. |
| **Machine Receipt** | Versioned JSON receipt families, CycloneDX SBOM. |
| **Pipeline Feed** | Streaming JSONL/CSV exports. |
| **Monorepo View** | Module rollup (`crates/`, `packages/`). |
| **Safety** | Redaction, path normalization, ignore profiles. |
| **Derived Analytics** | Doc density, test density, distribution, COCOMO. |
| **Git Intelligence** | Hotspots, freshness, coupling, bus factor. |
| **Context Planning** | Model-neutral token estimation and window-fit analysis. |
| **Context Packing** | Estimate-budgeted file selection with explicit policies. |
| **Advisory Syntax** | Explicit syntax and shadow tooling; no compiler/type-checking authority. |
| **Visualization** | SVG badges, Mermaid diagrams, HTML reports, tree output. |

## Analysis Presets

| Preset | Scope | Use Case |
| :--- | :--- | :--- |
| `receipt` | Derived metrics only | Quick health check |
| `health` | + TODO density | Code hygiene review |
| `risk` | + Git metrics | Risk assessment |
| `supply` | + Assets + deps | Dependency audit |
| `architecture` | + Import graph | Structure analysis |
| `topics` | + Semantic topics | Domain discovery |
| `security` | + License + entropy | Security review |
| `identity` | + Archetype + fingerprint | Project profiling |
| `git` | + Predictive churn | Trend analysis |
| `deep` | Everything | Comprehensive review |
| `fun` | Novelty outputs | Team morale |

## Boundaries (Non-Goals)

`tokmd` explicitly does **not**:

* Format or lint source code in place of tools such as rustfmt or eslint.
* Maintain its own vulnerability advisory database or replace security scanners.
* Replace test runners: routed build/test commands remain their own source of truth.
* Own the evidence transport backplane or global merge decision.
* Act as a whole-program compiler, semantic analyzer, or type checker. Explicit
  Tree-sitter syntax landmarks remain advisory; default receipt promotion needs
  a separate schema-reviewed decision.
* Score or rank developers or provide absolute quality judgments.

## Acceptance and Implementation Gaps

The [product-evidence alignment plan](plans/product-evidence-alignment.md)
records the consumer gaps found at audited source `5f5ca66c` on 2026-10-10,
implementation issues, dependencies, and proof boundaries. The existence of
this contract or a proposed PR does not close those gaps.

Acceptance follows the installed user journey: inspect a repository, review
the intended change, validate its evidence, and hand off useful bounded context.
It must cover degraded inputs and recovery, not only happy-path snapshots.
Browser acceptance follows the [capability matrix](browser-capability-matrix.md)
for the loaded bundle rather than assuming native command parity.

Maintainability work should prevent new or worsened debt before tightening
existing baselines. Refactor coherent responsibilities, not arbitrary helpers
to lower a metric. Performance claims require bounded, repeatable measurements
on identical inputs with semantic-output parity; a cache or data-structure
change is not automatically a user-visible improvement.

## Future Direction

* **MCP Server**: Future server/resource integration with Claude and MCP-compatible tools; `tokmd tools` already covers tool schema generation.
* **Watch Mode**: Continuous analysis during development.
* **Plugin System**: WASM-based extensible enrichers.
* **Smart Suggestions**: Context-aware file recommendations for LLM workflows.

These directions do not displace correctness repairs or authorize release,
schema, advisory-proof, or default-capability promotion.
