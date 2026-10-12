#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$repo_root"

readonly rust_version='1.99.0'
readonly deny_version='0.20.2'
readonly deny_bin="$repo_root/.tools/bin/cargo-deny"
readonly elixir_version='1.19.5'
readonly otp_release='28'

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

if ! command -v elixir >/dev/null 2>&1; then
    printf 'Missing Elixir %s / Erlang OTP %s compiler-control tooling. See docs/development.md.\n' "$elixir_version" "$otp_release" >&2
    exit 1
fi
# Match the trusted verifier's startup/code-path environment restrictions.
tooling_profile="$(env -u ERL_FLAGS -u ERL_AFLAGS -u ERL_ZFLAGS -u ERL_LIBS \
    -u ELIXIR_ERL_OPTIONS -u ERL_INETRC -u ELIXIR_CLI_DRY_RUN \
    -u BASH_ENV -u ENV -u ZDOTDIR \
    elixir -e 'IO.write(System.version() <> "/" <> System.otp_release())')"
if [[ "$tooling_profile" != "$elixir_version/$otp_release" ]]; then
    printf 'Expected Elixir %s / Erlang OTP release %s; observed %s. See docs/development.md.\n' "$elixir_version" "$otp_release" "$tooling_profile" >&2
    exit 1
fi

export RUSTUP_TOOLCHAIN="$rust_version"
export CARGO_BUILD_WARNINGS=deny
export RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings"
# Cargo's encoded form takes precedence over RUSTDOCFLAGS; use the checked form.
unset CARGO_ENCODED_RUSTDOCFLAGS

printf 'Checking Rust %s, Elixir %s / OTP release %s, default features, all workspace targets\n' "$rust_version" "$elixir_version" "$otp_release"
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo doc --workspace --no-deps --locked
cargo arch
cargo reference
cargo progress
"$deny_bin" --locked --workspace check --deny unmatched-skip advisories bans licenses sources

printf 'Infrastructure checks passed; workflow parity remains incomplete.\n'
