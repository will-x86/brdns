.PHONY: check lint test fmt fuzz wdot wdoh test-integration

check:
	cargo check --all-targets

lint:
	cargo clippy --all-targets

fmt:
	cargo fmt --check

test:
	cargo test

ci: fmt lint check test

fuzz:
	FUZZTEST_FUZZ_FOR=30s cargo test --test fuzz --features fuzz __fuzztest_mod__ -- --nocapture

watch:
	cargo watch -x "run --bin s"

test-integration:
	cargo test --test integration -- --nocapture
