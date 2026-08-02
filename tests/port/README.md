# Native ("port") tests

Per Rust convention, this port's own tests live as `#[cfg(test)]` modules
directly inside the source files they test, rather than in this directory:

- `src/lib.rs` — 38 unit + property tests (`mod tests`, `mod api_tests`,
  `mod property_tests`) covering the core algorithm, all 7 sort modes, Unicode
  digit handling, and edge cases found while porting.
- `src/main.rs` — 5 CLI contract tests (`mod cli_tests`) covering flag
  parsing, comparison output, and key formatting.

Run them with:
```bash
cargo test --release
```

This directory exists to satisfy the anatomy convention (`tests/port/` for
new, non-original tests); the tests themselves are colocated with the code
under Rust's standard `#[cfg(test)]` pattern rather than duplicated here.
