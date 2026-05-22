# Spec: Diff Input Classification

- Status: active
- Schema family, if any: n/a
- Related ADRs: `docs/adr/0010-diff-input-classification.md`
- Related proof scopes: `tokmd_cli`, `project_truth_docs`

## Contract

`tokmd diff` accepts two input references through positional arguments or
through `--from` and `--to`. Each input may be either a local receipt path or a
git revision.

The command must classify each input in this order:

1. If the input exists as a filesystem path, load a language receipt from that
   path.
2. If the input does not exist but is clearly path-like, fail with an invalid
   reference/path error before trying git revision resolution.
3. Otherwise, resolve the input as a git revision.

This order is required so missing receipt paths produce a path-input error even
when the current directory is not inside a git repository. Git repository
preconditions must not hide the more specific missing-path problem for clearly
path-like inputs.

## Inputs

Diff inputs can be supplied as:

```bash
tokmd diff <from> <to>
tokmd diff --from <from> --to <to>
```

An existing path is interpreted as a receipt path. Path loading has these
current rules:

- a directory input loads `lang.json` inside that directory;
- a `receipt.json` input loads sibling `lang.json`;
- any other file input is loaded directly as the language receipt.

A missing input is classified as path-like when any of these are true:

- it is an absolute path;
- it has a file extension;
- it starts with `./`;
- it starts with `../`;
- it starts with `.\\`;
- it starts with `..\\`.

Inputs that do not match those path-like rules remain eligible for git revision
resolution. Examples include `HEAD`, `main`, branch names, tag names, and other
extensionless ref names.

## Outputs

For a missing path-like input, `tokmd diff` fails and includes:

```text
invalid reference or path '<input>': path does not exist
```

The command may wrap that message with the higher-level source being loaded,
such as:

```text
Failed to load diff source '<input>'
```

For non-path inputs, git resolution owns the error. When git support is
disabled, unavailable, or the current directory is not inside a git repository,
the corresponding git-resolution error remains valid for non-path inputs.

## Compatibility

This spec preserves the current dual input model. It does not add explicit
path/ref mode flags, change receipt schemas, change diff output schemas, change
git revision scanning behavior, or alter public release behavior.

The path-like heuristic is intentionally conservative. Extensionless missing
relative paths can still be interpreted as git revisions. Expanding the
heuristic must be treated as a CLI behavior change and must update this spec,
ADR-0010, and tests in the same review.

## Proof Requirements

For documentation-only changes to this contract:

```bash
cargo xtask doc-artifacts --check
cargo xtask docs --check
cargo xtask proof-policy --check
cargo xtask affected --base origin/main --head HEAD --json-output target/proof/affected-diff-input-classification.json
cargo xtask proof --profile affected --base origin/main --head HEAD --plan --plan-json target/proof/proof-plan-diff-input-classification.json --evidence-json target/proof/proof-evidence-diff-input-classification.json
cargo fmt-check
git diff --check
```

For implementation changes to diff input classification, also run the focused
current-behavior tests:

```bash
cargo test -p tokmd --test cli_error_paths_w51 diff_nonexistent_before_after --verbose
cargo test -p tokmd --test cli_error_paths_w51 --verbose
cargo test -p tokmd --test cli_errors_w66 diff_with_nonexistent_files_produces_error --verbose
```

Hosted release/Nix validation should be used when the change is intended to
repair or protect a Nix sandbox failure path.

## Open Questions

- Whether extensionless missing relative paths should ever be treated as paths
  before git refs.
- Whether a future explicit `--from-path` / `--from-ref` mode would reduce
  ambiguity enough to justify the CLI surface.
