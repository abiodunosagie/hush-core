#!/usr/bin/env bash
# Builds hush-wasm and copies it into web/ for the browser demo.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p hush-wasm --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/hush_wasm.wasm web/hush.wasm
echo "web/hush.wasm $(wc -c < web/hush.wasm) bytes"
