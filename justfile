set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

# format, lint (native and wasm), and test: the gate before each commit
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p stemin-compile --no-default-features --features compile --target wasm32-unknown-unknown -- -D warnings
    cargo clippy -p stemin-format --no-default-features --target wasm32-unknown-unknown -- -D warnings
    cargo test --workspace

# format the workspace
fmt:
    cargo fmt --all

# cargo-zigbuild links every target from one machine, macOS included.
# build the five release binaries into target/tools, with SHA256SUMS
tools:
    #!/usr/bin/env bash
    set -euo pipefail
    out=target/tools
    mkdir -p "$out"
    targets=(
        "x86_64-unknown-linux-musl stemin-x86_64-linux"
        "aarch64-unknown-linux-musl stemin-aarch64-linux"
        "x86_64-apple-darwin stemin-x86_64-macos"
        "aarch64-apple-darwin stemin-aarch64-macos"
        "x86_64-pc-windows-gnu stemin-x86_64-windows.exe"
    )
    for pair in "${targets[@]}"; do
        read -r target name <<< "$pair"
        rustup target add "$target" > /dev/null
        nix shell nixpkgs#cargo-zigbuild nixpkgs#zig -c \
            cargo zigbuild --release --locked -p stemin --target "$target"
        ext=""
        [[ "$target" == *windows* ]] && ext=".exe"
        cp "target/$target/release/stemin$ext" "$out/$name"
    done
    (cd "$out" && sha256sum stemin-* > SHA256SUMS)
    ls -l "$out"

# The docs link `releases/latest/download/<file>`, so a new release moves every
# link at once. Tag the commit first: `git tag vX.Y.Z && git push --tags`.
# publish the binaries as a release, e.g. `just release-tools v0.2.0`
release-tools version: tools
    gh release create {{version}} target/tools/stemin-* target/tools/SHA256SUMS \
        --repo stemin-app/stemin-checker --verify-tag --title "stemin {{version}}" \
        --notes "The Stemin checker {{version}}. Usage: https://stemin.app/docs/check"
