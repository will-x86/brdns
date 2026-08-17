.PHONY: check lint test fmt fuzz wdot wdoh test-integration

# Build + type-check everything
check:
	cargo check --all-targets

# Lint
lint:
	cargo clippy --all-targets

# Format check
fmt:
	cargo fmt --check

# Full test suite (network/Postgres tests self-skip without env vars)
test:
	cargo test

# Run all quality gates
ci: fmt lint check test

fuzz:
	FUZZTEST_FUZZ_FOR=30s cargo test --test fuzz --features fuzz __fuzztest_mod__ -- --nocapture

wdot:
	cargo watch -x "run --bin s dot"

wdoh:
	cargo watch -x "run --bin s doh"

test-integration:
	cargo test --test integration -- --nocapture
