#!/bin/bash
TARGET="${CARGO_TARGET_DIR:-../../target}"
set -e
cd "$(dirname $0)"
cargo build --target wasm32-unknown-unknown --release
cp $TARGET/wasm32-unknown-unknown/release/mpc_contract.wasm ./res/
#wasm-opt -Oz --output ./res/mpc_contract.wasm ./res/status_message.wasm
