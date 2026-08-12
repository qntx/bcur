# Makefile for bcur

.PHONY: all build check check-no-std test clippy clippy-fix fmt doc update

all: fmt clippy-fix check-no-std test

build:
	cargo build --workspace --release --all-features

check:
	cargo check --workspace --all-features

# Bare-metal no_std check (core transport only; dcbor implies std).
check-no-std:
	cargo check -p bcur --target thumbv7m-none-eabi --no-default-features

update:
	cargo update

test:
	cargo test --workspace --all-features

bench:
	cargo bench --all-features

clippy:
	cargo +nightly clippy --workspace \
		--all-targets \
		--all-features \
		-- -D warnings

clippy-fix:
	cargo +nightly clippy --workspace \
		--fix \
		--all-targets \
		--all-features \
		--allow-dirty \
		--allow-staged \
		-- -D warnings

fmt:
	cargo +nightly fmt --all -- \
		--config unstable_features=true,group_imports=StdExternalCrate,imports_granularity=Module

doc:
	cargo +nightly doc --workspace --all-features --open
