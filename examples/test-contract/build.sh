#!/bin/bash
TARGET="${CARGO_TARGET_DIR:-../../target}"
set -e
cd "$(dirname $0)"

INCLUDE_COVERAGE="${1:-false}"

if [ "$INCLUDE_COVERAGE" = "true" ]; then
  cargo-wasmcov build  --wasmcov-dir /Users/jrmncos/forks/near-sdk-rs/examples/wasmcov -- --all --target wasm32-unknown-unknown --release
  cp ../wasmcov/target/wasm32-unknown-unknown/release/test_contract.wasm ./res/
else
  cargo build --target wasm32-unknown-unknown --release
  cp $TARGET/wasm32-unknown-unknown/release/test_contract.wasm ./res/
  #wasm-opt -Oz --output ./res/test_contract.wasm ./res/test_contract.wasm
fi