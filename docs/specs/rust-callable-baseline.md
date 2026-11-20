# Advisory Rust callable baseline

Issue [#765](https://github.com/EffortlessMetrics/tokmd-swarm/issues/765) starts with two source-controlled paths in `tokmd-analysis` at `07d878beee199568dbfa7698686ca1999addb7c8`. They cover Rust syntax landmark extraction and panic-seam analysis. The pilot reads regular blobs directly from Git. Both commits must be available locally. The path-to-module mapping describes the lexical source under observation, without resolving Cargo or compiler configuration.

From the repository, supply a full candidate commit ID:

```sh
cargo --locked xtask rust-maintainability --candidate "$(git rev-parse HEAD)"
cargo --locked xtask rust-maintainability --candidate "$(git rev-parse HEAD)" --json
cargo --locked xtask rust-maintainability --emit-baseline > target/rust-callable-baseline.json
git diff --no-index -- policy/rust-callable-baseline.json target/rust-callable-baseline.json
```

Comparison regenerates the pinned receipt and verifies its bytes against the committed copy. A reviewable baseline change includes the immutable source, metric/policy version, analyzer lock identity, changed population and supporting controls. The checker never writes the baseline. Missing Git objects or a stale baseline stop the command. Threshold observations, identity ambiguity and scope changes remain advisory.

## Measurement

The v1 receipt records the source commit, file blob, module, package, language, analyzer identity and per-file read/parse status. Source lines are one-based and inclusive. Byte ranges are zero-based and half-open, from the `fn` token through the closing brace or end of a bodyless signature. All offsets refer to the original blob.

- **Parameters** count syntactically declared arguments, including a variadic argument and excluding a receiver. The target maximum is six. A signature edit changes the exact identity.
- **Control nesting** counts the maximum syntactic nesting of `if`, `for`, `while`, `loop` and `match` expressions. An `else if` is a nested `if`. The target maximum is four. A declaration without a body, or one whose body contains a closure, async/const block or macro invocation, has unavailable nesting. Nested items have separate ownership.
- **Population** includes free functions, trait/impl methods and items declared directly in a measured function body or inline module. Attributes other than documentation exclude a callable or its enclosing module/trait/impl/file. Compiler expansion, local items inside unnamed blocks, anonymous callables, and files outside the two named paths are outside the measurement. There is no configuration evaluation or compiler identity claim.

The pilot measures neither boolean parameters/fields, NLOC, cognitive complexity, type complexity nor a maintainability index. It computes no debt ratio, coverage-adjusted grade or cross-population improvement. The heuristic complexity receipt and its coverage/denominator work in #688/#689 stay separate.

## Reading a comparison

An exact match requires one occurrence per side with the same path, lexical owners and token-normalized signature. Lines and byte ranges are evidence attached to the match. After exact matching, the checker groups unresolved symbols by a BLAKE3 fingerprint of the signature with its name replaced and the normalized body tokens. One-to-one matches are possible moves or renames for review. Multiple plausible pairings are ambiguous. An unmatched baseline row remains unresolved, including after a signature change, truncation that still parses, or a move outside the two files. The report makes no deletion or debt-retirement inference.

A missing file, unsupported Git entry, non-UTF-8 blob, excluded file or parse failure preserves its file status. Loss of a previously parsed file retains the old rows; loss of nesting inside an exact callable has its own observation. Lower syntax depth in one function alone cannot establish a whole-change improvement, for example after extracting a wrapper.

The rollout remains advisory pending broader metric and scope coverage, false-positive calibration, negative controls and review of the changed-path and periodic proof routes. No blocking workflow is enabled by this slice.
