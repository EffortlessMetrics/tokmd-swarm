# Changelog workflow

tokmd captures post-1.15.0 release intent in file-based Changie fragments. The
repository pins the expected Changie release in `.changie-version` and keeps
the prompt vocabulary in `.changie.yaml`.

## Capture a change

Run this while the change is fresh:

```bash
cargo change --kind fixed --component CLI \
  --body "Describe the user-visible correction"
```

`cargo change` is the repository's xtask shortcut for creating a Changie
fragment, so contributors do not need a separate global `changie` install for
this step. The CLI accepts case-insensitive component/kind input and writes
canonical values. Staged fragments must already use the exact configured
component spelling (`CLI`, `Release`, `Browser/WASM`, and so on) and lowercase
kind keys (`added`, `changed`, `fixed`, `security`, `documentation`, `internal`).
The staged format is a flat mapping with one `field: value` per line: required
`component`, `kind`, and `body`, plus optional `time`. Values are plain single-line
strings or JSON-quoted strings, as emitted by `cargo change`. Quote bodies with
YAML punctuation, comments, or newlines; duplicate/unknown fields, collections,
block scalars, and other YAML syntax are rejected with creation guidance. Blank
lines and standalone comments are allowed. The hook validates this pinned
fragment format; it does not implement a general YAML parser.
Optional `time` values use RFC3339 with a four-digit year, uppercase `T`/`Z`
and no leap seconds; quoted timestamps are accepted.
The `documentation` and `internal` kinds are intentionally `auto: none`: a
batch containing only those kinds must use an explicit version, never
`batch auto`.

Stage the fragment with the corresponding change, then run:

```bash
cargo precommit
```

This checks the Git index, so unstaged edits do not change its verdict. It
requires a fragment for user-visible or unknown paths, reports explicit
test/generated-only exemptions, and rejects invalid staged fragments. It
does not create fragments or edit the index. Optional `cargo --locked xtask hooks
install` adds the check to the existing local hook workflow described in
[CONTRIBUTING.md](../CONTRIBUTING.md#local-hooks). The pre-commit launcher runs
only this staged validator, preserving partial staging without lint fixes or
automatic restaging. The installer validates both managed hooks and prepares
their executable modes on Unix before configuring Git. An unrelated effective
hook configuration is preserved and reported as an error.

### Pinned layout and creation boundary

`.changes/unreleased/` is part of the pinned contract shared by `.changie.yaml`
and `xtask/src/tasks/changelog.rs`. Changes to `changesDir`, `unreleasedDir`,
components, or kind keys must update both surfaces together; a focused test
checks that the committed configuration and validator agree.

Fragments are direct `.yaml` children of that directory. The pinned tool does
not discover `.yml` or uppercase extensions, so creation and staged validation
reject them, including when a valid sibling fragment is staged. Custom
`--output` values cannot be absolute, traverse parents, select nested paths,
or contain colons/control characters. Creation rejects symlink or junction
directories. Contents are written and synced in a temporary file in the same
directory before installation without overwriting existing files or leaf
symlinks. Ordinary write failures discard the temporary file; abrupt process
termination can leave a `.tokmd-change-*.tmp` file, but no partial fragment is
installed. Generated filename collisions retry with a numbered suffix;
explicit `--output` collisions remain errors. These checks assume repository
directories and temporary files are not being replaced concurrently by another
process, and do not claim crash-durable directory metadata.

## Prepare a release

The historical import is not complete yet, so release preparation is currently
dry-run-only:

```bash
changie batch 1.15.1 --dry-run
changie merge --dry-run
```

Do not run a write-mode `changie merge` until the lossless historical baseline
is present in `.changes/`. Changie reconstructs `CHANGELOG.md` from its header
and version fragments; running it now would discard the existing pre-1.15
history. The future write-mode sequence will supply an explicit version,
review the generated version file, and then merge it into `CHANGELOG.md`.
Publishing, tagging, alias promotion, and release creation remain governed by
the [canonical release checklist](releases/release-checklist.md).

The 1.15 section of `CHANGELOG.md` is normalized into one stable entry and
compact release-candidate history, while the 1.14-and-earlier release text is
preserved. The stable 1.15 comparison links to the previous stable release, and
Unreleased starts at 1.15.0; older comparison references remain unchanged.
That checked-in changelog remains the source baseline until its
lossless Changie round-trip is landed under
[issue #530](https://github.com/EffortlessMetrics/tokmd-swarm/issues/530).
The configuration and staged-input slice does not manufacture historical
fragments or complete the round-trip requirement.

## Evidence boundary

The Changie files record release-note intent. They do not prove a release was
published, that artifacts are complete, or that a stable alias was promoted.
Those claims require the release receipts and exact consumer proof described in
the [release readiness guide](release-readiness.md).
