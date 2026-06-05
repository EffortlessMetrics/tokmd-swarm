# ripr PR lane

`ripr` is tokmd's static mutation-exposure lane. It catches mutation-shaped
weak-test and weak-oracle signal earlier and cheaper than runtime mutation
because it analyzes changed code statically at PR time.

It does **not** run mutants, report killed or survived outcomes, prove
correctness, or replace runtime mutation testing. Runtime mutation remains the
slower execution-backed backstop for risk surfaces where static exposure signal
or reviewer judgment says the spend is justified.

## Role in the evidence stack

| Layer | Tool | What it answers |
|-------|------|-----------------|
| Static code shape | rustc / Clippy | Is the local code shape acceptable? |
| Static mutation exposure | `ripr` | Could a behavioral mutation escape the current oracle surface? |
| Runtime mutation | `cargo-mutants` | Do tests kill concrete mutants for the selected target? |
| Coverage telemetry | `cargo-llvm-cov` / Codecov | Which execution surface was observed? |

`ripr` is therefore a first-class PR signal, but advisory until the repo has
calibrated finding quality and suppression policy. New high-confidence exposure
gaps should lead to focused tests, a scoped suppression with owner and expiry,
or a follow-up issue with an explicit claim boundary.

## Expected artifacts

The repo-facing wrapper should emit stable artifacts under `target/ripr/pr/`:

```text
target/ripr/pr/
  pr-summary.md
  repo-exposure.json
  review.md
  agent-packet.json
  first-useful-action.md
  first-useful-action.json
```

The suppression ledger is `policy/ripr-suppressions.toml`. Suppressions are for
intentional, explained test gaps only; they are not a way to hide annoying
findings.

## PR routing

Default PRs may run `ripr` for production Rust diffs because it is cheap static
signal. Deep runtime mutation belongs on label, main, nightly, or release lanes.
Do not broaden a PR just to satisfy `ripr`; add local proof or narrow the claim.
