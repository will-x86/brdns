.PHONY: fuzz

fuzz:
	FUZZTEST_FUZZ_FOR=30s cargo test --test fuzz --features fuzz __fuzztest_mod__ -- --nocapture

wdot:
	cargo watch -x "run --bin s dot"

wdoh:
	cargo watch -x "run --bin s doh"

test-integration:
	cargo test --test integration -- --nocapture
