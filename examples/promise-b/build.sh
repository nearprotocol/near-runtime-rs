#!/bin/bash
set -e
cd "`dirname $0`"
source ../flags.sh
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/promise_b.wasm ./res/
#wasm-opt -Oz --output ./res/status_message.wasm ./res/status_message.wasm

