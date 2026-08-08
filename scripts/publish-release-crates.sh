#!/usr/bin/env bash
set -euo pipefail

# Keep release-shell responsibilities limited to immutable-source and
# credential guards. Publish ordering, retries, receipts, registry visibility,
# and bootstrap handling belong to the repo-native xtask command.

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "CARGO_REGISTRY_TOKEN is required" >&2
  exit 1
fi

release_tag="${RELEASE_TAG:-${GITHUB_REF_NAME:-}}"
if [[ ! "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "a stable release tag is required via RELEASE_TAG or GITHUB_REF_NAME" >&2
  exit 1
fi

expected_version="${release_tag#v}"
actual_version="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; metadata = json.load(sys.stdin); print(next(package["version"] for package in metadata["packages"] if package["name"] == "tokmd"))')"
if [[ "$actual_version" != "$expected_version" ]]; then
  echo "release tag ${release_tag} does not match workspace version ${actual_version}" >&2
  exit 1
fi

echo "Running release publish preflight"
cargo xtask version-consistency
cargo xtask publish-surface --json

receipt_path="${PUBLISH_RECEIPT_PATH:-target/publishing/publish-receipt.json}"
publish_args=(
  publish
  --receipt "$receipt_path"
  --bootstrap tokmd-types,tokmd-envelope
  --yes
)
if [[ -f "$receipt_path" ]]; then
  publish_args+=(--resume)
fi

cargo xtask "${publish_args[@]}"

echo "Verifying the published crate surface"
cargo xtask publish-surface --json --verify-publish

echo "All publishable tokmd crates are published or already present."
