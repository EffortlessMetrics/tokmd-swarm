#!/usr/bin/env bash
set -euo pipefail

case "$(uname -m)" in
    x86_64) node_arch=x64 ;;
    aarch64|arm64) node_arch=arm64 ;;
    *) printf 'Unsupported Node.js architecture: %s\n' "$(uname -m)" >&2; exit 1 ;;
esac

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
if ! command -v rustup >/dev/null; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs |
        sh -s -- -y --default-toolchain none --no-modify-path
fi

# Use the exact toolchain from the required MSRV CI step.
workspace_toolchain=1.95.0
rustup toolchain install stable --profile minimal --no-self-update
rustup toolchain install "$workspace_toolchain" --profile minimal \
    --component rustfmt,clippy --no-self-update
rustup run "$workspace_toolchain" cargo fetch --locked --manifest-path Cargo.toml

tool_root="$HOME/.local/share/tokmd-codex"
node_root="$tool_root/node"
mkdir -p "$node_root"

# Resolve the latest release from Node.js, then download that exact archive.
node_dist=https://nodejs.org/dist
node_checksum=$(curl --proto '=https' --tlsv1.2 -fsSL "$node_dist/latest/SHASUMS256.txt" |
    awk -v arch="$node_arch" '$2 ~ ("^node-v[0-9]+\\.[0-9]+\\.[0-9]+-linux-" arch "\\.tar\\.xz$") { print }')
if [[ -z "$node_checksum" ]]; then
    printf 'No Node.js Current Linux archive found for %s\n' "$node_arch" >&2
    exit 1
fi
read -r _ node_archive <<< "$node_checksum"
node_version=${node_archive#node-}
node_version=${node_version%%-linux-*}

if [[ ! -x "$node_root/$node_version/bin/node" ]]; then
    stage=$(mktemp -d "$node_root/.install.XXXXXXXX")
    trap 'rm -rf -- "$stage"' EXIT
    curl --proto '=https' --tlsv1.2 -fsSL "$node_dist/$node_version/$node_archive" \
        -o "$stage/$node_archive"
    (
        cd "$stage"
        sha256sum --check --strict <<< "$node_checksum"
        mkdir extracted
        tar -xJf "$node_archive" -C extracted --strip-components=1
        mv -T extracted "$node_root/$node_version"
    )
fi
ln -sfnT "$node_version" "$node_root/current"

curl --proto '=https' --tlsv1.2 -fsSL https://astral.sh/uv/install.sh |
    UV_UNMANAGED_INSTALL="$tool_root/uv" sh
