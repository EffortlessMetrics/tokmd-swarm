# Changelog workflow

tokmd captures post-1.15.0 release intent in file-based Changie fragments. The
repository pins the expected Changie release in `.changie-version` and keeps
the prompt vocabulary in `.changie.yaml`.

## Capture a change

Run this while the change is fresh:

```bash
changie new --kind fixed --component CLI \
  --body "Describe the user-visible correction"
```

Use one of the configured components and kinds. `Documentation` and `Internal`
are intentionally `auto: none`: a batch containing only those kinds must use an
explicit version, never `batch auto`.

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

The historical changelog remains the source baseline until its lossless
Changie round-trip is landed. This configuration slice does not rewrite
`CHANGELOG.md` or manufacture historical fragments.

## Evidence boundary

The Changie files record release-note intent. They do not prove a release was
published, that artifacts are complete, or that a stable alias was promoted.
Those claims require the release receipts and exact consumer proof described in
the [release readiness guide](release-readiness.md).
