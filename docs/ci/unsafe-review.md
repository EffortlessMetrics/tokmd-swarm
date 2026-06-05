# unsafe-review lane

`unsafe-review` is advisory unsafe-contract review. It checks whether changed
unsafe seams are reviewable: there is a safety contract, a local guard, test
reach, and a witness route.

It does **not** prove memory safety or UB-free status unless a matching witness
receipt is attached. Miri, sanitizers, fuzzing, and targeted tests provide the
concrete execution backstops.

## Tool split

| Tool | Question |
|------|----------|
| `cargo-allow` | Is this unsafe/source exception allowed, owned, and receipted? |
| `unsafe-review` | Is this unsafe seam reviewable: contract, guard, test reach, and witness route? |
| Miri / sanitizers | Did a concrete execution expose UB or memory misuse? |

This separation matters: unsafe is not only a lint or allowlist concern. The
repo needs both durable exception ownership and a separate reviewability plane
for contracts and witnesses.

## Expected artifacts

The repo-facing wrapper should emit stable artifacts under `target/unsafe-review/`:

```text
target/unsafe-review/
  cards.json
  pr-summary.md
  github-summary.md
  cards.sarif
  comment-plan.json
  witness-plan.md
  lsp.json
  receipt-audit.json
```

## PR routing

Run this lane when a PR touches unsafe, FFI, native, GPU, parser, C ABI, raw
pointer, layout-sensitive, process, or sandbox surfaces. Start advisory, then
require evidence or a waiver only after the baseline is calibrated and existing
unsafe seams have owners.
