# Justfile for bcur quality gates

# Default: format + clippy (deny warnings)
all: fmt clippy test check-no-std

# Build release with all features
build:
    cargo build --workspace --release --all-features

# Check compile
check:
    cargo check --workspace --all-features

# Bare-metal no_std (core transport only)
check-no-std:
    cargo check -p bcur --target thumbv7m-none-eabi --no-default-features

# Update lockfile
update:
    cargo update

# Run tests
test:
    cargo test --workspace --all-features

# Dependency advisories, licenses, bans, sources
deny:
    cargo deny check

# Clippy (CI-style, deny warnings)
clippy:
    cargo +nightly clippy --workspace \
        --all-targets \
        --all-features \
        -- -D warnings

# Clippy with auto-fix
clippy-fix:
    cargo +nightly clippy --workspace \
        --fix \
        --all-targets \
        --all-features \
        --allow-dirty \
        --allow-staged \
        -- -D warnings

# Format
fmt:
    cargo +nightly fmt

# Format check (CI)
fmt-check:
    cargo +nightly fmt --all -- --check

# Docs
doc:
    cargo +nightly doc --workspace --all-features --no-deps

# Compile benches (no timing run)
bench-check:
    cargo bench --workspace --all-features --no-run

# Full quality gate (no network except deny advisory DB if present)
quality: fmt-check clippy test check-no-std deny

fuzz-targets := "decode_ur fountain_part bytewords encode_roundtrip"

fuzz-build:
    cargo +nightly fuzz build

# local smoke: 60s per target, all four
fuzz:
    #!/usr/bin/env bash
    set -euo pipefail
    for t in {{fuzz-targets}}; do
        cargo +nightly fuzz run "$t" -- -max_total_time=60 -timeout=5
    done
