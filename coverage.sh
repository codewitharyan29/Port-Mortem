#!/usr/bin/env bash
# Measure line/region coverage of the port's core (src/lib.rs) using the
# native Rust test suite. Requires: rustup + cargo-llvm-cov (stable toolchain).
#   cargo install cargo-llvm-cov
#   ./coverage.sh
set -e
cargo llvm-cov --release --summary-only
echo
echo "For an HTML report:  cargo llvm-cov --release --html  (opens target/llvm-cov/html)"
