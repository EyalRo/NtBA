#!/usr/bin/env bash
# Build the WebAssembly module and place it where web/index.html expects it.
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${1:-release}"
flags=()
[ "$profile" = release ] && flags+=(--release)

cargo build -p ntba-wasm --target wasm32-unknown-unknown "${flags[@]}"
cp "target/wasm32-unknown-unknown/$profile/ntba_wasm.wasm" web/ntba.wasm

# Optional size optimisation when binaryen is available (it is in the nix shell).
if [ "$profile" = release ] && command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -O3 web/ntba.wasm -o web/ntba.wasm
fi
echo "built web/ntba.wasm ($(wc -c < web/ntba.wasm) bytes)"
