.PHONY: all web serve test check determinism fmt sfx clean

all: check web

# Build web/ntba.wasm (release, wasm-opt'd when available).
web:
	scripts/build-web.sh

# Build and serve at http://localhost:8080 (override with PORT=...).
serve: web
	scripts/serve.sh

test:
	cargo test --workspace

# Everything CI runs.
check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cargo build -p ntba-wasm --target wasm32-unknown-unknown

# Cross-build determinism: native tests pin the hash, this checks the wasm build agrees.
determinism: web
	node scripts/check-determinism.mjs
	node scripts/test-net-wasm.mjs

fmt:
	cargo fmt --all

# Regenerate the C64-style sound effects in web/audio.
sfx:
	node scripts/make-sfx.mjs

clean:
	cargo clean
	rm -f web/ntba.wasm
