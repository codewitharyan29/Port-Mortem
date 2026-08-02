#!/usr/bin/env bash
# Build the WASM demo module into web/pkg/. Requires:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-pack
#
# Windows note: this script needs a bash (Git Bash, WSL). If you see
# "Windows Subsystem for Linux has no installed distributions", don't install
# WSL just for this -- skip this script and run the one real command below
# directly in PowerShell instead (it's the only line that matters):
#
#   wasm-pack build --target web --out-dir web/pkg --out-name natsort_core -- --features wasm
#
set -e
wasm-pack build --target web --out-dir web/pkg --out-name natsort_core -- --features wasm
echo
echo "Built. Now serve the web/ folder over HTTP and open index.html:"
echo "  cd web && python3 -m http.server 8000   # then open http://localhost:8000"
