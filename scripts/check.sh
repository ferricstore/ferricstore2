#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$repo_root"

readonly rust_version='1.99.0'
readonly deny_version='0.20.2'
readonly deny_bin="$repo_root/.tools/bin/cargo-deny"

if [[ $# -ne 0 ]]; then
    printf 'Usage: bash scripts/check.sh (no check may be skipped)\n' >&2
    exit 2
fi

if [[ ! -x "$deny_bin" ]]; then
    printf 'Missing repository-local cargo-deny %s. Install with:\n' "$deny_version" >&2
    printf 'cargo +%s install cargo-deny --version %s --locked --root .tools\n' "$rust_version" "$deny_version" >&2
    exit 1
fi
if [[ "$("$deny_bin" --version)" != "cargo-deny $deny_version" ]]; then
    printf 'Expected cargo-deny %s at %s\n' "$deny_version" "$deny_bin" >&2
    exit 1
fi

export RUSTUP_TOOLCHAIN="$rust_version"
export CARGO_BUILD_WARNINGS=deny
export RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings"
# Cargo's encoded form takes precedence over RUSTDOCFLAGS; use the checked form.
unset CARGO_ENCODED_RUSTDOCFLAGS

printf 'Checking Rust %s, default features, all workspace targets\n' "$rust_version"
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
cargo arch
cargo reference
"$deny_bin" --locked --workspace check --deny unmatched-skip advisories bans licenses sources

printf 'Infrastructure checks passed; workflow parity remains incomplete.\n'
