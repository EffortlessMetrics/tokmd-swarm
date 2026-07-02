# Spec: AST / Syntax Support Tier

- Status: active
- Schema family, if any: n/a (governance contract; receipt families unchanged)
- Related ADRs: `docs/adr/0008-ast-foundation.md`
- Related specs:
  `docs/specs/ast-shadow.md`,
  `docs/specs/ast-shadow-backend.md`,
  `docs/specs/syntax-receipts.md`
- Related proposal: `docs/proposals/ast-productization.md`
- Related proof scopes: `analysis_ast_shadow`, `project_truth_docs`

## Contract

This spec records the **support tier** for AST and syntax surfaces in tokmd. It
does not change product behavior, receipt schemas, or release semantics. It
exists so maintainers, agents, and users can tell which syntax capabilities are
experimental opt-in commands, which remain developer-only shadow evidence, and
which default receipts stay heuristic-only.

### Support tier vocabulary

| Tier | Meaning | User expectation |
| --- | --- | --- |
| `experimental` | Shipped opt-in surface with advisory receipts; may change without semver-major receipt bumps when confined to syntax artifact families | Available in default binaries behind explicit flags or commands; not implied by `analyze` or cockpit |
| `developer-tooling` | Maintainer/agent evidence collection; not a public product command | Run via `cargo xtask` or analysis examples; not advertised as end-user workflow |
| `unchanged` | Default public receipts and bindings keep heuristic facts only | Existing commands behave as before AST work landed |

### Capability map

| Capability | Support tier | User entry | Receipt / artifact |
| --- | --- | --- | --- |
| `tokmd syntax` | experimental | `tokmd syntax <paths>` | `tokmd.syntax_receipt.v1` / `tokmd.syntax_receipts.v1` |
| Packet `syntax.json` | experimental | `tokmd packet generate --syntax` | packet-local `syntax.json` |
| AST shadow compare/check | developer-tooling | `cargo xtask ast-shadow-compare`, `cargo xtask ast-shadow-check` | `tokmd.ast_shadow.v1` directory under `target/tokmd-ast-shadow/` |
| Default `tokmd analyze` | unchanged | existing presets | heuristic analysis receipts only |
| Default `tokmd cockpit` | unchanged | existing commands | heuristic review evidence only |
| Default `tokmd context` / `handoff` | unchanged | existing commands | no syntax facts |
| FFI / Python / Node / WASM | unchanged | existing bindings | no syntax or AST capability |
| Browser runner | unchanged | `web/runner` | no tree-sitter or syntax receipts |

### Build and feature policy

- The `ast` feature remains enabled in default `tokmd` binary builds on main.
  Opt-in behavior is at the **command and packet-flag** boundary, not by
  requiring a custom build profile.
- Syntax and shadow paths stay behind the explicit `ast` feature in library
  crates (`tokmd-analysis`, xtask runners).
- Adding tree-sitter grammars or changing the locked parser registry requires
  `docs/specs/syntax-receipts.md` review and matching proof.

### Promotion boundary

Nothing in this spec promotes AST-derived facts onto default public receipts.
Promotion requires **all** of:

1. a fresh schema-reviewed proposal naming the target receipt family;
2. function-boundary candidate criteria cleared per
   `docs/plans/ast-function-boundary-candidate.md` when precision claims change;
3. updated proof scopes and verifiers before support tier moves from
   `experimental` to `supported`.

Release-notes promotion from `experimental` to `supported` is a separate
maintainer decision and is **out of scope** for this spec.

## Inputs

Maintainers and agents use this spec when:

- choosing AST/syntax work packets;
- writing adoption or integration docs;
- deciding whether a gap is governance drift vs missing implementation;
- scoping PRs so default receipts stay unchanged.

## Outputs

Consumers should be able to answer:

- which syntax entrypoints are safe to document for end users today;
- which surfaces are maintainer-only shadow evidence;
- which bindings and browser paths must remain AST-free until a future spec
  says otherwise.

## Compatibility

- This spec must stay aligned with `docs/proposals/ast-productization.md`
  (accepted) and `docs/plans/ast-productization.md` (complete).
- If `docs/specs/syntax-receipts.md` or `docs/specs/ast-shadow.md` disagree on
  behavior, those behavior specs win; this file only records support tier and
  promotion boundaries.
- `docs/capabilities/wasm.json` must not advertise syntax or AST commands while
  this spec lists bindings as `unchanged`.

## Proof Requirements

When updating this spec or the capability map:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo test -p tokmd --features ast --test cli_syntax_integration --verbose
cargo test -p tokmd-analysis --features ast ast --verbose
```

Required CI for behavior-adjacent AST changes remains `Tokmd Rust Result`
(`cargo test --all-features`). Docs-only updates to this file route through
`project_truth_docs` and `doc_artifacts_policy` scopes.

## Claim boundary

- **Establishes**: experimental vs developer-tooling vs unchanged support tiers
  for AST/syntax surfaces; opt-in command boundaries; promotion prerequisites.
- **Does not establish**: release readiness, semver support promises for syntax
  receipts, browser AST feasibility, default-receipt schema changes, or
  function-boundary precision on public analysis output.

## Open Questions

1. **Analyze preset integration:** should a future `syntax` or `bun-ub` preset
   call `tokmd syntax` internally, or stay packet-only? (Deferred.)
2. **Explicit `backend_id` wire field:** derived labels suffice today; reopen
   only when a consumer needs stable cross-artifact backend identity on the wire.
3. **Minimal distribution profile:** split a `tokmd-minimal` build without `ast`?
   (Rejected for current lane; see proposal Option B.)
