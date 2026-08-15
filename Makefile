.PHONY: fuzz

fuzz:
	FUZZTEST_FUZZ_FOR=30s cargo test __fuzztest_mod__ -- --nocapture

wdot:
	cargo watch -x "run --bin s dot"

wdoh:
	cargo watch -x "run --bin s dos"
