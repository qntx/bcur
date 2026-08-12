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

# Full quality gate (no network except deny advisory DB if present)
quality: fmt-check clippy test check-no-std deny
